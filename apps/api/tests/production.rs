mod common;

use axum::{http::StatusCode, Router};
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

async fn product(app: &Router, token: &str, name: &str) -> Uuid {
    let (status, body) = call(
        app,
        "POST",
        "/products",
        Some(json!({ "name": name, "sku": format!("SKU-{name}"), "sell_price": "15000" })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    body["id"].as_str().expect("id").parse().expect("uuid")
}

async fn create_recipe(
    app: &Router,
    token: &str,
    product_id: Uuid,
    body: Value,
) -> (StatusCode, Value) {
    call(
        app,
        "POST",
        &format!("/products/{product_id}/recipes"),
        Some(body),
        Some(token),
    )
    .await
}

fn recipe_body(material_id: Uuid, qty: &str) -> Value {
    json!({
        "yield_qty": "10",
        "labor_minutes": 30,
        "items": [{ "material_id": material_id, "qty": qty, "waste_pct": "5" }]
    })
}

async fn stocked_batch(
    app: &Router,
    token: &str,
    material_id: Uuid,
    qty: &str,
    expires_at: &str,
    unit_cost: &str,
) -> Value {
    let (status, batch) = stock_in(
        app,
        token,
        json!({
            "material_id": material_id,
            "qty_received": qty,
            "expires_at": expires_at,
            "unit_cost": unit_cost,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{batch}");
    batch
}

async fn produce(app: &Router, token: &str, body: Value) -> (StatusCode, Value) {
    call(app, "POST", "/production-orders", Some(body), Some(token)).await
}

async fn preview(app: &Router, token: &str, body: Value) -> (StatusCode, Value) {
    call(
        app,
        "POST",
        "/production-orders/preview",
        Some(body),
        Some(token),
    )
    .await
}

async fn product_row(app: &Router, token: &str, id: Uuid) -> Value {
    let (status, products) = call(app, "GET", "/products", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{products}");

    products
        .as_array()
        .expect("products")
        .iter()
        .find(|product| product["id"].as_str() == Some(&id.to_string()))
        .cloned()
        .expect("product in list")
}

async fn batch_remaining(app: &Router, token: &str, material_id: Uuid) -> Vec<(String, String)> {
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
        .iter()
        .map(|batch| {
            (
                batch["id"].as_str().expect("batch id").to_string(),
                batch["qty_remaining"]
                    .as_str()
                    .expect("qty_remaining")
                    .to_string(),
            )
        })
        .collect()
}

#[sqlx::test(migrations = "../../migrations")]
async fn production_consumes_fefo_and_adds_finished_goods(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let product_id = product(&app, token, "Kue lapis").await;
    let flour = material(&app, token, "Tepung terigu").await;

    let (status, recipe) = create_recipe(&app, token, product_id, recipe_body(flour, "2.5")).await;
    assert_eq!(status, StatusCode::CREATED, "{recipe}");

    let near = stocked_batch(&app, token, flour, "1", "2026-06-01", "12000").await;
    let far = stocked_batch(&app, token, flour, "5", "2026-12-01", "9000").await;

    // 2.5 kg × 1.05 waste × 10/10 = 2.625 kg: one whole near batch plus part
    // of the far one.
    let (status, order) = produce(
        &app,
        token,
        json!({ "product_id": product_id, "qty": "10", "produced_on": "2026-11-10" }),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "{order}");
    assert_eq!(order["product_name"], "Kue lapis");
    assert_eq!(order["recipe_version"], 1);
    assert_eq!(order["planned_qty"], "10.000");
    assert_eq!(order["actual_qty"], "10.000");
    // 1 × 12000 + 1.625 × 9000 = 26625, spread over 10 units.
    assert_eq!(order["material_cost"], "26625.00");
    assert_eq!(order["labor_cost"], "0");
    assert_eq!(order["overhead_cost"], "0");
    assert_eq!(order["unit_cost"], "2662.5000");
    assert_eq!(order["produced_on"], "2026-11-10");
    assert_eq!(order["status"], "done");

    let consumptions = order["consumptions"].as_array().expect("consumptions");
    assert_eq!(consumptions.len(), 2);
    assert_eq!(consumptions[0]["batch_id"], near["id"]);
    assert_eq!(consumptions[0]["qty"], "1.000");
    assert_eq!(consumptions[0]["unit_cost"], "12000.0000");
    assert_eq!(consumptions[1]["batch_id"], far["id"]);
    assert_eq!(consumptions[1]["qty"], "1.625");
    assert_eq!(consumptions[1]["unit_cost"], "9000.0000");

    // The near batch is depleted and disappears from active stock.
    let remaining = batch_remaining(&app, token, flour).await;
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].0, far["id"].as_str().expect("far id"));
    assert_eq!(remaining[0].1, "3.375");

    let (status, movements) = call(&app, "GET", "/stock/movements", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{movements}");
    let uses: Vec<&Value> = movements
        .as_array()
        .expect("movements")
        .iter()
        .filter(|row| row["type"] == "production_use")
        .collect();
    assert_eq!(uses.len(), 2);

    let near_use = uses
        .iter()
        .find(|row| row["batch_id"] == near["id"])
        .expect("near batch use");
    assert_eq!(near_use["qty"], "-1.000");
    assert_eq!(near_use["unit_cost"], "12000.0000");

    let far_use = uses
        .iter()
        .find(|row| row["batch_id"] == far["id"])
        .expect("far batch use");
    assert_eq!(far_use["qty"], "-1.625");
    assert_eq!(far_use["unit_cost"], "9000.0000");

    assert_eq!(
        product_row(&app, token, product_id).await["stock_qty"],
        "10.000"
    );

    let (orders,): (i64,) = sqlx::query_as("select count(*) from production_orders")
        .fetch_one(&pool)
        .await
        .expect("count orders");
    assert_eq!(orders, 1);

    let (consumed,): (i64,) = sqlx::query_as("select count(*) from production_consumptions")
        .fetch_one(&pool)
        .await
        .expect("count consumptions");
    assert_eq!(consumed, 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_shortage_blocks_the_whole_order(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let product_id = product(&app, token, "Kue lapis").await;
    let flour = material(&app, token, "Tepung terigu").await;
    let sugar = material(&app, token, "Gula pasir").await;

    let (status, recipe) = create_recipe(
        &app,
        token,
        product_id,
        json!({
            "yield_qty": "10",
            "items": [
                { "material_id": flour, "qty": "2.5", "waste_pct": "5" },
                { "material_id": sugar, "qty": "2.5", "waste_pct": "5" }
            ]
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{recipe}");

    stocked_batch(&app, token, flour, "10", "2026-12-01", "12000").await;
    stocked_batch(&app, token, sugar, "1", "2026-12-01", "15000").await;

    let (status, body) = produce(
        &app,
        token,
        json!({ "product_id": product_id, "qty": "10" }),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "insufficient_stock");

    let shortages = body["error"]["details"]["shortages"]
        .as_array()
        .expect("shortages");
    assert_eq!(shortages.len(), 1);
    assert_eq!(shortages[0]["material_id"], sugar.to_string());
    assert_eq!(shortages[0]["material_name"], "Gula pasir");
    assert_eq!(shortages[0]["required"], "2.625");
    assert_eq!(shortages[0]["available"], "1.000");
    assert_eq!(shortages[0]["shortfall"], "1.625");

    // The sufficient material must be untouched: the run never started.
    let flour_remaining = batch_remaining(&app, token, flour).await;
    assert_eq!(flour_remaining.len(), 1);
    assert_eq!(flour_remaining[0].1, "10.000");

    let sugar_remaining = batch_remaining(&app, token, sugar).await;
    assert_eq!(sugar_remaining.len(), 1);
    assert_eq!(sugar_remaining[0].1, "1.000");

    assert_eq!(product_row(&app, token, product_id).await["stock_qty"], "0");

    let (status, movements) = call(&app, "GET", "/stock/movements", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{movements}");
    assert!(
        movements
            .as_array()
            .expect("movements")
            .iter()
            .all(|row| row["type"] == "purchase"),
        "a blocked run writes no movements"
    );

    let (orders,): (i64,) = sqlx::query_as("select count(*) from production_orders")
        .fetch_one(&pool)
        .await
        .expect("count orders");
    assert_eq!(orders, 0);

    let (consumed,): (i64,) = sqlx::query_as("select count(*) from production_consumptions")
        .fetch_one(&pool)
        .await
        .expect("count consumptions");
    assert_eq!(consumed, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_product_without_a_recipe_cannot_be_produced(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let product_id = product(&app, token, "Bolu").await;

    let (status, body) =
        produce(&app, token, json!({ "product_id": product_id, "qty": "5" })).await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "recipe_required");
}

#[sqlx::test(migrations = "../../migrations")]
async fn production_is_scoped_to_the_tenant(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let product_id = product(&app, token, "Kue lapis").await;
    let flour = material(&app, token, "Tepung terigu").await;
    let (_, recipe) = create_recipe(&app, token, product_id, recipe_body(flour, "2.5")).await;
    assert_eq!(recipe["version"], 1);
    stocked_batch(&app, token, flour, "5", "2026-12-01", "12000").await;

    let other = signup(&app, &otp, "other@example.test").await;
    let other_token = other["access_token"].as_str().expect("token");

    let (status, body) = produce(
        &app,
        other_token,
        json!({ "product_id": product_id, "qty": "10" }),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error"]["code"], "product_not_found");

    let remaining = batch_remaining(&app, token, flour).await;
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].1, "5.000");
    assert_eq!(product_row(&app, token, product_id).await["stock_qty"], "0");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_viewer_cannot_produce(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let product_id = product(&app, token, "Kue lapis").await;
    let flour = material(&app, token, "Tepung terigu").await;
    create_recipe(&app, token, product_id, recipe_body(flour, "2.5")).await;
    stocked_batch(&app, token, flour, "5", "2026-12-01", "12000").await;

    sqlx::query("update memberships set role = 'viewer'")
        .execute(&pool)
        .await
        .expect("demote owner");

    // A viewer may look at the plan, but cannot record the run.
    let (status, body) = preview(
        &app,
        token,
        json!({ "product_id": product_id, "qty": "10" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["can_produce"], true);

    let (status, body) = produce(
        &app,
        token,
        json!({ "product_id": product_id, "qty": "10" }),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"]["code"], "permission_denied");
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_order_keeps_the_recipe_version_it_consumed(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let product_id = product(&app, token, "Kue lapis").await;
    let flour = material(&app, token, "Tepung terigu").await;

    let (_, first) = create_recipe(&app, token, product_id, recipe_body(flour, "2.5")).await;
    stocked_batch(&app, token, flour, "5", "2026-12-01", "12000").await;

    let (status, order) = produce(
        &app,
        token,
        json!({ "product_id": product_id, "qty": "10" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{order}");

    let (status, second) = create_recipe(&app, token, product_id, recipe_body(flour, "3")).await;
    assert_eq!(status, StatusCode::CREATED, "{second}");
    assert_eq!(second["version"], 2);

    let (recorded,): (Uuid,) = sqlx::query_as("select recipe_id from production_orders")
        .fetch_one(&pool)
        .await
        .expect("order recipe");
    assert_eq!(
        recorded.to_string(),
        first["id"].as_str().expect("first recipe id")
    );

    let (version,): (i32,) = sqlx::query_as("select version from recipes where id = $1")
        .bind(recorded)
        .fetch_one(&pool)
        .await
        .expect("recipe version");
    assert_eq!(version, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_later_purchase_does_not_change_a_past_orders_hpp(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let product_id = product(&app, token, "Kue lapis").await;
    let flour = material(&app, token, "Tepung terigu").await;
    create_recipe(&app, token, product_id, recipe_body(flour, "2.5")).await;

    stocked_batch(&app, token, flour, "5", "2026-12-01", "12000").await;

    let (status, first) = produce(
        &app,
        token,
        json!({ "product_id": product_id, "qty": "10" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{first}");
    assert_eq!(first["unit_cost"], "3150.0000");

    let first_id = first["id"].as_str().expect("order id");

    // A new purchase costs much more. The old order keeps its snapshot.
    stocked_batch(&app, token, flour, "5", "2027-01-01", "20000").await;

    let (status, second) = produce(
        &app,
        token,
        json!({ "product_id": product_id, "qty": "10" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{second}");

    // 2.375 kg left at 12000 plus 0.25 kg at 20000 = 33500 over 10 units.
    assert_eq!(second["material_cost"], "33500.00");
    assert_eq!(second["unit_cost"], "3350.0000");

    let (recorded,): (String,) =
        sqlx::query_as("select unit_cost::text from production_orders where id = $1")
            .bind(first_id.parse::<Uuid>().expect("uuid"))
            .fetch_one(&pool)
            .await
            .expect("first order cost");
    assert_eq!(recorded, "3150.0000");
}

#[sqlx::test(migrations = "../../migrations")]
async fn preview_shows_requirements_batches_and_hpp(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let product_id = product(&app, token, "Kue lapis").await;
    let flour = material(&app, token, "Tepung terigu").await;
    create_recipe(&app, token, product_id, recipe_body(flour, "2.5")).await;

    let near = stocked_batch(&app, token, flour, "1", "2026-06-01", "12000").await;
    let far = stocked_batch(&app, token, flour, "5", "2026-12-01", "9000").await;

    let (status, body) = preview(
        &app,
        token,
        json!({ "product_id": product_id, "qty": "10" }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["can_produce"], true);
    assert_eq!(body["product_name"], "Kue lapis");
    assert_eq!(body["sell_price"], "15000.00");
    assert_eq!(body["stock_qty"], "0");
    assert_eq!(body["recipe_version"], 1);
    assert_eq!(body["yield_qty"], "10.000");
    assert_eq!(body["qty"], "10.000");
    assert_eq!(body["material_cost"], "26625.00");
    assert_eq!(body["labor_cost"], "0.00");
    assert_eq!(body["overhead_cost"], "0.00");
    assert_eq!(body["unit_cost"], "2662.5000");
    assert_eq!(body["margin"], "12337.50");
    assert_eq!(body["margin_pct"], "82.25");

    let materials = body["materials"].as_array().expect("materials");
    assert_eq!(materials.len(), 1);
    assert_eq!(materials[0]["material_name"], "Tepung terigu");
    assert_eq!(materials[0]["unit_code"], "kg");
    assert_eq!(materials[0]["required"], "2.625");
    assert_eq!(materials[0]["available"], "2.625");
    assert_eq!(materials[0]["shortfall"], "0.000");

    let batches = materials[0]["batches"].as_array().expect("batches");
    assert_eq!(batches.len(), 2);
    assert_eq!(batches[0]["batch_id"], near["id"]);
    assert_eq!(batches[0]["expires_at"], "2026-06-01");
    assert_eq!(batches[0]["qty"], "1.000");
    assert_eq!(batches[0]["unit_cost"], "12000.0000");
    assert_eq!(batches[1]["batch_id"], far["id"]);
    assert_eq!(batches[1]["qty"], "1.625");
    assert_eq!(batches[1]["unit_cost"], "9000.0000");

    // A preview never moves stock.
    let remaining = batch_remaining(&app, token, flour).await;
    assert_eq!(remaining.len(), 2);

    let (orders,): (i64,) = sqlx::query_as("select count(*) from production_orders")
        .fetch_one(&pool)
        .await
        .expect("count orders");
    assert_eq!(orders, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn preview_blocks_a_short_run_without_a_cost(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let product_id = product(&app, token, "Kue lapis").await;
    let flour = material(&app, token, "Tepung terigu").await;
    create_recipe(&app, token, product_id, recipe_body(flour, "2.5")).await;
    stocked_batch(&app, token, flour, "1", "2026-12-01", "12000").await;

    let (status, body) = preview(
        &app,
        token,
        json!({ "product_id": product_id, "qty": "10" }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["can_produce"], false);
    assert_eq!(body["material_cost"], Value::Null);
    assert_eq!(body["unit_cost"], Value::Null);
    assert_eq!(body["margin"], Value::Null);

    let materials = body["materials"].as_array().expect("materials");
    assert_eq!(materials[0]["required"], "2.625");
    assert_eq!(materials[0]["available"], "1.000");
    assert_eq!(materials[0]["shortfall"], "1.625");
    assert_eq!(
        materials[0]["batches"].as_array().expect("batches").len(),
        1
    );
}
