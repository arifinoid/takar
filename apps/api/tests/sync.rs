mod common;

use axum::{http::StatusCode, Router};
use common::{call, signup, stock_in, test_app, CaptureOtp};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

async fn material(app: &Router, token: &str) -> Uuid {
    let kg = common::unit_id(app, token, "kg").await;

    let (status, body) = call(
        app,
        "POST",
        "/materials",
        Some(json!({ "name": "Tepung terigu", "base_unit_id": kg })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    body["id"].as_str().expect("id").parse().expect("uuid")
}

async fn sync(app: &Router, token: &str, entries: Value) -> (StatusCode, Value) {
    call(
        app,
        "POST",
        "/sync/stock-in",
        Some(json!({ "entries": entries })),
        Some(token),
    )
    .await
}

#[sqlx::test(migrations = "../../migrations")]
async fn replaying_a_receipt_does_not_create_a_second_batch(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let material_id = material(&app, token).await;
    let client_ref = Uuid::new_v4();

    let body = json!({
        "material_id": material_id,
        "qty_received": "5",
        "unit_cost": "12000",
        "received_at": "2026-11-10",
        "client_ref": client_ref,
    });

    let (status, first) = stock_in(&app, token, body.clone()).await;
    assert_eq!(status, StatusCode::CREATED, "{first}");

    // The shell retries because it never saw the response.
    let (status, second) = stock_in(&app, token, body).await;
    assert_eq!(status, StatusCode::OK, "{second}");
    assert_eq!(second["id"], first["id"]);

    let (batches,): (i64,) = sqlx::query_as("select count(*) from material_batches")
        .fetch_one(&pool)
        .await
        .expect("count batches");
    assert_eq!(batches, 1);

    let (movements,): (i64,) =
        sqlx::query_as("select count(*) from stock_movements where type = 'purchase'")
            .fetch_one(&pool)
            .await
            .expect("count movements");
    assert_eq!(movements, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_sync_endpoint_reports_every_entry(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let material_id = material(&app, token).await;

    let first_ref = Uuid::new_v4();
    let duplicate_ref = Uuid::new_v4();
    let missing_ref = Uuid::new_v4();

    let entry = |client_ref: Option<Uuid>, material_id: Uuid| {
        let mut value = json!({
            "material_id": material_id,
            "qty_received": "2",
            "unit_cost": "9000",
            "received_at": "2026-11-10",
        });

        if let Some(client_ref) = client_ref {
            value["client_ref"] = json!(client_ref);
        }

        value
    };

    // The first queue drains normally.
    let (status, body) = sync(&app, token, json!([entry(Some(first_ref), material_id)])).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["results"][0]["status"], "created");
    assert_eq!(body["results"][0]["client_ref"], first_ref.to_string());

    let (status, body) = sync(
        &app,
        token,
        json!([
            entry(Some(first_ref), material_id),
            entry(Some(duplicate_ref), material_id),
            entry(Some(missing_ref), Uuid::new_v4()),
            entry(None, material_id),
        ]),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let results = body["results"].as_array().expect("results");
    assert_eq!(results.len(), 4);

    assert_eq!(results[0]["status"], "duplicate");
    assert_eq!(results[0]["client_ref"], first_ref.to_string());
    assert_eq!(results[1]["status"], "created");
    assert_eq!(results[2]["status"], "rejected");
    assert_eq!(results[2]["code"], "material_not_found");
    assert_eq!(results[3]["status"], "rejected");
    assert_eq!(results[3]["code"], "client_ref_required");

    // One duplicate, one new batch, and the rejected entries wrote nothing.
    let (batches,): (i64,) = sqlx::query_as("select count(*) from material_batches")
        .fetch_one(&pool)
        .await
        .expect("count batches");
    assert_eq!(batches, 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_viewer_cannot_sync(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let material_id = material(&app, token).await;

    sqlx::query("update memberships set role = 'viewer'")
        .execute(&pool)
        .await
        .expect("demote owner");

    let (status, body) = sync(
        &app,
        token,
        json!([{
            "client_ref": Uuid::new_v4(),
            "material_id": material_id,
            "qty_received": "1",
            "unit_cost": "1000",
        }]),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"]["code"], "permission_denied");
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_same_reference_from_another_tenant_is_a_new_batch(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let first = signup(&app, &otp, "first@example.test").await;
    let first_token = first["access_token"].as_str().expect("token");
    let first_material = material(&app, first_token).await;

    let second = signup(&app, &otp, "second@example.test").await;
    let second_token = second["access_token"].as_str().expect("token");
    let second_material = material(&app, second_token).await;

    let client_ref = Uuid::new_v4();

    let (status, _) = stock_in(
        &app,
        first_token,
        json!({
            "material_id": first_material,
            "qty_received": "1",
            "unit_cost": "1000",
            "client_ref": client_ref,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // Two phones may pick the same UUID; the reference is per tenant.
    let (status, body) = stock_in(
        &app,
        second_token,
        json!({
            "material_id": second_material,
            "qty_received": "1",
            "unit_cost": "1000",
            "client_ref": client_ref,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
}
