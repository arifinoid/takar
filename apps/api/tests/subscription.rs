mod common;

use axum::{http::StatusCode, Router};
use common::{call, signup, test_app, CaptureOtp};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

async fn subscription(app: &Router, token: &str) -> (StatusCode, Value) {
    call(app, "GET", "/subscription", None, Some(token)).await
}

async fn create_material(app: &Router, token: &str, name: &str) -> (StatusCode, Value) {
    let kg = common::unit_id(app, token, "kg").await;

    call(
        app,
        "POST",
        "/materials",
        Some(json!({ "name": name, "base_unit_id": kg })),
        Some(token),
    )
    .await
}

/// Fills the tenant with active materials without twenty round trips.
async fn fill_materials(pool: &PgPool, app: &Router, token: &str, tenant_id: Uuid, count: i64) {
    let kg = common::unit_id(app, token, "kg").await;

    sqlx::query(
        "insert into materials (tenant_id, name, base_unit_id)
         select $1, 'Bahan ' || g, $2 from generate_series(1, $3) g",
    )
    .bind(tenant_id)
    .bind(kg)
    .bind(count)
    .execute(pool)
    .await
    .expect("materials");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_new_tenant_starts_on_the_trial_limits(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let (status, body) = subscription(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    assert_eq!(body["plan"], "trial");
    assert_eq!(body["limits"]["users"], 1);
    assert_eq!(body["limits"]["materials"], 20);
    assert_eq!(body["limits"]["recipes"], 5);
    assert_eq!(body["usage"]["users"], 1);
    assert_eq!(body["usage"]["materials"], 0);
    assert!(!body["trial_ends_at"].is_null());
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_owner_uses_the_only_trial_seat(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let (status, body) = call(
        &app,
        "POST",
        "/members/invitations",
        Some(json!({ "contact": "staff@example.test", "role": "admin" })),
        Some(token),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"]["code"], "plan_limit_reached");
    assert_eq!(body["error"]["details"]["resource"], "users");
    assert_eq!(body["error"]["details"]["limit"], 1);
    assert_eq!(body["error"]["details"]["current"], 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_material_cap_stops_the_next_create(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant_id = common::tenant_of(&app, token).await;

    fill_materials(&pool, &app, token, tenant_id, 20).await;

    let (status, body) = create_material(&app, token, "Bahan ke-21").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"]["code"], "plan_limit_reached");
    assert_eq!(body["error"]["details"]["resource"], "materials");
    assert_eq!(body["error"]["details"]["limit"], 20);
    assert_eq!(body["error"]["details"]["current"], 20);

    let (status, body) = subscription(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["usage"]["materials"], 20);
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_archived_material_frees_a_slot(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant_id = common::tenant_of(&app, token).await;

    fill_materials(&pool, &app, token, tenant_id, 20).await;

    // Archived rows are history, not consumption.
    sqlx::query("update materials set is_active = false where name = 'Bahan 1'")
        .execute(&pool)
        .await
        .expect("archive");

    let (status, body) = create_material(&app, token, "Bahan pengganti").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn recipe_versions_do_not_consume_the_cap(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant_id = common::tenant_of(&app, token).await;

    let (status, material) = create_material(&app, token, "Tepung terigu").await;
    assert_eq!(status, StatusCode::CREATED, "{material}");
    let material_id = material["id"]
        .as_str()
        .expect("id")
        .parse::<Uuid>()
        .expect("uuid");

    sqlx::query(
        "insert into products (tenant_id, name, sell_price)
         select $1, 'Produk ' || g, 10000 from generate_series(1, 6) g",
    )
    .bind(tenant_id)
    .execute(&pool)
    .await
    .expect("products");

    // Five products already have an active recipe, filling the trial cap.
    sqlx::query(
        "insert into recipes (tenant_id, product_id, version, yield_qty)
         select $1, id, 1, 1 from products
         where tenant_id = $1 order by name limit 5",
    )
    .bind(tenant_id)
    .execute(&pool)
    .await
    .expect("recipes");

    let product_with_recipe: Uuid =
        sqlx::query_scalar("select id from products where tenant_id = $1 order by name limit 1")
            .bind(tenant_id)
            .fetch_one(&pool)
            .await
            .expect("product");

    let body = json!({
        "yield_qty": "10",
        "labor_minutes": 0,
        "items": [{ "material_id": material_id, "qty": "1" }],
    });

    // A new version replaces the old one, so it costs no slot.
    let (status, body) = call(
        &app,
        "POST",
        &format!("/products/{product_with_recipe}/recipes"),
        Some(body.clone()),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let product_without_recipe: Uuid = sqlx::query_scalar(
        "select p.id from products p
         where p.tenant_id = $1
           and not exists (select 1 from recipes r where r.product_id = p.id and r.is_active)
         limit 1",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .expect("product");

    let (status, body) = call(
        &app,
        "POST",
        &format!("/products/{product_without_recipe}/recipes"),
        Some(body),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"]["code"], "plan_limit_reached");
    assert_eq!(body["error"]["details"]["resource"], "recipes");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_downgrade_keeps_the_data_and_blocks_new_rows(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let tenant_id = common::tenant_of(&app, token).await;

    sqlx::query("update tenants set plan = 'pro' where id = $1")
        .bind(tenant_id)
        .execute(&pool)
        .await
        .expect("plan");

    fill_materials(&pool, &app, token, tenant_id, 25).await;

    sqlx::query("update tenants set plan = 'trial' where id = $1")
        .bind(tenant_id)
        .execute(&pool)
        .await
        .expect("downgrade");

    // Nothing is deleted or hidden by the downgrade.
    let (status, body) = call(&app, "GET", "/materials", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body.as_array().expect("materials").len(), 25);

    let (status, body) = subscription(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["plan"], "trial");
    assert_eq!(body["usage"]["materials"], 25);
    assert_eq!(body["limits"]["materials"], 20);

    let (status, body) = create_material(&app, token, "Bahan baru").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"]["code"], "plan_limit_reached");
}
