use async_trait::async_trait;
use rand::Rng;

#[async_trait]
pub trait OtpSender: Send + Sync {
    async fn send(&self, contact: &str, code: &str) -> Result<(), String>;
}

/// Development sender: writes the code to the log instead of a provider.
pub struct LogOtpSender;

#[async_trait]
impl OtpSender for LogOtpSender {
    async fn send(&self, contact: &str, code: &str) -> Result<(), String> {
        tracing::info!(contact, code, "development OTP code");
        Ok(())
    }
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
}
