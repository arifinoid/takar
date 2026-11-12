mod common;

use axum::{http::StatusCode, Router};
use common::{call, call_raw, signup, test_app, CaptureOtp};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

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

async fn set_stock(pool: &PgPool, product_id: Uuid, qty: &str) {
    sqlx::query("update products set stock_qty = $2 where id = $1")
        .bind(product_id)
        .bind(qty.parse::<rust_decimal::Decimal>().expect("qty"))
        .execute(pool)
        .await
        .expect("stock");
}

async fn stock_of(pool: &PgPool, product_id: Uuid) -> String {
    let (qty,): (rust_decimal::Decimal,) =
        sqlx::query_as("select stock_qty from products where id = $1")
            .bind(product_id)
            .fetch_one(pool)
            .await
            .expect("stock");

    qty.to_string()
}

async fn import(app: &Router, token: &str, csv: &str) -> (StatusCode, Value) {
    call_raw(app, "/sales/import?format=csv", csv, Some(token)).await
}

async fn import_with_offset(
    app: &Router,
    token: &str,
    csv: &str,
    offset: i32,
) -> (StatusCode, Value) {
    call_raw(
        app,
        &format!("/sales/import?format=csv&tz_offset={offset}"),
        csv,
        Some(token),
    )
    .await
}

async fn sales(app: &Router, token: &str) -> (StatusCode, Value) {
    call(app, "GET", "/sales", None, Some(token)).await
}

#[sqlx::test(migrations = "../../migrations")]
async fn rows_sharing_a_reference_become_one_sale(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let bread = product(&app, token, "Roti sobek", "15000").await;
    let cake = product(&app, token, "Bolu pandan", "50000").await;
    set_stock(&pool, bread, "10").await;
    set_stock(&pool, cake, "10").await;

    let csv = "sold_at,channel,product,qty,price,note,sale_ref\n\
               2026-11-01 09:30,toko,Roti sobek,2,15000,,INV-001\n\
               2026-11-01 09:30,toko,Bolu pandan,1,,,INV-001\n";

    let (status, body) = import(&app, token, csv).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["created"], 1);
    assert_eq!(body["duplicate"], 0);
    assert_eq!(body["rejected"], 0);
    assert_eq!(body["rows"].as_array().expect("rows").len(), 2);
    assert_eq!(body["rows"][0]["status"], "created");
    assert_eq!(body["rows"][0]["sale_ref"], "INV-001");
    assert_eq!(body["rows"][1]["sale_id"], body["rows"][0]["sale_id"]);

    let (status, body) = sales(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let items = body["items"].as_array().expect("sales");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["items"].as_array().expect("lines").len(), 2);
    assert_eq!(items[0]["total"], "80000.00");
    assert_eq!(items[0]["channel"], "toko");

    assert_eq!(stock_of(&pool, bread).await, "8.000");
    assert_eq!(stock_of(&pool, cake).await, "9.000");
}

#[sqlx::test(migrations = "../../migrations")]
async fn importing_the_same_file_again_changes_nothing(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let bread = product(&app, token, "Roti sobek", "15000").await;
    set_stock(&pool, bread, "10").await;

    let csv = "product,qty,price,sale_ref\nRoti sobek,2,15000,INV-7\n";

    let (status, body) = import(&app, token, csv).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["created"], 1);

    let (status, body) = import(&app, token, csv).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["created"], 0);
    assert_eq!(body["duplicate"], 1);
    assert_eq!(body["rows"][0]["status"], "duplicate");

    assert_eq!(stock_of(&pool, bread).await, "8.000");

    let (count,): (i64,) = sqlx::query_as("select count(*) from sales")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(count, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_bad_row_does_not_block_the_others(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let bread = product(&app, token, "Roti sobek", "15000").await;
    set_stock(&pool, bread, "10").await;

    let csv = "product,qty\n\
               Roti sobek,1\n\
               Kue lapis,1\n\
               Roti sobek,2\n";

    let (status, body) = import(&app, token, csv).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["created"], 2);
    assert_eq!(body["rejected"], 1);
    assert_eq!(body["rows"][1]["row"], 3);
    assert_eq!(body["rows"][1]["code"], "unknown_product");
    assert_eq!(body["rows"][1]["status"], "rejected");

    // Both good rows landed, the bad one did not stop them.
    assert_eq!(stock_of(&pool, bread).await, "7.000");

    let (count,): (i64,) = sqlx::query_as("select count(*) from sales")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(count, 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_short_group_is_rejected_without_writing(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let bread = product(&app, token, "Roti sobek", "15000").await;
    set_stock(&pool, bread, "1").await;

    let csv = "product,qty,sale_ref\nRoti sobek,5,INV-9\n";

    let (status, body) = import(&app, token, csv).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["created"], 0);
    assert_eq!(body["rejected"], 1);
    assert_eq!(body["rows"][0]["code"], "insufficient_stock");

    assert_eq!(stock_of(&pool, bread).await, "1.000");

    let (count,): (i64,) = sqlx::query_as("select count(*) from sales")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(count, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn naive_timestamps_use_the_shop_offset(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let bread = product(&app, token, "Roti sobek", "15000").await;
    set_stock(&pool, bread, "10").await;

    let csv = "product,qty,sold_at\nRoti sobek,1,2026-11-01 09:30\n";

    let (status, body) = import_with_offset(&app, token, csv, 420).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["created"], 1);

    let (status, body) = sales(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["items"][0]["sold_at"], "2026-11-01T02:30:00Z");
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_explicit_offset_wins_over_the_shop_one(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let bread = product(&app, token, "Roti sobek", "15000").await;
    set_stock(&pool, bread, "10").await;

    let csv = "product,qty,sold_at\nRoti sobek,1,2026-11-01T09:30:00+07:00\n";

    let (status, body) = import_with_offset(&app, token, csv, 0).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (status, body) = sales(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["items"][0]["sold_at"], "2026-11-01T02:30:00Z");
}

#[sqlx::test(migrations = "../../migrations")]
async fn headers_can_be_reordered_and_use_local_names(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let bread = product(&app, token, "Roti sobek", "15000").await;
    set_stock(&pool, bread, "10").await;

    let csv = "kanal,jumlah,produk,harga\nreseller,3,Roti sobek,12000\n";

    let (status, body) = import(&app, token, csv).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["created"], 1);

    let (status, body) = sales(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["items"][0]["channel"], "reseller");
    assert_eq!(body["items"][0]["total"], "36000.00");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_file_without_a_product_column_is_refused(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let (status, body) = import(&app, token, "qty,price\n1,1000\n").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "missing_header");

    let (status, body) = import(&app, token, "").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "empty_file");
}

#[sqlx::test(migrations = "../../migrations")]
async fn excel_files_are_refused_with_a_clear_code(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let (status, body) =
        call_raw(&app, "/sales/import", "PK\u{3}\u{4}not-really", Some(token)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "unsupported_format");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_viewer_cannot_import(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    sqlx::query("update memberships set role = 'viewer'")
        .execute(&pool)
        .await
        .expect("demote owner");

    let (status, body) = import(&app, token, "product,qty\nRoti,1\n").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"]["code"], "permission_denied");
}
