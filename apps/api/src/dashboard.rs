//! Overview payload: one request with everything the first screen needs.
//!
//! The dashboard is the page an owner opens on a phone over mobile data, so it
//! is deliberately a single bounded response instead of a fan-out over the
//! stock endpoints. Counts and lists are derived from the batch ledger; nothing
//! here is stored.

use axum::{extract::State, Json};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::Serialize;
use uuid::Uuid;

use crate::auth::guard::{CanReadMaterials, Require};
use crate::master::units::UnitRef;
use crate::{error::ApiError, AppState};

/// Batches expiring inside this window are worth acting on now.
const EXPIRY_HORIZON_DAYS: i32 = 7;
/// Enough rows to fill the fold on a phone without making the payload heavy.
const LIST_LIMIT: i64 = 8;

#[derive(Debug, Serialize)]
pub struct DashboardView {
    pub summary: DashboardSummary,
    /// Materials under their declared minimum, worst shortfall first.
    pub critical: Vec<CriticalRow>,
    /// Active batches at or past their expiry window, earliest first.
    pub expiring: Vec<ExpiringRow>,
}

#[derive(Debug, Serialize)]
pub struct DashboardSummary {
    pub materials: i64,
    pub below_min: i64,
    /// Zero on hand with a minimum above zero, a subset of `below_min`.
    pub out_of_stock: i64,
    /// Batches expiring within `horizon_days` but not yet past it.
    pub expiring: i64,
    /// Batches already past `expires_at` with stock left to use or waste.
    pub expired: i64,
    pub active_batches: i64,
    pub stock_value: Decimal,
    /// Echoed so the UI does not hardcode the window in its copy.
    pub horizon_days: i32,
}

#[derive(Debug, Serialize)]
pub struct CriticalRow {
    pub material_id: Uuid,
    pub name: String,
    pub base_unit: UnitRef,
    pub on_hand: Decimal,
    pub min_stock: Decimal,
    pub shortfall: Decimal,
    pub active_batches: i64,
    pub nearest_expiry: Option<NaiveDate>,
}

#[derive(Debug, Serialize)]
pub struct ExpiringRow {
    pub batch_id: Uuid,
    pub material_id: Uuid,
    pub material_name: String,
    pub base_unit: UnitRef,
    pub qty_remaining: Decimal,
    pub unit_cost: Decimal,
    pub expires_at: NaiveDate,
    /// Days from today, negative once the batch is past its expiry date.
    pub days_left: i32,
}

type SummaryRow = (i64, i64, i64, Decimal, i64, i64, i64);
type CriticalTuple = (
    Uuid,
    String,
    Uuid,
    String,
    Decimal,
    Decimal,
    Decimal,
    i64,
    Option<NaiveDate>,
);
type ExpiringTuple = (
    Uuid,
    Uuid,
    String,
    Uuid,
    String,
    Decimal,
    Decimal,
    NaiveDate,
    i32,
);

