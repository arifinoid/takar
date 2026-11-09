mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use common::{add_member, call, login, signup, tenant_of, test_app, CaptureOtp};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt;

async fn upload(app: &Router, token: &str, format: &str, body: &str) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/onboarding/import?format={format}"))
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/octet-stream")
        .body(Body::from(body.to_string()))
        .expect("request");

    let response = app.clone().oneshot(request).await.expect("response");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();

    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn material_names(app: &Router, token: &str) -> Vec<String> {
    let (status, body) = call(app, "GET", "/materials", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let mut names: Vec<String> = body
        .as_array()
        .expect("materials")
        .iter()
        .map(|row| row["name"].as_str().expect("name").to_string())
        .collect();
    names.sort();
    names
}

#[sqlx::test(migrations = "../../migrations")]
async fn templates_are_listed_with_their_items(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let (status, body) = call(&app, "GET", "/onboarding/templates", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let templates = body.as_array().expect("templates");
    assert_eq!(templates.len(), 4);

    let bakery = templates
        .iter()
        .find(|template| template["id"] == "bakery")
        .expect("bakery template");

    assert_eq!(bakery["item_count"], 20);
    assert_eq!(bakery["items"].as_array().expect("items").len(), 20);

    let first = &bakery["items"][0];
    assert!(first["name"].as_str().expect("name").len() > 3);
    assert!(first["unit"].as_str().expect("unit").len() <= 12);
    assert!(first["min_stock"].as_str().is_some());
}

#[sqlx::test(migrations = "../../migrations")]
async fn applying_a_template_is_idempotent(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let (status, body) = call(
        &app,
        "POST",
        "/onboarding/templates/bakery/apply",
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["created"], 20);
    assert_eq!(body["skipped"], 0);
    assert_eq!(body["errors"].as_array().expect("errors").len(), 0);

    let names = material_names(&app, token).await;
    assert_eq!(names.len(), 20);
    assert!(names.contains(&"Tepung terigu protein tinggi".to_string()));

    let (status, body) = call(
        &app,
        "POST",
        "/onboarding/templates/bakery/apply",
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["created"], 0);
    assert_eq!(body["skipped"], 20);
    assert_eq!(material_names(&app, token).await.len(), 20);
}

#[sqlx::test(migrations = "../../migrations")]
async fn applied_materials_carry_their_unit_and_shelf_life(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    call(
        &app,
        "POST",
        "/onboarding/templates/coffee/apply",
        None,
        Some(token),
    )
    .await;

    let (_, materials) = call(&app, "GET", "/materials", None, Some(token)).await;
    let row = materials
        .as_array()
        .expect("materials")
        .iter()
        .find(|row| row["name"] == "Biji kopi arabika")
        .expect("arabica");

    assert_eq!(row["base_unit"]["code"], "kg");
    assert_eq!(row["min_stock"], "3.000");
    assert_eq!(row["shelf_life_days"], 180);
}

#[sqlx::test(migrations = "../../migrations")]
async fn unknown_template_is_not_found(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let (status, _) = call(
        &app,
        "POST",
        "/onboarding/templates/nope/apply",
        None,
        Some(token),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../../migrations")]
async fn csv_import_creates_rows_and_reports_the_bad_ones(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let csv = "Nama,Satuan,Stok minimum,Umur simpan (hari)\n\
               Tepung terigu,kg,5,180\n\
               Gula pasir,kg,3,\n\
               Cabai,kilogram,1,7\n\
               ,kg,1,1\n\
               Garam,ons,1,1\n\
               Merica,kg,-1,1\n";

    let (status, body) = upload(&app, token, "csv", csv).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    assert_eq!(body["created"], 3, "header skipped, aliases accepted");
    assert_eq!(body["skipped"], 0);

    let errors = body["errors"].as_array().expect("errors");
    let codes: Vec<(&str, i64)> = errors
        .iter()
        .map(|error| {
            (
                error["code"].as_str().expect("code"),
                error["row"].as_i64().expect("row"),
            )
        })
        .collect();

    assert_eq!(
        codes,
        vec![
            ("name_required", 5),
            ("unknown_unit", 6),
            ("invalid_min_stock", 7)
        ],
        "row numbers match the spreadsheet"
    );

    assert_eq!(
        material_names(&app, token).await,
        vec!["Cabai", "Gula pasir", "Tepung terigu"]
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn csv_import_accepts_the_indonesian_decimal_comma(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    // Excel writes the decimal comma, so the field has to be quoted in CSV.
    let csv = "Nama,Satuan,Stok minimum,Umur simpan\nTepung,kg,\"2,5\",90\n";

    let (status, body) = upload(&app, token, "csv", csv).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["created"], 1);

    let (_, materials) = call(&app, "GET", "/materials", None, Some(token)).await;
    let row = &materials.as_array().expect("materials")[0];

    assert_eq!(row["min_stock"], "2.500");
    assert_eq!(row["shelf_life_days"], 90);
}

#[sqlx::test(migrations = "../../migrations")]
async fn csv_import_skips_names_that_already_exist(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    common::create_material(&app, token, "Tepung terigu").await;

    let csv = "Nama,Satuan\ntepung terigu,kg\nGula pasir,kg\n";

    let (status, body) = upload(&app, token, "csv", csv).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["created"], 1);
    assert_eq!(body["skipped"], 1, "matching is case-insensitive");
}

#[sqlx::test(migrations = "../../migrations")]
async fn csv_import_rejects_more_rows_than_the_cap(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let mut csv = String::from("Nama,Satuan\n");

    for index in 0..501 {
        csv.push_str(&format!("Bahan {index},kg\n"));
    }

    let (status, body) = upload(&app, token, "csv", &csv).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "too_many_rows");
}

#[sqlx::test(migrations = "../../migrations")]
async fn csv_import_rejects_an_empty_file(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let (status, body) = upload(&app, token, "csv", "").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "empty_file");
}

#[sqlx::test(migrations = "../../migrations")]
async fn import_needs_material_write(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let tenant_id = tenant_of(&app, owner["access_token"].as_str().expect("token")).await;

    add_member(&pool, tenant_id, "viewer@example.test", "Viewer", "viewer").await;
    let viewer = login(&app, &otp, "viewer@example.test").await;
    let token = viewer["access_token"].as_str().expect("token");

    let (status, body) = upload(&app, token, "csv", "Nama,Satuan\nGula,kg\n").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"]["code"], "permission_denied");

    let (status, _) = call(
        &app,
        "POST",
        "/onboarding/templates/bakery/apply",
        None,
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

/// The file a bakery actually has is a spreadsheet, not a CSV. The fixture is
/// a minimal hand-built workbook, so the test does not depend on a writer crate.
#[sqlx::test(migrations = "../../migrations")]
async fn xlsx_import_reads_a_real_workbook(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool, otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let bytes = std::fs::read("tests/fixtures/materials.xlsx").expect("fixture");

    let request = Request::builder()
        .method("POST")
        .uri("/onboarding/import")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/octet-stream")
        .body(Body::from(bytes))
        .expect("request");

    let response = app.clone().oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::OK);

    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let body: Value = serde_json::from_slice(&bytes).expect("json");

    assert_eq!(body["created"], 3, "{body}");
    assert_eq!(body["errors"].as_array().expect("errors").len(), 0);

    let (_, materials) = call(&app, "GET", "/materials", None, Some(token)).await;
    let row = materials
        .as_array()
        .expect("materials")
        .iter()
        .find(|row| row["name"] == "Gula pasir")
        .expect("gula pasir");

    assert_eq!(row["base_unit"]["code"], "kg");
    assert_eq!(row["min_stock"], "3.000");
    assert_eq!(row["shelf_life_days"], Value::Null);
}
