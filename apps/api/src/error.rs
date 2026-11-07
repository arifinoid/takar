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
    NotFound(&'static str),
    TooManyRequests(&'static str),
    Conflict(&'static str),
    ServiceUnavailable,
    Internal(anyhow::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = match self {
            ApiError::BadRequest(code) => (StatusCode::BAD_REQUEST, code),
            ApiError::Unauthorized(code) => (StatusCode::UNAUTHORIZED, code),
            ApiError::Forbidden(code) => (StatusCode::FORBIDDEN, code),
            ApiError::NotFound(code) => (StatusCode::NOT_FOUND, code),
            ApiError::TooManyRequests(code) => (StatusCode::TOO_MANY_REQUESTS, code),
            ApiError::Conflict(code) => (StatusCode::CONFLICT, code),
            ApiError::ServiceUnavailable => {
                (StatusCode::SERVICE_UNAVAILABLE, "database_unavailable")
            }
            ApiError::Internal(error) => {
                tracing::error!(%error, "internal error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
            }
        };

        (
            status,
            Json(json!({
                "error": {
                    "code": code,
                    "message": code.replace('_', " "),
                }
            })),
        )
            .into_response()
    }
}

impl ApiError {
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
