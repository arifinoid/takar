//! Process configuration, read once at startup.

pub const DEV_JWT_SECRET: &str = "dev-only-change-me";

pub struct Config {
    pub port: u16,
    pub database_url: Option<String>,
    pub jwt_secret: String,
    pub otp_ttl_seconds: i64,
    pub jwt_ttl_seconds: i64,
    /// Fixed OTP for local sign-in. Honored by debug builds only.
    pub dev_otp_code: Option<String>,
}

impl Config {
    pub fn from_env() -> Self {
        let port = std::env::var("PORT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(3001);

        let database_url = std::env::var("DATABASE_URL")
            .ok()
            .filter(|value| !value.is_empty());

        let jwt_secret = std::env::var("JWT_SECRET")
            .ok()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEV_JWT_SECRET.to_string());

        let otp_ttl_seconds = std::env::var("OTP_TTL_SECONDS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(300);

        let jwt_ttl_seconds = std::env::var("JWT_TTL_SECONDS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(86_400);

        let dev_otp_code = std::env::var("DEV_OTP_CODE")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| {
                let valid =
                    value.len() == 6 && value.chars().all(|character| character.is_ascii_digit());
                if !valid {
                    tracing::warn!("DEV_OTP_CODE must be exactly 6 digits, ignoring it");
                }
                valid
            });

        Self {
            port,
            database_url,
            jwt_secret,
            otp_ttl_seconds,
            jwt_ttl_seconds,
            dev_otp_code,
        }
    }
}
