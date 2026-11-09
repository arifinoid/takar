use axum::{extract::State, http::StatusCode, Json};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    contact::Contact,
    extract::{AuthContext, TenantSelection},
    jwt::TokenScope,
    otp, secret,
};
use crate::{error::ApiError, AppState};

#[derive(Debug, Deserialize)]
pub struct OtpRequestBody {
    pub contact: String,
}

#[derive(Debug, Deserialize)]
pub struct VerifyOtpBody {
    pub contact: String,
    pub code: String,
    pub name: Option<String>,
    pub tenant_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SelectTenantBody {
    pub tenant_id: Uuid,
}

#[derive(Debug, Serialize)]
pub struct TenantSummary {
    pub id: Uuid,
    pub name: String,
    pub role: String,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub access_token: String,
    pub token_type: &'static str,
    pub expires_in: i64,
    pub requires_tenant_selection: bool,
    /// True when this contact has a pending invitation and can accept it with
    /// the returned token before choosing a tenant.
    pub pending_invitation: bool,
    pub tenants: Vec<TenantSummary>,
}

#[derive(Debug, Serialize)]
pub struct UserView {
    pub id: Uuid,
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TenantView {
    pub id: Uuid,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct MeResponse {
    pub user: UserView,
    pub tenant: TenantView,
    pub role: takar_domain::Role,
    pub permissions: Vec<String>,
}

/// Fixed code for local development. Release builds always randomize, so a
/// misconfigured environment cannot turn this into a backdoor.
fn dev_code(state: &AppState) -> Option<String> {
    if cfg!(debug_assertions) {
        state.dev_otp_code.clone()
    } else {
        None
    }
}

/// Codes a single contact may request inside one ten-minute window. Generous
/// enough for a retry or a second device, tight enough that one number cannot
/// burn a free provider quota.
const MAX_CODES_PER_WINDOW: i64 = 5;

pub async fn request_otp(
    State(state): State<AppState>,
    Json(body): Json<OtpRequestBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let contact = Contact::parse(&body.contact)?;
    let db = state.db()?;

    // Free provider quotas are finite, so one contact cannot ask for a new code
    // as often as it likes.
    let recent: (i64,) = sqlx::query_as(
        "select count(*) from otp_codes
         where contact = $1 and created_at > now() - interval '10 minutes'",
    )
    .bind(contact.as_str())
    .fetch_one(db)
    .await?;

    if recent.0 >= MAX_CODES_PER_WINDOW {
        return Err(ApiError::TooManyRequests("rate_limited"));
    }

    let code = dev_code(&state).unwrap_or_else(otp::generate_code);
    let code_hash = secret::hash(&state.jwt.secret, &code);
    let expires_at = Utc::now() + Duration::seconds(state.otp_ttl_seconds);

    sqlx::query("insert into otp_codes (contact, code_hash, expires_at) values ($1, $2, $3)")
        .bind(contact.as_str())
        .bind(code_hash)
        .bind(expires_at)
        .execute(db)
        .await?;

    state
        .otp
        .send(&contact, &code, state.otp_ttl_seconds)
        .await
        .map_err(|error| {
            tracing::error!(%error, "otp delivery failed");
            ApiError::Dependency("otp_delivery_failed")
        })?;

    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({ "expires_in": state.otp_ttl_seconds })),
    ))
}

