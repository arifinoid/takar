//! Sales.
//!
//! A sale is the only writer that removes finished goods. Production adds to
//! `products.stock_qty`; this module subtracts from it. The whole document is
//! written in one transaction: if any line is short, nothing is written, so
//! stock never drifts from the recorded sales.
//!
//! The price on a line is snapshotted at sale time, exactly like a batch cost
//! is snapshotted at production time. A later `sell_price` change must not
//! rewrite yesterday's revenue.
//!
//! `client_ref` makes a replayed import idempotent: sending the same document
//! twice returns the first sale with `200` instead of decrementing stock again.

use std::collections::{HashMap, HashSet};

use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::auth::guard::{CanReadSales, CanWriteSales, Require};
use crate::{error::ApiError, AppState};

/// Money is stored as `numeric(18,2)`.
const MONEY_SCALE: u32 = 2;
/// A note is a human remark, not a document.
const NOTE_MAX: usize = 500;
const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;

/// Channels the app can record. The database keeps the value as text, so
/// adding one here needs no migration.
pub(crate) const CHANNELS: [&str; 4] = ["toko", "reseller", "online", "lainnya"];

#[derive(Debug, Deserialize)]
pub struct SaleBody {
    /// Defaults to now. A backdated sale is allowed; the stock still moves
    /// today because finished goods are not tracked by date.
    pub sold_at: Option<DateTime<Utc>>,
    pub channel: Option<String>,
    pub note: Option<String>,
    pub client_ref: Option<Uuid>,
    pub items: Vec<SaleItemBody>,
}

#[derive(Debug, Deserialize)]
pub struct SaleItemBody {
    pub product_id: Uuid,
    pub qty: Decimal,
    /// Defaults to the product's current `sell_price`.
    pub price: Option<Decimal>,
}

#[derive(Debug, Serialize)]
pub struct SaleView {
    pub id: Uuid,
    pub sold_at: DateTime<Utc>,
    pub channel: String,
    pub total: Decimal,
    pub note: Option<String>,
    pub client_ref: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub items: Vec<SaleItemView>,
}

#[derive(Debug, Serialize)]
pub struct SaleItemView {
    pub product_id: Uuid,
    pub product_name: String,
    pub qty: Decimal,
    pub price: Decimal,
    pub subtotal: Decimal,
}

#[derive(Debug, Serialize)]
pub struct SaleList {
    pub items: Vec<SaleView>,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub limit: Option<i64>,
}

#[derive(Debug)]
struct Line {
    product_id: Uuid,
    qty: Decimal,
    price: Decimal,
}

type SaleRow = (
    Uuid,
    DateTime<Utc>,
    String,
    Decimal,
    Option<String>,
    Option<Uuid>,
    DateTime<Utc>,
);

type ItemRow = (Uuid, Uuid, String, Decimal, Decimal);

const SALE_COLUMNS: &str =
    "select id, sold_at, channel, total, note, client_ref, created_at from sales";

pub async fn create_sale(
    State(state): State<AppState>,
    guard: Require<CanWriteSales>,
    Json(body): Json<SaleBody>,
) -> Result<(StatusCode, Json<SaleView>), ApiError> {
    let auth = guard.auth;
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let (created, sale) = write_sale(&mut tx, auth.tenant_id, auth.user_id, &body).await?;

    tx.commit().await?;

    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };

    Ok((status, Json(sale)))
}

