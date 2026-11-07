use chrono::Utc;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::ApiError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenScope {
    Session,
    TenantSelect,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,
    pub tenant_id: Option<Uuid>,
    pub scope: TokenScope,
    pub iat: i64,
    pub exp: i64,
}

#[derive(Clone)]
pub struct JwtConfig {
    pub secret: String,
    pub ttl_seconds: i64,
}

impl JwtConfig {
    pub fn issue(
        &self,
        user_id: Uuid,
        tenant_id: Option<Uuid>,
        scope: TokenScope,
        ttl_seconds: i64,
    ) -> Result<String, ApiError> {
        let now = Utc::now().timestamp();
        let claims = Claims {
            sub: user_id,
            tenant_id,
            scope,
            iat: now,
            exp: now + ttl_seconds,
        };

        encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(self.secret.as_bytes()),
        )
        .map_err(ApiError::from)
    }

    pub fn decode(&self, token: &str) -> Result<Claims, ApiError> {
        let data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.secret.as_bytes()),
            &Validation::new(Algorithm::HS256),
        )?;

        Ok(data.claims)
    }
}
