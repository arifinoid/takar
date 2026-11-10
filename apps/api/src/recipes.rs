//! Versioned recipes.
//!
//! A recipe is never edited in place. Creating a version deactivates the
//! previous one, and every version keeps its own items, so a production order
//! that points at version 2 still shows what version 2 actually consumed after
//! the shop moves to version 3.

use std::collections::{HashMap, HashSet};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
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
pub struct RecipeView {
    pub id: Uuid,
    pub product_id: Uuid,
    pub version: i32,
    pub yield_qty: Decimal,
    pub labor_minutes: i32,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub items: Vec<RecipeItemView>,
}

#[derive(Debug, Serialize)]
pub struct RecipeItemView {
    pub material_id: Uuid,
    pub material_name: String,
    pub unit_code: String,
    pub qty: Decimal,
    pub waste_pct: Decimal,
}

#[derive(Debug, Deserialize)]
pub struct RecipeBody {
    pub yield_qty: Decimal,
    pub labor_minutes: Option<i32>,
    pub items: Vec<RecipeItemBody>,
}

#[derive(Debug, Deserialize)]
pub struct RecipeItemBody {
    pub material_id: Uuid,
    pub qty: Decimal,
    pub waste_pct: Option<Decimal>,
}

type RecipeRow = (Uuid, Uuid, i32, Decimal, i32, bool, DateTime<Utc>);
type ItemRow = (Uuid, Uuid, String, String, Decimal, Decimal);
type ValidatedItem = (Uuid, Decimal, Decimal);
type ValidatedRecipe = (Decimal, i32, Vec<ValidatedItem>);

const SELECT: &str =
    "select id, product_id, version, yield_qty, labor_minutes, is_active, created_at
     from recipes";

fn recipe_view(row: RecipeRow, items: Vec<RecipeItemView>) -> RecipeView {
    let (id, product_id, version, yield_qty, labor_minutes, is_active, created_at) = row;

    RecipeView {
        id,
        product_id,
        version,
        yield_qty,
        labor_minutes,
        is_active,
        created_at,
        items,
    }
}

async fn load_items(
    tx: &mut Transaction<'_, Postgres>,
    recipe_ids: &[Uuid],
) -> Result<HashMap<Uuid, Vec<RecipeItemView>>, ApiError> {
    let rows: Vec<ItemRow> = sqlx::query_as(
        "select ri.recipe_id, ri.material_id, m.name, u.code, ri.qty, ri.waste_pct
         from recipe_items ri
         join materials m on m.id = ri.material_id
         join units u on u.id = m.base_unit_id
         where ri.recipe_id = any($1)
         order by m.name",
    )
    .bind(recipe_ids)
    .fetch_all(&mut **tx)
    .await?;

    let mut grouped: HashMap<Uuid, Vec<RecipeItemView>> = HashMap::new();

    for (recipe_id, material_id, material_name, unit_code, qty, waste_pct) in rows {
        grouped.entry(recipe_id).or_default().push(RecipeItemView {
            material_id,
            material_name,
            unit_code,
            qty,
            waste_pct,
        });
    }

    Ok(grouped)
}

async fn fetch_recipe(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    id: Uuid,
) -> Result<RecipeView, ApiError> {
    let row: Option<RecipeRow> =
        sqlx::query_as(&format!("{SELECT} where tenant_id = $1 and id = $2"))
            .bind(tenant_id)
            .bind(id)
            .fetch_optional(&mut **tx)
            .await?;

    let row = row.ok_or(ApiError::NotFound("recipe_not_found"))?;
    let mut items = load_items(tx, &[row.0]).await?;

    Ok(recipe_view(row, items.remove(&id).unwrap_or_default()))
}

async fn ensure_product(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    product_id: Uuid,
) -> Result<(), ApiError> {
    let row: Option<(Uuid,)> = sqlx::query_as(
        "select id from products where id = $1 and tenant_id = $2 and is_active for update",
    )
    .bind(product_id)
    .bind(tenant_id)
    .fetch_optional(&mut **tx)
    .await?;

    row.map(|_| ())
        .ok_or(ApiError::NotFound("product_not_found"))
}

