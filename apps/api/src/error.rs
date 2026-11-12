use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

#[derive(Debug)]
pub enum ApiError {
    BadRequest(&'static str),
    Unauthorized(&'static str),
    Forbidden(&'static str),
    /// A refusal that carries structured details, so the client can show the
    /// limit and the current usage instead of a bare code.
    ForbiddenDetails(&'static str, serde_json::Value),
    NotFound(&'static str),
    TooManyRequests(&'static str),
    Conflict(&'static str),
    /// A conflict that carries structured details, so the client can show
    /// exactly what blocked the action instead of a bare code.
    ConflictDetails(&'static str, serde_json::Value),
    ServiceUnavailable,
    /// A dependency outside our control failed: email, WhatsApp, a gateway.
    /// Carries its own code so it is never reported as a database problem.
    Dependency(&'static str),
    Internal(anyhow::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, details) = match self {
            ApiError::BadRequest(code) => (StatusCode::BAD_REQUEST, code, None),
            ApiError::Unauthorized(code) => (StatusCode::UNAUTHORIZED, code, None),
            ApiError::Forbidden(code) => (StatusCode::FORBIDDEN, code, None),
            ApiError::ForbiddenDetails(code, details) => {
                (StatusCode::FORBIDDEN, code, Some(details))
            }
            ApiError::NotFound(code) => (StatusCode::NOT_FOUND, code, None),
            ApiError::TooManyRequests(code) => (StatusCode::TOO_MANY_REQUESTS, code, None),
            ApiError::Conflict(code) => (StatusCode::CONFLICT, code, None),
            ApiError::ConflictDetails(code, details) => (StatusCode::CONFLICT, code, Some(details)),
            ApiError::ServiceUnavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "database_unavailable",
                None,
            ),
            ApiError::Dependency(code) => (StatusCode::SERVICE_UNAVAILABLE, code, None),
            ApiError::Internal(error) => {
                tracing::error!(%error, "internal error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error", None)
            }
        };

        let mut body = json!({
            "error": {
                "code": code,
                "message": code.replace('_', " "),
            }
        });

        if let Some(details) = details {
            body["error"]["details"] = details;
        }

        (status, Json(body)).into_response()
    }
}

impl ApiError {
    /// Stable code for callers that report a failure per item, such as the
    /// offline sync endpoint.
    pub fn code(&self) -> &'static str {
        match self {
            ApiError::BadRequest(code)
            | ApiError::Unauthorized(code)
            | ApiError::Forbidden(code)
            | ApiError::ForbiddenDetails(code, _)
            | ApiError::NotFound(code)
            | ApiError::TooManyRequests(code)
            | ApiError::Conflict(code)
            | ApiError::ConflictDetails(code, _)
            | ApiError::Dependency(code) => code,
            ApiError::ServiceUnavailable => "database_unavailable",
            ApiError::Internal(_) => "internal_error",
        }
    }

    /// Maps write-time Postgres errors onto API errors.
    ///
    /// `duplicate` names the conflict code the caller wants for a unique
    /// violation, since only the handler knows which index was hit.
    pub fn from_write(error: sqlx::Error, duplicate: &'static str) -> Self {
        if let sqlx::Error::Database(database) = &error {
            match database.code().as_deref() {
                Some("23505") => return ApiError::Conflict(duplicate),
                Some("23503") => return ApiError::BadRequest("invalid_reference"),
                Some("23514") => return ApiError::BadRequest("invalid_value"),
                _ => {}
            }
        }

        ApiError::Internal(error.into())
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        ApiError::Internal(error.into())
    }
}

impl From<jsonwebtoken::errors::Error> for ApiError {
    fn from(error: jsonwebtoken::errors::Error) -> Self {
        tracing::debug!(%error, "jwt rejected");
        ApiError::Unauthorized("invalid_token")
    }
}
