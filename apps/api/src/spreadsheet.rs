//! Reading a spreadsheet a user uploaded.
//!
//! The file arrives as raw bytes rather than multipart: one field, no boundary
//! parsing, and the browser already knows the extension. Excel files go through
//! calamine; CSV gets a small reader because a spreadsheet export is the most
//! common way a small kitchen already has its data.
//!
//! This module only turns bytes into rows of text. What a row means belongs to
//! the importer that asked for it.

use std::io::Cursor;

use calamine::{open_workbook_auto_from_rs, Data, Reader};

use crate::error::ApiError;

pub enum Format {
    Csv,
    Xlsx,
}

impl Format {
    /// `format` wins when given; otherwise a ZIP header means xlsx and
    /// everything else is read as CSV.
    pub fn from_request(value: Option<&str>, body: &[u8]) -> Result<Format, ApiError> {
        match value {
            Some("csv") => Ok(Format::Csv),
            Some("xlsx") => Ok(Format::Xlsx),
            Some(_) => Err(ApiError::BadRequest("unsupported_format")),
            None => {
                if body.starts_with(b"PK") {
                    Ok(Format::Xlsx)
                } else {
                    Ok(Format::Csv)
                }
            }
        }
    }

    pub fn parse(self, bytes: &[u8]) -> Result<Vec<Vec<String>>, ApiError> {
        match self {
            Format::Csv => Ok(parse_csv(bytes)),
            Format::Xlsx => parse_xlsx(bytes),
        }
    }
}

pub fn cell_text(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        Data::String(value) => value.trim().to_string(),
        Data::Float(value) => trim_number(*value),
        Data::Int(value) => value.to_string(),
        Data::Bool(value) => value.to_string(),
        Data::DateTime(value) => trim_number(value.as_f64()),
        Data::DateTimeIso(value) | Data::DurationIso(value) => value.clone(),
        Data::Error(error) => format!("{error:?}"),
    }
}

/// "2.5" stays "2.5"; "2.0" becomes "2" so the parser sees a clean number.
fn trim_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

fn parse_xlsx(bytes: &[u8]) -> Result<Vec<Vec<String>>, ApiError> {
    let mut workbook = open_workbook_auto_from_rs(Cursor::new(bytes))
        .map_err(|_| ApiError::BadRequest("invalid_file"))?;

    let range = workbook
        .worksheet_range_at(0)
        .ok_or(ApiError::BadRequest("empty_file"))?
        .map_err(|_| ApiError::BadRequest("invalid_file"))?;

    Ok(range
        .rows()
        .map(|row| row.iter().map(cell_text).collect())
        .collect())
}

fn parse_csv(bytes: &[u8]) -> Vec<Vec<String>> {
    let text = String::from_utf8_lossy(bytes);
    let mut rows = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut characters = text.chars().peekable();

    while let Some(character) = characters.next() {
        match character {
            '"' if quoted => {
                if characters.peek() == Some(&'"') {
                    characters.next();
                    field.push('"');
                } else {
                    quoted = false;
                }
            }
            '"' if field.is_empty() => quoted = true,
            ',' if !quoted => {
                row.push(field.trim().to_string());
                field.clear();
            }
            '\r' if !quoted => {}
            '\n' if !quoted => {
                row.push(field.trim().to_string());
                rows.push(std::mem::take(&mut row));
                field.clear();
            }
            other => field.push(other),
        }
    }

    if !field.is_empty() || !row.is_empty() {
        row.push(field.trim().to_string());
        rows.push(row);
    }

    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted_fields_keep_their_commas() {
        let rows = parse_csv(b"sale_ref,product,qty\nINV-1,\"Roti, sobek\",2\n");

        assert_eq!(rows[1], vec!["INV-1", "Roti, sobek", "2"]);
    }

    #[test]
    fn doubled_quotes_become_one_quote() {
        let rows = parse_csv(b"product\n\"Roti \"\"spesial\"\"\"\n");

        assert_eq!(rows[1], vec!["Roti \"spesial\""]);
    }

    #[test]
    fn crlf_and_a_missing_final_newline_are_both_fine() {
        let rows = parse_csv(b"a,b\r\n1,2");

        assert_eq!(rows, vec![vec!["a", "b"], vec!["1", "2"]]);
    }

    #[test]
    fn a_zip_header_is_detected_as_xlsx() {
        assert!(matches!(
            Format::from_request(None, b"PK\x03\x04"),
            Ok(Format::Xlsx)
        ));
        assert!(matches!(
            Format::from_request(None, b"a,b"),
            Ok(Format::Csv)
        ));
        assert!(Format::from_request(Some("pdf"), b"").is_err());
    }
}
