use std::sync::Arc;

use async_trait::async_trait;
use rand::Rng;

use crate::auth::contact::Contact;
use crate::config::Config;
use crate::notify::{self, LogTransport, Message, Transport};

/// Transactional copy is Indonesian: the product ships to Indonesian kitchens
/// first, and a sign-in code is read by the person holding the phone.
const SUBJECT: &str = "Kode masuk Takar";

#[async_trait]
pub trait OtpSender: Send + Sync {
    async fn send(&self, contact: &Contact, code: &str, ttl_seconds: i64) -> Result<(), String>;
}

/// Routes the code to the transport that matches the contact.
///
/// Providers are picked from the environment, so switching one is a restart,
/// not a code change. Debug builds fall back to the log, which keeps local
/// sign-in working without credentials.
pub struct ChannelOtpSender {
    email: Option<Arc<dyn Transport>>,
    phone: Option<Arc<dyn Transport>>,
    fallback: Option<Arc<dyn Transport>>,
}

impl ChannelOtpSender {
    fn transport_for(&self, contact: &Contact) -> Option<&Arc<dyn Transport>> {
        match contact {
            Contact::Email(_) => self.email.as_ref(),
            Contact::Phone(_) => self.phone.as_ref(),
        }
    }
}

#[async_trait]
impl OtpSender for ChannelOtpSender {
    async fn send(&self, contact: &Contact, code: &str, ttl_seconds: i64) -> Result<(), String> {
        let transport = self
            .transport_for(contact)
            .or(self.fallback.as_ref())
            .ok_or_else(|| format!("no transport configured for {}", contact.as_str()))?;

        let text = message(code, ttl_seconds);

        notify::deliver(
            transport.as_ref(),
            &Message {
                to: contact.as_str(),
                subject: SUBJECT,
                text: &text,
            },
        )
        .await
        .map_err(|error| error.to_string())
    }
}

pub fn message(code: &str, ttl_seconds: i64) -> String {
    let minutes = (ttl_seconds / 60).max(1);

    format!(
        "Kode masuk Takar: {code}\n\nBerlaku {minutes} menit. Jangan bagikan kode ini ke siapa pun."
    )
}

/// Builds the sender once, at startup, and reports what is live.
pub fn build(config: &Config) -> Arc<dyn OtpSender> {
    let client = notify::client();

    let email: Option<Arc<dyn Transport>> = config.otp.email.as_ref().map(|provider| {
        Arc::new(notify::BrevoTransport::new(
            client.clone(),
            provider.api_key.clone(),
            provider.from_email.clone(),
            provider.from_name.clone(),
        )) as Arc<dyn Transport>
    });

    let phone: Option<Arc<dyn Transport>> = config.otp.whatsapp.as_ref().map(|provider| {
        Arc::new(notify::FonnteTransport::new(
            client.clone(),
            provider.token.clone(),
        )) as Arc<dyn Transport>
    });

    tracing::info!(
        email = email.as_ref().map(|transport| transport.name()),
        whatsapp = phone.as_ref().map(|transport| transport.name()),
        fallback = cfg!(debug_assertions),
        "otp delivery"
    );

    if email.is_none() && phone.is_none() && !cfg!(debug_assertions) {
        tracing::warn!("no OTP transport configured: sign-in codes cannot be delivered");
    }

    Arc::new(ChannelOtpSender {
        email,
        phone,
        fallback: cfg!(debug_assertions).then(|| Arc::new(LogTransport) as Arc<dyn Transport>),
    })
}

pub fn generate_code() -> String {
    format!("{:06}", rand::thread_rng().gen_range(0..1_000_000))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_six_digits() {
        for _ in 0..100 {
            let code = generate_code();
            assert_eq!(code.len(), 6);
            assert!(code.chars().all(|character| character.is_ascii_digit()));
        }
    }

    #[test]
    fn message_carries_the_code_and_the_window() {
        let text = message("123456", 300);

        assert!(text.contains("123456"));
        assert!(text.contains("5 menit"));
    }

    #[test]
    fn message_never_reports_zero_minutes() {
        assert!(message("123456", 30).contains("1 menit"));
    }
}
