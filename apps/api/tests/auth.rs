mod common;

use axum::http::StatusCode;
use common::{call, login, signup, state, tenant_of, test_app, user_of, CaptureOtp};
use serde_json::json;
use sqlx::PgPool;
use takar_api::router;
use uuid::Uuid;

#[sqlx::test(migrations = "../../migrations")]
async fn signup_issues_a_session_token(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());

    let body = signup(&app, &otp, "owner@example.test").await;

    assert_eq!(body["requires_tenant_selection"], false);
    assert_eq!(body["tenants"].as_array().expect("tenants").len(), 1);

    let token = body["access_token"].as_str().expect("token");
    let (status, me) = call(&app, "GET", "/me", None, Some(token)).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["role"], "owner");
    assert_eq!(me["user"]["name"], "Test Owner");
    assert_eq!(me["tenant"]["name"], "Test Tenant");

    let permissions = me["permissions"].as_array().expect("permissions");
    assert!(permissions.iter().any(|value| value == "tenant_manage"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn second_tenant_requires_selection(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let first = signup(&app, &otp, "multi@example.test").await;
    let first_token = first["access_token"].as_str().expect("token");
    let user_id = user_of(&app, first_token).await;

    let second_tenant = Uuid::new_v4();
    sqlx::query("insert into tenants (id, name) values ($1, 'Second Tenant')")
        .bind(second_tenant)
        .execute(&pool)
        .await
        .expect("insert tenant");
    sqlx::query("insert into memberships (tenant_id, user_id, role) values ($1, $2, 'admin')")
        .bind(second_tenant)
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("insert membership");

    let body = login(&app, &otp, "multi@example.test").await;

    assert_eq!(body["requires_tenant_selection"], true);
    assert_eq!(body["tenants"].as_array().expect("tenants").len(), 2);

    let selection_token = body["access_token"].as_str().expect("token");

    let (status, _) = call(&app, "GET", "/me", None, Some(selection_token)).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "tenant-selection tokens must not reach protected routes"
    );

    let (status, tenants) = call(&app, "GET", "/auth/tenants", None, Some(selection_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(tenants.as_array().expect("tenants").len(), 2);

    let (status, selected) = call(
        &app,
        "POST",
        "/auth/select-tenant",
        Some(json!({ "tenant_id": second_tenant })),
        Some(selection_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let session = selected["access_token"].as_str().expect("token");
    let (status, me) = call(&app, "GET", "/me", None, Some(session)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["tenant"]["id"], second_tenant.to_string());
    assert_eq!(me["role"], "admin");
}

#[sqlx::test(migrations = "../../migrations")]
async fn dev_otp_code_replaces_the_generated_code(pool: PgPool) {
    let otp = CaptureOtp::default();
    let mut app_state = state(pool, otp.clone());
    app_state.dev_otp_code = Some("000000".to_string());
    let app = router(app_state);

    let (status, _) = call(
        &app,
        "POST",
        "/auth/otp/request",
        Some(json!({ "contact": "+628000000000" })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(otp.last_code(), "000000");

    let (status, body) = call(
        &app,
        "POST",
        "/auth/otp/verify",
        Some(json!({ "contact": "+628000000000", "code": "000000" })),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["requires_tenant_selection"], false);
}

#[sqlx::test(migrations = "../../migrations")]
async fn wrong_code_is_rejected(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());

    let (status, _) = call(
        &app,
        "POST",
        "/auth/otp/request",
        Some(json!({ "contact": "wrong@example.test" })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);

    let code = otp.last_code();
    let wrong = if code == "000000" { "000001" } else { "000000" };

    let (status, body) = call(
        &app,
        "POST",
        "/auth/otp/verify",
        Some(json!({ "contact": "wrong@example.test", "code": wrong })),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "invalid_code");
}

#[sqlx::test(migrations = "../../migrations")]
async fn suspended_membership_loses_access(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let body = signup(&app, &otp, "suspended@example.test").await;
    let token = body["access_token"].as_str().expect("token");

    sqlx::query("update memberships set status = 'suspended' where user_id = (select id from users where email = $1)")
        .bind("suspended@example.test")
        .execute(&pool)
        .await
        .expect("suspend membership");

    let (status, _) = call(&app, "GET", "/me", None, Some(token)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../../migrations")]
async fn selecting_a_foreign_tenant_is_forbidden(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let first = signup(&app, &otp, "foreign@example.test").await;
    let first_token = first["access_token"].as_str().expect("token");
    let user_id = user_of(&app, first_token).await;

    let second_tenant = Uuid::new_v4();
    sqlx::query("insert into tenants (id, name) values ($1, 'Other Tenant')")
        .bind(second_tenant)
        .execute(&pool)
        .await
        .expect("insert tenant");
    sqlx::query("insert into memberships (tenant_id, user_id, role) values ($1, $2, 'viewer')")
        .bind(second_tenant)
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("insert membership");

    let body = login(&app, &otp, "foreign@example.test").await;
    let selection_token = body["access_token"].as_str().expect("token");

    let (status, _) = call(
        &app,
        "POST",
        "/auth/select-tenant",
        Some(json!({ "tenant_id": Uuid::new_v4() })),
        Some(selection_token),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}

/// The number people actually type is the local one. It must land on the
/// account created with the international form, not create a second one.
#[sqlx::test(migrations = "../../migrations")]
async fn local_phone_format_signs_into_the_same_account(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());

    let first = signup(&app, &otp, "+6281234567890").await;
    let first_token = first["access_token"].as_str().expect("token");
    let tenant_id = tenant_of(&app, first_token).await;
    let user_id = user_of(&app, first_token).await;

    for contact in ["081234567890", "6281234567890", "+62 812-3456-7890"] {
        let body = login(&app, &otp, contact).await;

        assert_eq!(
            body["requires_tenant_selection"], false,
            "{contact} resolved to more than one account"
        );
        assert_eq!(body["tenants"].as_array().expect("tenants").len(), 1);

        let token = body["access_token"].as_str().expect("token");
        assert_eq!(user_of(&app, token).await, user_id, "{contact}");
        assert_eq!(tenant_of(&app, token).await, tenant_id, "{contact}");
    }
}
