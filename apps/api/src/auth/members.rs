//! Member and invitation management.
//!
//! Every handler declares a permission through Require. Resource invariants,
//! such as keeping at least one active owner, run inside the same transaction
//! as the write so two concurrent requests cannot both succeed.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};
use takar_domain::Role;
use uuid::Uuid;

use super::{
    contact::Contact,
    extract::UserIdentity,
    guard::{CanChangeRoles, CanInviteMembers, CanManageMembers, CanReadMembers, Require},
    handlers::{AuthResponse, TenantSummary},
    jwt::TokenScope,
    secret,
};
use crate::{error::ApiError, AppState};

const INVITATION_TTL_DAYS: i64 = 7;

/// Row shape shared by the member list and single-member lookups.
type MemberRow = (
    Uuid,
    String,
    Option<String>,
    Option<String>,
    String,
    String,
    DateTime<Utc>,
);

/// Row shape of a pending invitation.
type InvitationRow = (
    Uuid,
    Option<String>,
    Option<String>,
    String,
    String,
    DateTime<Utc>,
    DateTime<Utc>,
);

/// Row shape returned by auth_find_invitation.
type InvitationLookupRow = (
    Uuid,
    Uuid,
    Option<String>,
    Option<String>,
    String,
    String,
    DateTime<Utc>,
    Uuid,
);

