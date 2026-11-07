pub mod contact;
pub mod extract;
pub mod handlers;
pub mod jwt;
pub mod otp;

use axum::{
    routing::{get, post},
    Router,
};

use crate::AppState;

pub use extract::{AuthContext, TenantSelection};
pub use jwt::{JwtConfig, TokenScope};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/otp/request", post(handlers::request_otp))
        .route("/auth/otp/verify", post(handlers::verify_otp))
        .route("/auth/tenants", get(handlers::list_tenants))
        .route("/auth/select-tenant", post(handlers::select_tenant))
        .route("/me", get(handlers::me))
}
