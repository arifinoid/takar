mod common;

use axum::{http::StatusCode, Router};
use common::{call, signup, stock_in, test_app, CaptureOtp};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

async fn supplier(app: &Router, token: &str, lead_time_days: i32) -> Uuid {
    let (status, body) = call(
        app,
        "POST",
        "/suppliers",
        Some(json!({ "name": "Pasar Induk", "lead_time_days": lead_time_days })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    body["id"].as_str().expect("id").parse().expect("uuid")
}

async fn material(app: &Router, token: &str, body: Value) -> Uuid {
    let kg = common::unit_id(app, token, "kg").await;
    let mut body = body;

    body["base_unit_id"] = json!(kg);

    let (status, created) = call(app, "POST", "/materials", Some(body), Some(token)).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");

    created["id"].as_str().expect("id").parse().expect("uuid")
}

async fn stock(app: &Router, token: &str, material_id: Uuid, qty: &str) {
    let (status, batch) = stock_in(
        app,
        token,
        json!({
            "material_id": material_id,
            "qty_received": qty,
            "unit_cost": "10000",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{batch}");
}

async fn waste(app: &Router, token: &str, material_id: Uuid, qty: &str) {
    let (status, batches) = call(
        app,
        "GET",
        &format!("/stock/batches?material_id={material_id}"),
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{batches}");

    let batch_id = batches
        .as_array()
        .expect("batches")
        .first()
        .expect("a batch")["id"]
        .as_str()
        .expect("batch id");

    let (status, body) = call(
        app,
        "POST",
        "/stock/corrections",
        Some(json!({ "batch_id": batch_id, "type": "waste", "qty": format!("-{qty}") })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
}

async fn recommendations(app: &Router, token: &str) -> (StatusCode, Value) {
    call(app, "GET", "/purchasing/recommendations", None, Some(token)).await
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_order_covers_usage_during_the_lead_time(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let supplier_id = supplier(&app, token, 5).await;
    let flour = material(
        &app,
        token,
        json!({
            "name": "Tepung terigu",
            "min_stock": "2",
            "default_supplier_id": supplier_id,
        }),
    )
    .await;

    stock(&app, token, flour, "10").await;
    waste(&app, token, flour, "9").await;

    // 9 kg used over 30 days is 0.3 a day; five days of lead time needs 1.5,
    // plus 2 in reserve, against 1 on hand.
    let (status, body) = recommendations(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["window_days"], 30);

    let items = body["items"].as_array().expect("items");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["material_name"], "Tepung terigu");
    assert_eq!(items[0]["on_hand"], "1.000");
    assert_eq!(items[0]["average_daily_usage"], "0.300");
    assert_eq!(items[0]["lead_time_days"], 5);
    assert_eq!(items[0]["supplier_name"], "Pasar Induk");
    assert_eq!(items[0]["target_qty"], "3.500");
    assert_eq!(items[0]["recommended_qty"], "2.500");
    assert_eq!(items[0]["estimated_cost"], "25000.00");
    assert_eq!(items[0]["insufficient_history"], false);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_material_without_history_is_flagged_and_refills_the_minimum(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let sugar = material(
        &app,
        token,
        json!({ "name": "Gula pasir", "min_stock": "4" }),
    )
    .await;

    stock(&app, token, sugar, "1").await;

    let (status, body) = recommendations(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let items = body["items"].as_array().expect("items");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["insufficient_history"], true);
    assert_eq!(items[0]["average_daily_usage"], "0");
    assert_eq!(items[0]["recommended_qty"], "3.000");
    assert_eq!(items[0]["supplier_name"], Value::Null);
    assert_eq!(items[0]["lead_time_days"], 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn shelf_life_caps_a_long_lead_time(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let supplier_id = supplier(&app, token, 10).await;
    let cream = material(
        &app,
        token,
        json!({
            "name": "Krim segar",
            "min_stock": "0",
            "shelf_life_days": 3,
            "default_supplier_id": supplier_id,
        }),
    )
    .await;

    // 60 used over 30 days is 2 a day; 3 days of shelf life covers 6.
    stock(&app, token, cream, "62").await;
    waste(&app, token, cream, "60").await;

    let (status, body) = recommendations(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let items = body["items"].as_array().expect("items");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["target_qty"], "20.000");
    assert_eq!(items[0]["recommended_qty"], "4.000");
    assert_eq!(items[0]["capped_by_shelf_life"], true);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_material_that_is_already_covered_is_left_out(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let salt = material(&app, token, json!({ "name": "Garam", "min_stock": "1" })).await;
    stock(&app, token, salt, "10").await;

    let (status, body) = recommendations(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["items"].as_array().expect("items").len(), 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn recommendations_stay_inside_the_tenant(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let first = signup(&app, &otp, "first@example.test").await;
    let first_token = first["access_token"].as_str().expect("token");
    let flour = material(
        &app,
        first_token,
        json!({ "name": "Tepung terigu", "min_stock": "5" }),
    )
    .await;
    stock(&app, first_token, flour, "1").await;

    let second = signup(&app, &otp, "second@example.test").await;
    let second_token = second["access_token"].as_str().expect("token");

    let (status, body) = recommendations(&app, second_token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["items"].as_array().expect("items").len(), 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_viewer_can_read_recommendations(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    material(
        &app,
        token,
        json!({ "name": "Gula pasir", "min_stock": "2" }),
    )
    .await;

    sqlx::query("update memberships set role = 'viewer'")
        .execute(&pool)
        .await
        .expect("demote owner");

    let (status, body) = recommendations(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["items"].as_array().expect("items").len(), 1);
}

async fn set_window(app: &Router, token: &str, days: i32) {
    let (status, body) = call(
        app,
        "PUT",
        "/settings",
        Some(json!({ "usage_window_days": days })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_tenant_window_decides_the_average(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let supplier_id = supplier(&app, token, 1).await;
    let flour = material(
        &app,
        token,
        json!({
            "name": "Tepung terigu",
            "min_stock": "0",
            "default_supplier_id": supplier_id,
        }),
    )
    .await;

    // 30 kg used now: 1 a day over 30 days, 3 a day over 10.
    stock(&app, token, flour, "30").await;
    waste(&app, token, flour, "30").await;

    let (status, body) = recommendations(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["window_days"], 30);
    assert_eq!(body["items"][0]["average_daily_usage"], "1.000");
    assert_eq!(body["items"][0]["recommended_qty"], "1.000");

    set_window(&app, token, 10).await;

    let (status, body) = recommendations(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["window_days"], 10);
    assert_eq!(body["items"][0]["average_daily_usage"], "3.000");
    assert_eq!(body["items"][0]["recommended_qty"], "3.000");
}

#[sqlx::test(migrations = "../../migrations")]
async fn movements_older_than_the_window_do_not_count(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant_id = common::tenant_of(&app, token).await;

    let flour = material(
        &app,
        token,
        json!({ "name": "Tepung terigu", "min_stock": "15" }),
    )
    .await;

    stock(&app, token, flour, "30").await;

    // A waste movement 20 days ago: inside the default window, outside a week.
    let (batch_id,): (Uuid,) =
        sqlx::query_as("select id from material_batches where material_id = $1 and tenant_id = $2")
            .bind(flour)
            .bind(tenant_id)
            .fetch_one(&pool)
            .await
            .expect("batch");

    sqlx::query(
        "insert into stock_movements (tenant_id, batch_id, type, qty, created_at)
         values ($1, $2, 'waste', -20, now() - interval '20 days')",
    )
    .bind(tenant_id)
    .bind(batch_id)
    .execute(&pool)
    .await
    .expect("movement");

    sqlx::query("update material_batches set qty_remaining = qty_remaining - 20 where id = $1")
        .bind(batch_id)
        .execute(&pool)
        .await
        .expect("decrement batch");

    let (status, body) = recommendations(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["items"][0]["insufficient_history"], false);
    assert_eq!(body["items"][0]["average_daily_usage"], "0.6667");
    assert_eq!(body["items"][0]["recommended_qty"], "5.000");

    set_window(&app, token, 7).await;

    let (status, body) = recommendations(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["items"][0]["insufficient_history"], true);
    assert_eq!(body["items"][0]["average_daily_usage"], "0");
}
