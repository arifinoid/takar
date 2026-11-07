#![allow(dead_code)]

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

pub const SECRET: &str = "test-secret";

#[derive(Clone, Default)]
pub struct CaptureOtp {
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
    pub fn last_code(&self) -> String {
        self.codes
            .lock()
            .expect("lock")
            .last()
            .expect("an OTP was sent")
            .1
            .clone()
    }
}

pub fn state(pool: PgPool, otp: CaptureOtp) -> AppState {
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

pub fn test_app(pool: PgPool, otp: CaptureOtp) -> Router {
    router(state(pool, otp))
}

pub async fn call(
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

/// First login for a contact with no memberships: creates the tenant and the
/// owner membership, so the response carries a usable session token.
pub async fn signup(app: &Router, otp: &CaptureOtp, contact: &str) -> Value {
    let body = login(app, otp, contact).await;

    assert_eq!(
        body["pending_invitation"], false,
        "unexpected invitation for {contact}"
    );
    assert_eq!(body["requires_tenant_selection"], false);

    body
}

/// Requests a code and verifies it without asserting the outcome.
pub async fn login(app: &Router, otp: &CaptureOtp, contact: &str) -> Value {
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

pub async fn tenant_of(app: &Router, token: &str) -> Uuid {
    let (status, me) = call(app, "GET", "/me", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK);

    me["tenant"]["id"]
        .as_str()
        .expect("tenant id")
        .parse()
        .expect("uuid")
}

pub async fn user_of(app: &Router, token: &str) -> Uuid {
    let (status, me) = call(app, "GET", "/me", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK);

    me["user"]["id"]
        .as_str()
        .expect("user id")
        .parse()
        .expect("uuid")
}

/// Creates a user and an active membership without going through invitations.
pub async fn add_member(
    pool: &PgPool,
    tenant_id: Uuid,
    contact: &str,
    name: &str,
    role: &str,
) -> Uuid {
    let user_id = Uuid::new_v4();

    sqlx::query("insert into users (id, email, name) values ($1, $2, $3)")
        .bind(user_id)
        .bind(contact)
        .bind(name)
        .execute(pool)
        .await
        .expect("insert user");

    sqlx::query("insert into memberships (tenant_id, user_id, role) values ($1, $2, $3)")
        .bind(tenant_id)
        .bind(user_id)
        .bind(role)
        .execute(pool)
        .await
        .expect("insert membership");

    user_id
}
