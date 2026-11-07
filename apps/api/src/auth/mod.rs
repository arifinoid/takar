pub mod contact;
pub mod extract;
pub mod guard;
pub mod handlers;
pub mod jwt;
pub mod members;
pub mod otp;
pub mod secret;

use axum::{
    routing::{delete, get, patch, post},
    Router,
};

use crate::AppState;

pub use extract::{AuthContext, TenantSelection, UserIdentity};
pub use jwt::{JwtConfig, TokenScope};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/otp/request", post(handlers::request_otp))
        .route("/auth/otp/verify", post(handlers::verify_otp))
        .route("/auth/tenants", get(handlers::list_tenants))
        .route("/auth/select-tenant", post(handlers::select_tenant))
        // Pre-tenant: the invitee holds a token, not a tenant.
        .route("/auth/accept-invitation", post(members::accept_invitation))
        .route("/me", get(handlers::me))
        .route("/members", get(members::list_members))
        .route("/members/{user_id}", delete(members::remove_member))
        .route("/members/{user_id}/role", patch(members::change_role))
        .route("/members/{user_id}/status", patch(members::change_status))
        .route(
            "/members/invitations",
            get(members::list_invitations).post(members::create_invitation),
        )
        .route(
            "/members/invitations/{id}",
            delete(members::revoke_invitation),
        )
}
