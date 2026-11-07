use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::PgPool;
use takar_api::{
    auth::{jwt::JwtConfig, otp::OtpSender},
    router, AppState,
};
use tower::ServiceExt;
use uuid::Uuid;

const SECRET: &str = "test-secret";

#[derive(Clone, Default)]
struct CaptureOtp {
    codes: Arc<Mutex<Vec<(String, String)>>>,
}

#[async_trait]
impl OtpSender for CaptureOtp {
    async fn send(&self, contact: &str, code: &str) -> Result<(), String> {
        self.codes
            .lock()
            .expect("lock")
            .push((contact.to_string(), code.to_string()));
        Ok(())
    }
}

impl CaptureOtp {
    fn last_code(&self) -> String {
        self.codes
            .lock()
            .expect("lock")
            .last()
            .expect("an OTP was sent")
            .1
            .clone()
    }
}

fn state(pool: PgPool, otp: CaptureOtp) -> AppState {
    AppState {
        db: Some(pool),
        jwt: JwtConfig {
            secret: SECRET.to_string(),
            ttl_seconds: 3_600,
        },
        otp: Arc::new(otp),
        otp_ttl_seconds: 300,
        jwt_ttl_seconds: 3_600,
    }
}

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: Option<&str>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");

    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }

    let request = builder
        .body(Body::from(
            body.map(|value| value.to_string()).unwrap_or_default(),
        ))
        .expect("request");

    let response = app.clone().oneshot(request).await.expect("response");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();

    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };

    (status, value)
}

async fn signup(app: &Router, otp: &CaptureOtp, contact: &str) -> Value {
    let (status, _) = call(
        app,
        "POST",
        "/auth/otp/request",
        Some(json!({ "contact": contact })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);

    let code = otp.last_code();

    let (status, body) = call(
        app,
        "POST",
        "/auth/otp/verify",
        Some(json!({
            "contact": contact,
            "code": code,
            "name": "Test Owner",
            "tenant_name": "Test Tenant"
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "verify failed: {body}");

    body
}

#[sqlx::test(migrations = "../../migrations")]
async fn signup_issues_a_session_token(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = router(state(pool, otp.clone()));

    let body = signup(&app, &otp, "owner@example.test").await;

    assert_eq!(body["requires_tenant_selection"], false);
    assert_eq!(body["tenants"].as_array().expect("tenants").len(), 1);

    let token = body["access_token"].as_str().expect("token");
    let (status, me) = call(&app, "GET", "/me", None, Some(token)).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["role"], "owner");
    assert_eq!(me["user"]["name"], "Test Owner");
    assert_eq!(me["tenant"]["name"], "Test Tenant");

    let permissions = me["permissions"].as_array().expect("permissions");
    assert!(permissions.iter().any(|value| value == "tenant_manage"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn second_tenant_requires_selection(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = router(state(pool.clone(), otp.clone()));

    let first = signup(&app, &otp, "multi@example.test").await;
    let first_token = first["access_token"].as_str().expect("token");

    let (_, me) = call(&app, "GET", "/me", None, Some(first_token)).await;
    let user_id: Uuid = me["user"]["id"]
        .as_str()
        .expect("user id")
        .parse()
        .expect("uuid");

    let second_tenant = Uuid::new_v4();
    sqlx::query("insert into tenants (id, name) values ($1, 'Second Tenant')")
        .bind(second_tenant)
        .execute(&pool)
        .await
        .expect("insert tenant");
    sqlx::query("insert into memberships (tenant_id, user_id, role) values ($1, $2, 'admin')")
        .bind(second_tenant)
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("insert membership");

    let body = signup(&app, &otp, "multi@example.test").await;

    assert_eq!(body["requires_tenant_selection"], true);
    assert_eq!(body["tenants"].as_array().expect("tenants").len(), 2);

    let selection_token = body["access_token"].as_str().expect("token");

    let (status, _) = call(&app, "GET", "/me", None, Some(selection_token)).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "tenant-selection tokens must not reach protected routes"
    );

    let (status, tenants) = call(&app, "GET", "/auth/tenants", None, Some(selection_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(tenants.as_array().expect("tenants").len(), 2);

    let (status, selected) = call(
        &app,
        "POST",
        "/auth/select-tenant",
        Some(json!({ "tenant_id": second_tenant })),
        Some(selection_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let session = selected["access_token"].as_str().expect("token");
    let (status, me) = call(&app, "GET", "/me", None, Some(session)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["tenant"]["id"], second_tenant.to_string());
    assert_eq!(me["role"], "admin");
}

#[sqlx::test(migrations = "../../migrations")]
async fn wrong_code_is_rejected(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = router(state(pool, otp.clone()));

    let (status, _) = call(
        &app,
        "POST",
        "/auth/otp/request",
        Some(json!({ "contact": "wrong@example.test" })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);

    let code = otp.last_code();
    let wrong = if code == "000000" { "000001" } else { "000000" };

    let (status, body) = call(
        &app,
        "POST",
        "/auth/otp/verify",
        Some(json!({ "contact": "wrong@example.test", "code": wrong })),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "invalid_code");
}

#[sqlx::test(migrations = "../../migrations")]
async fn suspended_membership_loses_access(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = router(state(pool.clone(), otp.clone()));

    let body = signup(&app, &otp, "suspended@example.test").await;
    let token = body["access_token"].as_str().expect("token");

    sqlx::query("update memberships set status = 'suspended' where user_id = (select id from users where email = $1)")
        .bind("suspended@example.test")
        .execute(&pool)
        .await
        .expect("suspend membership");

    let (status, _) = call(&app, "GET", "/me", None, Some(token)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../../migrations")]
async fn selecting_a_foreign_tenant_is_forbidden(pool: PgPool) {
    let otp = CaptureOtp::default();
    let app = router(state(pool.clone(), otp.clone()));

    let first = signup(&app, &otp, "foreign@example.test").await;
    let first_token = first["access_token"].as_str().expect("token");

    let (_, me) = call(&app, "GET", "/me", None, Some(first_token)).await;
    let user_id: Uuid = me["user"]["id"]
        .as_str()
        .expect("user id")
        .parse()
        .expect("uuid");

    let second_tenant = Uuid::new_v4();
    sqlx::query("insert into tenants (id, name) values ($1, 'Other Tenant')")
        .bind(second_tenant)
        .execute(&pool)
        .await
        .expect("insert tenant");
    sqlx::query("insert into memberships (tenant_id, user_id, role) values ($1, $2, 'viewer')")
        .bind(second_tenant)
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("insert membership");

    let body = signup(&app, &otp, "foreign@example.test").await;
    let selection_token = body["access_token"].as_str().expect("token");

    let (status, _) = call(
        &app,
        "POST",
        "/auth/select-tenant",
        Some(json!({ "tenant_id": Uuid::new_v4() })),
        Some(selection_token),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}
