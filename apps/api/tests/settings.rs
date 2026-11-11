mod common;

use axum::{http::StatusCode, Router};
use common::{call, signup, test_app, CaptureOtp};
use serde_json::{json, Value};
use sqlx::PgPool;

async fn get_settings(app: &Router, token: &str) -> (StatusCode, Value) {
    call(app, "GET", "/settings", None, Some(token)).await
}

async fn put_rate(app: &Router, token: &str, rate: &str) -> (StatusCode, Value) {
    call(
        app,
        "PUT",
        "/settings",
        Some(json!({ "labor_rate_per_hour": rate })),
        Some(token),
    )
    .await
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_new_tenant_starts_at_zero_and_the_owner_sets_the_rate(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let (status, body) = get_settings(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["labor_rate_per_hour"], "0");

    let (status, body) = put_rate(&app, token, "75000").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["labor_rate_per_hour"], "75000.00");

    let (status, body) = get_settings(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["labor_rate_per_hour"], "75000.00");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_viewer_can_read_the_rate_but_not_change_it(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    put_rate(&app, token, "50000").await;

    sqlx::query("update memberships set role = 'viewer'")
        .execute(&pool)
        .await
        .expect("demote owner");

    // The rate explains the HPP every member sees, so reading stays open.
    let (status, body) = get_settings(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["labor_rate_per_hour"], "50000.00");

    let (status, body) = put_rate(&app, token, "60000").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"]["code"], "permission_denied");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_negative_rate_is_rejected(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let (status, body) = put_rate(&app, token, "-1").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "invalid_labor_rate");

    let (status, body) = get_settings(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["labor_rate_per_hour"], "0");
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_rate_belongs_to_one_tenant(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let first = signup(&app, &otp, "first@example.test").await;
    let first_token = first["access_token"].as_str().expect("token");
    put_rate(&app, first_token, "80000").await;

    let second = signup(&app, &otp, "second@example.test").await;
    let second_token = second["access_token"].as_str().expect("token");

    let (status, body) = get_settings(&app, second_token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["labor_rate_per_hour"], "0");
}
