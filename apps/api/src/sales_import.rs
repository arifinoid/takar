//! Importing sales from a CSV.
//!
//! A spreadsheet is the fastest way for a small kitchen to catch up on past
//! sales, and the most dangerous: a replayed file must never subtract the same
//! stock twice. Rows are grouped into sales by the user's own `sale_ref`
//! (invoice or receipt number). That reference is hashed into the sale's
//! `client_ref`, so importing the same file again reports `duplicate` instead
//! of writing a second sale. A row without a reference is imported as a new
//! sale every time — the server has nothing to recognise it by.
//!
//! Each sale is its own transaction, so one bad row never rolls back the rest,
//! the same rule the offline sync queue follows.

use std::collections::HashMap;
use std::str::FromStr;

use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Query, State},
    routing::post,
    Json, Router,
};
use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, TimeZone, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::guard::{CanWriteSales, Require};
use crate::sales::{write_sale, SaleBody, SaleItemBody, CHANNELS};
use crate::spreadsheet::Format;
use crate::{error::ApiError, AppState};

/// Same cap as onboarding: a longer file is a mistake, not a month of sales.
const MAX_ROWS: usize = 500;

/// Deriving the client reference from the user's own key means the same
/// invoice number always maps to the same UUID, on any device.
const SALE_NAMESPACE: Uuid = Uuid::NAMESPACE_URL;

