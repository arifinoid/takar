//! Production orders.
//!
//! One order consumes material batches in FEFO order and adds finished goods.
//! Everything runs in a single transaction: if any material is short, the
//! request is rejected before the first write, so stock never moves halfway
//! through a run. The order points at the recipe version it used, and each
//! consumption snapshots the batch cost at the moment of use.

use std::collections::HashMap;

use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use takar_core::{Allocation, StockBatch};

use crate::auth::guard::{CanWriteProduction, Require};
use crate::{error::ApiError, AppState};

/// Batches store three decimals. Requirements round up to the same scale so
/// the batch, the movement, and the consumption always agree.
const LEDGER_SCALE: u32 = 3;

#[derive(Debug, Deserialize)]
pub struct ProductionBody {
    pub product_id: Uuid,
    /// How much finished product the run is expected to make, in product units.
    pub qty: Decimal,
    pub produced_on: Option<NaiveDate>,
}

#[derive(Debug, Serialize)]
pub struct ProductionOrderView {
    pub id: Uuid,
    pub product_id: Uuid,
    pub product_name: String,
    pub recipe_id: Uuid,
    pub recipe_version: i32,
    pub planned_qty: Decimal,
    pub actual_qty: Decimal,
    pub produced_on: NaiveDate,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub consumptions: Vec<ConsumptionView>,
}

#[derive(Debug, Serialize)]
pub struct ConsumptionView {
    pub batch_id: Uuid,
    pub material_id: Uuid,
    pub material_name: String,
    pub qty: Decimal,
    pub unit_cost: Decimal,
}

/// One material the run cannot cover, with the gap the user has to close.
#[derive(Debug, Serialize)]
pub struct ShortageView {
    pub material_id: Uuid,
    pub material_name: String,
    pub required: Decimal,
    pub available: Decimal,
    pub shortfall: Decimal,
}

type ProductRow = (Uuid, String, Option<Uuid>, Option<i32>, Option<Decimal>);
type ItemRow = (Uuid, String, Decimal, Decimal);
type BatchRow = (Uuid, NaiveDate, Option<NaiveDate>, Decimal, Decimal);
type Plan = (Uuid, String, Allocation<Uuid>, HashMap<Uuid, Decimal>);