/// Writes one sale inside an existing transaction.
///
/// Shared with the importer so a typed sale and an imported one obey exactly
/// the same rules: price snapshot, stock check, and `client_ref` duplicate
/// detection. Returns whether a sale was created; `false` means the reference
/// was already used and the stored sale is returned unchanged.
pub(crate) async fn write_sale(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    user_id: Uuid,
    body: &SaleBody,
) -> Result<(bool, SaleView), ApiError> {
    let channel = validate_channel(body.channel.as_deref())?;
    let note = validate_note(body.note.as_deref())?;

    if body.items.is_empty() {
        return Err(ApiError::BadRequest("items_required"));
    }

    // The primary key is (sale_id, product_id), so two lines for the same
    // product would silently collapse. Reject instead of guessing which price
    // the user meant.
    let mut seen = HashSet::with_capacity(body.items.len());

    for item in &body.items {
        // Zero is neither positive nor negative in rust_decimal, so compare
        // against zero instead of testing the sign.
        if item.qty <= Decimal::ZERO {
            return Err(ApiError::BadRequest("invalid_qty"));
        }

        if item.price.is_some_and(|price| price < Decimal::ZERO) {
            return Err(ApiError::BadRequest("invalid_price"));
        }

        if !seen.insert(item.product_id) {
            return Err(ApiError::BadRequest("duplicate_product"));
        }
    }

    // A replayed document returns the first sale. Checked inside the
    // transaction so two concurrent replays cannot both create one.
    if let Some(client_ref) = body.client_ref {
        let existing: Option<Uuid> =
            sqlx::query_scalar("select id from sales where tenant_id = $1 and client_ref = $2")
                .bind(tenant_id)
                .bind(client_ref)
                .fetch_optional(&mut **tx)
                .await?;

        if let Some(id) = existing {
            return Ok((false, fetch_sale(tx, tenant_id, id).await?));
        }
    }

    // Lock the rows so a concurrent sale cannot pass the same stock check.
    let ids: Vec<Uuid> = body.items.iter().map(|item| item.product_id).collect();

    let products: Vec<(Uuid, String, Decimal, Decimal)> = sqlx::query_as(
        "select id, name, sell_price, stock_qty
         from products
         where tenant_id = $1 and id = any($2) and is_active
         for update",
    )
    .bind(tenant_id)
    .bind(&ids)
    .fetch_all(&mut **tx)
    .await?;

    let by_id: HashMap<Uuid, (String, Decimal, Decimal)> = products
        .into_iter()
        .map(|(id, name, price, stock)| (id, (name, price, stock)))
        .collect();

    let mut lines = Vec::with_capacity(body.items.len());
    let mut shortages = Vec::new();

    for item in &body.items {
        let Some((name, sell_price, available)) = by_id.get(&item.product_id) else {
            // Either another tenant's product or a deleted one. Both look the
            // same from here on purpose.
            return Err(ApiError::NotFound("product_not_found"));
        };

        if *available < item.qty {
            // Three decimals, the ledger scale, so the client can show the
            // numbers without reformatting them.
            shortages.push(serde_json::json!({
                "product_id": item.product_id,
                "product_name": name,
                "available": format!("{available:.3}"),
                "requested": format!("{:.3}", item.qty),
            }));
        }

        lines.push(Line {
            product_id: item.product_id,
            qty: item.qty,
            price: item.price.unwrap_or(*sell_price),
        });
    }

    // Report every gap at once, the same shape production uses for materials.
    if !shortages.is_empty() {
        return Err(ApiError::ConflictDetails(
            "insufficient_stock",
            serde_json::json!({ "shortages": shortages }),
        ));
    }

    let total: Decimal = lines
        .iter()
        .map(|line| line.qty * line.price)
        .sum::<Decimal>()
        .round_dp_with_strategy(MONEY_SCALE, RoundingStrategy::MidpointAwayFromZero);

    let sale_id: Uuid = sqlx::query_scalar(
        "insert into sales (tenant_id, sold_at, channel, total, note, client_ref, created_by)
         values ($1, coalesce($2, now()), $3, $4, $5, $6, $7)
         returning id",
    )
    .bind(tenant_id)
    .bind(body.sold_at)
    .bind(channel)
    .bind(total)
    .bind(note)
    .bind(body.client_ref)
    .bind(user_id)
    .fetch_one(&mut **tx)
    .await?;

    for line in &lines {
        sqlx::query(
            "insert into sale_items (sale_id, tenant_id, product_id, qty, price)
             values ($1, $2, $3, $4, $5)",
        )
        .bind(sale_id)
        .bind(tenant_id)
        .bind(line.product_id)
        .bind(line.qty)
        .bind(line.price)
        .execute(&mut **tx)
        .await?;

        sqlx::query(
            "update products
             set stock_qty = stock_qty - $3, updated_at = now()
             where id = $1 and tenant_id = $2",
        )
        .bind(line.product_id)
        .bind(tenant_id)
        .bind(line.qty)
        .execute(&mut **tx)
        .await?;
    }

    let sale = fetch_sale(tx, tenant_id, sale_id).await?;

    Ok((true, sale))
}

