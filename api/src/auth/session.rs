use chrono::{Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::users::User;

pub const SESSION_DAYS: i64 = 30;

/// 建一個 session，回 session id。用隨機的 UUID v4，不用可預測的 v7。
pub async fn create(db: &PgPool, user_id: Uuid) -> Result<Uuid, sqlx::Error> {
    let sid = Uuid::new_v4();
    sqlx::query("INSERT INTO sessions (id, user_id, expires_at) VALUES ($1, $2, $3)")
        .bind(sid)
        .bind(user_id)
        .bind(Utc::now() + Duration::days(SESSION_DAYS))
        .execute(db)
        .await?;
    Ok(sid)
}

/// 用 session id 找還沒過期的使用者
pub async fn find_user(db: &PgPool, sid: Uuid) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>(
        "SELECT u.* FROM sessions s JOIN users u ON u.id = s.user_id WHERE s.id = $1 AND s.expires_at > now()",
    )
    .bind(sid)
    .fetch_optional(db)
    .await
}

pub async fn delete(db: &PgPool, sid: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM sessions WHERE id = $1")
        .bind(sid)
        .execute(db)
        .await?;
    Ok(())
}

/// 使用者所有 session 全部登出（重設密碼時用；計畫 2）
pub async fn delete_all_for_user(db: &PgPool, user_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM sessions WHERE user_id = $1")
        .bind(user_id)
        .execute(db)
        .await?;
    Ok(())
}
