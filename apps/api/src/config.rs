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
    /// Public base URL, used for links inside messages.
    pub app_url: String,
    pub otp: OtpConfig,
}

/// Delivery providers, all optional. Nothing here is required to boot: with no
/// provider configured, debug builds log the code and release builds refuse.
#[derive(Default)]
pub struct OtpConfig {
    pub email: Option<EmailProvider>,
    pub whatsapp: Option<WhatsAppProvider>,
}

pub struct EmailProvider {
    pub api_key: String,
    pub from_email: String,
    pub from_name: String,
}

pub struct WhatsAppProvider {
    pub token: String,
}

fn env_string(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
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

        let app_url = std::env::var("APP_URL")
            .ok()
            .map(|value| value.trim().trim_end_matches('/').to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "https://takar.arifinoid.web.id".to_string());

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
            app_url,
            otp: otp_config(),
        }
    }
}

fn otp_config() -> OtpConfig {
    OtpConfig {
        email: email_provider(),
        whatsapp: whatsapp_provider(),
    }
}

fn email_provider() -> Option<EmailProvider> {
    match env_string("EMAIL_PROVIDER").as_deref() {
        None | Some("log") => None,
        Some("brevo") => {
            let api_key = env_string("BREVO_API_KEY");
            let from_email = env_string("EMAIL_FROM");

            match (api_key, from_email) {
                (Some(api_key), Some(from_email)) => Some(EmailProvider {
                    api_key,
                    from_email,
                    from_name: env_string("EMAIL_FROM_NAME").unwrap_or_else(|| "Takar".to_string()),
                }),
                _ => {
                    tracing::warn!(
                        "EMAIL_PROVIDER=brevo needs BREVO_API_KEY and EMAIL_FROM; email disabled"
                    );
                    None
                }
            }
        }
        Some(other) => {
            tracing::warn!(provider = other, "unknown EMAIL_PROVIDER; email disabled");
            None
        }
    }
}

fn whatsapp_provider() -> Option<WhatsAppProvider> {
    match env_string("WHATSAPP_PROVIDER").as_deref() {
        None | Some("log") => None,
        Some("fonnte") => match env_string("FONNTE_TOKEN") {
            Some(token) => Some(WhatsAppProvider { token }),
            None => {
                tracing::warn!("WHATSAPP_PROVIDER=fonnte needs FONNTE_TOKEN; whatsapp disabled");
                None
            }
        },
        Some(other) => {
            tracing::warn!(
                provider = other,
                "unknown WHATSAPP_PROVIDER; whatsapp disabled"
            );
            None
        }
    }
}
