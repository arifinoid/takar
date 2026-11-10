mod common;

use axum::{http::StatusCode, Router};
use common::{call, signup, tenant_of, test_app, CaptureOtp};
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

#[sqlx::test(migrations = "../../migrations")]
async fn creating_a_recipe_starts_at_version_one(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let product_id = product(&app, token, "Kue lapis").await;
    let flour = material(&app, token, "Tepung terigu").await;

    let (status, recipe) = create_recipe(&app, token, product_id, recipe_body(flour, "2.5")).await;

    assert_eq!(status, StatusCode::CREATED, "{recipe}");
    assert_eq!(recipe["version"], 1);
    assert_eq!(recipe["is_active"], true);
    assert_eq!(recipe["yield_qty"], "10.000");
    assert_eq!(recipe["labor_minutes"], 30);
    assert_eq!(recipe["items"].as_array().expect("items").len(), 1);
    assert_eq!(recipe["items"][0]["material_name"], "Tepung terigu");
    assert_eq!(recipe["items"][0]["unit_code"], "kg");
    assert_eq!(recipe["items"][0]["qty"], "2.5000");
    assert_eq!(recipe["items"][0]["waste_pct"], "5.00");

    let (status, listed) = call(
        &app,
        "GET",
        &format!("/products/{product_id}/recipes"),
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(listed.as_array().expect("recipes").len(), 1);

    let id = recipe["id"].as_str().expect("recipe id");
    let (status, fetched) = call(&app, "GET", &format!("/recipes/{id}"), None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{fetched}");
    assert_eq!(fetched["id"], recipe["id"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_new_version_deactivates_the_previous_one_and_keeps_its_items(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let product_id = product(&app, token, "Kue lapis").await;
    let flour = material(&app, token, "Tepung terigu").await;
    let sugar = material(&app, token, "Gula pasir").await;

    let (_, first) = create_recipe(&app, token, product_id, recipe_body(flour, "2.5")).await;

    let (status, second) = create_recipe(
        &app,
        token,
        product_id,
        json!({
            "yield_qty": "12",
            "labor_minutes": 40,
            "items": [
                { "material_id": flour, "qty": "3", "waste_pct": "2" },
                { "material_id": sugar, "qty": "1.5", "waste_pct": "0" }
            ]
        }),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "{second}");
    assert_eq!(second["version"], 2);
    assert_eq!(second["is_active"], true);
    assert_eq!(second["items"].as_array().expect("items").len(), 2);

    let (status, versions) = call(
        &app,
        "GET",
        &format!("/products/{product_id}/recipes"),
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{versions}");

    let versions = versions.as_array().expect("versions");
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[0]["version"], 2, "newest first");
    assert_eq!(versions[0]["is_active"], true);
    assert_eq!(versions[1]["version"], 1);
    assert_eq!(versions[1]["is_active"], false);

    let first_id = first["id"].as_str().expect("first id");
    let (status, old) = call(
        &app,
        "GET",
        &format!("/recipes/{first_id}"),
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{old}");
    assert_eq!(
        old["items"].as_array().expect("items").len(),
        1,
        "the old version keeps exactly the items it was created with"
    );
    assert_eq!(old["items"][0]["material_name"], "Tepung terigu");
    assert_eq!(old["items"][0]["qty"], "2.5000");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_recipe_needs_at_least_one_item(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let product_id = product(&app, token, "Kue lapis").await;

    let (status, body) = create_recipe(
        &app,
        token,
        product_id,
        json!({ "yield_qty": "10", "items": [] }),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "recipe_items_required");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_recipe_rejects_unknown_materials_and_duplicates(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let product_id = product(&app, token, "Kue lapis").await;
    let flour = material(&app, token, "Tepung terigu").await;

    let (status, body) =
        create_recipe(&app, token, product_id, recipe_body(Uuid::new_v4(), "1")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_material");

    let (status, body) = create_recipe(
        &app,
        token,
        product_id,
        json!({
            "yield_qty": "10",
            "items": [
                { "material_id": flour, "qty": "1" },
                { "material_id": flour, "qty": "2" }
            ]
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "duplicate_material");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_recipe_rejects_zero_yield_and_negative_waste(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let product_id = product(&app, token, "Kue lapis").await;
    let flour = material(&app, token, "Tepung terigu").await;

    let (status, body) = create_recipe(
        &app,
        token,
        product_id,
        json!({
            "yield_qty": "0",
            "items": [{ "material_id": flour, "qty": "1" }]
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_yield");

    let (status, body) = create_recipe(
        &app,
        token,
        product_id,
        json!({
            "yield_qty": "10",
            "items": [{ "material_id": flour, "qty": "1", "waste_pct": "-1" }]
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_waste");
}

#[sqlx::test(migrations = "../../migrations")]
async fn products_and_recipes_stay_inside_their_tenant(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let first = signup(&app, &otp, "first@example.test").await;
    let first_token = first["access_token"].as_str().expect("token");
    let first_product = product(&app, first_token, "Kue lapis").await;

    let second = signup(&app, &otp, "second@example.test").await;
    let second_token = second["access_token"].as_str().expect("token");

    let (status, body) = call(
        &app,
        "GET",
        &format!("/products/{first_product}/recipes"),
        None,
        Some(second_token),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error"]["code"], "product_not_found");

    let outsider_material = material(&app, second_token, "Gula pasir").await;
    let own_product = product(&app, second_token, "Bolu").await;

    let (status, body) = create_recipe(
        &app,
        second_token,
        own_product,
        recipe_body(outsider_material, "1"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let first_flour = material(&app, first_token, "Tepung terigu").await;
    let (status, body) = create_recipe(
        &app,
        second_token,
        own_product,
        recipe_body(first_flour, "1"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "invalid_material");

    let _ = tenant_of(&app, first_token).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_viewer_cannot_write_products_or_recipes(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let product_id = product(&app, token, "Kue lapis").await;
    let flour = material(&app, token, "Tepung terigu").await;

    sqlx::query("update memberships set role = 'viewer'")
        .execute(&pool)
        .await
        .expect("demote owner");

    let (status, body) = create_recipe(&app, token, product_id, recipe_body(flour, "1")).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");

    let (status, body) = call(
        &app,
        "POST",
        "/products",
        Some(json!({ "name": "Bolu" })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
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

#[sqlx::test(migrations = "../../migrations")]
async fn a_product_points_at_its_newest_recipe(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let product_id = product(&app, token, "Kue lapis").await;
    let flour = material(&app, token, "Tepung terigu").await;

    let before = product_row(&app, token, product_id).await;
    assert!(before["current_recipe_id"].is_null());
    assert!(before["current_recipe_version"].is_null());

    let (_, first) = create_recipe(&app, token, product_id, recipe_body(flour, "2.5")).await;
    let after_first = product_row(&app, token, product_id).await;
    assert_eq!(after_first["current_recipe_id"], first["id"]);
    assert_eq!(after_first["current_recipe_version"], 1);

    let (_, second) = create_recipe(
        &app,
        token,
        product_id,
        json!({
            "yield_qty": "12",
            "items": [{ "material_id": flour, "qty": "3" }]
        }),
    )
    .await;

    let after_second = product_row(&app, token, product_id).await;
    assert_eq!(after_second["current_recipe_id"], second["id"]);
    assert_eq!(after_second["current_recipe_version"], 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_recipe_cannot_become_another_products_current_recipe(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let first_product = product(&app, token, "Kue lapis").await;
    let second_product = product(&app, token, "Bolu").await;
    let flour = material(&app, token, "Tepung terigu").await;

    let (_, recipe) = create_recipe(&app, token, second_product, recipe_body(flour, "1")).await;
    let recipe_id: Uuid = recipe["id"].as_str().expect("id").parse().expect("uuid");

    let result = sqlx::query("update products set current_recipe_id = $1 where id = $2")
        .bind(recipe_id)
        .bind(first_product)
        .execute(&pool)
        .await;

    assert!(
        result.is_err(),
        "a product may only point at a recipe of its own"
    );
}
