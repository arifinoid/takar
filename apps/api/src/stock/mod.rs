//! Stock routes: batches and, later, on-hand queries and movements.

pub mod batches;

use axum::{routing::post, Router};

use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/stock/batches", post(batches::stock_in))
}
