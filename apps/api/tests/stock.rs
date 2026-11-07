mod common;

use axum::http::StatusCode;
use common::{call, login, signup, tenant_of, test_app, CaptureOtp};
use rust_decimal::Decimal;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

async fn unit_id(app: &axum::Router, token: &str, code: &str) -> Uuid {
    let (_, units) = call(app, "GET", "/units", None, Some(token)).await;

    units
        .as_array()
        .expect("units")
        .iter()
        .find(|unit| unit["code"] == code)
        .expect("unit")["id"]
        .as_str()
        .expect("id")
        .parse()
        .expect("uuid")
}

async fn create_material(app: &axum::Router, token: &str, name: &str) -> Uuid {
    let kg = unit_id(app, token, "kg").await;

    let (status, material) = call(
        app,
        "POST",
        "/materials",
        Some(json!({ "name": name, "base_unit_id": kg })),
        Some(token),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "{material}");

    material["id"].as_str().expect("id").parse().expect("uuid")
}

async fn stock_in(app: &axum::Router, token: &str, body: Value) -> (StatusCode, Value) {
    call(app, "POST", "/stock/batches", Some(body), Some(token)).await
}

#[sqlx::test(migrations = "../../migrations")]
async fn stock_in_creates_a_batch_and_a_purchase_movement(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant_id = tenant_of(&app, token).await;
    let material_id = create_material(&app, token, "Tepung terigu").await;

    let (status, batch) = stock_in(
        &app,
        token,
        json!({
            "material_id": material_id,
            "qty_received": "10.5",
            "unit_cost": "12500",
            "expires_at": "2026-12-31"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "{batch}");
    assert_eq!(batch["material_id"], material_id.to_string());
    assert_eq!(batch["qty_received"], "10.5");
    assert_eq!(batch["qty_remaining"], "10.5");
    assert_eq!(batch["unit_cost"], "12500");
    assert_eq!(batch["expires_at"], "2026-12-31");

    let batch_id: Uuid = batch["id"].as_str().expect("id").parse().expect("uuid");

    let movements: Vec<(String, Decimal, Option<Uuid>)> = sqlx::query_as(
        "select type, qty, created_by from stock_movements where tenant_id = $1 and batch_id = $2",
    )
    .bind(tenant_id)
    .bind(batch_id)
    .fetch_all(&pool)
    .await
    .expect("movements");

    assert_eq!(movements.len(), 1, "one opening movement per batch");
    assert_eq!(movements[0].0, "purchase");
    assert_eq!(movements[0].1, Decimal::new(105, 1));
    assert!(
        movements[0].2.is_some(),
        "the movement records who created it"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_latest_purchase_updates_the_material_cost(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let material_id = create_material(&app, token, "Gula").await;

    for cost in ["10000", "13250.5"] {
        let (status, body) = stock_in(
            &app,
            token,
            json!({ "material_id": material_id, "qty_received": "5", "unit_cost": cost }),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
    }

    let (_, materials) = call(&app, "GET", "/materials", None, Some(token)).await;
    let material = materials
        .as_array()
        .expect("materials")
        .iter()
        .find(|material| material["id"] == material_id.to_string())
        .expect("material");

    assert_eq!(material["last_cost"], "13250.5000");
}

#[sqlx::test(migrations = "../../migrations")]
async fn stock_in_rejects_bad_input(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let material_id = create_material(&app, token, "Beras").await;

    let (status, body) = stock_in(
        &app,
        token,
        json!({ "material_id": material_id, "qty_received": "0", "unit_cost": "1000" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_quantity");

    let (status, body) = stock_in(
        &app,
        token,
        json!({ "material_id": material_id, "qty_received": "1", "unit_cost": "-5" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_cost");

    let (status, body) = stock_in(
        &app,
        token,
        json!({ "material_id": Uuid::new_v4(), "qty_received": "1", "unit_cost": "1000" }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "material_not_found");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_batch_cannot_use_another_tenants_supplier(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());

    let first = signup(&app, &otp, "first@example.test").await;
    let first_token = first["access_token"].as_str().expect("token");
    let material_id = create_material(&app, first_token, "Tepung").await;

    let second = signup(&app, &otp, "second@example.test").await;
    let second_token = second["access_token"].as_str().expect("token");

    let (status, supplier) = call(
        &app,
        "POST",
        "/suppliers",
        Some(json!({ "name": "Pemasok Lain" })),
        Some(second_token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{supplier}");
    let supplier_id = supplier["id"].as_str().expect("id");

    let (status, body) = stock_in(
        &app,
        first_token,
        json!({
            "material_id": material_id,
            "supplier_id": supplier_id,
            "qty_received": "1",
            "unit_cost": "1000"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_supplier");
}

#[sqlx::test(migrations = "../../migrations")]
async fn viewers_cannot_stock_in(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let owner_token = owner["access_token"].as_str().expect("token");
    let tenant_id = tenant_of(&app, owner_token).await;
    let material_id = create_material(&app, owner_token, "Tepung").await;

    common::add_member(&pool, tenant_id, "viewer@example.test", "Viewer", "viewer").await;
    let viewer = login(&app, &otp, "viewer@example.test").await;
    let viewer_token = viewer["access_token"].as_str().expect("token");

    let (status, body) = stock_in(
        &app,
        viewer_token,
        json!({ "material_id": material_id, "qty_received": "1", "unit_cost": "1000" }),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "permission_denied");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_material_from_another_tenant_cannot_be_stocked(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());

    let first = signup(&app, &otp, "first@example.test").await;
    let first_token = first["access_token"].as_str().expect("token");

    let second = signup(&app, &otp, "second@example.test").await;
    let second_token = second["access_token"].as_str().expect("token");
    let foreign_material = create_material(&app, second_token, "Cabai").await;

    let (status, body) = stock_in(
        &app,
        first_token,
        json!({ "material_id": foreign_material, "qty_received": "1", "unit_cost": "1000" }),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "material_not_found");
}
