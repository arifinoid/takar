//! Starter catalogs for a new tenant.
//!
//! Reference data, like the unit list: the product owns it, the tenant applies
//! it. Items are seeded as ordinary materials, so everything downstream treats
//! them the same as hand-typed ones.

use axum::{
    extract::{Path, State},
    Json,
};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::{Postgres, Transaction};
use std::collections::HashSet;
use std::str::FromStr;
use uuid::Uuid;

use crate::auth::guard::{CanReadMaterials, CanWriteMaterials, Require};
use crate::error::ApiError;
use crate::AppState;

use super::ImportResult;

pub struct TemplateItem {
    pub name: &'static str,
    pub unit: &'static str,
    pub min_stock: &'static str,
    pub shelf_life_days: Option<i32>,
}

pub struct Template {
    pub id: &'static str,
    pub items: &'static [TemplateItem],
}

const fn item(
    name: &'static str,
    unit: &'static str,
    min_stock: &'static str,
    shelf_life_days: Option<i32>,
) -> TemplateItem {
    TemplateItem {
        name,
        unit,
        min_stock,
        shelf_life_days,
    }
}

const BAKERY: &[TemplateItem] = &[
    item("Tepung terigu protein tinggi", "kg", "5", Some(180)),
    item("Tepung terigu protein sedang", "kg", "5", Some(180)),
    item("Tepung maizena", "kg", "1", Some(365)),
    item("Gula pasir", "kg", "3", Some(730)),
    item("Gula halus", "kg", "1", Some(365)),
    item("Mentega tawar", "kg", "2", Some(90)),
    item("Margarin", "kg", "1", Some(120)),
    item("Telur ayam", "kg", "5", Some(21)),
    item("Susu UHT", "l", "4", Some(120)),
    item("Krim kental manis", "pcs", "6", Some(365)),
    item("Ragi instan", "g", "500", Some(365)),
    item("Baking powder", "g", "500", Some(365)),
    item("Baking soda", "g", "250", Some(730)),
    item("Garam", "kg", "1", Some(1825)),
    item("Cokelat compound", "kg", "2", Some(365)),
    item("Keju cheddar", "kg", "1", Some(180)),
    item("Vanili", "g", "100", Some(730)),
    item("Pewarna makanan", "ml", "200", Some(730)),
    item("Kertas roti", "pcs", "50", None),
    item("Box kue", "pcs", "100", None),
];

const COFFEE: &[TemplateItem] = &[
    item("Biji kopi arabika", "kg", "3", Some(180)),
    item("Biji kopi robusta", "kg", "2", Some(180)),
    item("Susu UHT full cream", "l", "12", Some(120)),
    item("Susu kental manis", "pcs", "12", Some(365)),
    item("Gula aren cair", "l", "2", Some(90)),
    item("Gula pasir", "kg", "5", Some(730)),
    item("Sirup vanila", "l", "2", Some(365)),
    item("Sirup karamel", "l", "2", Some(365)),
    item("Bubuk matcha", "kg", "1", Some(180)),
    item("Bubuk cokelat", "kg", "2", Some(365)),
    item("Teh celup", "pcs", "200", Some(365)),
    item("Es batu", "kg", "10", None),
    item("Cup plastik 16 oz", "pcs", "500", None),
    item("Lid cup", "pcs", "500", None),
    item("Sedotan", "pcs", "500", None),
    item("Tisu", "pcs", "20", None),
];

const RICE_BOWL: &[TemplateItem] = &[
    item("Beras", "kg", "25", Some(365)),
    item("Ayam fillet", "kg", "10", Some(7)),
    item("Daging sapi giling", "kg", "5", Some(7)),
    item("Telur ayam", "kg", "5", Some(21)),
    item("Tahu", "pcs", "30", Some(3)),
    item("Tempe", "pcs", "30", Some(3)),
    item("Bawang merah", "kg", "3", Some(30)),
    item("Bawang putih", "kg", "3", Some(60)),
    item("Cabai merah", "kg", "2", Some(7)),
    item("Cabai rawit", "kg", "1", Some(7)),
    item("Kecap manis", "l", "3", Some(365)),
    item("Saus tiram", "l", "2", Some(365)),
    item("Minyak goreng", "l", "10", Some(180)),
    item("Garam", "kg", "2", Some(1825)),
    item("Gula pasir", "kg", "3", Some(730)),
    item("Merica", "kg", "1", Some(730)),
    item("Daun bawang", "kg", "1", Some(7)),
    item("Sawi", "kg", "3", Some(5)),
    item("Box makanan", "pcs", "200", None),
    item("Sendok plastik", "pcs", "200", None),
];