fn validate(body: &RecipeBody) -> Result<ValidatedRecipe, ApiError> {
    if body.yield_qty <= Decimal::ZERO {
        return Err(ApiError::BadRequest("invalid_yield"));
    }

    let labor_minutes = body.labor_minutes.unwrap_or(0);

    if labor_minutes < 0 {
        return Err(ApiError::BadRequest("invalid_labor_minutes"));
    }

    if body.items.is_empty() {
        return Err(ApiError::BadRequest("recipe_items_required"));
    }

    let mut seen = HashSet::new();
    let mut items = Vec::with_capacity(body.items.len());

    for item in &body.items {
        if item.qty <= Decimal::ZERO {
            return Err(ApiError::BadRequest("invalid_qty"));
        }

        let waste_pct = item.waste_pct.unwrap_or(Decimal::ZERO);

        if waste_pct.is_sign_negative() || waste_pct > Decimal::from(100) {
            return Err(ApiError::BadRequest("invalid_waste"));
        }

        if !seen.insert(item.material_id) {
            return Err(ApiError::BadRequest("duplicate_material"));
        }

        items.push((item.material_id, item.qty, waste_pct));
    }

    Ok((body.yield_qty, labor_minutes, items))
}

async fn ensure_materials(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    items: &[ValidatedItem],
) -> Result<(), ApiError> {
    let ids: Vec<Uuid> = items.iter().map(|(id, _, _)| *id).collect();

    let (found,): (i64,) = sqlx::query_as(
        "select count(*) from materials
         where tenant_id = $1 and is_active and id = any($2)",
    )
    .bind(tenant_id)
    .bind(&ids)
    .fetch_one(&mut **tx)
    .await?;

    if found as usize != ids.len() {
        return Err(ApiError::BadRequest("invalid_material"));
    }

    Ok(())
}

pub async fn list_recipes(
    State(state): State<AppState>,
    guard: Require<CanReadRecipes>,
    Path(product_id): Path<Uuid>,
) -> Result<Json<Vec<RecipeView>>, ApiError> {
    let auth = guard.auth;
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    ensure_product(&mut tx, auth.tenant_id, product_id).await?;

    let rows: Vec<RecipeRow> = sqlx::query_as(&format!(
        "{SELECT} where tenant_id = $1 and product_id = $2 order by version desc"
    ))
    .bind(auth.tenant_id)
    .bind(product_id)
    .fetch_all(&mut *tx)
    .await?;

    let ids: Vec<Uuid> = rows.iter().map(|row| row.0).collect();
    let mut items = load_items(&mut tx, &ids).await?;

    tx.commit().await?;

    Ok(Json(
        rows.into_iter()
            .map(|row| {
                let id = row.0;
                recipe_view(row, items.remove(&id).unwrap_or_default())
            })
            .collect(),
    ))
}

pub async fn get_recipe(
    State(state): State<AppState>,
    guard: Require<CanReadRecipes>,
    Path(id): Path<Uuid>,
) -> Result<Json<RecipeView>, ApiError> {
    let auth = guard.auth;
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let recipe = fetch_recipe(&mut tx, auth.tenant_id, id).await?;
    tx.commit().await?;

    Ok(Json(recipe))
}

pub async fn create_recipe(
    State(state): State<AppState>,
    guard: Require<CanWriteRecipes>,
    Path(product_id): Path<Uuid>,
    Json(body): Json<RecipeBody>,
) -> Result<(StatusCode, Json<RecipeView>), ApiError> {
    let auth = guard.auth;
    let (yield_qty, labor_minutes, items) = validate(&body)?;
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    ensure_product(&mut tx, auth.tenant_id, product_id).await?;
    ensure_materials(&mut tx, auth.tenant_id, &items).await?;

    let (version,): (i32,) =
        sqlx::query_as("select coalesce(max(version), 0) + 1 from recipes where product_id = $1")
            .bind(product_id)
            .fetch_one(&mut *tx)
            .await?;

    sqlx::query("update recipes set is_active = false where product_id = $1 and is_active")
        .bind(product_id)
        .execute(&mut *tx)
        .await?;

    let (id,): (Uuid,) = sqlx::query_as(
        "insert into recipes
             (tenant_id, product_id, version, yield_qty, labor_minutes, created_by)
         values ($1, $2, $3, $4, $5, $6)
         returning id",
    )
    .bind(auth.tenant_id)
    .bind(product_id)
    .bind(version)
    .bind(yield_qty)
    .bind(labor_minutes)
    .bind(auth.user_id)
    .fetch_one(&mut *tx)
    .await?;

    for (material_id, qty, waste_pct) in &items {
        sqlx::query(
            "insert into recipe_items (recipe_id, material_id, qty, waste_pct)
             values ($1, $2, $3, $4)",
        )
        .bind(id)
        .bind(material_id)
        .bind(qty)
        .bind(waste_pct)
        .execute(&mut *tx)
        .await?;
    }

    let recipe = fetch_recipe(&mut tx, auth.tenant_id, id).await?;
    tx.commit().await?;

    Ok((StatusCode::CREATED, Json(recipe)))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/products/{product_id}/recipes",
            get(list_recipes).post(create_recipe),
        )
        .route("/recipes/{id}", get(get_recipe))
}
