mod common;

use axum::{http::StatusCode, Router};
use chrono::{Duration, NaiveDate, Utc};
use common::{call, signup, stock_in, test_app, CaptureOtp};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

async fn material(app: &Router, token: &str, name: &str) -> Uuid {
    let kg = common::unit_id(app, token, "kg").await;

    let (status, body) = call(
        app,
        "POST",
        "/materials",
        Some(json!({ "name": name, "base_unit_id": kg })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    body["id"].as_str().expect("id").parse().expect("uuid")
}

async fn product(app: &Router, token: &str, name: &str, sell_price: &str) -> Uuid {
    let (status, body) = call(
        app,
        "POST",
        "/products",
        Some(json!({ "name": name, "sell_price": sell_price })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    body["id"].as_str().expect("id").parse().expect("uuid")
}

async fn stock(app: &Router, token: &str, material_id: Uuid, qty: &str, cost: &str) {
    let (status, body) = stock_in(
        app,
        token,
        json!({
            "material_id": material_id,
            "qty_received": qty,
            "unit_cost": cost,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
}

async fn set_stock(pool: &PgPool, product_id: Uuid, qty: &str) {
    sqlx::query("update products set stock_qty = $2 where id = $1")
        .bind(product_id)
        .bind(qty.parse::<rust_decimal::Decimal>().expect("qty"))
        .execute(pool)
        .await
        .expect("stock");
}

/// A production order with a cost, without running the recipe: the report only
/// reads the snapshot, and building a full BOM would test production instead.
async fn produced(
    pool: &PgPool,
    tenant_id: Uuid,
    product_id: Uuid,
    unit_cost: &str,
    produced_on: NaiveDate,
) {
    let existing: Option<Uuid> =
        sqlx::query_scalar("select id from recipes where product_id = $1 and tenant_id = $2")
            .bind(product_id)
            .bind(tenant_id)
            .fetch_optional(pool)
            .await
            .expect("recipe lookup");

    let recipe_id = match existing {
        Some(id) => id,
        None => sqlx::query_scalar(
            "insert into recipes (tenant_id, product_id, version, yield_qty)
             values ($1, $2, 1, 1)
             returning id",
        )
        .bind(tenant_id)
        .bind(product_id)
        .fetch_one(pool)
        .await
        .expect("recipe"),
    };

    sqlx::query(
        "insert into production_orders
             (tenant_id, product_id, recipe_id, planned_qty, actual_qty, produced_on, unit_cost)
         values ($1, $2, $3, 1, 1, $4, $5)",
    )
    .bind(tenant_id)
    .bind(product_id)
    .bind(recipe_id)
    .bind(produced_on)
    .bind(unit_cost.parse::<rust_decimal::Decimal>().expect("cost"))
    .execute(pool)
    .await
    .expect("production order");
}

async fn sell(app: &Router, token: &str, body: Value) -> (StatusCode, Value) {
    call(app, "POST", "/sales", Some(body), Some(token)).await
}

async fn report(app: &Router, token: &str) -> (StatusCode, Value) {
    call(app, "GET", "/reports", None, Some(token)).await
}

#[sqlx::test(migrations = "../../migrations")]
async fn stock_is_valued_at_the_cost_each_batch_arrived_with(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let flour = material(&app, token, "Tepung terigu").await;
    stock(&app, token, flour, "10", "10000").await;
    stock(&app, token, flour, "10", "20000").await;

    let (status, body) = report(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // 10 x 10.000 + 10 x 20.000. Valuing at the material's latest price would
    // have said 400.000 and re-priced stock that was already on the shelf.
    assert_eq!(body["stock"]["materials"], "300000.00");
    assert_eq!(body["stock"]["products"], "0.00");
    assert_eq!(body["stock"]["total"], "300000.00");

    // The overview screen shows the same number.
    let (status, dashboard) = call(&app, "GET", "/dashboard", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{dashboard}");
    assert_eq!(dashboard["summary"]["stock_value"], "300000.00");
}

#[sqlx::test(migrations = "../../migrations")]
async fn waste_is_valued_at_the_batch_it_left(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let flour = material(&app, token, "Tepung terigu").await;
    stock(&app, token, flour, "10", "10000").await;

    let (status, batches) = call(
        &app,
        "GET",
        &format!("/stock/batches?material_id={flour}"),
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
        &app,
        "POST",
        "/stock/corrections",
        Some(json!({ "batch_id": batch_id, "type": "waste", "qty": "-3" })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let (status, body) = report(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["waste"]["value"], "30000.00");
    assert_eq!(body["waste"]["movements"], 1);
    assert_eq!(
        body["waste"]["by_material"][0]["material_name"],
        "Tepung terigu"
    );
    assert_eq!(body["waste"]["by_material"][0]["qty"], "3.000");
    assert_eq!(body["waste"]["by_material"][0]["value"], "30000.00");
}

#[sqlx::test(migrations = "../../migrations")]
async fn margin_uses_the_cost_snapshotted_at_sale_time(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant_id = common::tenant_of(&app, token).await;

    let cake = product(&app, token, "Bolu pandan", "20000").await;
    set_stock(&pool, cake, "5").await;

    produced(&pool, tenant_id, cake, "12000", Utc::now().date_naive()).await;

    let (status, body) = sell(
        &app,
        token,
        json!({ "items": [{ "product_id": cake, "qty": "2" }] }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let (status, body) = report(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let row = &body["products"][0];
    assert_eq!(row["name"], "Bolu pandan");
    assert_eq!(row["unit_cost"], "12000.0000");
    assert_eq!(row["margin_per_unit"], "8000.00");
    assert_eq!(row["margin_pct"], "40.00");
    assert_eq!(row["sold_qty"], "2.000");
    assert_eq!(row["revenue"], "40000.00");
    assert_eq!(row["cost_of_goods"], "24000.00");
    assert_eq!(row["margin"], "16000.00");
    assert_eq!(row["cost_incomplete"], false);

    // A later run at a higher cost changes the forward-looking unit margin,
    // never the margin already earned.
    produced(
        &pool,
        tenant_id,
        cake,
        "15000",
        Utc::now().date_naive() + Duration::days(1),
    )
    .await;

    let (status, body) = report(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let row = &body["products"][0];
    assert_eq!(row["unit_cost"], "15000.0000");
    assert_eq!(row["margin_per_unit"], "5000.00");
    assert_eq!(row["cost_of_goods"], "24000.00");
    assert_eq!(row["margin"], "16000.00");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_product_never_produced_has_no_margin(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let cake = product(&app, token, "Bolu pandan", "15000").await;
    set_stock(&pool, cake, "5").await;

    let (status, body) = sell(
        &app,
        token,
        json!({ "items": [{ "product_id": cake, "qty": "1" }] }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let (status, body) = report(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let row = &body["products"][0];
    assert_eq!(row["unit_cost"], Value::Null);
    assert_eq!(row["margin_per_unit"], Value::Null);
    assert_eq!(row["margin_pct"], Value::Null);
    assert_eq!(row["revenue"], "15000.00");
    assert_eq!(row["cost_of_goods"], Value::Null);
    assert_eq!(row["margin"], Value::Null);
    assert_eq!(row["cost_incomplete"], true);
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_the_tenant_window_counts(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let (status, body) = call(
        &app,
        "PUT",
        "/settings",
        Some(json!({ "usage_window_days": 7 })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let cake = product(&app, token, "Bolu pandan", "20000").await;
    set_stock(&pool, cake, "10").await;

    let old = (Utc::now() - Duration::days(20)).to_rfc3339();

    let (status, body) = sell(
        &app,
        token,
        json!({
            "sold_at": old,
            "items": [{ "product_id": cake, "qty": "4" }],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let (status, body) = sell(
        &app,
        token,
        json!({ "items": [{ "product_id": cake, "qty": "1" }] }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let (status, body) = report(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["window_days"], 7);
    assert_eq!(body["products"][0]["sold_qty"], "1.000");
    assert_eq!(body["products"][0]["revenue"], "20000.00");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_viewer_can_read_the_report(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    material(&app, token, "Tepung terigu").await;

    sqlx::query("update memberships set role = 'viewer'")
        .execute(&pool)
        .await
        .expect("demote owner");

    let (status, body) = report(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["products"].as_array().expect("products").len(), 0);
}
