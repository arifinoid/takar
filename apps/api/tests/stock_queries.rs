mod common;

use axum::http::StatusCode;
use common::{call, create_material, login, signup, stock_in, tenant_of, test_app, CaptureOtp};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

fn find(rows: &Value, material_id: Uuid) -> &Value {
    rows.as_array()
        .expect("rows")
        .iter()
        .find(|row| row["material_id"] == material_id.to_string())
        .expect("material row")
}

#[sqlx::test(migrations = "../../migrations")]
async fn on_hand_sums_active_batches_and_flags_low_stock(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let kg = common::unit_id(&app, token, "kg").await;
    let (status, flour) = call(
        &app,
        "POST",
        "/materials",
        Some(json!({ "name": "Tepung", "base_unit_id": kg, "min_stock": "5" })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{flour}");
    let flour_id = flour["id"]
        .as_str()
        .expect("id")
        .parse::<Uuid>()
        .expect("uuid");

    let (status, chili) = call(
        &app,
        "POST",
        "/materials",
        Some(json!({ "name": "Cabai", "base_unit_id": kg, "min_stock": "3" })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{chili}");
    let chili_id = chili["id"]
        .as_str()
        .expect("id")
        .parse::<Uuid>()
        .expect("uuid");

    for (qty, expires_at) in [("10.5", "2026-12-31"), ("4", "2026-10-01")] {
        let (status, batch) = stock_in(
            &app,
            token,
            json!({
                "material_id": flour_id,
                "qty_received": qty,
                "unit_cost": "12000",
                "expires_at": expires_at
            }),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{batch}");
    }

    let (status, stock) = call(&app, "GET", "/stock", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{stock}");

    let flour_row = find(&stock, flour_id);
    assert_eq!(flour_row["on_hand"], "14.500");
    assert_eq!(flour_row["min_stock"], "5.000");
    assert_eq!(flour_row["below_min"], false);
    assert_eq!(flour_row["active_batches"], 2);
    assert_eq!(flour_row["nearest_expiry"], "2026-10-01");
    assert_eq!(flour_row["base_unit"]["code"], "kg");
    assert_eq!(flour_row["last_cost"], "12000.0000");

    let chili_row = find(&stock, chili_id);
    // Postgres sends zero as an empty digit array, so sqlx decodes it without
    // a scale; the client parses decimals, not their formatting.
    assert_eq!(chili_row["on_hand"], "0");
    assert_eq!(chili_row["active_batches"], 0);
    assert_eq!(chili_row["nearest_expiry"], Value::Null);
    assert_eq!(
        chili_row["below_min"], true,
        "a material below its minimum with no batches is critical"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn active_batches_follow_fefo_and_skip_depleted_ones(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let material_id = create_material(&app, token, "Tepung").await;

    let mut batches = Vec::new();
    for (qty, received_at, expires_at) in [
        ("1", "2026-01-01", None),
        ("2", "2026-01-02", Some("2026-03-01")),
        ("3", "2026-01-03", Some("2026-02-01")),
    ] {
        let (status, batch) = stock_in(
            &app,
            token,
            json!({
                "material_id": material_id,
                "qty_received": qty,
                "unit_cost": "1000",
                "received_at": received_at,
                "expires_at": expires_at
            }),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{batch}");
        batches.push(batch);
    }

    let (status, active) = call(
        &app,
        "GET",
        &format!("/stock/batches?material_id={material_id}"),
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{active}");

    let quantities: Vec<&str> = active
        .as_array()
        .expect("batches")
        .iter()
        .map(|batch| batch["qty_remaining"].as_str().expect("qty"))
        .collect();
    assert_eq!(
        quantities,
        vec!["3.000", "2.000", "1.000"],
        "earliest expiry first, null expiry last"
    );

    let depleted_id = batches[2]["id"].as_str().expect("id");
    sqlx::query("update material_batches set qty_remaining = 0 where id = $1")
        .bind(depleted_id.parse::<Uuid>().expect("uuid"))
        .execute(&pool)
        .await
        .expect("deplete batch");

    let (status, active) = call(
        &app,
        "GET",
        &format!("/stock/batches?material_id={material_id}"),
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{active}");

    let quantities: Vec<&str> = active
        .as_array()
        .expect("batches")
        .iter()
        .map(|batch| batch["qty_remaining"].as_str().expect("qty"))
        .collect();
    assert_eq!(
        quantities,
        vec!["2.000", "1.000"],
        "depleted batches are history, not stock"
    );

    let (_, stock) = call(&app, "GET", "/stock", None, Some(token)).await;
    assert_eq!(find(&stock, material_id)["on_hand"], "3.000");
}

#[sqlx::test(migrations = "../../migrations")]
async fn movement_history_is_newest_first_and_filterable(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let flour_id = create_material(&app, token, "Tepung").await;
    let sugar_id = create_material(&app, token, "Gula").await;

    let (_, flour_batch) = stock_in(
        &app,
        token,
        json!({ "material_id": flour_id, "qty_received": "2", "unit_cost": "1000" }),
    )
    .await;
    let (_, sugar_batch) = stock_in(
        &app,
        token,
        json!({ "material_id": sugar_id, "qty_received": "3", "unit_cost": "2000" }),
    )
    .await;

    let (status, movements) = call(&app, "GET", "/stock/movements", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{movements}");

    let rows = movements.as_array().expect("movements");
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0]["batch_id"], sugar_batch["id"],
        "newest movement first"
    );
    assert_eq!(rows[0]["material_id"], sugar_id.to_string());
    assert_eq!(rows[0]["type"], "purchase");
    assert_eq!(rows[0]["qty"], "3.000");
    assert_eq!(rows[1]["batch_id"], flour_batch["id"]);

    let (status, by_material) = call(
        &app,
        "GET",
        &format!("/stock/movements?material_id={flour_id}"),
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{by_material}");
    assert_eq!(by_material.as_array().expect("movements").len(), 1);

    let batch_id = flour_batch["id"].as_str().expect("id");
    let (status, by_batch) = call(
        &app,
        "GET",
        &format!("/stock/movements?batch_id={batch_id}"),
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{by_batch}");
    assert_eq!(by_batch.as_array().expect("movements").len(), 1);

    let (status, limited) = call(&app, "GET", "/stock/movements?limit=0", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{limited}");
    assert_eq!(
        limited.as_array().expect("movements").len(),
        1,
        "a zero limit still returns one row"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn stock_reads_are_tenant_scoped(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());

    let first = signup(&app, &otp, "first@example.test").await;
    let first_token = first["access_token"].as_str().expect("token");
    let material_id = create_material(&app, first_token, "Tepung").await;
    stock_in(
        &app,
        first_token,
        json!({ "material_id": material_id, "qty_received": "1", "unit_cost": "1000" }),
    )
    .await;

    let second = signup(&app, &otp, "second@example.test").await;
    let second_token = second["access_token"].as_str().expect("token");

    let (status, stock) = call(&app, "GET", "/stock", None, Some(second_token)).await;
    assert_eq!(status, StatusCode::OK, "{stock}");
    assert!(
        stock.as_array().expect("stock").is_empty(),
        "another tenant sees no materials"
    );

    let (status, movements) = call(&app, "GET", "/stock/movements", None, Some(second_token)).await;
    assert_eq!(status, StatusCode::OK, "{movements}");
    assert!(movements.as_array().expect("movements").is_empty());

    let (status, body) = call(
        &app,
        "GET",
        &format!("/stock/batches?material_id={material_id}"),
        None,
        Some(second_token),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "material_not_found");
}

#[sqlx::test(migrations = "../../migrations")]
async fn viewers_can_read_stock(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let owner_token = owner["access_token"].as_str().expect("token");
    let tenant_id = tenant_of(&app, owner_token).await;
    let material_id = create_material(&app, owner_token, "Tepung").await;
    stock_in(
        &app,
        owner_token,
        json!({ "material_id": material_id, "qty_received": "1", "unit_cost": "1000" }),
    )
    .await;

    common::add_member(&pool, tenant_id, "viewer@example.test", "Viewer", "viewer").await;
    let viewer = login(&app, &otp, "viewer@example.test").await;
    let viewer_token = viewer["access_token"].as_str().expect("token");

    let (status, stock) = call(&app, "GET", "/stock", None, Some(viewer_token)).await;
    assert_eq!(status, StatusCode::OK, "{stock}");
    assert_eq!(find(&stock, material_id)["on_hand"], "1.000");

    let (status, batches) = call(&app, "GET", "/stock/batches", None, Some(viewer_token)).await;
    assert_eq!(status, StatusCode::OK, "{batches}");
    assert_eq!(batches.as_array().expect("batches").len(), 1);

    let (status, movements) = call(&app, "GET", "/stock/movements", None, Some(viewer_token)).await;
    assert_eq!(status, StatusCode::OK, "{movements}");
}
