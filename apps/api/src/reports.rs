//! Business summary: what the stock is worth, what was lost, and what each
//! product earns.
//!
//! Every number comes from the ledger and the snapshots taken when the
//! documents were written. Nothing here is recalculated from today's prices,
//! so a new purchase or a new production run cannot rewrite an earlier month.
//!
//! Margin needs a cost of goods sold. `sale_items.unit_cost` holds the HPP at
//! the moment of the sale; sales written before that column existed have none,
//! and the response marks them instead of valuing them at today's cost.

use axum::{extract::State, routing::get, Json, Router};
use rust_decimal::{Decimal, RoundingStrategy};
use serde::Serialize;
use uuid::Uuid;

use crate::auth::guard::{CanReadReports, Require};
use crate::{error::ApiError, AppState};

/// Money is stored as `numeric(18,2)`.
const MONEY_SCALE: u32 = 2;
/// Enough rows to show the biggest losses without paging.
const WASTE_LIMIT: i64 = 10;

#[derive(Debug, Serialize)]
pub struct BusinessView {
    /// Window for sales and waste, taken from the tenant's usage setting.
    pub window_days: i32,
    pub stock: StockValue,
    pub waste: WasteValue,
    pub products: Vec<ProductMargin>,
}

#[derive(Debug, Serialize)]
pub struct StockValue {
    /// Material batches at the cost they were received at.
    pub materials: Decimal,
    /// Finished goods at their most recent production cost.
    pub products: Decimal,
    pub total: Decimal,
}

#[derive(Debug, Serialize)]
pub struct WasteValue {
    pub value: Decimal,
    pub movements: i64,
    /// Biggest losses first.
    pub by_material: Vec<WasteRow>,
}

#[derive(Debug, Serialize)]
pub struct WasteRow {
    pub material_id: Uuid,
    pub material_name: String,
    pub unit_code: String,
    pub qty: Decimal,
    pub value: Decimal,
}

#[derive(Debug, Serialize)]
pub struct ProductMargin {
    pub product_id: Uuid,
    pub name: String,
    pub sell_price: Decimal,
    /// Latest production cost per unit, null when never produced.
    pub unit_cost: Option<Decimal>,
    pub margin_per_unit: Option<Decimal>,
    /// Percent of the selling price, null when the price is zero.
    pub margin_pct: Option<Decimal>,
    pub sold_qty: Decimal,
    pub revenue: Decimal,
    /// Null when no unit sold in the window had a cost snapshot.
    pub cost_of_goods: Option<Decimal>,
    pub margin: Option<Decimal>,
    /// True when at least one sold line had no cost, so the totals cannot be
    /// trusted for this product.
    pub cost_incomplete: bool,
}

type ProductRow = (
    Uuid,
    String,
    Decimal,
    Option<Decimal>,
    Decimal,
    Decimal,
    Option<Decimal>,
    i64,
);

type WasteTuple = (Uuid, String, String, Decimal, Decimal);

