//! Onboarding: starter templates and spreadsheet import.
//!
//! Both paths end in the same place — ordinary materials in the tenant — so
//! the rest of the product never has to know where a row came from.

pub mod import;
pub mod templates;

use axum::{
    extract::DefaultBodyLimit,
    routing::{get, post},
    Router,
};
use serde::Serialize;

use crate::AppState;

/// Upper bound from D1: onboarding is available on every plan, with a row cap.
pub const MAX_IMPORT_ROWS: usize = 500;

#[derive(Debug, Serialize)]
pub struct RowError {
    /// Spreadsheet row number, so the message points at what the user sees.
    pub row: usize,
    pub code: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ImportResult {
    pub created: i64,
    pub skipped: i64,
    pub errors: Vec<RowError>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/onboarding/templates", get(templates::list_templates))
        .route(
            "/onboarding/templates/{id}/apply",
            post(templates::apply_template),
        )
        .route(
            "/onboarding/import",
            post(import::import_materials).layer(DefaultBodyLimit::max(5 * 1024 * 1024)),
        )
}