#[derive(Debug, Deserialize)]
pub struct ImportQuery {
    pub format: Option<String>,
    /// Minutes east of UTC for timestamps written without an offset.
    /// Jakarta is 420. Defaults to UTC.
    pub tz_offset: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct ImportRow {
    /// Spreadsheet row number, header included, so the message points at what
    /// the user sees.
    pub row: usize,
    pub sale_ref: Option<String>,
    /// `created`, `duplicate`, or `rejected`.
    pub status: &'static str,
    pub sale_id: Option<Uuid>,
    /// The API error code when the row was rejected.
    pub code: Option<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct ImportSummary {
    /// Sales written, not rows.
    pub created: i64,
    /// Sales already present because their `sale_ref` had been imported.
    pub duplicate: i64,
    /// Rows that could not be read or were refused by the stock check.
    pub rejected: i64,
    pub rows: Vec<ImportRow>,
}

struct ParsedRow {
    line: usize,
    sale_ref: Option<String>,
    sold_at: Option<DateTime<Utc>>,
    channel: Option<String>,
    note: Option<String>,
    product_id: Uuid,
    qty: Decimal,
    price: Option<Decimal>,
}

struct Group {
    sale_ref: Option<String>,
    rows: Vec<ParsedRow>,
}

pub async fn import_sales(
    State(state): State<AppState>,
    guard: Require<CanWriteSales>,
    Query(query): Query<ImportQuery>,
    body: Bytes,
) -> Result<Json<ImportSummary>, ApiError> {
    if body.is_empty() {
        return Err(ApiError::BadRequest("empty_file"));
    }

    // Excel would arrive with dates as serial numbers, so only CSV is read
    // here. "Save as CSV" is one menu item away in every spreadsheet app.
    if matches!(
        Format::from_request(query.format.as_deref(), &body)?,
        Format::Xlsx
    ) {
        return Err(ApiError::BadRequest("unsupported_format"));
    }

    let tz_offset = query.tz_offset.unwrap_or(0);

    if !(-720..=840).contains(&tz_offset) {
        return Err(ApiError::BadRequest("invalid_tz_offset"));
    }

    let rows = Format::Csv.parse(&body)?;
    let (header, data) = split_header(&rows)?;

    if data.is_empty() {
        return Err(ApiError::BadRequest("empty_file"));
    }

    if data.len() > MAX_ROWS {
        return Err(ApiError::BadRequest("too_many_rows"));
    }

    let auth = guard.auth;
    let db = state.db()?;

    let products = {
        let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

        let rows: Vec<(Uuid, String, Option<String>, Decimal)> = sqlx::query_as(
            "select id, name, sku, sell_price from products where tenant_id = $1 and is_active",
        )
        .bind(auth.tenant_id)
        .fetch_all(&mut *tx)
        .await?;

        tx.commit().await?;

        product_index(rows)
    };

    let mut summary = ImportSummary {
        created: 0,
        duplicate: 0,
        rejected: 0,
        rows: Vec::new(),
    };

    let mut groups: Vec<Group> = Vec::new();
    let mut by_ref: HashMap<String, usize> = HashMap::new();

    for (number, row) in data {
        match parse_row(number, row, &header, &products, tz_offset) {
            Err(code) => {
                summary.rejected += 1;
                summary.rows.push(ImportRow {
                    row: number,
                    sale_ref: None,
                    status: "rejected",
                    sale_id: None,
                    code: Some(code),
                });
            }
            Ok(parsed) => match &parsed.sale_ref {
                Some(reference) => {
                    let index = *by_ref.entry(reference.clone()).or_insert_with(|| {
                        groups.push(Group {
                            sale_ref: Some(reference.clone()),
                            rows: Vec::new(),
                        });
                        groups.len() - 1
                    });

                    groups[index].rows.push(parsed);
                }
                None => groups.push(Group {
                    sale_ref: None,
                    rows: vec![parsed],
                }),
            },
        }
    }

    for group in groups {
        let sale_ref = group.sale_ref.clone();
        let client_ref = sale_ref.as_ref().map(|reference| {
            Uuid::new_v5(
                &SALE_NAMESPACE,
                format!("{}:{reference}", auth.tenant_id).as_bytes(),
            )
        });

        let body = SaleBody {
            sold_at: group.rows.iter().find_map(|row| row.sold_at),
            channel: group.rows.iter().find_map(|row| row.channel.clone()),
            note: group.rows.iter().find_map(|row| row.note.clone()),
            client_ref,
            items: group
                .rows
                .iter()
                .map(|row| SaleItemBody {
                    product_id: row.product_id,
                    qty: row.qty,
                    price: row.price,
                })
                .collect(),
        };

        let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

        match write_sale(&mut tx, auth.tenant_id, auth.user_id, &body).await {
            Ok((created, sale)) => {
                tx.commit().await?;

                let status = if created { "created" } else { "duplicate" };

                if created {
                    summary.created += 1;
                } else {
                    summary.duplicate += 1;
                }

                for row in group.rows {
                    summary.rows.push(ImportRow {
                        row: row.line,
                        sale_ref: sale_ref.clone(),
                        status,
                        sale_id: Some(sale.id),
                        code: None,
                    });
                }
            }
            Err(error) => {
                tx.rollback().await?;

                let code = error.code();

                for row in group.rows {
                    summary.rejected += 1;
                    summary.rows.push(ImportRow {
                        row: row.line,
                        sale_ref: sale_ref.clone(),
                        status: "rejected",
                        sale_id: None,
                        code: Some(code),
                    });
                }
            }
        }
    }

    summary.rows.sort_by_key(|row| row.row);

    Ok(Json(summary))
}

type ProductIndex = HashMap<String, (Uuid, Decimal)>;
type Header = HashMap<&'static str, usize>;
/// A spreadsheet row with the number the user sees in their editor.
type NumberedRow<'a> = (usize, &'a Vec<String>);

/// Names and SKUs both identify a product. A name wins when they collide,
/// because that is what the template asks the user to type.
fn product_index(rows: Vec<(Uuid, String, Option<String>, Decimal)>) -> ProductIndex {
    let mut index = HashMap::with_capacity(rows.len() * 2);

    for (id, name, sku, price) in rows {
        index.insert(name.trim().to_lowercase(), (id, price));

        if let Some(sku) = sku {
            let key = sku.trim().to_lowercase();

            if !key.is_empty() {
                index.entry(key).or_insert((id, price));
            }
        }
    }

    index
}

/// Column positions by name, so the template can be reordered without breaking
/// the import.
fn split_header(rows: &[Vec<String>]) -> Result<(Header, Vec<NumberedRow<'_>>), ApiError> {
    let first = rows.first().ok_or(ApiError::BadRequest("empty_file"))?;
    let mut header = HashMap::new();

    for (index, cell) in first.iter().enumerate() {
        if let Some(name) = column_name(cell) {
            header.entry(name).or_insert(index);
        }
    }

    if !header.contains_key("product") {
        return Err(ApiError::BadRequest("missing_header"));
    }

    let data = rows
        .iter()
        .enumerate()
        .skip(1)
        .map(|(index, row)| (index + 1, row))
        .filter(|(_, row)| row.iter().any(|cell| !cell.trim().is_empty()))
        .collect();

    Ok((header, data))
}

/// Header aliases in both languages, because the file comes from a person.
fn column_name(cell: &str) -> Option<&'static str> {
    match cell
        .trim()
        .trim_start_matches('\u{feff}')
        .to_lowercase()
        .as_str()
    {
        "sold_at" | "sold at" | "date" | "tanggal" | "waktu" => Some("sold_at"),
        "channel" | "kanal" => Some("channel"),
        "product" | "produk" | "nama produk" => Some("product"),
        "qty" | "quantity" | "jumlah" => Some("qty"),
        "price" | "harga" => Some("price"),
        "note" | "catatan" | "keterangan" => Some("note"),
        "sale_ref" | "sale ref" | "invoice" | "no nota" | "referensi" => Some("sale_ref"),
        _ => None,
    }
}

