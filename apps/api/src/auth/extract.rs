use axum::{
    extract::FromRequestParts,
    http::{header::AUTHORIZATION, request::Parts},
};
use sqlx::PgPool;
use takar_domain::Role;
use uuid::Uuid;

use super::jwt::TokenScope;
use crate::{error::ApiError, AppState};

pub struct AuthContext {
    pub user_id: Uuid,
    pub tenant_id: Uuid,
    pub role: Role,
}

pub struct TenantSelection {
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

async fn load_role(pool: &PgPool, tenant_id: Uuid, user_id: Uuid) -> Result<Role, ApiError> {
    let mut tx = takar_db::begin_scoped(pool, Some(tenant_id), Some(user_id)).await?;

    let row: Option<(String,)> = sqlx::query_as(
        "select role from memberships
         where tenant_id = $1 and user_id = $2 and status = 'active'",
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;

    tx.commit().await?;

    let (role,) = row.ok_or(ApiError::Unauthorized("membership_inactive"))?;
    role.parse()
        .map_err(|_| ApiError::Unauthorized("invalid_role"))
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

        let role = load_role(state.db()?, tenant_id, claims.sub).await?;

        Ok(Self {
            user_id: claims.sub,
            tenant_id,
            role,
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
