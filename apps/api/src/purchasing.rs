//! Purchase recommendations.
//!
//! Usage comes from the movement ledger over a fixed window, so every number on
//! the page can be explained: this much was used per day, the supplier takes
//! this long, and the minimum stock keeps a reserve. The arithmetic lives in
//! `takar_core::reorder`; this module only gathers the inputs.

use std::collections::HashMap;

use axum::{extract::State, routing::get, Json, Router};
use rust_decimal::Decimal;
use serde::Serialize;
use uuid::Uuid;

use takar_core::reorder::{average_daily_usage, reorder_plan, ReorderInput};

use crate::auth::guard::{CanReadMaterials, Require};
use crate::{error::ApiError, AppState};

/// Usage is averaged over the last 30 days. Long enough to smooth a weekend,
/// short enough to follow a menu change.
const WINDOW_DAYS: i32 = 30;

#[derive(Debug, Serialize)]
pub struct RecommendationList {
    pub window_days: i32,
    pub items: Vec<RecommendationView>,
}

#[derive(Debug, Serialize)]
pub struct RecommendationView {
    pub material_id: Uuid,
    pub material_name: String,
    pub unit_code: String,
    pub on_hand: Decimal,
    pub min_stock: Decimal,
    pub average_daily_usage: Decimal,
    pub lead_time_days: i32,
    pub supplier_name: Option<String>,
    pub shelf_life_days: Option<i32>,
    pub target_qty: Decimal,
    pub recommended_qty: Decimal,
    pub capped_by_shelf_life: bool,
    /// True when the material had no outbound movement in the window, so the
    /// quantity only closes the gap to `min_stock`.
    pub insufficient_history: bool,
    pub last_cost: Decimal,
    pub estimated_cost: Decimal,
}

type MaterialRow = (
    Uuid,
    String,
    String,
    Decimal,
    Option<i32>,
    Decimal,
    i32,
    Option<String>,
);

pub async fn recommendations(
    State(state): State<AppState>,
    guard: Require<CanReadMaterials>,
) -> Result<Json<RecommendationList>, ApiError> {
    let auth = guard.auth;
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let materials: Vec<MaterialRow> = sqlx::query_as(
        "select m.id, m.name, u.code, m.min_stock, m.shelf_life_days, m.last_cost,
                coalesce(s.lead_time_days, 0), s.name
         from materials m
         join units u on u.id = m.base_unit_id
         left join suppliers s on s.id = m.default_supplier_id and s.is_active
         where m.tenant_id = $1 and m.is_active
         order by m.name",
    )
    .bind(auth.tenant_id)
    .fetch_all(&mut *tx)
    .await?;

    let on_hand: HashMap<Uuid, Decimal> = sqlx::query_as::<_, (Uuid, Decimal)>(
        "select material_id, sum(qty_remaining)
         from material_batches
         where tenant_id = $1 and qty_remaining > 0
         group by material_id",
    )
    .bind(auth.tenant_id)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .collect();

    // Only real outflow counts. Purchases add stock and adjustments are
    // corrections, so neither says anything about consumption.
    let usage: HashMap<Uuid, (Decimal, i64)> = sqlx::query_as::<_, (Uuid, Decimal, i64)>(
        "select b.material_id, sum(-m.qty), count(*)
         from stock_movements m
         join material_batches b on b.id = m.batch_id
         where m.tenant_id = $1
           and m.type in ('production_use', 'waste', 'expired')
           and m.created_at >= now() - make_interval(days => $2)
         group by b.material_id",
    )
    .bind(auth.tenant_id)
    .bind(WINDOW_DAYS)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .map(|(material_id, used, movements)| (material_id, (used, movements)))
    .collect();

    tx.commit().await?;

    let mut items = Vec::new();

    for (id, name, unit_code, min_stock, shelf_life_days, last_cost, lead_time_days, supplier) in
        materials
    {
        let (used, movements) = usage.get(&id).copied().unwrap_or((Decimal::ZERO, 0));
        let stock = on_hand.get(&id).copied().unwrap_or(Decimal::ZERO);

        let average = average_daily_usage(used, WINDOW_DAYS as u32)
            .map_err(|error| ApiError::Internal(anyhow::Error::new(error)))?;

        let plan = reorder_plan(ReorderInput {
            on_hand: stock,
            min_stock,
            average_daily_usage: average,
            lead_time_days,
            shelf_life_days,
        })
        .map_err(|error| ApiError::Internal(anyhow::Error::new(error)))?;

        if plan.recommended_qty <= Decimal::ZERO {
            continue;
        }

        items.push(RecommendationView {
            material_id: id,
            material_name: name,
            unit_code,
            on_hand: stock,
            min_stock,
            average_daily_usage: average,
            lead_time_days,
            supplier_name: supplier,
            shelf_life_days,
            target_qty: plan.target_qty,
            recommended_qty: plan.recommended_qty,
            capped_by_shelf_life: plan.capped_by_shelf_life,
            insufficient_history: movements == 0,
            last_cost,
            estimated_cost: (plan.recommended_qty * last_cost).round_dp(2),
        });
    }

    // Most urgent first: the largest order, then the alphabetical tie-break.
    items.sort_by(|left, right| {
        right
            .recommended_qty
            .cmp(&left.recommended_qty)
            .then_with(|| left.material_name.cmp(&right.material_name))
    });

    Ok(Json(RecommendationList {
        window_days: WINDOW_DAYS,
        items,
    }))
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/purchasing/recommendations", get(recommendations))
}
