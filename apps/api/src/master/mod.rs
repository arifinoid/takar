//! Master data routes: units, suppliers, and materials.

pub mod materials;
pub mod suppliers;
pub mod units;

use axum::{
    routing::{get, put},
    Router,
};

use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/units", get(units::list_units))
        .route(
            "/suppliers",
            get(suppliers::list_suppliers).post(suppliers::create_supplier),
        )
        .route(
            "/suppliers/{id}",
            put(suppliers::update_supplier).delete(suppliers::remove_supplier),
        )
        .route(
            "/materials",
            get(materials::list_materials).post(materials::create_material),
        )
        .route(
            "/materials/{id}",
            put(materials::update_material).delete(materials::remove_material),
        )
}
