//! Billing: a manual transfer, and the subscription it buys.
//!
//! D4 decided manual verification with a gateway-ready shape. The tenant owner
//! records a transfer; the operator checks it against the bank and verifies it
//! with `bin/billing`. Verification calls `activate`, the only function that
//! writes a subscription, so a gateway webhook later reuses it unchanged.
//!
//! Nothing here decides access. That is derived from dates in
//! `takar_domain::billing`, so a late verification or a support extension
//! cannot leave a stale status behind.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get},
    Json, Router,
};
use chrono::{DateTime, Duration, Months, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use takar_domain::billing::{access, ends_at, Access};
use takar_domain::plan::Plan;

use crate::auth::guard::{CanManageBilling, Require};
use crate::{error::ApiError, AppState};

/// Matches `payments.amount`.
const MONEY_SCALE: u32 = 2;

/// A payment buys between one and twelve months.
const MIN_MONTHS: i32 = 1;
const MAX_MONTHS: i32 = 12;

/// How many payments the billing screen shows.
const HISTORY: i64 = 20;

/// Validated by the API so adding one is a code change, not a migration.
const METHODS: [&str; 4] = ["transfer", "qris", "cash", "lainnya"];

#[derive(Debug, Serialize)]
pub struct BillingView {
    pub plan: String,
    pub access: Access,
    pub trial_ends_at: Option<DateTime<Utc>>,
    pub paid_until: Option<DateTime<Utc>>,
    /// When access runs out, grace included. Null when no date is set.
    pub ends_at: Option<DateTime<Utc>>,
    /// Negative once the entitlement has passed. Null when `ends_at` is null.
    pub days_left: Option<i64>,
    /// Monthly price of the current plan. Null on the trial.
    pub price_per_month: Option<i64>,
    /// Every plan the tenant can buy, so the screen never hardcodes a price.
    pub plans: Vec<PlanOption>,
    pub payments: Vec<PaymentView>,
}

#[derive(Debug, Serialize)]
pub struct PlanOption {
    pub id: String,
    pub price: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PaymentView {
    pub id: Uuid,
    pub amount: Decimal,
    pub method: String,
    pub reference: Option<String>,
    pub plan: String,
    pub period_start: NaiveDate,
    pub period_end: NaiveDate,
    pub status: String,
    pub note: Option<String>,
    pub verified_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct NewPayment {
    pub plan: String,
    /// Defaults to the plan price times `months`.
    pub amount: Option<Decimal>,
    pub method: String,
    pub reference: Option<String>,
    pub note: Option<String>,
    pub months: Option<i32>,
}

/// The owner's view: status, what it costs, and the payments so far.
pub async fn get_billing(
    State(state): State<AppState>,
    guard: Require<CanManageBilling>,
) -> Result<Json<BillingView>, ApiError> {
    let auth = guard.auth;
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let (plan, trial_ends_at, paid_until): (String, Option<DateTime<Utc>>, Option<DateTime<Utc>>) =
        sqlx::query_as("select plan, trial_ends_at, paid_until from tenants where id = $1")
            .bind(auth.tenant_id)
            .fetch_one(&mut *tx)
            .await?;

    let payments: Vec<PaymentView> = sqlx::query_as(
        "select id, amount, method, reference, plan, period_start, period_end,
                status, note, verified_at, created_at
         from payments
         where tenant_id = $1
         order by created_at desc
         limit $2",
    )
    .bind(auth.tenant_id)
    .bind(HISTORY)
    .fetch_all(&mut *tx)
    .await?;

    tx.commit().await?;

    let plan = Plan::parse(&plan)
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("unknown plan {plan}")))?;
    let now = Utc::now();
    let ends_at = ends_at(trial_ends_at, paid_until);

    Ok(Json(BillingView {
        plan: plan.as_str().to_string(),
        access: access(trial_ends_at, paid_until, now),
        trial_ends_at,
        paid_until,
        ends_at,
        days_left: ends_at.map(|end| (end - now).num_days()),
        price_per_month: plan.monthly_price(),
        plans: Plan::paid()
            .iter()
            .map(|plan| PlanOption {
                id: plan.as_str().to_string(),
                price: plan.monthly_price().unwrap_or_default(),
            })
            .collect(),
        payments,
    }))
}

/// Records a transfer the operator still has to check.
///
/// The period starts today, or the day after the current paid period when the
/// tenant pays early, so a renewal never overlaps and never leaves a gap.
pub async fn create_payment(
    State(state): State<AppState>,
    guard: Require<CanManageBilling>,
    Json(body): Json<NewPayment>,
) -> Result<(StatusCode, Json<PaymentView>), ApiError> {
    let auth = guard.auth;

    let plan = Plan::parse(&body.plan)
        .filter(|plan| plan.monthly_price().is_some())
        .ok_or(ApiError::BadRequest("invalid_plan"))?;

    let months = body.months.unwrap_or(1);
    if !(MIN_MONTHS..=MAX_MONTHS).contains(&months) {
        return Err(ApiError::BadRequest("invalid_months"));
    }

    if !METHODS.contains(&body.method.as_str()) {
        return Err(ApiError::BadRequest("invalid_method"));
    }

    let price = plan
        .monthly_price()
        .ok_or(ApiError::BadRequest("invalid_plan"))?;

    let mut amount = match body.amount {
        Some(amount) => amount,
        None => Decimal::from(price) * Decimal::from(months),
    };

    if amount <= Decimal::ZERO {
        return Err(ApiError::BadRequest("invalid_amount"));
    }

    amount.rescale(MONEY_SCALE);

    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let paid_until: Option<DateTime<Utc>> =
        sqlx::query_scalar("select paid_until from tenants where id = $1")
            .bind(auth.tenant_id)
            .fetch_one(&mut *tx)
            .await?;

    let today = Utc::now().date_naive();
    let start = paid_until
        .map(|until| until.date_naive())
        .filter(|until| *until > today)
        .unwrap_or(today);

    let end = start
        .checked_add_months(Months::new(months as u32))
        .ok_or(ApiError::BadRequest("invalid_months"))?;

    let payment: PaymentView = sqlx::query_as(
        "insert into payments
            (tenant_id, amount, method, reference, plan, period_start, period_end, note, created_by)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         returning id, amount, method, reference, plan, period_start, period_end,
                   status, note, verified_at, created_at",
    )
    .bind(auth.tenant_id)
    .bind(amount)
    .bind(&body.method)
    .bind(&body.reference)
    .bind(plan.as_str())
    .bind(start)
    .bind(end)
    .bind(&body.note)
    .bind(auth.user_id)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok((StatusCode::CREATED, Json(payment)))
}

/// Withdraws a payment the tenant recorded by mistake. Verified rows stay.
pub async fn delete_payment(
    State(state): State<AppState>,
    guard: Require<CanManageBilling>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let auth = guard.auth;
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let deleted = sqlx::query(
        "delete from payments
         where id = $1 and tenant_id = $2 and status = 'pending'",
    )
    .bind(id)
    .bind(auth.tenant_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    tx.commit().await?;

    if deleted == 0 {
        return Err(ApiError::NotFound("payment_not_found"));
    }

    Ok(StatusCode::NO_CONTENT)
}

/// The one function that writes a subscription.
///
/// Called by the operator CLI today and by a gateway webhook later. The end
/// date only ever moves forward, so verifying an older payment after a newer
/// one cannot shorten access.
pub async fn activate(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    plan: Plan,
    period_end: NaiveDate,
) -> Result<(), ApiError> {
    // A payment covers its last day, so access runs to the next midnight.
    let until = (period_end + Duration::days(1))
        .and_hms_opt(0, 0, 0)
        .ok_or(ApiError::BadRequest("invalid_period"))?
        .and_utc();

    sqlx::query(
        "update tenants
         set plan = $2,
             paid_until = greatest(coalesce(paid_until, $3), $3),
             updated_at = now()
         where id = $1",
    )
    .bind(tenant_id)
    .bind(plan.as_str())
    .bind(until)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/billing", get(get_billing))
        .route("/billing/payments", axum::routing::post(create_payment))
        .route("/billing/payments/{id}", delete(delete_payment))
}
