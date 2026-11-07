use async_trait::async_trait;
use rand::Rng;
use sha2::{Digest, Sha256};

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

pub fn hash_code(secret: &str, code: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(secret.as_bytes());
    hasher.update(b":");
    hasher.update(code.as_bytes());
    hex::encode(hasher.finalize())
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
    fn hashes_are_stable_and_secret_dependent() {
        assert_eq!(hash_code("secret", "123456"), hash_code("secret", "123456"));
        assert_ne!(hash_code("secret", "123456"), hash_code("other", "123456"));
        assert_ne!(hash_code("secret", "123456"), hash_code("secret", "654321"));
    }
}