pub async fn list_sales(
    State(state): State<AppState>,
    guard: Require<CanReadSales>,
    Query(query): Query<ListQuery>,
) -> Result<Json<SaleList>, ApiError> {
    let auth = guard.auth;
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let rows: Vec<SaleRow> = sqlx::query_as(&format!(
        "{SALE_COLUMNS}
         where tenant_id = $1
         order by sold_at desc, created_at desc
         limit $2"
    ))
    .bind(auth.tenant_id)
    .bind(limit)
    .fetch_all(&mut *tx)
    .await?;

    let ids: Vec<Uuid> = rows.iter().map(|row| row.0).collect();

    let item_rows: Vec<ItemRow> = sqlx::query_as(
        "select si.sale_id, si.product_id, p.name, si.qty, si.price
         from sale_items si
         join products p on p.id = si.product_id
         where si.tenant_id = $1 and si.sale_id = any($2)
         order by p.name",
    )
    .bind(auth.tenant_id)
    .bind(&ids)
    .fetch_all(&mut *tx)
    .await?;

    let mut items: HashMap<Uuid, Vec<SaleItemView>> = HashMap::new();

    for (sale_id, product_id, product_name, qty, price) in item_rows {
        items.entry(sale_id).or_default().push(SaleItemView {
            product_id,
            product_name,
            qty,
            price,
            subtotal: (qty * price)
                .round_dp_with_strategy(MONEY_SCALE, RoundingStrategy::MidpointAwayFromZero),
        });
    }

    let sales = rows
        .into_iter()
        .map(|row| {
            let id = row.0;
            view(row, items.remove(&id).unwrap_or_default())
        })
        .collect();

    tx.commit().await?;

    Ok(Json(SaleList { items: sales }))
}

async fn fetch_sale(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    id: Uuid,
) -> Result<SaleView, ApiError> {
    let row: SaleRow = sqlx::query_as(&format!("{SALE_COLUMNS} where tenant_id = $1 and id = $2"))
        .bind(tenant_id)
        .bind(id)
        .fetch_one(&mut **tx)
        .await?;

    let items: Vec<ItemRow> = sqlx::query_as(
        "select si.sale_id, si.product_id, p.name, si.qty, si.price
         from sale_items si
         join products p on p.id = si.product_id
         where si.tenant_id = $1 and si.sale_id = $2
         order by p.name",
    )
    .bind(tenant_id)
    .bind(id)
    .fetch_all(&mut **tx)
    .await?;

    let items = items
        .into_iter()
        .map(|(_, product_id, product_name, qty, price)| SaleItemView {
            product_id,
            product_name,
            qty,
            price,
            subtotal: (qty * price)
                .round_dp_with_strategy(MONEY_SCALE, RoundingStrategy::MidpointAwayFromZero),
        })
        .collect();

    Ok(view(row, items))
}

fn view(row: SaleRow, items: Vec<SaleItemView>) -> SaleView {
    let (id, sold_at, channel, total, note, client_ref, created_at) = row;

    SaleView {
        id,
        sold_at,
        channel,
        total,
        note,
        client_ref,
        created_at,
        items,
    }
}

fn validate_channel(channel: Option<&str>) -> Result<&str, ApiError> {
    match channel {
        None => Ok(CHANNELS[0]),
        Some(value) if CHANNELS.contains(&value) => Ok(value),
        Some(_) => Err(ApiError::BadRequest("invalid_channel")),
    }
}

fn validate_note(note: Option<&str>) -> Result<Option<&str>, ApiError> {
    match note.map(str::trim) {
        None => Ok(None),
        Some("") => Ok(None),
        Some(value) if value.chars().count() > NOTE_MAX => {
            Err(ApiError::BadRequest("note_too_long"))
        }
        Some(value) => Ok(Some(value)),
    }
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/sales", get(list_sales).post(create_sale))
}