pub async fn create_order(
    State(state): State<AppState>,
    guard: Require<CanWriteProduction>,
    Json(body): Json<ProductionBody>,
) -> Result<(StatusCode, Json<ProductionOrderView>), ApiError> {
    let auth = guard.auth;

    // Round once, up front: the plan, the ledger, and the response then all
    // speak the same three-decimal quantity.
    let mut production_qty = body.qty;
    production_qty.rescale(LEDGER_SCALE);

    if production_qty <= Decimal::ZERO {
        return Err(ApiError::BadRequest("invalid_quantity"));
    }

    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    // Lock the product first so two runs for the same product serialize, and
    // read the recipe version that is current right now.
    let product: Option<ProductRow> = sqlx::query_as(
        "select p.id, p.name, p.current_recipe_id, r.version, r.yield_qty
         from products p
         left join recipes r on r.id = p.current_recipe_id
         where p.id = $1 and p.tenant_id = $2 and p.is_active
         for update of p",
    )
    .bind(body.product_id)
    .bind(auth.tenant_id)
    .fetch_optional(&mut *tx)
    .await?;

    let (_, product_name, current_recipe_id, current_version, current_yield) =
        product.ok_or(ApiError::NotFound("product_not_found"))?;

    let (recipe_id, recipe_version, yield_qty) =
        match (current_recipe_id, current_version, current_yield) {
            (Some(recipe_id), Some(version), Some(yield_qty)) => (recipe_id, version, yield_qty),
            _ => return Err(ApiError::Conflict("recipe_required")),
        };

    // Material order is part of the lock order: every run locks batches in
    // material id order, so two runs sharing materials cannot deadlock.
    let items: Vec<ItemRow> = sqlx::query_as(
        "select ri.material_id, m.name, ri.qty, ri.waste_pct
         from recipe_items ri
         join materials m on m.id = ri.material_id
         where ri.recipe_id = $1
         order by ri.material_id",
    )
    .bind(recipe_id)
    .fetch_all(&mut *tx)
    .await?;

    // The API refuses to create an itemless recipe; this guards against rows
    // that reached the database another way.
    if items.is_empty() {
        return Err(ApiError::Conflict("recipe_items_required"));
    }

    let mut plans: Vec<Plan> = Vec::with_capacity(items.len());
    let mut shortages: Vec<ShortageView> = Vec::new();

    for (material_id, material_name, item_qty, waste_pct) in items {
        let required =
            takar_core::material_requirement(item_qty, waste_pct, production_qty, yield_qty)
                .map_err(|error| ApiError::Internal(anyhow::Error::new(error)))?
                .round_dp_with_strategy(LEDGER_SCALE, RoundingStrategy::ToPositiveInfinity);

        let rows: Vec<BatchRow> = sqlx::query_as(
            "select id, received_at, expires_at, qty_remaining, unit_cost
             from material_batches
             where tenant_id = $1 and material_id = $2 and qty_remaining > 0
             order by expires_at asc nulls last, received_at asc
             for update",
        )
        .bind(auth.tenant_id)
        .bind(material_id)
        .fetch_all(&mut *tx)
        .await?;

        let mut costs: HashMap<Uuid, Decimal> = HashMap::with_capacity(rows.len());
        let mut batches: Vec<StockBatch<Uuid>> = Vec::with_capacity(rows.len());

        for (id, received_at, expires_at, qty_remaining, unit_cost) in rows {
            costs.insert(id, unit_cost);
            batches.push(StockBatch {
                id,
                received_at,
                expires_at,
                qty_remaining,
            });
        }

        let allocation = takar_core::allocate(&batches, required)
            .map_err(|error| ApiError::Internal(anyhow::Error::new(error)))?;

        if !allocation.is_fully_allocated() {
            shortages.push(ShortageView {
                material_id,
                material_name: material_name.clone(),
                required,
                available: allocation.allocated,
                shortfall: allocation.shortfall,
            });
        }

        plans.push((material_id, material_name, allocation, costs));
    }

    // D3: block the whole run and show every gap at once. Nothing has been
    // written yet, and dropping the transaction rolls back cleanly.
    if !shortages.is_empty() {
        return Err(ApiError::ConflictDetails(
            "insufficient_stock",
            serde_json::json!({ "shortages": shortages }),
        ));
    }

    let produced_on = body.produced_on.unwrap_or_else(|| Utc::now().date_naive());

    let (order_id, created_at): (Uuid, DateTime<Utc>) = sqlx::query_as(
        "insert into production_orders
             (tenant_id, product_id, recipe_id, planned_qty, actual_qty, produced_on, created_by)
         values ($1, $2, $3, $4, $4, $5, $6)
         returning id, created_at",
    )
    .bind(auth.tenant_id)
    .bind(body.product_id)
    .bind(recipe_id)
    .bind(production_qty)
    .bind(produced_on)
    .bind(auth.user_id)
    .fetch_one(&mut *tx)
    .await?;

    let mut consumptions: Vec<ConsumptionView> = Vec::new();

    for (material_id, material_name, allocation, costs) in plans {
        for line in allocation.lines {
            let unit_cost = costs[&line.id];

            sqlx::query(
                "update material_batches
                 set qty_remaining = qty_remaining - $3
                 where id = $1 and tenant_id = $2",
            )
            .bind(line.id)
            .bind(auth.tenant_id)
            .bind(line.qty)
            .execute(&mut *tx)
            .await?;

            sqlx::query(
                "insert into stock_movements
                     (tenant_id, batch_id, type, qty, unit_cost, created_by)
                 values ($1, $2, 'production_use', $3, $4, $5)",
            )
            .bind(auth.tenant_id)
            .bind(line.id)
            .bind(-line.qty)
            .bind(unit_cost)
            .bind(auth.user_id)
            .execute(&mut *tx)
            .await?;

            sqlx::query(
                "insert into production_consumptions
                     (production_order_id, batch_id, qty, unit_cost)
                 values ($1, $2, $3, $4)",
            )
            .bind(order_id)
            .bind(line.id)
            .bind(line.qty)
            .bind(unit_cost)
            .execute(&mut *tx)
            .await?;

            consumptions.push(ConsumptionView {
                batch_id: line.id,
                material_id,
                material_name: material_name.clone(),
                qty: line.qty,
                unit_cost,
            });
        }
    }

    sqlx::query(
        "update products
         set stock_qty = stock_qty + $3, updated_at = now()
         where id = $1 and tenant_id = $2",
    )
    .bind(body.product_id)
    .bind(auth.tenant_id)
    .bind(production_qty)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok((
        StatusCode::CREATED,
        Json(ProductionOrderView {
            id: order_id,
            product_id: body.product_id,
            product_name,
            recipe_id,
            recipe_version,
            planned_qty: production_qty,
            actual_qty: production_qty,
            produced_on,
            status: "done".to_string(),
            created_at,
            consumptions,
        }),
    ))
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/production-orders", post(create_order))
}
