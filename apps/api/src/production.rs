//! Production orders.
//!
//! One order consumes material batches in FEFO order and adds finished goods.
//! Everything runs in a single transaction: if any material is short, the
//! request is rejected before the first write, so stock never moves halfway
//! through a run. The order points at the recipe version it used, and each
//! consumption snapshots the batch cost at the moment of use.
//!
//! The preview endpoint runs the same plan without writing, so the UI can show
//! requirements, the exact batches FEFO would take, and the expected HPP before
//! the user confirms.

use std::collections::HashMap;

use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use takar_core::{Allocation, Consumption, ProductionCost, StockBatch};

use crate::auth::guard::{CanReadProduction, CanWriteProduction, Require};
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
    pub material_cost: Decimal,
    pub labor_cost: Decimal,
    pub overhead_cost: Decimal,
    /// HPP per finished unit (D7: four decimals).
    pub unit_cost: Decimal,
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

#[derive(Debug, Serialize)]
pub struct ProductionPreviewView {
    pub product_id: Uuid,
    pub product_name: String,
    pub sell_price: Decimal,
    pub stock_qty: Decimal,
    pub recipe_id: Uuid,
    pub recipe_version: i32,
    pub yield_qty: Decimal,
    /// Minutes of labor for one batch, and the tenant rate that prices it.
    pub labor_minutes: i32,
    pub labor_rate_per_hour: Decimal,
    pub qty: Decimal,
    /// False when any material is short; the UI must not allow the run.
    pub can_produce: bool,
    /// Cost fields are null when the run cannot be completed, because a
    /// partial allocation would understate the HPP.
    pub material_cost: Option<Decimal>,
    pub labor_cost: Option<Decimal>,
    pub overhead_cost: Option<Decimal>,
    pub unit_cost: Option<Decimal>,
    pub margin: Option<Decimal>,
    pub margin_pct: Option<Decimal>,
    pub materials: Vec<PreviewMaterialView>,
}

#[derive(Debug, Serialize)]
pub struct PreviewMaterialView {
    pub material_id: Uuid,
    pub material_name: String,
    pub unit_code: String,
    pub required: Decimal,
    /// What the active batches can cover.
    pub available: Decimal,
    pub shortfall: Decimal,
    /// The batches FEFO would consume, with the quantity from each.
    pub batches: Vec<PreviewBatchView>,
}

