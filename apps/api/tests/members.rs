mod common;

use axum::http::StatusCode;
use common::{add_member, call, login, signup, tenant_of, test_app, user_of, CaptureOtp};
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrations = "../../migrations")]
async fn invitation_grants_access_after_acceptance(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let owner = signup(&app, &otp, "owner@example.test").await;
    let owner_token = owner["access_token"].as_str().expect("token");

    let (status, created) = call(
        &app,
        "POST",
        "/members/invitations",
        Some(json!({ "contact": "Baker@Example.test", "role": "production" })),
        Some(owner_token),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(created["email"], "baker@example.test");
    assert_eq!(created["role"], "production");
    assert_eq!(created["status"], "pending");

    let invite_token = created["token"].as_str().expect("token").to_string();

    let pending = login(&app, &otp, "baker@example.test").await;
    assert_eq!(pending["pending_invitation"], true);
    assert_eq!(pending["requires_tenant_selection"], true);
    assert_eq!(
        pending["tenants"].as_array().expect("tenants").len(),
        0,
        "accepting an invitation must not create a tenant"
    );

    let selection_token = pending["access_token"].as_str().expect("token");
    let (status, accepted) = call(
        &app,
        "POST",
        "/auth/accept-invitation",
        Some(json!({ "token": invite_token })),
        Some(selection_token),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{accepted}");

    let session = accepted["access_token"].as_str().expect("token");
    let (status, me) = call(&app, "GET", "/me", None, Some(session)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["role"], "production");
    assert_eq!(me["tenant"]["name"], "Test Tenant");

    let (status, body) = call(
        &app,
        "POST",
        "/auth/accept-invitation",
        Some(json!({ "token": invite_token })),
        Some(session),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "invitation_not_pending");
}

#[sqlx::test(migrations = "../../migrations")]
async fn viewer_can_read_members_but_cannot_invite(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let owner = signup(&app, &otp, "owner@example.test").await;
    let owner_token = owner["access_token"].as_str().expect("token");
    let tenant_id = tenant_of(&app, owner_token).await;

    add_member(&pool, tenant_id, "viewer@example.test", "Viewer", "viewer").await;

    let viewer = login(&app, &otp, "viewer@example.test").await;
    let viewer_token = viewer["access_token"].as_str().expect("token");

    let (status, members) = call(&app, "GET", "/members", None, Some(viewer_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(members.as_array().expect("members").len(), 2);

    let (status, body) = call(
        &app,
        "POST",
        "/members/invitations",
        Some(json!({ "contact": "new@example.test", "role": "viewer" })),
        Some(viewer_token),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "permission_denied");
}

#[sqlx::test(migrations = "../../migrations")]
async fn admin_cannot_invite_peers_or_owners(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let owner = signup(&app, &otp, "owner@example.test").await;
    let owner_token = owner["access_token"].as_str().expect("token");
    let tenant_id = tenant_of(&app, owner_token).await;

    add_member(&pool, tenant_id, "admin@example.test", "Admin", "admin").await;

    let admin = login(&app, &otp, "admin@example.test").await;
    let admin_token = admin["access_token"].as_str().expect("token");

    let (status, body) = call(
        &app,
        "POST",
        "/members/invitations",
        Some(json!({ "contact": "peer@example.test", "role": "admin" })),
        Some(admin_token),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "role_outranks_caller");

    let (status, body) = call(
        &app,
        "POST",
        "/members/invitations",
        Some(json!({ "contact": "boss@example.test", "role": "owner" })),
        Some(admin_token),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "role_not_invitable");

    let (status, body) = call(
        &app,
        "POST",
        "/members/invitations",
        Some(json!({ "contact": "staff@example.test", "role": "viewer" })),
        Some(admin_token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn admin_cannot_suspend_an_owner(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let owner = signup(&app, &otp, "owner@example.test").await;
    let owner_token = owner["access_token"].as_str().expect("token");
    let owner_id = user_of(&app, owner_token).await;
    let tenant_id = tenant_of(&app, owner_token).await;

    add_member(&pool, tenant_id, "admin@example.test", "Admin", "admin").await;

    let admin = login(&app, &otp, "admin@example.test").await;
    let admin_token = admin["access_token"].as_str().expect("token");

    let (status, body) = call(
        &app,
        "PATCH",
        &format!("/members/{owner_id}/status"),
        Some(json!({ "status": "suspended" })),
        Some(admin_token),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "role_outranks_caller");
}

#[sqlx::test(migrations = "../../migrations")]
async fn last_owner_cannot_be_demoted_suspended_or_removed(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());

    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let owner_id = user_of(&app, token).await;

    let (status, body) = call(
        &app,
        "PATCH",
        &format!("/members/{owner_id}/role"),
        Some(json!({ "role": "admin" })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "last_owner");

    let (status, body) = call(
        &app,
        "PATCH",
        &format!("/members/{owner_id}/status"),
        Some(json!({ "status": "suspended" })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "last_owner");

    let (status, body) = call(
        &app,
        "DELETE",
        &format!("/members/{owner_id}"),
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "last_owner");
}

#[sqlx::test(migrations = "../../migrations")]
async fn owner_can_step_down_when_a_co_owner_exists(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let owner_id = user_of(&app, token).await;
    let tenant_id = tenant_of(&app, token).await;

    add_member(&pool, tenant_id, "co@example.test", "Co Owner", "owner").await;

    let (status, member) = call(
        &app,
        "PATCH",
        &format!("/members/{owner_id}/role"),
        Some(json!({ "role": "admin" })),
        Some(token),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{member}");
    assert_eq!(member["role"], "admin");

    // The role change applies on the next request without a new login.
    let (status, body) = call(
        &app,
        "PATCH",
        &format!("/members/{owner_id}/role"),
        Some(json!({ "role": "viewer" })),
        Some(token),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "permission_denied");
}

#[sqlx::test(migrations = "../../migrations")]
async fn invitation_for_another_contact_is_rejected(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let owner = signup(&app, &otp, "owner@example.test").await;
    let owner_token = owner["access_token"].as_str().expect("token");

    let (status, created) = call(
        &app,
        "POST",
        "/members/invitations",
        Some(json!({ "contact": "invited@example.test", "role": "viewer" })),
        Some(owner_token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");

    let invite_token = created["token"].as_str().expect("token");

    let intruder = signup(&app, &otp, "intruder@example.test").await;
    let intruder_token = intruder["access_token"].as_str().expect("token");

    let (status, body) = call(
        &app,
        "POST",
        "/auth/accept-invitation",
        Some(json!({ "token": invite_token })),
        Some(intruder_token),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "invitation_contact_mismatch");
}

#[sqlx::test(migrations = "../../migrations")]
async fn revoked_invitation_cannot_be_accepted(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());

    let owner = signup(&app, &otp, "owner@example.test").await;
    let owner_token = owner["access_token"].as_str().expect("token");

    let (status, created) = call(
        &app,
        "POST",
        "/members/invitations",
        Some(json!({ "contact": "later@example.test", "role": "viewer" })),
        Some(owner_token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");

    let invitation_id = created["id"].as_str().expect("id");
    let invite_token = created["token"].as_str().expect("token");

    let (status, _) = call(
        &app,
        "DELETE",
        &format!("/members/invitations/{invitation_id}"),
        None,
        Some(owner_token),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let invitee = login(&app, &otp, "later@example.test").await;
    assert_eq!(invitee["pending_invitation"], false);
    let invitee_token = invitee["access_token"].as_str().expect("token");

    let (status, body) = call(
        &app,
        "POST",
        "/auth/accept-invitation",
        Some(json!({ "token": invite_token })),
        Some(invitee_token),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "invitation_not_pending");
}

#[sqlx::test(migrations = "../../migrations")]
async fn duplicate_and_redundant_invitations_are_rejected(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let owner = signup(&app, &otp, "owner@example.test").await;
    let owner_token = owner["access_token"].as_str().expect("token");
    let tenant_id = tenant_of(&app, owner_token).await;

    add_member(&pool, tenant_id, "member@example.test", "Member", "viewer").await;

    let (status, body) = call(
        &app,
        "POST",
        "/members/invitations",
        Some(json!({ "contact": "member@example.test", "role": "viewer" })),
        Some(owner_token),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "already_a_member");

    let (status, _) = call(
        &app,
        "POST",
        "/members/invitations",
        Some(json!({ "contact": "new@example.test", "role": "viewer" })),
        Some(owner_token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, body) = call(
        &app,
        "POST",
        "/members/invitations",
        Some(json!({ "contact": "NEW@example.test", "role": "admin" })),
        Some(owner_token),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "invitation_exists");
}

#[sqlx::test(migrations = "../../migrations")]
async fn invitations_are_listed_until_they_are_accepted(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());

    let owner = signup(&app, &otp, "owner@example.test").await;
    let owner_token = owner["access_token"].as_str().expect("token");

    let (status, created) = call(
        &app,
        "POST",
        "/members/invitations",
        Some(json!({ "contact": "listed@example.test", "role": "viewer" })),
        Some(owner_token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");

    let (status, invitations) =
        call(&app, "GET", "/members/invitations", None, Some(owner_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(invitations.as_array().expect("invitations").len(), 1);
    assert_eq!(invitations[0]["email"], "listed@example.test");

    let invite_token = created["token"].as_str().expect("token");
    let invitee = login(&app, &otp, "listed@example.test").await;
    let invitee_token = invitee["access_token"].as_str().expect("token");

    let (status, _) = call(
        &app,
        "POST",
        "/auth/accept-invitation",
        Some(json!({ "token": invite_token })),
        Some(invitee_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, invitations) =
        call(&app, "GET", "/members/invitations", None, Some(owner_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(invitations.as_array().expect("invitations").is_empty());
}
