mod common;

use axum::http::StatusCode;
use common::{call, login, signup, test_app, CaptureOtp};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

async fn unit_id(app: &axum::Router, token: &str, code: &str) -> Uuid {
    let (status, units) = call(app, "GET", "/units", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK);

    units
        .as_array()
        .expect("units")
        .iter()
        .find(|unit| unit["code"] == code)
        .unwrap_or_else(|| panic!("unit {code} is missing"))["id"]
        .as_str()
        .expect("id")
        .parse()
        .expect("uuid")
}

async fn create_material(app: &axum::Router, token: &str, body: Value) -> (StatusCode, Value) {
    call(app, "POST", "/materials", Some(body), Some(token)).await
}

#[sqlx::test(migrations = "../../migrations")]
async fn system_units_are_seeded(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let (status, units) = call(&app, "GET", "/units", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK);

    let codes: Vec<&str> = units
        .as_array()
        .expect("units")
        .iter()
        .map(|unit| unit["code"].as_str().expect("code"))
        .collect();

    for code in ["g", "kg", "ml", "l", "pcs", "lusin"] {
        assert!(codes.contains(&code), "{code} is missing from {codes:?}");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn owner_creates_a_material_with_a_base_unit(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let kg = unit_id(&app, token, "kg").await;

    let (status, material) = create_material(
        &app,
        token,
        json!({
            "name": "Tepung terigu",
            "base_unit_id": kg,
            "min_stock": "12.5",
            "shelf_life_days": 30
        }),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "{material}");
    assert_eq!(material["name"], "Tepung terigu");
    assert_eq!(material["base_unit"]["code"], "kg");
    assert_eq!(material["min_stock"], "12.500");
    assert_eq!(material["shelf_life_days"], 30);
    assert_eq!(material["last_cost"], "0");

    let (status, list) = call(&app, "GET", "/materials", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list.as_array().expect("materials").len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn duplicate_material_names_are_rejected(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let kg = unit_id(&app, token, "kg").await;

    let body = json!({ "name": "Gula", "base_unit_id": kg });

    let (status, _) = create_material(&app, token, body.clone()).await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, error) = create_material(&app, token, body).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["error"]["code"], "material_exists");

    let (status, error) =
        create_material(&app, token, json!({ "name": "gula", "base_unit_id": kg })).await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "names compare case-insensitively"
    );
    assert_eq!(error["error"]["code"], "material_exists");
}

#[sqlx::test(migrations = "../../migrations")]
async fn unknown_units_and_negative_numbers_are_rejected(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let kg = unit_id(&app, token, "kg").await;

    let (status, error) = create_material(
        &app,
        token,
        json!({ "name": "Tepung", "base_unit_id": Uuid::new_v4() }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error["error"]["code"], "invalid_reference");

    let (status, error) = create_material(
        &app,
        token,
        json!({ "name": "Tepung", "base_unit_id": kg, "min_stock": "-1" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error["error"]["code"], "invalid_min_stock");

    let (status, error) = create_material(
        &app,
        token,
        json!({ "name": "Tepung", "base_unit_id": kg, "shelf_life_days": -3 }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error["error"]["code"], "invalid_shelf_life");
}

#[sqlx::test(migrations = "../../migrations")]
async fn viewers_can_read_but_not_write_master_data(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let owner_token = owner["access_token"].as_str().expect("token");
    let tenant_id = common::tenant_of(&app, owner_token).await;

    common::add_member(&pool, tenant_id, "viewer@example.test", "Viewer", "viewer").await;
    let viewer = login(&app, &otp, "viewer@example.test").await;
    let viewer_token = viewer["access_token"].as_str().expect("token");

    let (status, _) = call(&app, "GET", "/materials", None, Some(viewer_token)).await;
    assert_eq!(status, StatusCode::OK);

    let kg = unit_id(&app, viewer_token, "kg").await;
    let (status, error) = create_material(
        &app,
        viewer_token,
        json!({ "name": "Tepung", "base_unit_id": kg }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(error["error"]["code"], "permission_denied");

    let (status, error) = call(
        &app,
        "POST",
        "/suppliers",
        Some(json!({ "name": "Pasar" })),
        Some(viewer_token),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(error["error"]["code"], "permission_denied");
}

#[sqlx::test(migrations = "../../migrations")]
async fn materials_never_leak_across_tenants(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());

    let first = signup(&app, &otp, "first@example.test").await;
    let first_token = first["access_token"].as_str().expect("token");
    let kg = unit_id(&app, first_token, "kg").await;

    let second = signup(&app, &otp, "second@example.test").await;
    let second_token = second["access_token"].as_str().expect("token");

    let (status, foreign) = create_material(
        &app,
        second_token,
        json!({ "name": "Cabai", "base_unit_id": kg }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{foreign}");
    let foreign_id = foreign["id"].as_str().expect("id");

    let (status, list) = call(&app, "GET", "/materials", None, Some(first_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(list.as_array().expect("materials").is_empty());

    let (status, _) = call(
        &app,
        "PUT",
        &format!("/materials/{foreign_id}"),
        Some(json!({ "name": "Bawang", "base_unit_id": kg })),
        Some(first_token),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = call(
        &app,
        "DELETE",
        &format!("/materials/{foreign_id}"),
        None,
        Some(first_token),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../../migrations")]
async fn suppliers_round_trip_and_link_to_materials(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let kg = unit_id(&app, token, "kg").await;

    let (status, supplier) = call(
        &app,
        "POST",
        "/suppliers",
        Some(
            json!({ "name": "Toko Sumber Rejeki", "phone": "+628111111111", "lead_time_days": 3 }),
        ),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{supplier}");
    let supplier_id = supplier["id"].as_str().expect("id");

    let (status, error) = call(
        &app,
        "POST",
        "/suppliers",
        Some(json!({ "name": "toko sumber rejeki" })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["error"]["code"], "supplier_exists");

    let (status, error) = call(
        &app,
        "POST",
        "/suppliers",
        Some(json!({ "name": "Negatif", "lead_time_days": -1 })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error["error"]["code"], "invalid_lead_time");

    let (status, material) = create_material(
        &app,
        token,
        json!({ "name": "Tepung", "base_unit_id": kg, "default_supplier_id": supplier_id }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{material}");
    assert_eq!(material["default_supplier_id"], supplier_id);

    let (status, updated) = call(
        &app,
        "PUT",
        &format!("/suppliers/{supplier_id}"),
        Some(json!({ "name": "Sumber Rejeki", "lead_time_days": 5 })),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(updated["name"], "Sumber Rejeki");
    assert_eq!(updated["lead_time_days"], 5);

    let (status, _) = call(
        &app,
        "DELETE",
        &format!("/suppliers/{supplier_id}"),
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, suppliers) = call(&app, "GET", "/suppliers", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(suppliers.as_array().expect("suppliers").is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_material_cannot_point_at_another_tenants_supplier(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());

    let first = signup(&app, &otp, "first@example.test").await;
    let first_token = first["access_token"].as_str().expect("token");
    let kg = unit_id(&app, first_token, "kg").await;

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

    let (status, error) = create_material(
        &app,
        first_token,
        json!({ "name": "Tepung", "base_unit_id": kg, "default_supplier_id": supplier_id }),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error["error"]["code"], "invalid_supplier");
}

#[sqlx::test(migrations = "../../migrations")]
async fn deleting_a_material_hides_it_from_the_list(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");
    let kg = unit_id(&app, token, "kg").await;

    let (status, material) =
        create_material(&app, token, json!({ "name": "Kopi", "base_unit_id": kg })).await;
    assert_eq!(status, StatusCode::CREATED);
    let id = material["id"].as_str().expect("id");

    let (status, _) = call(
        &app,
        "DELETE",
        &format!("/materials/{id}"),
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, list) = call(&app, "GET", "/materials", None, Some(token)).await;
    assert!(list.as_array().expect("materials").is_empty());

    let (status, _) = call(
        &app,
        "DELETE",
        &format!("/materials/{id}"),
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
