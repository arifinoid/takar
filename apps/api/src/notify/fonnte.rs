//! Fonnte WhatsApp gateway.
//!
//! Indonesian provider with a free trial quota, so it is the cheapest way to
//! reach a phone number while the product is small. Swapping it for the Meta
//! Cloud API later means one more module, not a change anywhere else.

use async_trait::async_trait;
use reqwest::Client;

use super::{check, Message, Transport, TransportError};

pub struct FonnteTransport {
    client: Client,
    token: String,
}

impl FonnteTransport {
    pub fn new(client: Client, token: String) -> Self {
        Self { client, token }
    }
}

#[async_trait]
impl Transport for FonnteTransport {
    fn name(&self) -> &'static str {
        "fonnte"
    }

    async fn send(&self, message: &Message<'_>) -> Result<(), TransportError> {
        // Fonnte wants the number without the plus: 62812... rather than +62812...
        let target = message.to.trim_start_matches('+');

        let response = self
            .client
            .post("https://api.fonnte.com/send")
            .header("authorization", &self.token)
            .form(&[("target", target), ("message", message.text)])
            .send()
            .await
            .map_err(|error| TransportError::retryable(error.to_string()))?;

        let response = check(response).await?;

        // Fonnte answers 200 even when it refuses the message, and says so in
        // the body. Treat a refused message as a permanent failure.
        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|error| TransportError::retryable(error.to_string()))?;

        match body.get("status").and_then(|value| value.as_bool()) {
            Some(true) => Ok(()),
            _ => {
                let reason = body
                    .get("reason")
                    .and_then(|value| value.as_str())
                    .unwrap_or("message rejected");

                Err(TransportError::permanent(reason.to_string()))
            }
        }
    }
}
