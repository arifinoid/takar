//! Brevo transactional email.
//!
//! Free tier is 300 messages a day, which is plenty for sign-in codes, and it
//! does not need a verified domain: a verified sender address is enough.

use async_trait::async_trait;
use reqwest::Client;
use serde::Serialize;

use super::{check, Message, Transport, TransportError};

pub struct BrevoTransport {
    client: Client,
    api_key: String,
    from_email: String,
    from_name: String,
}

impl BrevoTransport {
    pub fn new(client: Client, api_key: String, from_email: String, from_name: String) -> Self {
        Self {
            client,
            api_key,
            from_email,
            from_name,
        }
    }
}

#[derive(Serialize)]
struct Sender<'a> {
    email: &'a str,
    name: &'a str,
}

#[derive(Serialize)]
struct Recipient<'a> {
    email: &'a str,
}

#[derive(Serialize)]
struct Payload<'a> {
    sender: Sender<'a>,
    to: Vec<Recipient<'a>>,
    subject: &'a str,
    text_content: &'a str,
}

#[async_trait]
impl Transport for BrevoTransport {
    fn name(&self) -> &'static str {
        "brevo"
    }

    async fn send(&self, message: &Message<'_>) -> Result<(), TransportError> {
        let payload = Payload {
            sender: Sender {
                email: &self.from_email,
                name: &self.from_name,
            },
            to: vec![Recipient { email: message.to }],
            subject: message.subject,
            text_content: message.text,
        };

        let response = self
            .client
            .post("https://api.brevo.com/v3/smtp/email")
            .header("api-key", &self.api_key)
            .header("accept", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|error| TransportError::retryable(error.to_string()))?;

        check(response).await?;

        Ok(())
    }
}
