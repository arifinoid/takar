//! Outbound messages.
//!
//! One trait, one implementation per provider, selected by configuration. OTP
//! delivery uses it today; alert delivery (P1-07) will use the same seam, so a
//! provider swap stays a config change rather than a code change.

pub mod brevo;
pub mod fonnte;

use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;

pub use brevo::BrevoTransport;
pub use fonnte::FonnteTransport;

/// A provider that hangs must not hang the request that triggered it.
const TIMEOUT: Duration = Duration::from_secs(5);
const RETRY_DELAY: Duration = Duration::from_millis(300);

/// One message, whatever the channel. Chat transports ignore the subject.
pub struct Message<'a> {
    pub to: &'a str,
    pub subject: &'a str,
    pub text: &'a str,
}

#[derive(Debug)]
pub struct TransportError {
    /// Worth one more attempt: network trouble, a 5xx, or a rate limit.
    pub retryable: bool,
    pub message: String,
}

impl TransportError {
    pub fn retryable(message: impl Into<String>) -> Self {
        Self {
            retryable: true,
            message: message.into(),
        }
    }

    pub fn permanent(message: impl Into<String>) -> Self {
        Self {
            retryable: false,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

#[async_trait]
pub trait Transport: Send + Sync {
    /// Stable name for logs and startup output.
    fn name(&self) -> &'static str;

    async fn send(&self, message: &Message<'_>) -> Result<(), TransportError>;
}

/// Development transport: the message goes to the log instead of a provider.
pub struct LogTransport;

#[async_trait]
impl Transport for LogTransport {
    fn name(&self) -> &'static str {
        "log"
    }

    async fn send(&self, message: &Message<'_>) -> Result<(), TransportError> {
        tracing::info!(
            to = message.to,
            subject = message.subject,
            text = message.text,
            "message not sent: log transport"
        );
        Ok(())
    }
}

pub fn client() -> Client {
    Client::builder()
        .timeout(TIMEOUT)
        .build()
        .expect("http client")
}

/// Sends once, retries once when the failure looks transient.
pub async fn deliver(
    transport: &dyn Transport,
    message: &Message<'_>,
) -> Result<(), TransportError> {
    match transport.send(message).await {
        Err(error) if error.retryable => {
            tracing::warn!(
                provider = transport.name(),
                error = %error,
                "delivery failed, retrying once"
            );
            tokio::time::sleep(RETRY_DELAY).await;
            transport.send(message).await
        }
        result => result,
    }
}

/// Turns a provider response into a transport error, keeping the body short.
pub(crate) async fn check(
    response: reqwest::Response,
) -> Result<reqwest::Response, TransportError> {
    let status = response.status();

    if status.is_success() {
        return Ok(response);
    }

    let body: String = response.text().await.unwrap_or_default();
    let excerpt: String = body.chars().take(200).collect();

    let error = format!("{status}: {excerpt}");

    if status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        Err(TransportError::retryable(error))
    } else {
        Err(TransportError::permanent(error))
    }
}
