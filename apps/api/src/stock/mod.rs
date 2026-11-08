//! Stock routes: batch stock-in and the reads that explain the ledger.

pub mod batches;
pub mod corrections;
pub mod queries;

use axum::{
    routing::{get, post},
    Router,
};

use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/stock", get(queries::on_hand))
        .route(
            "/stock/batches",
            get(queries::list_batches).post(batches::stock_in),
        )
        .route("/stock/movements", get(queries::list_movements))
        .route("/stock/corrections", post(corrections::correct))
}
