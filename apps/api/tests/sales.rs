mod common;

use axum::{http::StatusCode, Router};
use common::{call, signup, test_app, CaptureOtp};
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

async fn sell(app: &Router, token: &str, body: Value) -> (StatusCode, Value) {
    call(app, "POST", "/sales", Some(body), Some(token)).await
}

async fn sales(app: &Router, token: &str) -> (StatusCode, Value) {
    call(app, "GET", "/sales", None, Some(token)).await
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_sale_decrements_stock_and_snapshots_the_price(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let bread = product(&app, token, "Roti sobek", "15000").await;
    set_stock(&pool, bread, "10").await;

    let (status, body) = sell(
        &app,
        token,
        json!({ "items": [{ "product_id": bread, "qty": "3" }] }),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["channel"], "toko");
    assert_eq!(body["total"], "45000.00");
    assert_eq!(body["items"][0]["price"], "15000.00");
    assert_eq!(body["items"][0]["subtotal"], "45000.00");
    assert_eq!(body["items"][0]["product_name"], "Roti sobek");

    assert_eq!(stock_of(&pool, bread).await, "7.000");

    // Changing the product price later must not rewrite the sale.
    sqlx::query("update products set sell_price = 20000 where id = $1")
        .bind(bread)
        .execute(&pool)
        .await
        .expect("reprice");

    let (status, body) = sales(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["items"][0]["total"], "45000.00");
    assert_eq!(body["items"][0]["items"][0]["price"], "15000.00");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_line_price_overrides_the_product_price(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let cake = product(&app, token, "Bolu", "50000").await;
    set_stock(&pool, cake, "5").await;

    let (status, body) = sell(
        &app,
        token,
        json!({
            "channel": "reseller",
            "items": [{ "product_id": cake, "qty": "2", "price": "40000" }],
        }),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["channel"], "reseller");
    assert_eq!(body["total"], "80000.00");

    let (sell_price,): (rust_decimal::Decimal,) =
        sqlx::query_as("select sell_price from products where id = $1")
            .bind(cake)
            .fetch_one(&pool)
            .await
            .expect("price");
    assert_eq!(sell_price.to_string(), "50000.00");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_short_line_rejects_the_whole_sale(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let bread = product(&app, token, "Roti", "10000").await;
    let cake = product(&app, token, "Bolu", "20000").await;
    set_stock(&pool, bread, "10").await;
    set_stock(&pool, cake, "1").await;

    let (status, body) = sell(
        &app,
        token,
        json!({
            "items": [
                { "product_id": bread, "qty": "2" },
                { "product_id": cake, "qty": "5" },
            ],
        }),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "insufficient_stock");
    assert_eq!(
        body["error"]["details"]["shortages"][0]["product_name"],
        "Bolu"
    );
    assert_eq!(
        body["error"]["details"]["shortages"][0]["available"],
        "1.000"
    );
    assert_eq!(
        body["error"]["details"]["shortages"][0]["requested"],
        "5.000"
    );

    // Nothing was written: the bread line must not have moved either.
    assert_eq!(stock_of(&pool, bread).await, "10.000");
    assert_eq!(stock_of(&pool, cake).await, "1.000");

    let (count,): (i64,) = sqlx::query_as("select count(*) from sales")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(count, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn replaying_a_client_ref_creates_one_sale(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let bread = product(&app, token, "Roti", "10000").await;
    set_stock(&pool, bread, "10").await;

    let client_ref = Uuid::new_v4();
    let body = json!({
        "client_ref": client_ref,
        "items": [{ "product_id": bread, "qty": "2" }],
    });

    let (status, first) = sell(&app, token, body.clone()).await;
    assert_eq!(status, StatusCode::CREATED, "{first}");

    let (status, second) = sell(&app, token, body).await;
    assert_eq!(status, StatusCode::OK, "{second}");
    assert_eq!(second["id"], first["id"]);

    assert_eq!(stock_of(&pool, bread).await, "8.000");

    let (count,): (i64,) = sqlx::query_as("select count(*) from sales")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(count, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn another_tenants_product_is_not_found(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());

    let first = signup(&app, &otp, "first@example.test").await;
    let first_token = first["access_token"].as_str().expect("token");
    let bread = product(&app, first_token, "Roti", "10000").await;
    set_stock(&pool, bread, "10").await;

    let second = signup(&app, &otp, "second@example.test").await;
    let second_token = second["access_token"].as_str().expect("token");

    let (status, body) = sell(
        &app,
        second_token,
        json!({ "items": [{ "product_id": bread, "qty": "1" }] }),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error"]["code"], "product_not_found");
    assert_eq!(stock_of(&pool, bread).await, "10.000");

    let (status, body) = sales(&app, second_token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["items"].as_array().expect("items").len(), 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_list_is_newest_first_with_its_lines(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let bread = product(&app, token, "Roti", "10000").await;
    let cake = product(&app, token, "Bolu", "20000").await;
    set_stock(&pool, bread, "20").await;
    set_stock(&pool, cake, "20").await;

    let (status, _) = sell(
        &app,
        token,
        json!({
            "sold_at": "2026-11-01T09:00:00Z",
            "channel": "online",
            "items": [{ "product_id": bread, "qty": "1" }],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, _) = sell(
        &app,
        token,
        json!({
            "sold_at": "2026-11-02T09:00:00Z",
            "channel": "toko",
            "note": "pagi",
            "items": [
                { "product_id": bread, "qty": "2" },
                { "product_id": cake, "qty": "1" },
            ],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, body) = sales(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let items = body["items"].as_array().expect("items");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["channel"], "toko");
    assert_eq!(items[0]["note"], "pagi");
    assert_eq!(items[0]["total"], "40000.00");
    assert_eq!(items[0]["items"].as_array().expect("lines").len(), 2);
    assert_eq!(items[1]["channel"], "online");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_viewer_can_read_but_not_record(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let bread = product(&app, token, "Roti", "10000").await;
    set_stock(&pool, bread, "10").await;

    sqlx::query("update memberships set role = 'viewer'")
        .execute(&pool)
        .await
        .expect("demote owner");

    let (status, body) = sales(&app, token).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (status, body) = sell(
        &app,
        token,
        json!({ "items": [{ "product_id": bread, "qty": "1" }] }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"]["code"], "permission_denied");
}

#[sqlx::test(migrations = "../../migrations")]
async fn malformed_sales_are_rejected_before_any_write(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = test_app(pool.clone(), otp.clone());
    let owner = signup(&app, &otp, "owner@example.test").await;
    let token = owner["access_token"].as_str().expect("token");

    let bread = product(&app, token, "Roti", "10000").await;
    set_stock(&pool, bread, "10").await;

    let cases = [
        (json!({ "items": [] }), "items_required"),
        (
            json!({ "items": [{ "product_id": bread, "qty": "0" }] }),
            "invalid_qty",
        ),
        (
            json!({ "channel": "pasar", "items": [{ "product_id": bread, "qty": "1" }] }),
            "invalid_channel",
        ),
        (
            json!({
                "items": [
                    { "product_id": bread, "qty": "1" },
                    { "product_id": bread, "qty": "2" },
                ],
            }),
            "duplicate_product",
        ),
    ];

    for (body, code) in cases {
        let (status, response) = sell(&app, token, body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{response}");
        assert_eq!(response["error"]["code"], code);
    }

    assert_eq!(stock_of(&pool, bread).await, "10.000");

    let (count,): (i64,) = sqlx::query_as("select count(*) from sales")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(count, 0);
}
