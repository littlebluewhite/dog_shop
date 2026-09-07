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