#[derive(Debug, Serialize)]
pub struct MemberView {
    pub user_id: Uuid,
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub role: Role,
    pub status: String,
    pub joined_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct ChangeRoleBody {
    pub role: Role,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberStatus {
    Active,
    Suspended,
}

impl MemberStatus {
    const fn as_str(self) -> &'static str {
        match self {
            MemberStatus::Active => "active",
            MemberStatus::Suspended => "suspended",
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ChangeStatusBody {
    pub status: MemberStatus,
}

#[derive(Debug, Serialize)]
pub struct InvitationView {
    pub id: Uuid,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub role: Role,
    pub status: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct InvitationCreated {
    #[serde(flatten)]
    pub invitation: InvitationView,
    /// Returned once, the same way an OTP code is. Only the hash is stored.
    pub token: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateInvitationBody {
    pub contact: String,
    pub role: Role,
}

#[derive(Debug, Deserialize)]
pub struct AcceptInvitationBody {
    pub token: String,
}

fn parse_role(value: &str) -> Result<Role, ApiError> {
    value.parse().map_err(|_| {
        ApiError::Internal(anyhow::anyhow!(
            "unknown role stored in memberships: {value}"
        ))
    })
}

async fn fetch_member(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<MemberView, ApiError> {
    let row: Option<MemberRow> = sqlx::query_as(
        "select m.user_id, u.name, u.email, u.phone, m.role, m.status, m.joined_at
         from memberships m
         join users u on u.id = m.user_id
         where m.tenant_id = $1 and m.user_id = $2",
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await?;

    let (user_id, name, email, phone, role, status, joined_at) =
        row.ok_or(ApiError::NotFound("member_not_found"))?;

    Ok(MemberView {
        user_id,
        name,
        email,
        phone,
        role: parse_role(&role)?,
        status,
        joined_at,
    })
}

/// Fails unless another active owner remains once this membership changes.
///
/// The tenant row is locked first so two owners cannot demote each other at
/// the same time and leave the tenant ownerless.
async fn ensure_another_owner(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<(), ApiError> {
    sqlx::query("select id from tenants where id = $1 for update")
        .bind(tenant_id)
        .fetch_one(&mut **tx)
        .await?;

    let (remaining,): (i64,) = sqlx::query_as(
        "select count(*)
         from memberships
         where tenant_id = $1 and role = 'owner' and status = 'active' and user_id <> $2",
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_one(&mut **tx)
    .await?;

    if remaining == 0 {
        return Err(ApiError::Conflict("last_owner"));
    }

    Ok(())
}

pub async fn list_members(
    State(state): State<AppState>,
    guard: Require<CanReadMembers>,
) -> Result<Json<Vec<MemberView>>, ApiError> {
    let auth = guard.auth;
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let rows: Vec<MemberRow> = sqlx::query_as(
        "select m.user_id, u.name, u.email, u.phone, m.role, m.status, m.joined_at
         from memberships m
         join users u on u.id = m.user_id
         where m.tenant_id = $1
         order by case m.role
                    when 'owner' then 0
                    when 'admin' then 1
                    when 'production' then 2
                    else 3
                  end,
                  u.name",
    )
    .bind(auth.tenant_id)
    .fetch_all(&mut *tx)
    .await?;

    tx.commit().await?;

    let mut members = Vec::with_capacity(rows.len());
    for (user_id, name, email, phone, role, status, joined_at) in rows {
        members.push(MemberView {
            user_id,
            name,
            email,
            phone,
            role: parse_role(&role)?,
            status,
            joined_at,
        });
    }

    Ok(Json(members))
}

pub async fn change_role(
    State(state): State<AppState>,
    guard: Require<CanChangeRoles>,
    Path(user_id): Path<Uuid>,
    Json(body): Json<ChangeRoleBody>,
) -> Result<Json<MemberView>, ApiError> {
    let auth = guard.auth;
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let current = fetch_member(&mut tx, auth.tenant_id, user_id).await?;

    if current.role == Role::Owner && body.role != Role::Owner {
        ensure_another_owner(&mut tx, auth.tenant_id, user_id).await?;
    }

    sqlx::query("update memberships set role = $1 where tenant_id = $2 and user_id = $3")
        .bind(body.role.as_str())
        .bind(auth.tenant_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    let member = fetch_member(&mut tx, auth.tenant_id, user_id).await?;
    tx.commit().await?;

    Ok(Json(member))
}

pub async fn change_status(
    State(state): State<AppState>,
    guard: Require<CanManageMembers>,
    Path(user_id): Path<Uuid>,
    Json(body): Json<ChangeStatusBody>,
) -> Result<Json<MemberView>, ApiError> {
    let auth = guard.auth;
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let current = fetch_member(&mut tx, auth.tenant_id, user_id).await?;

    if !auth.role.can_manage(current.role) {
        return Err(ApiError::Forbidden("role_outranks_caller"));
    }

    if current.role == Role::Owner && matches!(body.status, MemberStatus::Suspended) {
        ensure_another_owner(&mut tx, auth.tenant_id, user_id).await?;
    }

    sqlx::query("update memberships set status = $1 where tenant_id = $2 and user_id = $3")
        .bind(body.status.as_str())
        .bind(auth.tenant_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    let member = fetch_member(&mut tx, auth.tenant_id, user_id).await?;
    tx.commit().await?;

    Ok(Json(member))
}

pub async fn remove_member(
    State(state): State<AppState>,
    guard: Require<CanManageMembers>,
    Path(user_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let auth = guard.auth;
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let current = fetch_member(&mut tx, auth.tenant_id, user_id).await?;

    if !auth.role.can_manage(current.role) {
        return Err(ApiError::Forbidden("role_outranks_caller"));
    }

    if current.role == Role::Owner {
        ensure_another_owner(&mut tx, auth.tenant_id, user_id).await?;
    }

    sqlx::query("delete from memberships where tenant_id = $1 and user_id = $2")
        .bind(auth.tenant_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_invitations(
    State(state): State<AppState>,
    guard: Require<CanReadMembers>,
) -> Result<Json<Vec<InvitationView>>, ApiError> {
    let auth = guard.auth;
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let rows: Vec<InvitationRow> = sqlx::query_as(
        "select id, email, phone, role, status, expires_at, created_at
         from invitations
         where tenant_id = $1 and status = 'pending'
         order by created_at desc",
    )
    .bind(auth.tenant_id)
    .fetch_all(&mut *tx)
    .await?;

    tx.commit().await?;

    let mut invitations = Vec::with_capacity(rows.len());
    for (id, email, phone, role, status, expires_at, created_at) in rows {
        invitations.push(InvitationView {
            id,
            email,
            phone,
            role: parse_role(&role)?,
            status,
            expires_at,
            created_at,
        });
    }

    Ok(Json(invitations))
}

pub async fn create_invitation(
    State(state): State<AppState>,
    guard: Require<CanInviteMembers>,
    Json(body): Json<CreateInvitationBody>,
) -> Result<(StatusCode, Json<InvitationCreated>), ApiError> {
    let auth = guard.auth;

    if body.role == Role::Owner {
        return Err(ApiError::Forbidden("role_not_invitable"));
    }

    if !auth.role.can_manage(body.role) {
        return Err(ApiError::Forbidden("role_outranks_caller"));
    }

    let contact = Contact::parse(&body.contact)?;
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let existing: Option<(Uuid,)> = sqlx::query_as(
        "select m.user_id
         from memberships m
         join users u on u.id = m.user_id
         where m.tenant_id = $1
           and (lower(u.email) = lower($2) or u.phone = $2)",
    )
    .bind(auth.tenant_id)
    .bind(contact.as_str())
    .fetch_optional(&mut *tx)
    .await?;

    if existing.is_some() {
        return Err(ApiError::Conflict("already_a_member"));
    }

    let token = secret::random_token();
    let token_hash = secret::hash(&state.jwt.secret, &token);
    let expires_at = Utc::now() + Duration::days(INVITATION_TTL_DAYS);

    let row: Result<(Uuid, DateTime<Utc>), sqlx::Error> = sqlx::query_as(
        "insert into invitations
             (tenant_id, email, phone, role, token_hash, invited_by, expires_at)
         values ($1, $2, $3, $4, $5, $6, $7)
         returning id, created_at",
    )
    .bind(auth.tenant_id)
    .bind(contact.email())
    .bind(contact.phone())
    .bind(body.role.as_str())
    .bind(&token_hash)
    .bind(auth.user_id)
    .bind(expires_at)
    .fetch_one(&mut *tx)
    .await;

    let (id, created_at) = row.map_err(|error| match &error {
        sqlx::Error::Database(database) if database.is_unique_violation() => {
            ApiError::Conflict("invitation_exists")
        }
        _ => error.into(),
    })?;

    tx.commit().await?;

    Ok((
        StatusCode::CREATED,
        Json(InvitationCreated {
            invitation: InvitationView {
                id,
                email: contact.email().map(str::to_string),
                phone: contact.phone().map(str::to_string),
                role: body.role,
                status: "pending".to_string(),
                expires_at,
                created_at,
            },
            token,
        }),
    ))
}

pub async fn revoke_invitation(
    State(state): State<AppState>,
    guard: Require<CanInviteMembers>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let auth = guard.auth;
    let db = state.db()?;
    let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

    let result = sqlx::query(
        "update invitations
         set status = 'revoked'
         where id = $1 and tenant_id = $2 and status = 'pending'",
    )
    .bind(id)
    .bind(auth.tenant_id)
    .execute(&mut *tx)
    .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("invitation_not_found"));
    }

    tx.commit().await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn accept_invitation(
    State(state): State<AppState>,
    identity: UserIdentity,
    Json(body): Json<AcceptInvitationBody>,
) -> Result<Json<AuthResponse>, ApiError> {
    let db = state.db()?;
    let token_hash = secret::hash(&state.jwt.secret, &body.token);

    let row: Option<InvitationLookupRow> = sqlx::query_as(
        "select id, tenant_id, email, phone, role, status, expires_at, invited_by
         from auth_find_invitation($1)",
    )
    .bind(&token_hash)
    .fetch_optional(db)
    .await?;

    let (invitation_id, tenant_id, email, phone, role, status, expires_at, invited_by) =
        row.ok_or(ApiError::NotFound("invitation_not_found"))?;

    if status != "pending" {
        return Err(ApiError::Forbidden("invitation_not_pending"));
    }

    if expires_at <= Utc::now() {
        return Err(ApiError::Forbidden("invitation_expired"));
    }

    let role = parse_role(&role)?;

    let mut tx = takar_db::begin_scoped(db, Some(tenant_id), Some(identity.user_id)).await?;

    let user: Option<(Option<String>, Option<String>)> =
        sqlx::query_as("select email, phone from users where id = $1")
            .bind(identity.user_id)
            .fetch_optional(&mut *tx)
            .await?;

    let (user_email, user_phone) = user.ok_or(ApiError::Unauthorized("invalid_token"))?;

    let email_matches = email
        .as_deref()
        .zip(user_email.as_deref())
        .is_some_and(|(invited, user)| invited.eq_ignore_ascii_case(user));
    let phone_matches = phone
        .as_deref()
        .zip(user_phone.as_deref())
        .is_some_and(|(invited, user)| invited == user);
    let matches_contact = email_matches || phone_matches;

    if !matches_contact {
        return Err(ApiError::Forbidden("invitation_contact_mismatch"));
    }

    let existing: Option<(Uuid,)> =
        sqlx::query_as("select user_id from memberships where tenant_id = $1 and user_id = $2")
            .bind(tenant_id)
            .bind(identity.user_id)
            .fetch_optional(&mut *tx)
            .await?;

    if existing.is_some() {
        return Err(ApiError::Conflict("already_a_member"));
    }

    sqlx::query(
        "insert into memberships (tenant_id, user_id, role, invited_by)
         values ($1, $2, $3, $4)",
    )
    .bind(tenant_id)
    .bind(identity.user_id)
    .bind(role.as_str())
    .bind(invited_by)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "update invitations
         set status = 'accepted', accepted_by = $1, accepted_at = now()
         where id = $2 and tenant_id = $3",
    )
    .bind(identity.user_id)
    .bind(invitation_id)
    .bind(tenant_id)
    .execute(&mut *tx)
    .await?;

    let (tenant_name,): (String,) = sqlx::query_as("select name from tenants where id = $1")
        .bind(tenant_id)
        .fetch_one(&mut *tx)
        .await?;

    tx.commit().await?;

    let access_token = state.jwt.issue(
        identity.user_id,
        Some(tenant_id),
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
            id: tenant_id,
            name: tenant_name,
            role: role.to_string(),
        }],
    }))
}
