//! Finished products.
//!
//! Products share the recipe permissions: in this phase a product exists to
//! own a recipe, and the RBAC matrix has no separate product permission.
//! `current_recipe_id` and the production gate belong to P2-02.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, put},
    Json, Router,
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::auth::guard::{CanReadRecipes, CanWriteRecipes, Require};
use crate::{error::ApiError, AppState};

#[derive(Debug, Serialize)]
pub struct ProductView {
    pub id: Uuid,
    pub name: String,
    pub sku: Option<String>,
    pub sell_price: Decimal,
    pub stock_qty: Decimal,
    /// Null until the product has a recipe; a product without one cannot be
    /// produced.
    pub current_recipe_id: Option<Uuid>,
    pub current_recipe_version: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct ProductBody {
    pub name: String,
    pub sku: Option<String>,
    pub sell_price: Option<Decimal>,
}

type ProductRow = (
    Uuid,
    String,
    Option<String>,
    Decimal,
    Decimal,
    Option<Uuid>,
    Option<i32>,
    DateTime<Utc>,
    DateTime<Utc>,
);

const SELECT: &str = "select p.id, p.name, p.sku, p.sell_price, p.stock_qty,
            p.current_recipe_id, r.version, p.created_at, p.updated_at
     from products p
     left join recipes r on r.id = p.current_recipe_id";

fn view(row: ProductRow) -> ProductView {
    let (
        id,
        name,
        sku,
        sell_price,
        stock_qty,
        current_recipe_id,
        current_recipe_version,
        created_at,
        updated_at,
    ) = row;

    ProductView {
        id,
        name,
        sku,
        sell_price,
        stock_qty,
        current_recipe_id,
        current_recipe_version,
        created_at,
        updated_at,
    }
}

fn validate(body: &ProductBody) -> Result<(&str, Option<String>, Decimal), ApiError> {
    let name = body.name.trim();

    if name.is_empty() {
        return Err(ApiError::BadRequest("name_required"));
    }

    let sku = body
        .sku
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let sell_price = body.sell_price.unwrap_or(Decimal::ZERO);

    if sell_price.is_sign_negative() {
        return Err(ApiError::BadRequest("invalid_sell_price"));
    }

    Ok((name, sku, sell_price))
}

async fn fetch_product(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    id: Uuid,
) -> Result<ProductView, ApiError> {
    let row: Option<ProductRow> = sqlx::query_as(&format!(
        "{SELECT} where p.tenant_id = $1 and p.id = $2 and p.is_active"
    ))
    .bind(tenant_id)
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?;

    row.map(view).ok_or(ApiError::NotFound("product_not_found"))
}

pub async fn list_products(
    State(state): State<AppState>,
    guard: Require<CanReadRecipes>,
) -> Result<Json<Vec<ProductView>>, ApiError> {
    let auth = guard.auth;
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let rows: Vec<ProductRow> = sqlx::query_as(&format!(
        "{SELECT} where p.tenant_id = $1 and p.is_active order by p.name"
    ))
    .bind(auth.tenant_id)
    .fetch_all(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(Json(rows.into_iter().map(view).collect()))
}

pub async fn create_product(
    State(state): State<AppState>,
    guard: Require<CanWriteRecipes>,
    Json(body): Json<ProductBody>,
) -> Result<(StatusCode, Json<ProductView>), ApiError> {
    let auth = guard.auth;
    let (name, sku, sell_price) = validate(&body)?;
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let (id,): (Uuid,) = sqlx::query_as(
        "insert into products (tenant_id, name, sku, sell_price)
         values ($1, $2, $3, $4)
         returning id",
    )
    .bind(auth.tenant_id)
    .bind(name)
    .bind(&sku)
    .bind(sell_price)
    .fetch_one(&mut *tx)
    .await
    .map_err(|error| ApiError::from_write(error, "product_exists"))?;

    let product = fetch_product(&mut tx, auth.tenant_id, id).await?;
    tx.commit().await?;

    Ok((StatusCode::CREATED, Json(product)))
}

pub async fn update_product(
    State(state): State<AppState>,
    guard: Require<CanWriteRecipes>,
    Path(id): Path<Uuid>,
    Json(body): Json<ProductBody>,
) -> Result<Json<ProductView>, ApiError> {
    let auth = guard.auth;
    let (name, sku, sell_price) = validate(&body)?;
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let result = sqlx::query(
        "update products
         set name = $3, sku = $4, sell_price = $5, updated_at = now()
         where id = $1 and tenant_id = $2 and is_active",
    )
    .bind(id)
    .bind(auth.tenant_id)
    .bind(name)
    .bind(&sku)
    .bind(sell_price)
    .execute(&mut *tx)
    .await
    .map_err(|error| ApiError::from_write(error, "product_exists"))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("product_not_found"));
    }

    let product = fetch_product(&mut tx, auth.tenant_id, id).await?;
    tx.commit().await?;

    Ok(Json(product))
}

pub async fn remove_product(
    State(state): State<AppState>,
    guard: Require<CanWriteRecipes>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let auth = guard.auth;
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let result = sqlx::query(
        "update products set is_active = false, updated_at = now()
         where id = $1 and tenant_id = $2 and is_active",
    )
    .bind(id)
    .bind(auth.tenant_id)
    .execute(&mut *tx)
    .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("product_not_found"));
    }

    tx.commit().await?;

    Ok(StatusCode::NO_CONTENT)
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/products", get(list_products).post(create_product))
        .route("/products/{id}", put(update_product).delete(remove_product))
}