pub async fn business(
    State(state): State<AppState>,
    guard: Require<CanReadReports>,
) -> Result<Json<BusinessView>, ApiError> {
    let auth = guard.auth;
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    // The same window the shopping list uses: one setting, one explanation.
    let (window_days,): (i32,) =
        sqlx::query_as("select usage_window_days from tenants where id = $1")
            .bind(auth.tenant_id)
            .fetch_one(&mut *tx)
            .await?;

    let (material_value, product_value): (Decimal, Decimal) = sqlx::query_as(
        "with latest as (
             select distinct on (product_id) product_id, unit_cost
             from production_orders
             where tenant_id = $1 and status = 'done' and unit_cost is not null
             order by product_id, produced_on desc, created_at desc
         )
         select
             (select coalesce(sum(b.qty_remaining * b.unit_cost), 0)::numeric(18,2)
              from material_batches b
              where b.tenant_id = $1 and b.qty_remaining > 0),
             (select coalesce(sum(p.stock_qty * latest.unit_cost), 0)::numeric(18,2)
              from products p
              join latest on latest.product_id = p.id
              where p.tenant_id = $1 and p.stock_qty > 0)",
    )
    .bind(auth.tenant_id)
    .fetch_one(&mut *tx)
    .await?;

    let (waste_value, waste_movements): (Decimal, i64) = sqlx::query_as(
        "select coalesce(sum(-mov.qty * coalesce(mov.unit_cost, 0)), 0)::numeric(18,2),
                count(*)
         from stock_movements mov
         where mov.tenant_id = $1
           and mov.type in ('waste', 'expired')
           and mov.created_at >= now() - make_interval(days => $2)",
    )
    .bind(auth.tenant_id)
    .bind(window_days)
    .fetch_one(&mut *tx)
    .await?;

    let waste_rows: Vec<WasteTuple> = sqlx::query_as(
        "select b.material_id, mat.name, u.code,
                sum(-mov.qty)::numeric(18,3),
                sum(-mov.qty * coalesce(mov.unit_cost, 0))::numeric(18,2)
         from stock_movements mov
         join material_batches b on b.id = mov.batch_id
         join materials mat on mat.id = b.material_id
         join units u on u.id = mat.base_unit_id
         where mov.tenant_id = $1
           and mov.type in ('waste', 'expired')
           and mov.created_at >= now() - make_interval(days => $2)
         group by b.material_id, mat.name, u.code
         order by 5 desc, mat.name
         limit $3",
    )
    .bind(auth.tenant_id)
    .bind(window_days)
    .bind(WASTE_LIMIT)
    .fetch_all(&mut *tx)
    .await?;

    let rows: Vec<ProductRow> = sqlx::query_as(
        "with latest as (
             select distinct on (product_id) product_id, unit_cost
             from production_orders
             where tenant_id = $1 and status = 'done' and unit_cost is not null
             order by product_id, produced_on desc, created_at desc
         ),
         sold as (
             select si.product_id,
                    sum(si.qty)::numeric(18,3) as qty,
                    sum(si.qty * si.price)::numeric(18,2) as revenue,
                    sum(si.qty * si.unit_cost)::numeric(18,2) as cogs,
                    count(*) filter (where si.unit_cost is null) as uncosted
             from sale_items si
             join sales s on s.id = si.sale_id
             where si.tenant_id = $1
               and s.sold_at >= now() - make_interval(days => $2)
             group by si.product_id
         )
         select p.id, p.name, p.sell_price, latest.unit_cost,
                coalesce(sold.qty, 0), coalesce(sold.revenue, 0),
                sold.cogs, coalesce(sold.uncosted, 0)
         from products p
         left join latest on latest.product_id = p.id
         left join sold on sold.product_id = p.id
         where p.tenant_id = $1 and p.is_active
         order by coalesce(sold.revenue, 0) desc, p.name",
    )
    .bind(auth.tenant_id)
    .bind(window_days)
    .fetch_all(&mut *tx)
    .await?;

    tx.commit().await?;

    let products = rows
        .into_iter()
        .map(
            |(product_id, name, sell_price, unit_cost, sold_qty, revenue, cogs, uncosted)| {
                let margin_per_unit = unit_cost.map(|cost| money(sell_price - cost));

                let margin_pct = match (margin_per_unit, sell_price) {
                    (Some(margin), price) if price > Decimal::ZERO => {
                        Some(money(margin / price * Decimal::from(100)))
                    }
                    _ => None,
                };

                let cost_incomplete = uncosted > 0;
                let cost_of_goods = if cost_incomplete { None } else { cogs };

                ProductMargin {
                    product_id,
                    name,
                    sell_price,
                    unit_cost,
                    margin_per_unit,
                    margin_pct,
                    sold_qty,
                    revenue: money(revenue),
                    cost_of_goods: cost_of_goods.map(money),
                    margin: cost_of_goods.map(|cost| money(revenue - cost)),
                    cost_incomplete,
                }
            },
        )
        .collect();

    Ok(Json(BusinessView {
        window_days,
        stock: StockValue {
            materials: money(material_value),
            products: money(product_value),
            total: money(material_value + product_value),
        },
        waste: WasteValue {
            value: money(waste_value),
            movements: waste_movements,
            by_material: waste_rows
                .into_iter()
                .map(
                    |(material_id, material_name, unit_code, qty, value)| WasteRow {
                        material_id,
                        material_name,
                        unit_code,
                        qty,
                        value: money(value),
                    },
                )
                .collect(),
        },
        products,
    }))
}

/// Two decimals and a fixed scale. A computed `coalesce(…, 0)` can arrive with
/// scale zero, which would serialize as `"0"` instead of `"0.00"`; rescaling
/// keeps every money field the same shape for the client.
fn money(value: Decimal) -> Decimal {
    let mut value =
        value.round_dp_with_strategy(MONEY_SCALE, RoundingStrategy::MidpointAwayFromZero);
    value.rescale(MONEY_SCALE);
    value
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/reports", get(business))
}
