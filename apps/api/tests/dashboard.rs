mod common;

use axum::{http::StatusCode, Router};
use common::{call, signup, stock_in, tenant_of, test_app, CaptureOtp};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

async fn material(app: &Router, token: &str, name: &str, min_stock: &str) -> Uuid {
    let kg = common::unit_id(app, token, "kg").await;

    let (status, body) = call(
        app,
        "POST",
        "/materials",
        Some(json!({ "name": name, "base_unit_id": kg, "min_stock": min_stock })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    body["id"].as_str().expect("id").parse().expect("uuid")
}

async fn batch(app: &Router, token: &str, material_id: Uuid, qty: &str, cost: &str) -> Uuid {
    let (status, body) = stock_in(
        app,
        token,
        json!({ "material_id": material_id, "qty_received": qty, "unit_cost": cost }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    body["id"].as_str().expect("id").parse().expect("uuid")
}

/// Expiry is set relative to the database clock, the same clock the endpoint
/// compares against, so the assertions do not depend on the machine date.
async fn set_expiry(pool: &PgPool, batch_id: Uuid, days: i32) {
    sqlx::query("update material_batches set expires_at = current_date + $2::int where id = $1")
        .bind(batch_id)
        .bind(days)
        .execute(pool)
        .await
        .expect("set expiry");
}

fn names(rows: &Value) -> Vec<&str> {
    rows.as_array()
        .expect("rows")
        .iter()
        .map(|row| {
            row["name"]
                .as_str()
                .or_else(|| row["material_name"].as_str())
                .expect("name")
        })
        .collect()
}

#[sqlx::test(migrations = "../../migrations")]
async fn overview_ranks_shortages_and_sums_stock_value(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let flour = material(&app, token, "Tepung", "5").await;
    let chili = material(&app, token, "Cabai", "3").await;
    let salt = material(&app, token, "Garam", "2").await;
    let sugar = material(&app, token, "Gula", "0").await;

    batch(&app, token, flour, "10", "12000").await;
    batch(&app, token, salt, "1", "3000").await;

    let (status, body) = call(&app, "GET", "/dashboard", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let summary = &body["summary"];
    assert_eq!(summary["materials"], 4);
    assert_eq!(summary["below_min"], 2, "cabai and garam are under minimum");
    assert_eq!(summary["out_of_stock"], 1, "cabai has nothing on hand");
    assert_eq!(summary["stock_value"], "123000.00");
    assert_eq!(summary["active_batches"], 2);
    assert_eq!(summary["horizon_days"], 7);

    // Worst ratio first: cabai is empty against a minimum of 3, garam is half.
    let ids: Vec<String> = body["critical"]
        .as_array()
        .expect("critical")
        .iter()
        .map(|row| row["material_id"].as_str().expect("id").to_string())
        .collect();
    assert_eq!(ids, vec![chili.to_string(), salt.to_string()]);
    assert!(
        !ids.contains(&flour.to_string()),
        "a stocked material is safe"
    );
    assert!(
        !ids.contains(&sugar.to_string()),
        "a material with no minimum is never critical"
    );

    let first = &body["critical"][0];
    assert_eq!(first["on_hand"], "0");
    assert_eq!(first["shortfall"], "3.000");
    assert_eq!(first["base_unit"]["code"], "kg");
    assert_eq!(first["active_batches"], 0);
    assert_eq!(first["nearest_expiry"], Value::Null);

    let second = &body["critical"][1];
    assert_eq!(second["on_hand"], "1.000");
    assert_eq!(second["shortfall"], "1.000");
}

#[sqlx::test(migrations = "../../migrations")]
async fn overview_lists_the_expiry_window_in_fefo_order(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let milk = material(&app, token, "Susu", "0").await;
    let cheese = material(&app, token, "Keju", "0").await;

    let late = batch(&app, token, milk, "1", "18000").await;
    let soon = batch(&app, token, cheese, "2", "90000").await;
    let today = batch(&app, token, milk, "3", "18000").await;
    let past = batch(&app, token, cheese, "4", "90000").await;
    let outside = batch(&app, token, milk, "5", "18000").await;

    for (batch_id, days) in [(past, -1), (today, 0), (soon, 3), (late, 7), (outside, 8)] {
        set_expiry(&pool, batch_id, days).await;
    }

    let (status, body) = call(&app, "GET", "/dashboard", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    assert_eq!(body["summary"]["expiring"], 3, "today, +3, and +7");
    assert_eq!(body["summary"]["expired"], 1);
    assert_eq!(body["summary"]["active_batches"], 5);

    let rows = body["expiring"].as_array().expect("expiring");
    assert_eq!(rows.len(), 4, "an expired batch is still worth acting on");
    assert_eq!(
        names(&body["expiring"]),
        vec!["Keju", "Susu", "Keju", "Susu"],
        "earliest expiry first"
    );

    let days: Vec<i64> = rows
        .iter()
        .map(|row| row["days_left"].as_i64().expect("days_left"))
        .collect();
    assert_eq!(days, vec![-1, 0, 3, 7]);
    assert_eq!(rows[0]["qty_remaining"], "4.000");
    assert_eq!(rows[0]["unit_cost"], "90000.0000");
    assert_eq!(rows[0]["base_unit"]["code"], "kg");
}

#[sqlx::test(migrations = "../../migrations")]
async fn overview_skips_depleted_batches_and_other_tenants(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let mine = material(&app, token, "Tepung", "5").await;
    let depleted = batch(&app, token, mine, "2", "12000").await;
    set_expiry(&pool, depleted, 2).await;

    sqlx::query("update material_batches set qty_remaining = 0 where id = $1")
        .bind(depleted)
        .execute(&pool)
        .await
        .expect("deplete");

    let other = signup(&app, &otp, "other@example.test").await;
    let other_token = other["access_token"].as_str().expect("token");
    let other_material = material(&app, other_token, "Cabai", "5").await;
    let other_batch = batch(&app, other_token, other_material, "1", "1000").await;
    set_expiry(&pool, other_batch, 1).await;

    let (status, body) = call(&app, "GET", "/dashboard", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    assert_eq!(body["summary"]["materials"], 1, "only this tenant");
    assert_eq!(body["summary"]["active_batches"], 0);
    assert_eq!(body["summary"]["expiring"], 0);
    assert!(body["expiring"].as_array().expect("expiring").is_empty());
    assert_eq!(body["summary"]["below_min"], 1, "tepung is empty");

    let other_tenant = tenant_of(&app, other_token).await;
    assert_ne!(other_tenant, tenant_of(&app, token).await);
}

#[sqlx::test(migrations = "../../migrations")]
async fn overview_caps_the_lists_but_not_the_counts(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    for index in 0..10 {
        material(&app, token, &format!("Bahan {index}"), "5").await;
    }

    let (status, body) = call(&app, "GET", "/dashboard", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    assert_eq!(body["summary"]["below_min"], 10);
    assert_eq!(
        body["critical"].as_array().expect("critical").len(),
        8,
        "the list is capped so the payload stays small"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn overview_requires_a_session(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp);

    let (status, _) = call(&app, "GET", "/dashboard", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
