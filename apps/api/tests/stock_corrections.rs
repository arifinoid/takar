mod common;

use axum::http::StatusCode;
use common::{call, create_material, login, signup, stock_in, tenant_of, test_app, CaptureOtp};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

async fn stocked_batch(app: &axum::Router, token: &str, material_id: Uuid, qty: &str) -> Value {
    let (status, batch) = stock_in(
        app,
        token,
        json!({ "material_id": material_id, "qty_received": qty, "unit_cost": "12000" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{batch}");
    batch
}

async fn correct(app: &axum::Router, token: &str, body: Value) -> (StatusCode, Value) {
    call(app, "POST", "/stock/corrections", Some(body), Some(token)).await
}

async fn remaining(app: &axum::Router, token: &str, material_id: Uuid) -> String {
    let (status, batches) = call(
        app,
        "GET",
        &format!("/stock/batches?material_id={material_id}"),
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{batches}");

    batches
        .as_array()
        .expect("batches")
        .first()
        .expect("an active batch")["qty_remaining"]
        .as_str()
        .expect("qty")
        .to_string()
}

#[sqlx::test(migrations = "../../migrations")]
async fn waste_removes_stock_and_records_a_negative_movement(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let material_id = create_material(&app, token, "Tepung").await;
    let batch = stocked_batch(&app, token, material_id, "10").await;

    let (status, body) = correct(
        &app,
        token,
        json!({
            "batch_id": batch["id"],
            "type": "waste",
            "qty": "-2.5",
            "note": "  spoiled  "
        }),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["type"], "waste");
    assert_eq!(body["qty"], "-2.5");
    assert_eq!(body["unit_cost"], "12000.0000");
    assert_eq!(body["note"], "spoiled");
    assert_eq!(body["material_id"], material_id.to_string());
    assert_eq!(body["batch_qty_remaining"], "7.500");

    assert_eq!(remaining(&app, token, material_id).await, "7.500");

    let (status, movements) = call(&app, "GET", "/stock/movements", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{movements}");
    let rows = movements.as_array().expect("movements");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["type"], "waste");
    assert_eq!(rows[0]["qty"], "-2.500");
}

#[sqlx::test(migrations = "../../migrations")]
async fn writing_off_a_whole_batch_depletes_it(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let material_id = create_material(&app, token, "Susu").await;
    let batch = stocked_batch(&app, token, material_id, "5").await;

    let (status, body) = correct(
        &app,
        token,
        json!({ "batch_id": batch["id"], "type": "expired", "qty": "-5" }),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["batch_qty_remaining"], "0");

    let (status, batches) = call(
        &app,
        "GET",
        &format!("/stock/batches?material_id={material_id}"),
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{batches}");
    assert!(
        batches.as_array().expect("batches").is_empty(),
        "a depleted batch is history, not stock"
    );

    let (status, stock) = call(&app, "GET", "/stock", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{stock}");
    let row = stock
        .as_array()
        .expect("stock")
        .iter()
        .find(|row| row["material_id"] == material_id.to_string())
        .expect("material row");
    assert_eq!(row["on_hand"], "0");
}

#[sqlx::test(migrations = "../../migrations")]
async fn adjustments_move_stock_both_ways(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let material_id = create_material(&app, token, "Gula").await;
    let batch = stocked_batch(&app, token, material_id, "10").await;

    for (qty, expected) in [("-3", "7.000"), ("2", "9.000")] {
        let (status, body) = correct(
            &app,
            token,
            json!({ "batch_id": batch["id"], "type": "adjustment", "qty": qty }),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        assert_eq!(body["type"], "adjustment");
        assert_eq!(body["batch_qty_remaining"], expected);
    }

    assert_eq!(remaining(&app, token, material_id).await, "9.000");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_correction_cannot_take_more_than_the_batch_holds(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let material_id = create_material(&app, token, "Beras").await;
    let batch = stocked_batch(&app, token, material_id, "5").await;

    let (status, body) = correct(
        &app,
        token,
        json!({ "batch_id": batch["id"], "type": "waste", "qty": "-6" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "insufficient_batch_quantity");

    let (status, body) = correct(
        &app,
        token,
        json!({ "batch_id": batch["id"], "type": "adjustment", "qty": "1" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "quantity_exceeds_received");

    assert_eq!(
        remaining(&app, token, material_id).await,
        "5.000",
        "a rejected correction leaves the batch untouched"
    );

    let (_, movements) = call(&app, "GET", "/stock/movements", None, Some(token)).await;
    assert_eq!(
        movements.as_array().expect("movements").len(),
        1,
        "no movement is written for a rejected correction"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn correction_signs_and_types_are_validated(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let material_id = create_material(&app, token, "Cabai").await;
    let batch = stocked_batch(&app, token, material_id, "5").await;

    for body in [
        json!({ "batch_id": batch["id"], "type": "waste", "qty": "1" }),
        json!({ "batch_id": batch["id"], "type": "expired", "qty": "0" }),
        json!({ "batch_id": batch["id"], "type": "adjustment", "qty": "0" }),
    ] {
        let (status, response) = correct(&app, token, body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(response["error"]["code"], "invalid_quantity");
    }

    let (status, _) = correct(
        &app,
        token,
        json!({ "batch_id": batch["id"], "type": "purchase", "qty": "1" }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "purchases belong to stock-in, not corrections"
    );

    assert_eq!(remaining(&app, token, material_id).await, "5.000");
}

#[sqlx::test(migrations = "../../migrations")]
async fn production_can_stock_in_but_not_adjust(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let owner_token = owner["access_token"].as_str().expect("token");
    let tenant_id = tenant_of(&app, owner_token).await;
    let material_id = create_material(&app, owner_token, "Tepung").await;
    let batch = stocked_batch(&app, owner_token, material_id, "5").await;

    common::add_member(
        &pool,
        tenant_id,
        "production@example.test",
        "Production",
        "production",
    )
    .await;
    let production = login(&app, &otp, "production@example.test").await;
    let token = production["access_token"].as_str().expect("token");

    let (status, body) = correct(
        &app,
        token,
        json!({ "batch_id": batch["id"], "type": "waste", "qty": "-1" }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "permission_denied");

    let (status, body) = stock_in(
        &app,
        token,
        json!({ "material_id": material_id, "qty_received": "1", "unit_cost": "1000" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_batch_from_another_tenant_cannot_be_corrected(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());

    let first = signup(&app, &otp, "first@example.test").await;
    let first_token = first["access_token"].as_str().expect("token");

    let second = signup(&app, &otp, "second@example.test").await;
    let second_token = second["access_token"].as_str().expect("token");
    let foreign_material = create_material(&app, second_token, "Cabai").await;
    let foreign_batch = stocked_batch(&app, second_token, foreign_material, "5").await;

    let (status, body) = correct(
        &app,
        first_token,
        json!({ "batch_id": foreign_batch["id"], "type": "waste", "qty": "-1" }),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "batch_not_found");
}
