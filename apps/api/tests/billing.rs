mod common;

use axum::{http::StatusCode, Router};
use chrono::{Duration, Utc};
use common::{call, signup, test_app, CaptureOtp};
use serde_json::{json, Value};
use sqlx::PgPool;
use takar_domain::plan::Plan;
use uuid::Uuid;

async fn billing(app: &Router, token: &str) -> (StatusCode, Value) {
    call(app, "GET", "/billing", None, Some(token)).await
}

async fn record(app: &Router, token: &str, plan: &str, months: i32) -> (StatusCode, Value) {
    call(
        app,
        "POST",
        "/billing/payments",
        Some(json!({
            "plan": plan,
            "method": "transfer",
            "reference": "TRX-1",
            "months": months,
        })),
        Some(token),
    )
    .await
}

/// Moves the trial into the past without going through billing.
async fn end_trial(pool: &PgPool, tenant_id: Uuid, days_ago: i64) {
    sqlx::query(
        "update tenants
         set trial_ends_at = now() - make_interval(days => $2)
         where id = $1",
    )
    .bind(tenant_id)
    .bind(days_ago as i32)
    .execute(pool)
    .await
    .expect("end trial");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_new_tenant_is_active_and_has_no_payments(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let (status, body) = billing(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    assert_eq!(body["plan"], "trial");
    assert_eq!(body["access"], "active");
    assert!(body["price_per_month"].is_null());
    assert_eq!(body["payments"].as_array().expect("payments").len(), 0);
    assert!(body["days_left"].as_i64().expect("days") > 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_owner_records_a_transfer_for_one_month(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let (status, body) = record(&app, token, "starter", 1).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    // The price is prefilled from the plan when the client omits it.
    assert_eq!(body["amount"], "79000.00");
    assert_eq!(body["status"], "pending");
    assert_eq!(body["plan"], "starter");
    assert!(body["verified_at"].is_null());

    // A month from today, not from the start of the trial.
    let start = body["period_start"].as_str().expect("start");
    let end = body["period_end"].as_str().expect("end");
    assert!(end > start, "{start}..{end}");

    let (status, body) = billing(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["payments"].as_array().expect("payments").len(), 1);
    // Recording a payment does not activate anything.
    assert_eq!(body["access"], "active");
    assert_eq!(body["plan"], "trial");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_plan_the_tenant_cannot_buy_is_rejected(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let (status, body) = record(&app, token, "trial", 1).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "invalid_plan");

    let (status, body) = record(&app, token, "pro", 13).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "invalid_months");
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_the_owner_may_read_or_record_payments(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant_id = common::tenant_of(&app, token).await;

    common::add_member(&pool, tenant_id, "admin@example.test", "Admin", "admin").await;
    let admin = common::login(&app, &otp, "admin@example.test").await;
    let admin_token = admin["access_token"].as_str().expect("token");

    let (status, body) = billing(&app, admin_token).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"]["code"], "permission_denied");

    let (status, body) = record(&app, admin_token, "starter", 1).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn verifying_a_payment_activates_the_subscription(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant_id = common::tenant_of(&app, token).await;

    end_trial(&pool, tenant_id, 30).await;

    let (_, body) = billing(&app, token).await;
    assert_eq!(body["access"], "read_only");

    let end = Utc::now().date_naive() + Duration::days(30);
    let mut tx = takar_db::begin_scoped(&pool, Some(tenant_id), None)
        .await
        .expect("begin");
    takar_api::billing::activate(&mut tx, tenant_id, Plan::Starter, end)
        .await
        .expect("activate");
    tx.commit().await.expect("commit");

    let (status, body) = billing(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["access"], "active");
    assert_eq!(body["plan"], "starter");
    assert_eq!(body["price_per_month"], 79_000);
    assert!(body["days_left"].as_i64().expect("days") >= 29);
}

#[sqlx::test(migrations = "../../migrations")]
async fn read_only_blocks_writes_but_keeps_reads_and_billing_open(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant_id = common::tenant_of(&app, token).await;

    end_trial(&pool, tenant_id, 30).await;

    let kg = common::unit_id(&app, token, "kg").await;
    let (status, body) = call(
        &app,
        "POST",
        "/materials",
        Some(json!({ "name": "Tepung", "base_unit_id": kg })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"]["code"], "subscription_expired");
    assert_eq!(body["error"]["details"]["access"], "read_only");

    // Reads stay open: the tenant can still see what it has.
    let (status, body) = call(&app, "GET", "/materials", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // And it can still pay, which is the way out.
    let (status, body) = record(&app, token, "starter", 1).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_grace_window_still_allows_writes(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant_id = common::tenant_of(&app, token).await;

    end_trial(&pool, tenant_id, 3).await;

    let (_, body) = billing(&app, token).await;
    assert_eq!(body["access"], "grace");

    let kg = common::unit_id(&app, token, "kg").await;
    let (status, body) = call(
        &app,
        "POST",
        "/materials",
        Some(json!({ "name": "Tepung", "base_unit_id": kg })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_verified_payment_is_never_shortened_by_a_later_one(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant_id = common::tenant_of(&app, token).await;

    let today = Utc::now().date_naive();

    for end in [today + Duration::days(60), today + Duration::days(10)] {
        let mut tx = takar_db::begin_scoped(&pool, Some(tenant_id), None)
            .await
            .expect("begin");
        takar_api::billing::activate(&mut tx, tenant_id, Plan::Starter, end)
            .await
            .expect("activate");
        tx.commit().await.expect("commit");
    }

    let paid_until: chrono::DateTime<Utc> =
        sqlx::query_scalar("select paid_until from tenants where id = $1")
            .bind(tenant_id)
            .fetch_one(&pool)
            .await
            .expect("paid_until");

    assert_eq!(paid_until.date_naive(), today + Duration::days(61));
}

#[sqlx::test(migrations = "../../migrations")]
async fn one_tenant_cannot_see_another_tenants_payments(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let first = signup(&app, &otp, "first@example.test").await;
    let first_token = first["access_token"].as_str().expect("token");
    let (status, body) = record(&app, first_token, "pro", 1).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let second = signup(&app, &otp, "second@example.test").await;
    let second_token = second["access_token"].as_str().expect("token");

    let (status, body) = billing(&app, second_token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["payments"].as_array().expect("payments").len(), 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_pending_payment_can_be_withdrawn(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let (_, payment) = record(&app, token, "starter", 1).await;
    let id = payment["id"].as_str().expect("id");

    let (status, body) = call(
        &app,
        "DELETE",
        &format!("/billing/payments/{id}"),
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");

    let (_, body) = billing(&app, token).await;
    assert_eq!(body["payments"].as_array().expect("payments").len(), 0);
}
