//! Batch stock-in.
//!
//! Every batch is created together with its opening purchase movement, so
//! on-hand stock always has a trail. A receipt may carry a client reference:
//! the Tauri shell generates one while offline, and replaying the same entry
//! returns the batch that already exists instead of creating a second one.

use axum::{extract::State, http::StatusCode, Json};
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};
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
    /// Set by an offline client so a replay never duplicates the batch.
    pub client_ref: Option<Uuid>,
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

/// One accepted receipt and whether this call is what created it.
pub(crate) struct StockInRecord {
    pub batch: BatchView,
    pub created: bool,
}

const BATCH_COLUMNS: &str = "id, material_id, supplier_id, received_at, expires_at,
     qty_received, qty_remaining, unit_cost, note, created_at";

async fn load_batch(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    id: Uuid,
) -> Result<BatchView, ApiError> {
    let batch: BatchView = sqlx::query_as(&format!(
        "select {BATCH_COLUMNS} from material_batches where id = $1 and tenant_id = $2"
    ))
    .bind(id)
    .bind(tenant_id)
    .fetch_one(&mut **tx)
    .await?;

    Ok(batch)
}

/// Validates and writes one receipt. Shared by the online route and the sync
/// endpoint so both apply exactly the same rules.
pub(crate) async fn insert_stock_in(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    user_id: Uuid,
    body: &StockInBody,
) -> Result<StockInRecord, ApiError> {
    if body.qty_received <= Decimal::ZERO {
        return Err(ApiError::BadRequest("invalid_quantity"));
    }

    if body.unit_cost.is_sign_negative() {
        return Err(ApiError::BadRequest("invalid_cost"));
    }

    let material: Option<(Uuid,)> =
        sqlx::query_as("select id from materials where id = $1 and tenant_id = $2 and is_active")
            .bind(body.material_id)
            .bind(tenant_id)
            .fetch_optional(&mut **tx)
            .await?;

    if material.is_none() {
        return Err(ApiError::NotFound("material_not_found"));
    }

    if let Some(supplier_id) = body.supplier_id {
        let supplier: Option<(Uuid,)> = sqlx::query_as(
            "select id from suppliers where id = $1 and tenant_id = $2 and is_active",
        )
        .bind(supplier_id)
        .bind(tenant_id)
        .fetch_optional(&mut **tx)
        .await?;

        if supplier.is_none() {
            return Err(ApiError::BadRequest("invalid_supplier"));
        }
    }

    // A replay is not an error: return the batch the first attempt created.
    if let Some(client_ref) = body.client_ref {
        let existing: Option<(Uuid,)> = sqlx::query_as(
            "select id from material_batches where tenant_id = $1 and client_ref = $2",
        )
        .bind(tenant_id)
        .bind(client_ref)
        .fetch_optional(&mut **tx)
        .await?;

        if let Some((id,)) = existing {
            return Ok(StockInRecord {
                batch: load_batch(tx, tenant_id, id).await?,
                created: false,
            });
        }
    }

    let received_at = body.received_at.unwrap_or_else(|| Utc::now().date_naive());
    let note = body
        .note
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    // The unique index on (tenant_id, client_ref) settles a race between two
    // replays: the loser reads the row the winner committed.
    let inserted: Option<(Uuid, DateTime<Utc>)> = sqlx::query_as(
        "insert into material_batches
             (tenant_id, material_id, supplier_id, received_at, expires_at,
              qty_received, qty_remaining, unit_cost, note, client_ref)
         values ($1, $2, $3, $4, $5, $6, $6, $7, $8, $9)
         on conflict (tenant_id, client_ref) do nothing
         returning id, created_at",
    )
    .bind(tenant_id)
    .bind(body.material_id)
    .bind(body.supplier_id)
    .bind(received_at)
    .bind(body.expires_at)
    .bind(body.qty_received)
    .bind(body.unit_cost)
    .bind(note)
    .bind(body.client_ref)
    .fetch_optional(&mut **tx)
    .await?;

    let (id, created_at) = match inserted {
        Some(row) => row,
        None => {
            let client_ref = body.client_ref.ok_or(ApiError::Internal(anyhow::anyhow!(
                "insert was skipped without a client reference"
            )))?;

            let (id,): (Uuid,) = sqlx::query_as(
                "select id from material_batches where tenant_id = $1 and client_ref = $2",
            )
            .bind(tenant_id)
            .bind(client_ref)
            .fetch_one(&mut **tx)
            .await?;

            return Ok(StockInRecord {
                batch: load_batch(tx, tenant_id, id).await?,
                created: false,
            });
        }
    };

    sqlx::query(
        "insert into stock_movements
             (tenant_id, batch_id, type, qty, unit_cost, note, created_by)
         values ($1, $2, 'purchase', $3, $4, $5, $6)",
    )
    .bind(tenant_id)
    .bind(id)
    .bind(body.qty_received)
    .bind(body.unit_cost)
    .bind(note)
    .bind(user_id)
    .execute(&mut **tx)
    .await?;

    // Read-only cache of the latest purchase cost; batches stay authoritative.
    sqlx::query("update materials set last_cost = $1, updated_at = now() where id = $2")
        .bind(body.unit_cost)
        .bind(body.material_id)
        .execute(&mut **tx)
        .await?;

    Ok(StockInRecord {
        batch: BatchView {
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
        },
        created: true,
    })
}

pub async fn stock_in(
    State(state): State<AppState>,
    guard: Require<CanWriteBatches>,
    Json(body): Json<StockInBody>,
) -> Result<(StatusCode, Json<BatchView>), ApiError> {
    let auth = guard.auth;
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let record = insert_stock_in(&mut tx, auth.tenant_id, auth.user_id, &body).await?;

    tx.commit().await?;

    let status = if record.created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };

    Ok((status, Json(record.batch)))
}
