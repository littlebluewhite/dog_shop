use rand::Rng;
use rand::RngCore;
use sha2::{Digest, Sha256};

/// 32 bytes 隨機 → 64 字 hex。忘記密碼 token、訪客訂單 token 都用這個（規格 §11）
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// 綠界 ExtraData 只有 20 字：20 碼英數 token（62^20 ≈ 2^119，猜不到）；門市選擇用（規格 §11）
pub const SHORT_TOKEN_LEN: usize = 20;

pub fn generate_short_token() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::rng();
    (0..SHORT_TOKEN_LEN)
        .map(|_| ALPHABET[rng.random_range(0..ALPHABET.len())] as char)
        .collect()
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

    #[test]
    fn short_token_is_20_alphanumeric_and_random() {
        let a = generate_short_token();
        let b = generate_short_token();
        assert_eq!(a.len(), 20);
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric()));
        assert_ne!(a, b);
    }
}