pub async fn verify_otp(
    State(state): State<AppState>,
    Json(body): Json<VerifyOtpBody>,
) -> Result<Json<AuthResponse>, ApiError> {
    let contact = Contact::parse(&body.contact)?;
    let db = state.db()?;
    let mut tx = db.begin().await?;

    let row: Option<(Uuid, String, i32)> = sqlx::query_as(
        "select id, code_hash, attempts
         from otp_codes
         where contact = $1 and consumed_at is null and expires_at > now()
         order by created_at desc
         limit 1",
    )
    .bind(contact.as_str())
    .fetch_optional(&mut *tx)
    .await?;

    let (otp_id, code_hash, attempts) = row.ok_or(ApiError::Unauthorized("invalid_code"))?;

    if attempts >= 5 {
        return Err(ApiError::TooManyRequests("too_many_attempts"));
    }

    if secret::hash(&state.jwt.secret, &body.code) != code_hash {
        sqlx::query("update otp_codes set attempts = attempts + 1 where id = $1")
            .bind(otp_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        return Err(ApiError::Unauthorized("invalid_code"));
    }

    sqlx::query("update otp_codes set consumed_at = now() where id = $1")
        .bind(otp_id)
        .execute(&mut *tx)
        .await?;

    let (existing_user,): (Option<Uuid>,) = sqlx::query_as("select auth_find_user_id($1)")
        .bind(contact.as_str())
        .fetch_one(&mut *tx)
        .await?;

    let display_name = body.name.unwrap_or_else(|| contact.as_str().to_string());

    let user_id = match existing_user {
        Some(user_id) => user_id,
        None => {
            let user_id = Uuid::new_v4();
            match &contact {
                Contact::Email(email) => {
                    sqlx::query("insert into users (id, email, name) values ($1, $2, $3)")
                        .bind(user_id)
                        .bind(email)
                        .bind(&display_name)
                        .execute(&mut *tx)
                        .await?;
                }
                Contact::Phone(phone) => {
                    sqlx::query("insert into users (id, phone, name) values ($1, $2, $3)")
                        .bind(user_id)
                        .bind(phone)
                        .bind(&display_name)
                        .execute(&mut *tx)
                        .await?;
                }
            }
            user_id
        }
    };

    sqlx::query("select set_config('app.user_id', $1, true)")
        .bind(user_id.to_string())
        .execute(&mut *tx)
        .await?;

    let mut memberships: Vec<(Uuid, String, String)> = sqlx::query_as(
        "select m.tenant_id, t.name, m.role
         from memberships m
         join tenants t on t.id = m.tenant_id
         where m.user_id = $1 and m.status = 'active'
         order by t.name",
    )
    .bind(user_id)
    .fetch_all(&mut *tx)
    .await?;

    let (has_invitation,): (bool,) = sqlx::query_as("select auth_has_pending_invitation($1)")
        .bind(contact.as_str())
        .fetch_one(&mut *tx)
        .await?;

    if memberships.is_empty() && !has_invitation {
        let tenant_id = Uuid::new_v4();
        let tenant_name = body
            .tenant_name
            .unwrap_or_else(|| format!("{display_name}'s workspace"));

        sqlx::query("insert into tenants (id, name) values ($1, $2)")
            .bind(tenant_id)
            .bind(&tenant_name)
            .execute(&mut *tx)
            .await?;

        sqlx::query("select set_config('app.tenant_id', $1, true)")
            .bind(tenant_id.to_string())
            .execute(&mut *tx)
            .await?;

        takar_db::alerts::seed_default_rules(&mut tx, tenant_id).await?;

        sqlx::query("insert into memberships (tenant_id, user_id, role) values ($1, $2, 'owner')")
            .bind(tenant_id)
            .bind(user_id)
            .execute(&mut *tx)
            .await?;

        memberships.push((tenant_id, tenant_name, "owner".to_string()));
    }

    tx.commit().await?;

    let tenants: Vec<TenantSummary> = memberships
        .iter()
        .map(|(id, name, role)| TenantSummary {
            id: *id,
            name: name.clone(),
            role: role.clone(),
        })
        .collect();

    let (access_token, expires_in, requires_tenant_selection) = if memberships.len() == 1 {
        let token = state.jwt.issue(
            user_id,
            Some(memberships[0].0),
            TokenScope::Session,
            state.jwt_ttl_seconds,
        )?;
        (token, state.jwt_ttl_seconds, false)
    } else {
        let token = state
            .jwt
            .issue(user_id, None, TokenScope::TenantSelect, 600)?;
        (token, 600, true)
    };

    Ok(Json(AuthResponse {
        access_token,
        token_type: "bearer",
        expires_in,
        requires_tenant_selection,
        pending_invitation: has_invitation,
        tenants,
    }))
}

pub async fn list_tenants(
    State(state): State<AppState>,
    selection: TenantSelection,
) -> Result<Json<Vec<TenantSummary>>, ApiError> {
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, None, Some(selection.user_id)).await?;

    let rows: Vec<(Uuid, String, String)> = sqlx::query_as(
        "select m.tenant_id, t.name, m.role
         from memberships m
         join tenants t on t.id = m.tenant_id
         where m.user_id = $1 and m.status = 'active'
         order by t.name",
    )
    .bind(selection.user_id)
    .fetch_all(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(Json(
        rows.into_iter()
            .map(|(id, name, role)| TenantSummary { id, name, role })
            .collect(),
    ))
}

pub async fn select_tenant(
    State(state): State<AppState>,
    selection: TenantSelection,
    Json(body): Json<SelectTenantBody>,
) -> Result<Json<AuthResponse>, ApiError> {
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, None, Some(selection.user_id)).await?;

    let row: Option<(String, String)> = sqlx::query_as(
        "select t.name, m.role
         from memberships m
         join tenants t on t.id = m.tenant_id
         where m.user_id = $1 and m.tenant_id = $2 and m.status = 'active'",
    )
    .bind(selection.user_id)
    .bind(body.tenant_id)
    .fetch_optional(&mut *tx)
    .await?;

    tx.commit().await?;

    let (name, role) = row.ok_or(ApiError::Forbidden("not_a_member"))?;
    let access_token = state.jwt.issue(
        selection.user_id,
        Some(body.tenant_id),
        TokenScope::Session,
        state.jwt_ttl_seconds,
    )?;

    Ok(Json(AuthResponse {
        access_token,
        token_type: "bearer",
        expires_in: state.jwt_ttl_seconds,
        requires_tenant_selection: false,
        pending_invitation: false,
        tenants: vec![TenantSummary {
            id: body.tenant_id,
            name,
            role,
        }],
    }))
}

pub async fn me(
    State(state): State<AppState>,
    auth: AuthContext,
) -> Result<Json<MeResponse>, ApiError> {
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let row: Option<(String, Option<String>, Option<String>, String)> = sqlx::query_as(
        "select u.name, u.email, u.phone, t.name
         from memberships m
         join users u on u.id = m.user_id
         join tenants t on t.id = m.tenant_id
         where m.tenant_id = $1 and m.user_id = $2",
    )
    .bind(auth.tenant_id)
    .bind(auth.user_id)
    .fetch_optional(&mut *tx)
    .await?;

    tx.commit().await?;

    let (name, email, phone, tenant_name) =
        row.ok_or(ApiError::Unauthorized("membership_inactive"))?;

    Ok(Json(MeResponse {
        user: UserView {
            id: auth.user_id,
            name,
            email,
            phone,
        },
        tenant: TenantView {
            id: auth.tenant_id,
            name: tenant_name,
        },
        role: auth.role,
        permissions: auth
            .role
            .permissions()
            .iter()
            .map(|permission| permission.as_str().to_string())
            .collect(),
    }))
}
