//! Stock routes: batch stock-in and the reads that explain the ledger.

pub mod batches;
pub mod queries;

use axum::{routing::get, Router};

use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/stock", get(queries::on_hand))
        .route(
            "/stock/batches",
            get(queries::list_batches).post(batches::stock_in),
        )
        .route("/stock/movements", get(queries::list_movements))
}
