//! Material master data.
//!
//! Stock itself lives in batches; a material only describes what is stocked,
//! in which base unit, and how low it may go.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use super::units::UnitRef;
use crate::auth::guard::{CanReadMaterials, CanWriteMaterials, Require};
use crate::subscription::ensure_capacity;
use crate::{error::ApiError, AppState};

#[derive(Debug, Serialize)]
pub struct MaterialView {
    pub id: Uuid,
    pub name: String,
    pub base_unit: UnitRef,
    pub min_stock: Decimal,
    pub shelf_life_days: Option<i32>,
    pub default_supplier_id: Option<Uuid>,
    pub last_cost: Decimal,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct MaterialBody {
    pub name: String,
    pub base_unit_id: Uuid,
    pub min_stock: Option<Decimal>,
    pub shelf_life_days: Option<i32>,
    pub default_supplier_id: Option<Uuid>,
}

type MaterialRow = (
    Uuid,
    String,
    Uuid,
    String,
    Decimal,
    Option<i32>,
    Option<Uuid>,
    Decimal,
    DateTime<Utc>,
    DateTime<Utc>,
);

const SELECT: &str = "select m.id, m.name, u.id, u.code, m.min_stock, m.shelf_life_days,
            m.default_supplier_id, m.last_cost, m.created_at, m.updated_at
     from materials m
     join units u on u.id = m.base_unit_id";

fn view(row: MaterialRow) -> MaterialView {
    let (
        id,
        name,
        unit_id,
        unit_code,
        min_stock,
        shelf_life_days,
        supplier_id,
        last_cost,
        created_at,
        updated_at,
    ) = row;

    MaterialView {
        id,
        name,
        base_unit: UnitRef {
            id: unit_id,
            code: unit_code,
        },
        min_stock,
        shelf_life_days,
        default_supplier_id: supplier_id,
        last_cost,
        created_at,
        updated_at,
    }
}

async fn fetch_material(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    id: Uuid,
) -> Result<MaterialView, ApiError> {
    let row: Option<MaterialRow> = sqlx::query_as(&format!(
        "{SELECT} where m.tenant_id = $1 and m.id = $2 and m.is_active"
    ))
    .bind(tenant_id)
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?;

    row.map(view)
        .ok_or(ApiError::NotFound("material_not_found"))
}

async fn ensure_supplier(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    supplier_id: Uuid,
) -> Result<(), ApiError> {
    let row: Option<(Uuid,)> =
        sqlx::query_as("select id from suppliers where id = $1 and tenant_id = $2 and is_active")
            .bind(supplier_id)
            .bind(tenant_id)
            .fetch_optional(&mut **tx)
            .await?;

    if row.is_none() {
        return Err(ApiError::BadRequest("invalid_supplier"));
    }

    Ok(())
}

fn validate(body: &MaterialBody) -> Result<(&str, Decimal), ApiError> {
    let name = body.name.trim();

    if name.is_empty() {
        return Err(ApiError::BadRequest("name_required"));
    }

    let min_stock = body.min_stock.unwrap_or(Decimal::ZERO);

    if min_stock.is_sign_negative() {
        return Err(ApiError::BadRequest("invalid_min_stock"));
    }

    if body.shelf_life_days.is_some_and(|days| days < 0) {
        return Err(ApiError::BadRequest("invalid_shelf_life"));
    }

    Ok((name, min_stock))
}

pub async fn list_materials(
    State(state): State<AppState>,
    guard: Require<CanReadMaterials>,
) -> Result<Json<Vec<MaterialView>>, ApiError> {
    let auth = guard.auth;
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let rows: Vec<MaterialRow> = sqlx::query_as(&format!(
        "{SELECT} where m.tenant_id = $1 and m.is_active order by m.name"
    ))
    .bind(auth.tenant_id)
    .fetch_all(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(Json(rows.into_iter().map(view).collect()))
}

pub async fn create_material(
    State(state): State<AppState>,
    guard: Require<CanWriteMaterials>,
    Json(body): Json<MaterialBody>,
) -> Result<(StatusCode, Json<MaterialView>), ApiError> {
    let auth = guard.auth;
    let (name, min_stock) = validate(&body)?;
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    if let Some(supplier_id) = body.default_supplier_id {
        ensure_supplier(&mut tx, auth.tenant_id, supplier_id).await?;
    }

    ensure_capacity(
        &mut tx,
        auth.tenant_id,
        takar_domain::plan::Resource::Materials,
    )
    .await?;

    let (id,): (Uuid,) = sqlx::query_as(
        "insert into materials
             (tenant_id, name, base_unit_id, min_stock, shelf_life_days, default_supplier_id)
         values ($1, $2, $3, $4, $5, $6)
         returning id",
    )
    .bind(auth.tenant_id)
    .bind(name)
    .bind(body.base_unit_id)
    .bind(min_stock)
    .bind(body.shelf_life_days)
    .bind(body.default_supplier_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|error| ApiError::from_write(error, "material_exists"))?;

    let material = fetch_material(&mut tx, auth.tenant_id, id).await?;
    tx.commit().await?;

    Ok((StatusCode::CREATED, Json(material)))
}

pub async fn update_material(
    State(state): State<AppState>,
    guard: Require<CanWriteMaterials>,
    Path(id): Path<Uuid>,
    Json(body): Json<MaterialBody>,
) -> Result<Json<MaterialView>, ApiError> {
    let auth = guard.auth;
    let (name, min_stock) = validate(&body)?;
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    if let Some(supplier_id) = body.default_supplier_id {
        ensure_supplier(&mut tx, auth.tenant_id, supplier_id).await?;
    }

    let result = sqlx::query(
        "update materials
         set name = $3, base_unit_id = $4, min_stock = $5, shelf_life_days = $6,
             default_supplier_id = $7, updated_at = now()
         where id = $1 and tenant_id = $2 and is_active",
    )
    .bind(id)
    .bind(auth.tenant_id)
    .bind(name)
    .bind(body.base_unit_id)
    .bind(min_stock)
    .bind(body.shelf_life_days)
    .bind(body.default_supplier_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| ApiError::from_write(error, "material_exists"))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("material_not_found"));
    }

    let material = fetch_material(&mut tx, auth.tenant_id, id).await?;
    tx.commit().await?;

    Ok(Json(material))
}

pub async fn remove_material(
    State(state): State<AppState>,
    guard: Require<CanWriteMaterials>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let auth = guard.auth;
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let result = sqlx::query(
        "update materials set is_active = false, updated_at = now()
         where id = $1 and tenant_id = $2 and is_active",
    )
    .bind(id)
    .bind(auth.tenant_id)
    .execute(&mut *tx)
    .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("material_not_found"));
    }

    tx.commit().await?;

    Ok(StatusCode::NO_CONTENT)
}
