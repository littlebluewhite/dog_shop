use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};

pub const MIN_PASSWORD_CHARS: usize = 8;

/// argon2id（crate 預設參數）。回傳 PHC 字串（$argon2id$v=19$...）
pub fn hash_password(password: &str) -> anyhow::Result<String> {
    let hash = Argon2::default()
        .hash_password(password.as_bytes())
        .map_err(|e| anyhow::anyhow!("hash password: {e}"))?;
    Ok(hash.to_string())
}

/// 對不上、或雜湊字串格式壞掉，都回 false
pub fn verify_password(password: &str, password_hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(password_hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

/// argon2 很吃 CPU（幾十毫秒），在 async 裡要丟到 blocking thread
pub async fn hash_password_async(password: String) -> anyhow::Result<String> {
    tokio::task::spawn_blocking(move || hash_password(&password)).await?
}

pub async fn verify_password_async(
    password: String,
    password_hash: String,
) -> anyhow::Result<bool> {
    Ok(tokio::task::spawn_blocking(move || verify_password(&password, &password_hash)).await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_and_verify_roundtrip() {
        let hash = hash_password("correct horse").unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(verify_password("correct horse", &hash));
        assert!(!verify_password("wrong", &hash));
    }

    #[test]
    fn two_hashes_differ_because_of_salt() {
        assert_ne!(
            hash_password("same").unwrap(),
            hash_password("same").unwrap()
        );
    }

    #[test]
    fn garbage_hash_is_false() {
        assert!(!verify_password("x", "not-a-hash"));
    }
}