/// Everything the overview screen shows, in one round trip.
pub async fn overview(
    State(state): State<AppState>,
    guard: Require<CanReadMaterials>,
) -> Result<Json<DashboardView>, ApiError> {
    let auth = guard.auth;
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let (materials, below_min, out_of_stock, stock_value, active_batches, expiring, expired): SummaryRow =
        sqlx::query_as(
             "with levels as (
                 select m.id, m.min_stock,
                        coalesce(sum(b.qty_remaining), 0)::numeric(18,3) as on_hand
                 from materials m
                 left join material_batches b
                        on b.tenant_id = m.tenant_id
                       and b.material_id = m.id
                       and b.qty_remaining > 0
                 where m.tenant_id = $1 and m.is_active
                 group by m.id
             ),
             batches as (
                 select count(*) filter (where b.qty_remaining > 0) as active_batches,
                        count(*) filter (
                            where b.qty_remaining > 0
                              and b.expires_at >= current_date
                              and b.expires_at <= current_date + $2::int
                        ) as expiring,
                        count(*) filter (
                            where b.qty_remaining > 0
                              and b.expires_at < current_date
                        ) as expired
                 from material_batches b
                 where b.tenant_id = $1
             )
             select (select count(*) from levels),
                    (select count(*) from levels where on_hand < min_stock),
                    (select count(*) from levels where on_hand = 0 and min_stock > 0),
                    -- Valued at the cost each batch was received at, not the
                    -- material's latest price: a new purchase must not revalue
                    -- stock that was already on the shelf.
                    (select coalesce(sum(b.qty_remaining * b.unit_cost), 0)::numeric(18,2)
                     from material_batches b
                     where b.tenant_id = $1 and b.qty_remaining > 0),
                    batches.active_batches,
                    batches.expiring,
                    batches.expired
             from batches",
        )
        .bind(auth.tenant_id)
        .bind(EXPIRY_HORIZON_DAYS)
        .fetch_one(&mut *tx)
        .await?;

    let critical_rows: Vec<CriticalTuple> = sqlx::query_as(
        "select m.id, m.name, u.id, u.code,
                coalesce(sum(b.qty_remaining), 0)::numeric(18,3) as on_hand,
                m.min_stock,
                (m.min_stock - coalesce(sum(b.qty_remaining), 0))::numeric(18,3) as shortfall,
                count(b.id) as active_batches,
                min(b.expires_at) as nearest_expiry
         from materials m
         join units u on u.id = m.base_unit_id
         left join material_batches b
                on b.tenant_id = m.tenant_id
               and b.material_id = m.id
               and b.qty_remaining > 0
         where m.tenant_id = $1 and m.is_active
         group by m.id, u.id
         having coalesce(sum(b.qty_remaining), 0) < m.min_stock
         order by (m.min_stock - coalesce(sum(b.qty_remaining), 0))
                      / nullif(m.min_stock, 0) desc,
                  m.name asc
         limit $2",
    )
    .bind(auth.tenant_id)
    .bind(LIST_LIMIT)
    .fetch_all(&mut *tx)
    .await?;

    let expiring_rows: Vec<ExpiringTuple> = sqlx::query_as(
        "select b.id, b.material_id, m.name, u.id, u.code,
                b.qty_remaining, b.unit_cost, b.expires_at,
                (b.expires_at - current_date) as days_left
         from material_batches b
         join materials m on m.id = b.material_id
         join units u on u.id = m.base_unit_id
         where b.tenant_id = $1
           and b.qty_remaining > 0
           and b.expires_at <= current_date + $2::int
         order by b.expires_at asc, b.received_at asc, b.id asc
         limit $3",
    )
    .bind(auth.tenant_id)
    .bind(EXPIRY_HORIZON_DAYS)
    .bind(LIST_LIMIT)
    .fetch_all(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(Json(DashboardView {
        summary: DashboardSummary {
            materials,
            below_min,
            out_of_stock,
            expiring,
            expired,
            active_batches,
            stock_value,
            horizon_days: EXPIRY_HORIZON_DAYS,
        },
        critical: critical_rows
            .into_iter()
            .map(
                |(
                    material_id,
                    name,
                    unit_id,
                    unit_code,
                    on_hand,
                    min_stock,
                    shortfall,
                    active_batches,
                    nearest_expiry,
                )| CriticalRow {
                    material_id,
                    name,
                    base_unit: UnitRef {
                        id: unit_id,
                        code: unit_code,
                    },
                    on_hand,
                    min_stock,
                    shortfall,
                    active_batches,
                    nearest_expiry,
                },
            )
            .collect(),
        expiring: expiring_rows
            .into_iter()
            .map(
                |(
                    batch_id,
                    material_id,
                    material_name,
                    unit_id,
                    unit_code,
                    qty_remaining,
                    unit_cost,
                    expires_at,
                    days_left,
                )| ExpiringRow {
                    batch_id,
                    material_id,
                    material_name,
                    base_unit: UnitRef {
                        id: unit_id,
                        code: unit_code,
                    },
                    qty_remaining,
                    unit_cost,
                    expires_at,
                    days_left,
                },
            )
            .collect(),
    }))
}
