//! Tenant settings.
//!
//! One value so far: the hourly labor rate used to cost production labor. It
//! belongs to the tenant rather than a recipe because it is a property of the
//! business; the recipe only says how long a batch takes.

use axum::{extract::State, routing::get, Json, Router};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::auth::guard::{CanManageTenant, Require};
use crate::auth::AuthContext;
use crate::{error::ApiError, AppState};

/// Matches `tenants.labor_rate_per_hour`.
const MONEY_SCALE: u32 = 2;

#[derive(Debug, Serialize)]
pub struct SettingsView {
    pub labor_rate_per_hour: Decimal,
}

#[derive(Debug, Deserialize)]
pub struct SettingsUpdate {
    pub labor_rate_per_hour: Decimal,
}

/// Every member may read the rate: it explains the HPP they see on a run.
pub async fn get_settings(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<SettingsView>, ApiError> {
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let (labor_rate_per_hour,): (Decimal,) =
        sqlx::query_as("select labor_rate_per_hour from tenants where id = $1")
            .bind(auth.tenant_id)
            .fetch_one(&mut *tx)
            .await?;

    tx.commit().await?;

    Ok(Json(SettingsView {
        labor_rate_per_hour,
    }))
}

pub async fn update_settings(
    State(state): State<AppState>,
    guard: Require<CanManageTenant>,
    Json(body): Json<SettingsUpdate>,
) -> Result<Json<SettingsView>, ApiError> {
    let auth = guard.auth;

    if body.labor_rate_per_hour.is_sign_negative() {
        return Err(ApiError::BadRequest("invalid_labor_rate"));
    }

    let mut rate = body.labor_rate_per_hour;
    rate.rescale(MONEY_SCALE);

    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let (labor_rate_per_hour,): (Decimal,) = sqlx::query_as(
        "update tenants
         set labor_rate_per_hour = $2, updated_at = now()
         where id = $1
         returning labor_rate_per_hour",
    )
    .bind(auth.tenant_id)
    .bind(rate)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(Json(SettingsView {
        labor_rate_per_hour,
    }))
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/settings", get(get_settings).put(update_settings))
}
