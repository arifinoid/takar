//! Supplier master data.
//!
//! Suppliers belong to the material master, so they share the material
//! permissions rather than introducing a separate one.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::guard::{CanReadMaterials, CanWriteMaterials, Require};
use crate::{error::ApiError, AppState};

#[derive(Debug, Serialize)]
pub struct SupplierView {
    pub id: Uuid,
    pub name: String,
    pub phone: Option<String>,
    pub lead_time_days: i32,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct SupplierBody {
    pub name: String,
    pub phone: Option<String>,
    pub lead_time_days: Option<i32>,
    pub notes: Option<String>,
}

type SupplierRow = (
    Uuid,
    String,
    Option<String>,
    i32,
    Option<String>,
    DateTime<Utc>,
    DateTime<Utc>,
);

const SELECT: &str = "select id, name, phone, lead_time_days, notes, created_at, updated_at";

fn view(row: SupplierRow) -> SupplierView {
    let (id, name, phone, lead_time_days, notes, created_at, updated_at) = row;

    SupplierView {
        id,
        name,
        phone,
        lead_time_days,
        notes,
        created_at,
        updated_at,
    }
}

fn validate(body: &SupplierBody) -> Result<(&str, i32), ApiError> {
    let name = body.name.trim();

    if name.is_empty() {
        return Err(ApiError::BadRequest("name_required"));
    }

    let lead_time_days = body.lead_time_days.unwrap_or(2);

    if lead_time_days < 0 {
        return Err(ApiError::BadRequest("invalid_lead_time"));
    }

    Ok((name, lead_time_days))
}

pub async fn list_suppliers(
    State(state): State<AppState>,
    guard: Require<CanReadMaterials>,
) -> Result<Json<Vec<SupplierView>>, ApiError> {
    let auth = guard.auth;
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let rows: Vec<SupplierRow> = sqlx::query_as(&format!(
        "{SELECT} from suppliers where tenant_id = $1 and is_active order by name"
    ))
    .bind(auth.tenant_id)
    .fetch_all(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(Json(rows.into_iter().map(view).collect()))
}

pub async fn create_supplier(
    State(state): State<AppState>,
    guard: Require<CanWriteMaterials>,
    Json(body): Json<SupplierBody>,
) -> Result<(StatusCode, Json<SupplierView>), ApiError> {
    let auth = guard.auth;
    let (name, lead_time_days) = validate(&body)?;
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let row: SupplierRow = sqlx::query_as(
        "insert into suppliers (tenant_id, name, phone, lead_time_days, notes)
         values ($1, $2, $3, $4, $5)
         returning id, name, phone, lead_time_days, notes, created_at, updated_at",
    )
    .bind(auth.tenant_id)
    .bind(name)
    .bind(body.phone.as_deref().map(str::trim))
    .bind(lead_time_days)
    .bind(body.notes.as_deref().map(str::trim))
    .fetch_one(&mut *tx)
    .await
    .map_err(|error| ApiError::from_write(error, "supplier_exists"))?;

    tx.commit().await?;

    Ok((StatusCode::CREATED, Json(view(row))))
}

pub async fn update_supplier(
    State(state): State<AppState>,
    guard: Require<CanWriteMaterials>,
    Path(id): Path<Uuid>,
    Json(body): Json<SupplierBody>,
) -> Result<Json<SupplierView>, ApiError> {
    let auth = guard.auth;
    let (name, lead_time_days) = validate(&body)?;
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let row: Option<SupplierRow> = sqlx::query_as(
        "update suppliers
         set name = $3, phone = $4, lead_time_days = $5, notes = $6, updated_at = now()
         where id = $1 and tenant_id = $2 and is_active
         returning id, name, phone, lead_time_days, notes, created_at, updated_at",
    )
    .bind(id)
    .bind(auth.tenant_id)
    .bind(name)
    .bind(body.phone.as_deref().map(str::trim))
    .bind(lead_time_days)
    .bind(body.notes.as_deref().map(str::trim))
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| ApiError::from_write(error, "supplier_exists"))?;

    let row = row.ok_or(ApiError::NotFound("supplier_not_found"))?;
    tx.commit().await?;

    Ok(Json(view(row)))
}

pub async fn remove_supplier(
    State(state): State<AppState>,
    guard: Require<CanWriteMaterials>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let auth = guard.auth;
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let result = sqlx::query(
        "update suppliers set is_active = false, updated_at = now()
         where id = $1 and tenant_id = $2 and is_active",
    )
    .bind(id)
    .bind(auth.tenant_id)
    .execute(&mut *tx)
    .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("supplier_not_found"));
    }

    tx.commit().await?;

    Ok(StatusCode::NO_CONTENT)
}
