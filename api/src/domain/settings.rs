use serde_json::Value;
use sqlx::PgPool;

pub const SHOP_KEY: &str = "shop";

pub async fn get(db: &PgPool, key: &str) -> Result<Option<Value>, sqlx::Error> {
    sqlx::query_scalar::<_, Value>("SELECT value FROM settings WHERE key = $1")
        .bind(key)
        .fetch_optional(db)
        .await
}
