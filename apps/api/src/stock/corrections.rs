//! Stock corrections: waste, expiry write-off, and count adjustments.
//!
//! Every correction is a signed movement, so on-hand never changes without a
//! trail. The batch row is locked while it is corrected, so two people cannot
//! take the same remaining quantity.

use axum::{extract::State, http::StatusCode, Json};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::queries::MovementView;
use crate::auth::guard::{CanAdjustStock, Require};
use crate::{error::ApiError, AppState};

/// The movement types a person may record by hand. Purchases come from
/// stock-in and production use comes from production orders, so neither is
/// accepted here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorrectionType {
    Waste,
    Expired,
    Adjustment,
}

impl CorrectionType {
    fn as_str(self) -> &'static str {
        match self {
            CorrectionType::Waste => "waste",
            CorrectionType::Expired => "expired",
            CorrectionType::Adjustment => "adjustment",
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CorrectionBody {
    pub batch_id: Uuid,
    #[serde(rename = "type")]
    pub correction_type: CorrectionType,
    /// Signed like every movement: outbound is negative, inbound positive.
    /// Waste and expiry must be negative; an adjustment may go either way.
    pub qty: Decimal,
    pub note: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CorrectionView {
    #[serde(flatten)]
    pub movement: MovementView,
    pub batch_qty_remaining: Decimal,
}

fn validate(kind: CorrectionType, qty: Decimal) -> Result<(), ApiError> {
    match kind {
        CorrectionType::Waste | CorrectionType::Expired if qty >= Decimal::ZERO => {
            Err(ApiError::BadRequest("invalid_quantity"))
        }
        CorrectionType::Adjustment if qty.is_zero() => {
            Err(ApiError::BadRequest("invalid_quantity"))
        }
        _ => Ok(()),
    }
}

pub async fn correct(
    State(state): State<AppState>,
    guard: Require<CanAdjustStock>,
    Json(body): Json<CorrectionBody>,
) -> Result<(StatusCode, Json<CorrectionView>), ApiError> {
    let auth = guard.auth;
    validate(body.correction_type, body.qty)?;

    let note = body
        .note
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    // Lock the batch: the check below is only meaningful while no one else can
    // change the same remaining quantity.
    let batch: Option<(Uuid, Decimal, Decimal, Decimal)> = sqlx::query_as(
        "select material_id, qty_received, qty_remaining, unit_cost
         from material_batches
         where id = $1 and tenant_id = $2
         for update",
    )
    .bind(body.batch_id)
    .bind(auth.tenant_id)
    .fetch_optional(&mut *tx)
    .await?;

    let (material_id, qty_received, qty_remaining, unit_cost) =
        batch.ok_or(ApiError::NotFound("batch_not_found"))?;

    let new_remaining = qty_remaining + body.qty;

    if new_remaining.is_sign_negative() {
        return Err(ApiError::BadRequest("insufficient_batch_quantity"));
    }

    if new_remaining > qty_received {
        return Err(ApiError::BadRequest("quantity_exceeds_received"));
    }

    let (movement_id, created_at): (Uuid, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "insert into stock_movements
             (tenant_id, batch_id, type, qty, unit_cost, note, created_by)
         values ($1, $2, $3, $4, $5, $6, $7)
         returning id, created_at",
    )
    .bind(auth.tenant_id)
    .bind(body.batch_id)
    .bind(body.correction_type.as_str())
    .bind(body.qty)
    .bind(unit_cost)
    .bind(note)
    .bind(auth.user_id)
    .fetch_one(&mut *tx)
    .await?;

    let (batch_qty_remaining,): (Decimal,) = sqlx::query_as(
        "update material_batches set qty_remaining = $3
         where id = $1 and tenant_id = $2
         returning qty_remaining",
    )
    .bind(body.batch_id)
    .bind(auth.tenant_id)
    .bind(new_remaining)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok((
        StatusCode::CREATED,
        Json(CorrectionView {
            movement: MovementView {
                id: movement_id,
                batch_id: body.batch_id,
                material_id,
                movement_type: body.correction_type.as_str().to_string(),
                qty: body.qty,
                unit_cost: Some(unit_cost),
                note: note.map(str::to_string),
                created_by: Some(auth.user_id),
                created_at,
            },
            batch_qty_remaining,
        }),
    ))
}