const DESSERT: &[TemplateItem] = &[
    item("Susu UHT", "l", "6", Some(120)),
    item("Krim kental manis", "pcs", "12", Some(365)),
    item("Gula pasir", "kg", "3", Some(730)),
    item("Agar-agar bubuk", "pcs", "20", Some(730)),
    item("Bubuk jelly", "pcs", "20", Some(730)),
    item("Buah kalengan", "pcs", "12", Some(365)),
    item("Nata de coco", "pcs", "12", Some(365)),
    item("Sirup markisa", "l", "2", Some(365)),
    item("Es batu", "kg", "10", None),
    item("Cup dessert", "pcs", "300", None),
    item("Sendok plastik", "pcs", "300", None),
    item("Pewarna makanan", "ml", "200", Some(730)),
    item("Vanili", "g", "100", Some(730)),
    item("Tepung maizena", "kg", "1", Some(365)),
    item("Telur ayam", "kg", "3", Some(21)),
    item("Keju cheddar", "kg", "1", Some(180)),
];

pub const TEMPLATES: &[Template] = &[
    Template {
        id: "bakery",
        items: BAKERY,
    },
    Template {
        id: "coffee",
        items: COFFEE,
    },
    Template {
        id: "rice_bowl",
        items: RICE_BOWL,
    },
    Template {
        id: "dessert",
        items: DESSERT,
    },
];

#[derive(Debug, Serialize)]
pub struct TemplateItemView {
    pub name: &'static str,
    pub unit: &'static str,
    pub min_stock: &'static str,
    pub shelf_life_days: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct TemplateView {
    pub id: &'static str,
    pub item_count: usize,
    pub items: Vec<TemplateItemView>,
}

pub async fn list_templates(_guard: Require<CanReadMaterials>) -> Json<Vec<TemplateView>> {
    Json(
        TEMPLATES
            .iter()
            .map(|template| TemplateView {
                id: template.id,
                item_count: template.items.len(),
                items: template
                    .items
                    .iter()
                    .map(|item| TemplateItemView {
                        name: item.name,
                        unit: item.unit,
                        min_stock: item.min_stock,
                        shelf_life_days: item.shelf_life_days,
                    })
                    .collect(),
            })
            .collect(),
    )
}

/// Unit ids by code. Only system units and the tenant's own are visible.
pub async fn unit_ids(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
) -> Result<std::collections::HashMap<String, Uuid>, ApiError> {
    let rows: Vec<(Uuid, String)> =
        sqlx::query_as("select id, code from units where tenant_id is null or tenant_id = $1")
            .bind(tenant_id)
            .fetch_all(&mut **tx)
            .await?;

    Ok(rows.into_iter().map(|(id, code)| (code, id)).collect())
}

/// Lower-cased names already in use, so an import can skip instead of fail.
pub async fn existing_names(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
) -> Result<HashSet<String>, ApiError> {
    let rows: Vec<(String,)> =
        sqlx::query_as("select lower(name) from materials where tenant_id = $1")
            .bind(tenant_id)
            .fetch_all(&mut **tx)
            .await?;

    Ok(rows.into_iter().map(|(name,)| name).collect())
}

pub async fn apply_template(
    State(state): State<AppState>,
    guard: Require<CanWriteMaterials>,
    Path(id): Path<String>,
) -> Result<Json<ImportResult>, ApiError> {
    let auth = guard.auth;
    let template = TEMPLATES
        .iter()
        .find(|template| template.id == id)
        .ok_or(ApiError::NotFound("template_not_found"))?;

    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let units = unit_ids(&mut tx, auth.tenant_id).await?;
    let mut taken = existing_names(&mut tx, auth.tenant_id).await?;

    let mut created = 0_i64;
    let mut skipped = 0_i64;

    for item in template.items {
        let key = item.name.to_lowercase();

        if taken.contains(&key) {
            skipped += 1;
            continue;
        }

        let unit_id = *units
            .get(item.unit)
            .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("missing unit {}", item.unit)))?;

        sqlx::query(
            "insert into materials (tenant_id, name, base_unit_id, min_stock, shelf_life_days)
             values ($1, $2, $3, $4, $5)",
        )
        .bind(auth.tenant_id)
        .bind(item.name)
        .bind(unit_id)
        .bind(Decimal::from_str(item.min_stock).unwrap_or_default())
        .bind(item.shelf_life_days)
        .execute(&mut *tx)
        .await?;

        taken.insert(key);
        created += 1;
    }

    tx.commit().await?;

    Ok(Json(ImportResult {
        created,
        skipped,
        errors: Vec::new(),
    }))
}
