use rand::RngCore;
use sha2::{Digest, Sha256};

/// 32 bytes 隨機 → 64 字 hex。忘記密碼 token、訪客訂單 token 都用這個（規格 §11）
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// DB 只存 token 的 SHA-256 hex（規格 §11）
pub fn sha256_hex(raw: &str) -> String {
    hex::encode(Sha256::digest(raw.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_64_hex_and_random() {
        let a = generate_token();
        let b = generate_token();
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    #[test]
    fn sha256_known_vector() {
        // echo -n abc | sha256sum
        assert_eq!(
            sha256_hex("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
