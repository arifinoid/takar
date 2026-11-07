use rand::RngCore;
use sha2::{Digest, Sha256};

/// Peppered SHA-256 used for OTP codes and invitation tokens.
pub fn hash(secret: &str, value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(secret.as_bytes());
    hasher.update(b":");
    hasher.update(value.as_bytes());
    hex::encode(hasher.finalize())
}

/// 32 random bytes, hex encoded. Returned once and never stored.
pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn hashes_are_stable_and_secret_dependent() {
        assert_eq!(hash("secret", "123456"), hash("secret", "123456"));
        assert_ne!(hash("secret", "123456"), hash("other", "123456"));
        assert_ne!(hash("secret", "123456"), hash("secret", "654321"));
    }

    #[test]
    fn tokens_are_256_bits_of_hex() {
        let mut seen = HashSet::new();

        for _ in 0..64 {
            let token = random_token();
            assert_eq!(token.len(), 64);
            assert!(token.chars().all(|character| character.is_ascii_hexdigit()));
            assert!(seen.insert(token), "token repeated");
        }
    }
}
