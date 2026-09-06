use chrono::{Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::tokens::{generate_token, sha256_hex};

pub const RESET_TTL_MINUTES: i64 = 60;

/// 產生原始 token 回給呼叫者（計畫 3 的寄信 job 用它組網址）；DB 只存 SHA-256（規格 §11）
pub async fn create(db: &PgPool, user_id: Uuid) -> Result<String, sqlx::Error> {
    let raw = generate_token();
    sqlx::query(
        "INSERT INTO password_resets (token_hash, user_id, expires_at) VALUES ($1, $2, $3)",
    )
    .bind(sha256_hex(&raw))
    .bind(user_id)
    .bind(Utc::now() + Duration::minutes(RESET_TTL_MINUTES))
    .execute(db)
    .await?;
    Ok(raw)
}

/// 用原始 token 換 user_id：要存在、沒用過、沒過期；成功同時標記用過（只能用一次）
pub async fn consume(db: &PgPool, raw: &str) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar::<_, Uuid>(
        "UPDATE password_resets SET used_at = now()
         WHERE token_hash = $1 AND used_at IS NULL AND expires_at > now()
         RETURNING user_id",
    )
    .bind(sha256_hex(raw))
    .fetch_optional(db)
    .await
}
