//! Subscription plan, usage, and the checks that keep a tenant inside it.
//!
//! Limits are entitlements, not permissions: the role says what a member may
//! do, the plan says how much of it the tenant may have. A create is refused
//! when the tenant is already at the ceiling; existing rows are never touched,
//! so a downgrade leaves the data intact and only blocks new additions.

use axum::{extract::State, routing::get, Json, Router};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use takar_domain::billing::{ends_at, Access};
use takar_domain::plan::{Plan, Resource};

use crate::auth::AuthContext;
use crate::{error::ApiError, AppState};

#[derive(Debug, Serialize)]
pub struct SubscriptionView {
    pub plan: String,
    /// Whether the tenant may still write. Every member sees it, because a
    /// refusal they hit has to be explainable.
    pub access: Access,
    /// Null for a tenant that was never on a trial.
    pub trial_ends_at: Option<DateTime<Utc>>,
    /// End of the last verified paid period.
    pub paid_until: Option<DateTime<Utc>>,
    /// When access runs out, grace included.
    pub ends_at: Option<DateTime<Utc>>,
    /// Null means the plan does not cap that resource.
    pub limits: LimitsView,
    pub usage: UsageView,
}

#[derive(Debug, Serialize)]
pub struct LimitsView {
    pub users: Option<i64>,
    pub materials: Option<i64>,
    pub recipes: Option<i64>,
    pub warehouses: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct UsageView {
    /// Active members plus pending invitations: an invitation reserves a seat,
    /// otherwise a tenant could invite ten people and accept them one by one.
    pub users: i64,
    pub materials: i64,
    pub recipes: i64,
    pub warehouses: i64,
}

/// Every member may see the plan: it explains why a create was refused.
pub async fn get_subscription(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<SubscriptionView>, ApiError> {
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let (plan, trial_ends_at, paid_until): (String, Option<DateTime<Utc>>, Option<DateTime<Utc>>) =
        sqlx::query_as("select plan, trial_ends_at, paid_until from tenants where id = $1")
            .bind(auth.tenant_id)
            .fetch_one(&mut *tx)
            .await?;

    let usage = UsageView {
        users: count(&mut tx, auth.tenant_id, Resource::Users).await?,
        materials: count(&mut tx, auth.tenant_id, Resource::Materials).await?,
        recipes: count(&mut tx, auth.tenant_id, Resource::Recipes).await?,
        warehouses: 0,
    };

    tx.commit().await?;

    let plan = Plan::parse(&plan)
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("unknown plan {plan}")))?;
    let limits = plan.limits();

    Ok(Json(SubscriptionView {
        plan: plan.as_str().to_string(),
        access: auth.access,
        trial_ends_at,
        paid_until,
        ends_at: ends_at(trial_ends_at, paid_until),
        limits: LimitsView {
            users: limits.users,
            materials: limits.materials,
            recipes: limits.recipes,
            warehouses: limits.warehouses,
        },
        usage,
    }))
}

/// Refuses a create that would push the tenant past its plan.
///
/// Call it inside the transaction that will do the insert, after the checks
/// that explain a more specific failure, so the caller sees the real reason.
pub(crate) async fn ensure_capacity(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    resource: Resource,
) -> Result<(), ApiError> {
    let (plan,): (String,) = sqlx::query_as("select plan from tenants where id = $1")
        .bind(tenant_id)
        .fetch_one(&mut **tx)
        .await?;

    let plan = Plan::parse(&plan)
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("unknown plan {plan}")))?;
    let limits = plan.limits();

    // An uncapped resource needs no count.
    let Some(limit) = limits.limit(resource) else {
        return Ok(());
    };

    let current = count(tx, tenant_id, resource).await?;

    if limits.exceeded(resource, current).is_some() {
        return Err(ApiError::ForbiddenDetails(
            "plan_limit_reached",
            serde_json::json!({
                "resource": resource.as_str(),
                "plan": plan.as_str(),
                "limit": limit,
                "current": current,
            }),
        ));
    }

    Ok(())
}

/// What counts against a limit.
///
/// Archived rows do not: an inactive material or a superseded recipe version
/// is history, not consumption. Warehouses have no table until P4-01.
async fn count(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    resource: Resource,
) -> Result<i64, ApiError> {
    let value = match resource {
        Resource::Users => {
            sqlx::query_scalar(
                "select (select count(*) from memberships
                         where tenant_id = $1 and status = 'active')
                      + (select count(*) from invitations
                         where tenant_id = $1 and status = 'pending'
                           and expires_at > now())",
            )
            .bind(tenant_id)
            .fetch_one(&mut **tx)
            .await?
        }
        Resource::Materials => {
            sqlx::query_scalar("select count(*) from materials where tenant_id = $1 and is_active")
                .bind(tenant_id)
                .fetch_one(&mut **tx)
                .await?
        }
        Resource::Recipes => {
            sqlx::query_scalar("select count(*) from recipes where tenant_id = $1 and is_active")
                .bind(tenant_id)
                .fetch_one(&mut **tx)
                .await?
        }
        Resource::Warehouses => 0,
    };

    Ok(value)
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/subscription", get(get_subscription))
}
