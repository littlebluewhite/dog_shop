use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgExecutor, PgPool};

/// 綠界地圖選完的門市（規格 §3 cvs_store_selections）。本計畫只讀；計畫 4 的 map-reply 才會寫
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CvsStore {
    pub token: String,
    pub sub_type: String,
    pub store_id: String,
    pub store_name: String,
    pub store_address: String,
    pub store_phone: String,
}

/// 未過期才回
pub async fn get_valid<'e, E: PgExecutor<'e>>(
    exec: E,
    token: &str,
) -> Result<Option<CvsStore>, sqlx::Error> {
    sqlx::query_as::<_, CvsStore>(
        "SELECT token, sub_type, store_id, store_name, store_address, store_phone
         FROM cvs_store_selections WHERE token = $1 AND expires_at > now()",
    )
    .bind(token)
    .fetch_optional(exec)
    .await
}

/// 計畫 4 的 map-reply 與測試用
pub async fn insert(db: &PgPool, store: &CvsStore, ttl_minutes: i64) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO cvs_store_selections (token, sub_type, store_id, store_name, store_address, store_phone, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(&store.token)
    .bind(&store.sub_type)
    .bind(&store.store_id)
    .bind(&store.store_name)
    .bind(&store.store_address)
    .bind(&store.store_phone)
    .bind(Utc::now() + Duration::minutes(ttl_minutes))
    .execute(db)
    .await?;
    Ok(())
}

/// 登記與門市選擇都是 1 小時（規格 §3）
pub const MAP_REQUEST_TTL_MINUTES: i64 = 60;
pub const STORE_TTL_MINUTES: i64 = 60;

/// 按「選擇門市」時先登記（與規格不同之處 34）
pub async fn insert_map_request(
    db: &PgPool,
    token: &str,
    sub_type: &str,
    ttl_minutes: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO cvs_map_requests (token, sub_type, expires_at) VALUES ($1, $2, $3)")
        .bind(token)
        .bind(sub_type)
        .bind(Utc::now() + Duration::minutes(ttl_minutes))
        .execute(db)
        .await?;
    Ok(())
}

/// 用掉一筆有效登記（刪掉它，單次使用），回登記時的超商種類；不存在或過期回 None
pub async fn take_map_request(db: &PgPool, token: &str) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(
        "DELETE FROM cvs_map_requests WHERE token = $1 AND expires_at > now() RETURNING sub_type",
    )
    .bind(token)
    .fetch_optional(db)
    .await
}
