//! Outbound messages.
//!
//! One trait, one implementation per provider, selected by configuration. OTP
//! delivery uses it today; alert delivery (P1-07) will use the same seam, so a
//! provider swap stays a config change rather than a code change.

pub mod brevo;
pub mod fonnte;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;

pub use brevo::BrevoTransport;
pub use fonnte::FonnteTransport;

use crate::config::Config;

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

/// Where a message goes. Each transport speaks exactly one of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Email,
    WhatsApp,
}

impl Channel {
    pub fn as_str(self) -> &'static str {
        match self {
            Channel::Email => "email",
            Channel::WhatsApp => "whatsapp",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "email" => Some(Channel::Email),
            "whatsapp" => Some(Channel::WhatsApp),
            _ => None,
        }
    }
}

/// Picks the transport for a channel, with a development fallback.
///
/// Both OTP and alerts go through this, so a provider swap is a config change
/// for every message the product sends.
pub struct Router {
    email: Option<Arc<dyn Transport>>,
    phone: Option<Arc<dyn Transport>>,
    fallback: Option<Arc<dyn Transport>>,
}

impl Router {
    pub fn new(
        email: Option<Arc<dyn Transport>>,
        phone: Option<Arc<dyn Transport>>,
        fallback: Option<Arc<dyn Transport>>,
    ) -> Self {
        Self {
            email,
            phone,
            fallback,
        }
    }

    pub fn transport(&self, channel: Channel) -> Option<&Arc<dyn Transport>> {
        let configured = match channel {
            Channel::Email => self.email.as_ref(),
            Channel::WhatsApp => self.phone.as_ref(),
        };

        configured.or(self.fallback.as_ref())
    }

    pub async fn send(
        &self,
        channel: Channel,
        to: &str,
        subject: &str,
        text: &str,
    ) -> Result<(), TransportError> {
        let transport = self.transport(channel).ok_or_else(|| {
            TransportError::permanent(format!("no transport configured for {}", channel.as_str()))
        })?;

        deliver(transport.as_ref(), &Message { to, subject, text }).await
    }
}

/// Builds the router once, at startup, and reports what is live.
pub fn router(config: &Config) -> Router {
    let client = client();

    let email: Option<Arc<dyn Transport>> = config.otp.email.as_ref().map(|provider| {
        Arc::new(BrevoTransport::new(
            client.clone(),
            provider.api_key.clone(),
            provider.from_email.clone(),
            provider.from_name.clone(),
        )) as Arc<dyn Transport>
    });

    let phone: Option<Arc<dyn Transport>> = config.otp.whatsapp.as_ref().map(|provider| {
        Arc::new(FonnteTransport::new(client.clone(), provider.token.clone())) as Arc<dyn Transport>
    });

    tracing::info!(
        email = email.as_ref().map(|transport| transport.name()),
        whatsapp = phone.as_ref().map(|transport| transport.name()),
        fallback = cfg!(debug_assertions),
        "message delivery"
    );

    if email.is_none() && phone.is_none() && !cfg!(debug_assertions) {
        tracing::warn!("no message transport configured: nothing can be delivered");
    }

    Router::new(
        email,
        phone,
        cfg!(debug_assertions).then(|| Arc::new(LogTransport) as Arc<dyn Transport>),
    )
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
