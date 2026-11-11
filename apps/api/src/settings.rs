//! Tenant settings.
//!
//! Two values so far: the hourly labor rate used to cost production labor, and
//! the window purchase recommendations average usage over. Both belong to the
//! tenant because they describe the business, not a single recipe.

use axum::{extract::State, routing::get, Json, Router};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::auth::guard::{CanManageTenant, Require};
use crate::auth::AuthContext;
use crate::{error::ApiError, AppState};

/// Matches `tenants.labor_rate_per_hour`.
const MONEY_SCALE: u32 = 2;

/// Matches the check on `tenants.usage_window_days`.
const MIN_WINDOW_DAYS: i32 = 1;
const MAX_WINDOW_DAYS: i32 = 365;

#[derive(Debug, Serialize)]
pub struct SettingsView {
    pub labor_rate_per_hour: Decimal,
    /// Days of movement history purchase recommendations average over.
    pub usage_window_days: i32,
}

/// Every field is optional so one screen can save one value without sending
/// the others back. Omitted fields keep their stored value.
#[derive(Debug, Deserialize)]
pub struct SettingsUpdate {
    pub labor_rate_per_hour: Option<Decimal>,
    pub usage_window_days: Option<i32>,
}

/// Every member may read the settings: the rate explains the HPP they see on a
/// run, and the window explains the shopping list.
pub async fn get_settings(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<SettingsView>, ApiError> {
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let (labor_rate_per_hour, usage_window_days): (Decimal, i32) =
        sqlx::query_as("select labor_rate_per_hour, usage_window_days from tenants where id = $1")
            .bind(auth.tenant_id)
            .fetch_one(&mut *tx)
            .await?;

    tx.commit().await?;

    Ok(Json(SettingsView {
        labor_rate_per_hour,
        usage_window_days,
    }))
}

pub async fn update_settings(
    State(state): State<AppState>,
    guard: Require<CanManageTenant>,
    Json(body): Json<SettingsUpdate>,
) -> Result<Json<SettingsView>, ApiError> {
    let auth = guard.auth;

    let rate = match body.labor_rate_per_hour {
        Some(value) if value.is_sign_negative() => {
            return Err(ApiError::BadRequest("invalid_labor_rate"))
        }
        Some(value) => {
            let mut value = value;
            value.rescale(MONEY_SCALE);
            Some(value)
        }
        None => None,
    };

    if let Some(days) = body.usage_window_days {
        if !(MIN_WINDOW_DAYS..=MAX_WINDOW_DAYS).contains(&days) {
            return Err(ApiError::BadRequest("invalid_usage_window"));
        }
    }

    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let (labor_rate_per_hour, usage_window_days): (Decimal, i32) = sqlx::query_as(
        "update tenants
         set labor_rate_per_hour = coalesce($2, labor_rate_per_hour),
             usage_window_days = coalesce($3, usage_window_days),
             updated_at = now()
         where id = $1
         returning labor_rate_per_hour, usage_window_days",
    )
    .bind(auth.tenant_id)
    .bind(rate)
    .bind(body.usage_window_days)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(Json(SettingsView {
        labor_rate_per_hour,
        usage_window_days,
    }))
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/settings", get(get_settings).put(update_settings))
}
