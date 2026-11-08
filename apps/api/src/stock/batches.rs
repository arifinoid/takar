//! Batch stock-in.
//!
//! Every batch is created together with its opening purchase movement, so
//! on-hand stock always has a trail. Corrections and consumption arrive in
//! later tasks and use the same movement table.

use axum::{extract::State, http::StatusCode, Json};
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::guard::{CanWriteBatches, Require};
use crate::{error::ApiError, AppState};

#[derive(Debug, Deserialize)]
pub struct StockInBody {
    pub material_id: Uuid,
    pub supplier_id: Option<Uuid>,
    pub received_at: Option<NaiveDate>,
    pub expires_at: Option<NaiveDate>,
    pub qty_received: Decimal,
    pub unit_cost: Decimal,
    pub note: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct BatchView {
    pub id: Uuid,
    pub material_id: Uuid,
    pub supplier_id: Option<Uuid>,
    pub received_at: NaiveDate,
    pub expires_at: Option<NaiveDate>,
    pub qty_received: Decimal,
    pub qty_remaining: Decimal,
    pub unit_cost: Decimal,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
}

pub async fn stock_in(
    State(state): State<AppState>,
    guard: Require<CanWriteBatches>,
    Json(body): Json<StockInBody>,
) -> Result<(StatusCode, Json<BatchView>), ApiError> {
    let auth = guard.auth;

    if body.qty_received <= Decimal::ZERO {
        return Err(ApiError::BadRequest("invalid_quantity"));
    }

    if body.unit_cost.is_sign_negative() {
        return Err(ApiError::BadRequest("invalid_cost"));
    }

    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let material: Option<(Uuid,)> =
        sqlx::query_as("select id from materials where id = $1 and tenant_id = $2 and is_active")
            .bind(body.material_id)
            .bind(auth.tenant_id)
            .fetch_optional(&mut *tx)
            .await?;

    if material.is_none() {
        return Err(ApiError::NotFound("material_not_found"));
    }

    if let Some(supplier_id) = body.supplier_id {
        let supplier: Option<(Uuid,)> = sqlx::query_as(
            "select id from suppliers where id = $1 and tenant_id = $2 and is_active",
        )
        .bind(supplier_id)
        .bind(auth.tenant_id)
        .fetch_optional(&mut *tx)
        .await?;

        if supplier.is_none() {
            return Err(ApiError::BadRequest("invalid_supplier"));
        }
    }

    let received_at = body.received_at.unwrap_or_else(|| Utc::now().date_naive());
    let note = body
        .note
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let (id, created_at): (Uuid, DateTime<Utc>) = sqlx::query_as(
        "insert into material_batches
             (tenant_id, material_id, supplier_id, received_at, expires_at,
              qty_received, qty_remaining, unit_cost, note)
         values ($1, $2, $3, $4, $5, $6, $6, $7, $8)
         returning id, created_at",
    )
    .bind(auth.tenant_id)
    .bind(body.material_id)
    .bind(body.supplier_id)
    .bind(received_at)
    .bind(body.expires_at)
    .bind(body.qty_received)
    .bind(body.unit_cost)
    .bind(note)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        "insert into stock_movements
             (tenant_id, batch_id, type, qty, unit_cost, note, created_by)
         values ($1, $2, 'purchase', $3, $4, $5, $6)",
    )
    .bind(auth.tenant_id)
    .bind(id)
    .bind(body.qty_received)
    .bind(body.unit_cost)
    .bind(note)
    .bind(auth.user_id)
    .execute(&mut *tx)
    .await?;

    // Read-only cache of the latest purchase cost; batches stay authoritative.
    sqlx::query("update materials set last_cost = $1, updated_at = now() where id = $2")
        .bind(body.unit_cost)
        .bind(body.material_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok((
        StatusCode::CREATED,
        Json(BatchView {
            id,
            material_id: body.material_id,
            supplier_id: body.supplier_id,
            received_at,
            expires_at: body.expires_at,
            qty_received: body.qty_received,
            qty_remaining: body.qty_received,
            unit_cost: body.unit_cost,
            note: note.map(str::to_string),
            created_at,
        }),
    ))
}
