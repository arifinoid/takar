//! Shared domain types and the HTTP API contract.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HealthResponse {
    /// Always `ok` when the process can serve traffic.
    pub status: String,
    /// Service name, useful when several processes share a log stream.
    pub service: String,
    /// Crate version compiled into the binary.
    pub version: String,
}

impl HealthResponse {
    pub fn ok() -> Self {
        Self {
            status: "ok".to_string(),
            service: "takar-api".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}