fn parse_row(
    line: usize,
    row: &[String],
    header: &Header,
    products: &ProductIndex,
    tz_offset: i32,
) -> Result<ParsedRow, &'static str> {
    let cell = |name: &str| -> &str {
        header
            .get(name)
            .and_then(|index| row.get(*index))
            .map(|value| value.trim())
            .unwrap_or_default()
    };

    let product = cell("product");

    if product.is_empty() {
        return Err("product_required");
    }

    // The price default comes from the product at write time, inside the same
    // transaction that locks it.
    let (product_id, _) = *products
        .get(&product.to_lowercase())
        .ok_or("unknown_product")?;

    let qty = parse_decimal(cell("qty")).ok_or("invalid_qty")?;

    if qty <= Decimal::ZERO {
        return Err("invalid_qty");
    }

    let price = match cell("price") {
        "" => None,
        value => {
            let price = parse_decimal(value).ok_or("invalid_price")?;

            if price < Decimal::ZERO {
                return Err("invalid_price");
            }

            Some(price)
        }
    };

    let sold_at = match cell("sold_at") {
        "" => None,
        value => Some(parse_sold_at(value, tz_offset).ok_or("invalid_sold_at")?),
    };

    let channel = match cell("channel") {
        "" => None,
        value => {
            let value = value.to_lowercase();

            if !CHANNELS.contains(&value.as_str()) {
                return Err("invalid_channel");
            }

            Some(value)
        }
    };

    let note = match cell("note") {
        "" => None,
        value => {
            if value.chars().count() > 500 {
                return Err("note_too_long");
            }

            Some(value.to_string())
        }
    };

    let sale_ref = match cell("sale_ref") {
        "" => None,
        value => Some(value.to_string()),
    };

    Ok(ParsedRow {
        line,
        sale_ref,
        sold_at,
        channel,
        note,
        product_id,
        qty,
        price,
    })
}

/// Accepts the Indonesian decimal comma, since that is what Excel writes here.
fn parse_decimal(value: &str) -> Option<Decimal> {
    let normalized = value.trim().replace(' ', "").replace(',', ".");
    Decimal::from_str(&normalized).ok()
}

/// An explicit offset wins. Without one the value is read in the shop's
/// timezone, which the client passes as `tz_offset`.
fn parse_sold_at(value: &str, tz_offset: i32) -> Option<DateTime<Utc>> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(value) {
        return Some(parsed.with_timezone(&Utc));
    }

    let offset = FixedOffset::east_opt(tz_offset * 60)?;

    for format in [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M",
    ] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(value, format) {
            return offset
                .from_local_datetime(&naive)
                .single()
                .map(|moment| moment.with_timezone(&Utc));
        }
    }

    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()?;

    offset
        .from_local_datetime(&date.and_hms_opt(0, 0, 0)?)
        .single()
        .map(|moment| moment.with_timezone(&Utc))
}

pub fn routes() -> Router<AppState> {
    Router::new().route(
        "/sales/import",
        post(import_sales).layer(DefaultBodyLimit::max(5 * 1024 * 1024)),
    )
}