#[derive(Debug, Serialize)]
pub struct PreviewBatchView {
    pub batch_id: Uuid,
    pub expires_at: Option<NaiveDate>,
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

type ProductRow = (
    Uuid,
    String,
    Decimal,
    Decimal,
    Option<Uuid>,
    Option<i32>,
    Option<Decimal>,
    Option<i32>,
);
type ItemRow = (Uuid, String, String, Decimal, Decimal);
type BatchRow = (Uuid, NaiveDate, Option<NaiveDate>, Decimal, Decimal);

/// A material requirement with the FEFO decision already made.
struct MaterialPlan {
    material_id: Uuid,
    material_name: String,
    unit_code: String,
    required: Decimal,
    allocation: Allocation<Uuid>,
    costs: HashMap<Uuid, Decimal>,
    lines: Vec<PlannedLine>,
}

struct PlannedLine {
    batch_id: Uuid,
    expires_at: Option<NaiveDate>,
    qty: Decimal,
    unit_cost: Decimal,
}

/// Everything a run would need, without touching stock.
struct PlannedOrder {
    product_name: String,
    sell_price: Decimal,
    stock_qty: Decimal,
    recipe_id: Uuid,
    recipe_version: i32,
    yield_qty: Decimal,
    labor_minutes: i32,
    labor_rate_per_hour: Decimal,
    materials: Vec<MaterialPlan>,
    shortages: Vec<ShortageView>,
}

/// Pads a decimal to a fixed number of decimals for a stable JSON contract.
fn at_scale(value: Decimal, scale: u32) -> Decimal {
    let mut value = value;
    value.rescale(scale);
    value
}

/// Three decimals, never zero or negative.
fn normalized_qty(value: Decimal) -> Result<Decimal, ApiError> {
    let mut qty = value;
    qty.rescale(LEDGER_SCALE);

    if qty <= Decimal::ZERO {
        return Err(ApiError::BadRequest("invalid_quantity"));
    }

    Ok(qty)
}

/// Reads the current recipe and allocates its materials. `lock` is true for a
/// real run and false for a preview, which must not hold rows.
async fn plan_order(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    product_id: Uuid,
    production_qty: Decimal,
    lock: bool,
) -> Result<PlannedOrder, ApiError> {
    let product_sql = if lock {
        "select p.id, p.name, p.sell_price, p.stock_qty,
                p.current_recipe_id, r.version, r.yield_qty, r.labor_minutes
         from products p
         left join recipes r on r.id = p.current_recipe_id
         where p.id = $1 and p.tenant_id = $2 and p.is_active
         for update of p"
    } else {
        "select p.id, p.name, p.sell_price, p.stock_qty,
                p.current_recipe_id, r.version, r.yield_qty, r.labor_minutes
         from products p
         left join recipes r on r.id = p.current_recipe_id
         where p.id = $1 and p.tenant_id = $2 and p.is_active"
    };

    let product: Option<ProductRow> = sqlx::query_as(product_sql)
        .bind(product_id)
        .bind(tenant_id)
        .fetch_optional(&mut **tx)
        .await?;

    let (
        _,
        product_name,
        sell_price,
        stock_qty,
        current_recipe_id,
        current_version,
        current_yield,
        current_labor_minutes,
    ) = product.ok_or(ApiError::NotFound("product_not_found"))?;

    let (recipe_id, recipe_version, yield_qty, labor_minutes) = match (
        current_recipe_id,
        current_version,
        current_yield,
        current_labor_minutes,
    ) {
        (Some(recipe_id), Some(version), Some(yield_qty), Some(labor_minutes)) => {
            (recipe_id, version, yield_qty, labor_minutes)
        }
        _ => return Err(ApiError::Conflict("recipe_required")),
    };

    // The rate belongs to the tenant, so it is read once per plan. Zero means
    // no rate is configured and the run snapshots material cost only.
    let (labor_rate_per_hour,): (Decimal,) =
        sqlx::query_as("select labor_rate_per_hour from tenants where id = $1")
            .bind(tenant_id)
            .fetch_one(&mut **tx)
            .await?;

    // Material order is part of the lock order: every run locks batches in
    // material id order, so two runs sharing materials cannot deadlock.
    let items: Vec<ItemRow> = sqlx::query_as(
        "select ri.material_id, m.name, u.code, ri.qty, ri.waste_pct
         from recipe_items ri
         join materials m on m.id = ri.material_id
         join units u on u.id = m.base_unit_id
         where ri.recipe_id = $1
         order by ri.material_id",
    )
    .bind(recipe_id)
    .fetch_all(&mut **tx)
    .await?;

    // The API refuses to create an itemless recipe; this guards against rows
    // that reached the database another way.
    if items.is_empty() {
        return Err(ApiError::Conflict("recipe_items_required"));
    }

    let batch_sql = if lock {
        "select id, received_at, expires_at, qty_remaining, unit_cost
         from material_batches
         where tenant_id = $1 and material_id = $2 and qty_remaining > 0
         order by expires_at asc nulls last, received_at asc
         for update"
    } else {
        "select id, received_at, expires_at, qty_remaining, unit_cost
         from material_batches
         where tenant_id = $1 and material_id = $2 and qty_remaining > 0
         order by expires_at asc nulls last, received_at asc"
    };

    let mut materials = Vec::with_capacity(items.len());
    let mut shortages = Vec::new();

    for (material_id, material_name, unit_code, item_qty, waste_pct) in items {
        let required =
            takar_core::material_requirement(item_qty, waste_pct, production_qty, yield_qty)
                .map_err(|error| ApiError::Internal(anyhow::Error::new(error)))?
                .round_dp_with_strategy(LEDGER_SCALE, RoundingStrategy::ToPositiveInfinity);

        let rows: Vec<BatchRow> = sqlx::query_as(batch_sql)
            .bind(tenant_id)
            .bind(material_id)
            .fetch_all(&mut **tx)
            .await?;

        let mut costs: HashMap<Uuid, Decimal> = HashMap::with_capacity(rows.len());
        let mut batches: Vec<StockBatch<Uuid>> = Vec::with_capacity(rows.len());
        let mut expiries: HashMap<Uuid, Option<NaiveDate>> = HashMap::with_capacity(rows.len());

        for (id, received_at, expires_at, qty_remaining, unit_cost) in rows {
            costs.insert(id, unit_cost);
            expiries.insert(id, expires_at);
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

        let lines = allocation
            .lines
            .iter()
            .map(|line| PlannedLine {
                batch_id: line.id,
                expires_at: expiries[&line.id],
                qty: line.qty,
                unit_cost: costs[&line.id],
            })
            .collect();

        materials.push(MaterialPlan {
            material_id,
            material_name,
            unit_code,
            required,
            allocation,
            costs,
            lines,
        });
    }

    Ok(PlannedOrder {
        product_name,
        sell_price,
        stock_qty,
        recipe_id,
        recipe_version,
        yield_qty,
        labor_minutes,
        labor_rate_per_hour,
        materials,
        shortages,
    })
}

/// Snapshot cost from the plan.
///
/// A recipe's `labor_minutes` covers one batch, so a run of `qty` units takes
/// `qty / yield_qty` batches of labor. Overhead stays zero until a tenant
/// overhead input exists; the core already accepts the component.
fn order_cost(planned: &PlannedOrder, production_qty: Decimal) -> Result<ProductionCost, ApiError> {
    let consumption_costs: Vec<Consumption> = planned
        .materials
        .iter()
        .flat_map(|material| {
            material.lines.iter().map(|line| Consumption {
                qty: line.qty,
                unit_cost: line.unit_cost,
            })
        })
        .collect();

    let hours = Decimal::from(planned.labor_minutes) / Decimal::from(60);
    let batches = production_qty / planned.yield_qty;
    let labor_cost = planned.labor_rate_per_hour * hours * batches;

    takar_core::production_cost(
        &consumption_costs,
        labor_cost,
        Decimal::ZERO,
        production_qty,
    )
    .map_err(|error| ApiError::Internal(anyhow::Error::new(error)))
}

pub async fn preview_order(
    State(state): State<AppState>,
    guard: Require<CanReadProduction>,
    Json(body): Json<ProductionBody>,
) -> Result<Json<ProductionPreviewView>, ApiError> {
    let auth = guard.auth;
    let production_qty = normalized_qty(body.qty)?;
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let planned = plan_order(
        &mut tx,
        auth.tenant_id,
        body.product_id,
        production_qty,
        false,
    )
    .await?;

    let can_produce = planned.shortages.is_empty();
    let cost = if can_produce {
        Some(order_cost(&planned, production_qty)?)
    } else {
        None
    };

    let (margin, margin_pct) = match &cost {
        Some(cost) => {
            let margin = planned.sell_price - cost.unit_cost;
            let margin_pct = if planned.sell_price > Decimal::ZERO {
                Some((margin / planned.sell_price * Decimal::from(100)).round_dp(2))
            } else {
                None
            };

            (Some(margin), margin_pct)
        }
        None => (None, None),
    };

    let materials = planned
        .materials
        .iter()
        .map(|material| PreviewMaterialView {
            material_id: material.material_id,
            material_name: material.material_name.clone(),
            unit_code: material.unit_code.clone(),
            required: material.required,
            available: material.allocation.allocated,
            shortfall: material.allocation.shortfall,
            batches: material
                .lines
                .iter()
                .map(|line| PreviewBatchView {
                    batch_id: line.batch_id,
                    expires_at: line.expires_at,
                    qty: line.qty,
                    unit_cost: line.unit_cost,
                })
                .collect(),
        })
        .collect();

    tx.commit().await?;

    Ok(Json(ProductionPreviewView {
        product_id: body.product_id,
        product_name: planned.product_name,
        sell_price: planned.sell_price,
        stock_qty: planned.stock_qty,
        recipe_id: planned.recipe_id,
        recipe_version: planned.recipe_version,
        yield_qty: planned.yield_qty,
        labor_minutes: planned.labor_minutes,
        labor_rate_per_hour: planned.labor_rate_per_hour,
        qty: production_qty,
        can_produce,
        material_cost: cost.map(|cost| at_scale(cost.material_cost, 2)),
        labor_cost: cost.map(|cost| at_scale(cost.labor_cost, 2)),
        overhead_cost: cost.map(|cost| at_scale(cost.overhead_cost, 2)),
        unit_cost: cost.map(|cost| at_scale(cost.unit_cost, 4)),
        margin: margin.map(|value| at_scale(value, 2)),
        margin_pct: margin_pct.map(|value| at_scale(value, 2)),
        materials,
    }))
}

pub async fn create_order(
    State(state): State<AppState>,
    guard: Require<CanWriteProduction>,
    Json(body): Json<ProductionBody>,
) -> Result<(StatusCode, Json<ProductionOrderView>), ApiError> {
    let auth = guard.auth;
    let production_qty = normalized_qty(body.qty)?;
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let planned = plan_order(
        &mut tx,
        auth.tenant_id,
        body.product_id,
        production_qty,
        true,
    )
    .await?;

    // D3: block the whole run and show every gap at once. Nothing has been
    // written yet, and dropping the transaction rolls back cleanly.
    if !planned.shortages.is_empty() {
        return Err(ApiError::ConflictDetails(
            "insufficient_stock",
            serde_json::json!({ "shortages": planned.shortages }),
        ));
    }

    let cost = order_cost(&planned, production_qty)?;
    let produced_on = body.produced_on.unwrap_or_else(|| Utc::now().date_naive());

    let (order_id, created_at, material_cost, labor_cost, overhead_cost, unit_cost): (
        Uuid,
        DateTime<Utc>,
        Decimal,
        Decimal,
        Decimal,
        Decimal,
    ) = sqlx::query_as(
        "insert into production_orders
             (tenant_id, product_id, recipe_id, planned_qty, actual_qty, produced_on,
              material_cost, labor_cost, overhead_cost, unit_cost, created_by)
         values ($1, $2, $3, $4, $4, $5, $6, $7, $8, $9, $10)
         returning id, created_at, material_cost, labor_cost, overhead_cost, unit_cost",
    )
    .bind(auth.tenant_id)
    .bind(body.product_id)
    .bind(planned.recipe_id)
    .bind(production_qty)
    .bind(produced_on)
    .bind(cost.material_cost)
    .bind(cost.labor_cost)
    .bind(cost.overhead_cost)
    .bind(cost.unit_cost)
    .bind(auth.user_id)
    .fetch_one(&mut *tx)
    .await?;

    let mut consumptions: Vec<ConsumptionView> = Vec::new();

    for material in planned.materials {
        let MaterialPlan {
            material_id,
            material_name,
            allocation,
            costs,
            ..
        } = material;

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
            product_name: planned.product_name,
            recipe_id: planned.recipe_id,
            recipe_version: planned.recipe_version,
            planned_qty: production_qty,
            actual_qty: production_qty,
            material_cost,
            labor_cost,
            overhead_cost,
            unit_cost,
            produced_on,
            status: "done".to_string(),
            created_at,
            consumptions,
        }),
    ))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/production-orders", post(create_order))
        .route("/production-orders/preview", post(preview_order))
}
