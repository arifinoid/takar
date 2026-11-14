use axum::{
    extract::FromRequestParts,
    http::{header::AUTHORIZATION, request::Parts, Method},
};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use takar_domain::billing::{access as access_state, Access};
use takar_domain::Role;
use uuid::Uuid;

use super::jwt::TokenScope;
use crate::{error::ApiError, AppState};

pub struct AuthContext {
    pub user_id: Uuid,
    pub tenant_id: Uuid,
    pub role: Role,
    /// Whether the tenant may still write. Derived from its dates on every
    /// request, so a payment or a support extension applies immediately.
    pub access: Access,
}

pub struct TenantSelection {
    pub user_id: Uuid,
}

/// Any authenticated user, whether or not a tenant is active yet.
///
/// Invitation acceptance needs this: an invitee may hold a tenant-selection
/// token (no membership yet) or a session token (already in another tenant).
pub struct UserIdentity {
    pub user_id: Uuid,
}

fn bearer(parts: &Parts) -> Result<&str, ApiError> {
    let header = parts
        .headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or(ApiError::Unauthorized("missing_token"))?;

    header
        .strip_prefix("Bearer ")
        .ok_or(ApiError::Unauthorized("invalid_token"))
}

struct Session {
    role: Role,
    access: Access,
}

#[derive(sqlx::FromRow)]
struct SessionRow {
    role: String,
    trial_ends_at: Option<DateTime<Utc>>,
    paid_until: Option<DateTime<Utc>>,
}

/// Role and access in one round trip: both hang off the same membership row.
async fn load_session(pool: &PgPool, tenant_id: Uuid, user_id: Uuid) -> Result<Session, ApiError> {
    let mut tx = takar_db::begin_scoped(pool, Some(tenant_id), Some(user_id)).await?;

    let row: Option<SessionRow> = sqlx::query_as(
        "select m.role, t.trial_ends_at, t.paid_until
         from memberships m
         join tenants t on t.id = m.tenant_id
         where m.tenant_id = $1 and m.user_id = $2 and m.status = 'active'",
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;

    tx.commit().await?;

    let row = row.ok_or(ApiError::Unauthorized("membership_inactive"))?;

    Ok(Session {
        role: row
            .role
            .parse()
            .map_err(|_| ApiError::Unauthorized("invalid_role"))?,
        access: access_state(row.trial_ends_at, row.paid_until, Utc::now()),
    })
}

/// Reads stay open in read-only mode; only mutations are refused.
fn is_write(method: &Method) -> bool {
    !matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}

/// Recording a payment is the way out of read-only mode, so the billing routes
/// keep accepting writes when everything else is closed.
fn is_billing(path: &str) -> bool {
    path == "/billing" || path.starts_with("/billing/")
}

impl FromRequestParts<AppState> for AuthContext {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let claims = state.jwt.decode(bearer(parts)?)?;

        if claims.scope != TokenScope::Session {
            return Err(ApiError::Unauthorized("tenant_required"));
        }

        let tenant_id = claims
            .tenant_id
            .ok_or(ApiError::Unauthorized("tenant_required"))?;

        let session = load_session(state.db()?, tenant_id, claims.sub).await?;

        if !session.access.allows_writes()
            && is_write(&parts.method)
            && !is_billing(parts.uri.path())
        {
            return Err(ApiError::ForbiddenDetails(
                "subscription_expired",
                serde_json::json!({ "access": session.access.as_str() }),
            ));
        }

        Ok(Self {
            user_id: claims.sub,
            tenant_id,
            role: session.role,
            access: session.access,
        })
    }
}

impl FromRequestParts<AppState> for UserIdentity {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let claims = state.jwt.decode(bearer(parts)?)?;

        Ok(Self {
            user_id: claims.sub,
        })
    }
}

impl FromRequestParts<AppState> for TenantSelection {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let claims = state.jwt.decode(bearer(parts)?)?;

        if claims.scope != TokenScope::TenantSelect {
            return Err(ApiError::Unauthorized("tenant_selection_required"));
        }

        Ok(Self {
            user_id: claims.sub,
        })
    }
}
