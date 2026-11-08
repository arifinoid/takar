//! HTTP layer for Takar. This is the only place that owns server behavior.

pub mod auth;
pub mod config;
pub mod dashboard;
pub mod error;
pub mod master;
pub mod stock;

use std::sync::Arc;

use axum::{extract::State, http::HeaderValue, routing::get, Json, Router};
use sqlx::PgPool;
use takar_domain::HealthResponse;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use utoipa::OpenApi;

use auth::otp::OtpSender;
use error::ApiError;

#[derive(Clone)]
pub struct AppState {
    /// Present only when `DATABASE_URL` is set and reachable at boot.
    pub db: Option<PgPool>,
    pub jwt: auth::JwtConfig,
    pub otp: Arc<dyn OtpSender>,
    pub otp_ttl_seconds: i64,
    pub jwt_ttl_seconds: i64,
    /// Fixed OTP for local sign-in. Ignored by release builds.
    pub dev_otp_code: Option<String>,
}

impl AppState {
    pub fn db(&self) -> Result<&PgPool, ApiError> {
        self.db.as_ref().ok_or(ApiError::ServiceUnavailable)
    }
}

#[derive(OpenApi)]
#[openapi(paths(health, openapi), components(schemas(HealthResponse)))]
pub struct ApiDoc;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(health))
        .route("/api-docs/openapi.json", get(openapi))
        .route("/dashboard", get(dashboard::overview))
        .merge(auth::routes())
        .merge(master::routes())
        .merge(stock::routes())
        .with_state(state)
        .layer(cors())
        .layer(TraceLayer::new_for_http())
}

#[utoipa::path(
    get,
    path = "/healthz",
    responses((status = 200, body = HealthResponse))
)]
async fn health(State(_state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse::ok())
}

#[utoipa::path(get, path = "/api-docs/openapi.json", responses((status = 200)))]
async fn openapi() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

fn cors() -> CorsLayer {
    let origins: Vec<HeaderValue> = [
        "http://localhost:3000",
        "http://127.0.0.1:3000",
        "https://takar.arifinoid.web.id",
        "http://tauri.localhost",
        "tauri://localhost",
    ]
    .iter()
    .filter_map(|origin| origin.parse().ok())
    .collect();

    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods(Any)
        .allow_headers(Any)
}
