//! Spreadsheet import.
//!
//! The file arrives as raw bytes rather than multipart: one field, no boundary
//! parsing, and the browser already knows the extension. Excel files go through
//! calamine; CSV gets a small reader because a spreadsheet export is the most
//! common way a small kitchen has its ingredient list already.

use axum::{
    body::Bytes,
    extract::{Query, State},
    Json,
};
use rust_decimal::Decimal;
use serde::Deserialize;
use std::collections::HashMap;
use std::str::FromStr;
use uuid::Uuid;

use crate::auth::guard::{CanWriteMaterials, Require};
use crate::error::ApiError;
use crate::spreadsheet::Format;
use crate::AppState;

use super::templates::{existing_names, unit_ids};
use super::{ImportResult, RowError, MAX_IMPORT_ROWS};

#[derive(Debug, Deserialize)]
pub struct ImportQuery {
    /// ~xlsx~ (the default) or ~csv~.
    pub format: Option<String>,
}

struct ParsedItem {
    name: String,
    unit_id: Uuid,
    min_stock: Decimal,
    shelf_life_days: Option<i32>,
}

pub async fn import_materials(
    State(state): State<AppState>,
    guard: Require<CanWriteMaterials>,
    Query(query): Query<ImportQuery>,
    body: Bytes,
) -> Result<Json<ImportResult>, ApiError> {
    if body.is_empty() {
        return Err(ApiError::BadRequest("empty_file"));
    }

    let rows = Format::from_request(query.format.as_deref(), &body)?.parse(&body)?;

    // Row numbers stay tied to the spreadsheet, header included.
    let data_rows: Vec<(usize, &Vec<String>)> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| (index + 1, row))
        .filter(|(number, row)| !(*number == 1 && is_header(row)))
        .filter(|(_, row)| row.iter().any(|cell| !cell.trim().is_empty()))
        .collect();

    if data_rows.is_empty() {
        return Err(ApiError::BadRequest("empty_file"));
    }

    if data_rows.len() > MAX_IMPORT_ROWS {
        return Err(ApiError::BadRequest("too_many_rows"));
    }

    let auth = guard.auth;
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let units = unit_ids(&mut tx, auth.tenant_id).await?;
    let mut taken = existing_names(&mut tx, auth.tenant_id).await?;

    let mut created = 0_i64;
    let mut skipped = 0_i64;
    let mut errors = Vec::new();

    for (number, row) in data_rows {
        match parse_row(row, &units) {
            Err(code) => errors.push(RowError { row: number, code }),
            Ok(item) => {
                let key = item.name.to_lowercase();

                if taken.contains(&key) {
                    skipped += 1;
                    continue;
                }

                sqlx::query(
                    "insert into materials (tenant_id, name, base_unit_id, min_stock, shelf_life_days)
                     values ($1, $2, $3, $4, $5)",
                )
                .bind(auth.tenant_id)
                .bind(&item.name)
                .bind(item.unit_id)
                .bind(item.min_stock)
                .bind(item.shelf_life_days)
                .execute(&mut *tx)
                .await?;

                taken.insert(key);
                created += 1;
            }
        }
    }

    tx.commit().await?;

    Ok(Json(ImportResult {
        created,
        skipped,
        errors,
    }))
}

fn is_header(row: &[String]) -> bool {
    row.iter()
        .take(2)
        .any(|cell| matches!(cell.trim().to_lowercase().as_str(), "nama" | "name"))
}

fn parse_row(row: &[String], units: &HashMap<String, Uuid>) -> Result<ParsedItem, &'static str> {
    let name = row.first().map(|cell| cell.trim()).unwrap_or_default();

    if name.is_empty() {
        return Err("name_required");
    }

    if name.chars().count() > 120 {
        return Err("name_too_long");
    }

    let unit_code = normalize_unit(row.get(1).map(|cell| cell.as_str()).unwrap_or_default())
        .ok_or("unknown_unit")?;

    let unit_id = *units.get(unit_code).ok_or("unknown_unit")?;

    let min_stock = match row.get(2).map(|cell| cell.trim()).unwrap_or_default() {
        "" => Decimal::ZERO,
        value => parse_decimal(value).ok_or("invalid_min_stock")?,
    };

    if min_stock.is_sign_negative() {
        return Err("invalid_min_stock");
    }

    let shelf_life_days = match row.get(3).map(|cell| cell.trim()).unwrap_or_default() {
        "" => None,
        value => {
            let days = parse_decimal(value).ok_or("invalid_shelf_life")?;
            let days = i32::try_from(days).map_err(|_| "invalid_shelf_life")?;

            if days < 0 {
                return Err("invalid_shelf_life");
            }

            Some(days)
        }
    };

    Ok(ParsedItem {
        name: name.to_string(),
        unit_id,
        min_stock,
        shelf_life_days,
    })
}

/// Spreadsheets are written by people: accept the spellings they actually use.
fn normalize_unit(value: &str) -> Option<&'static str> {
    match value.trim().to_lowercase().as_str() {
        "g" | "gr" | "gram" => Some("g"),
        "kg" | "kilo" | "kilogram" => Some("kg"),
        "ml" | "mililiter" | "millilitre" => Some("ml"),
        "l" | "liter" | "litre" => Some("l"),
        "pcs" | "pc" | "buah" | "biji" | "unit" => Some("pcs"),
        "lusin" | "dozen" => Some("lusin"),
        _ => None,
    }
}

/// Accepts the Indonesian decimal comma, since that is what Excel writes here.
fn parse_decimal(value: &str) -> Option<Decimal> {
    let normalized = value.trim().replace(' ', "").replace(',', ".");
    Decimal::from_str(&normalized).ok()
}
