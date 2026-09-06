mod common;

use dog_shop_api::domain::{password_resets, users};
use sqlx::PgPool;

async fn user_id(pool: &PgPool) -> uuid::Uuid {
    let hash = dog_shop_api::auth::password::hash_password("password123").unwrap();
    users::create(pool, "reset@test.local", &hash, "小美", "customer")
        .await
        .unwrap()
        .id
}

#[sqlx::test(migrations = "./migrations")]
async fn token_is_single_use(pool: PgPool) {
    let uid = user_id(&pool).await;
    let raw = password_resets::create(&pool, uid).await.unwrap();
    assert_eq!(raw.len(), 64);
    // DB 裡沒有明文
    let stored: String = sqlx::query_scalar("SELECT token_hash FROM password_resets")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_ne!(stored, raw);

    assert_eq!(
        password_resets::consume(&pool, &raw).await.unwrap(),
        Some(uid)
    );
    assert_eq!(
        password_resets::consume(&pool, &raw).await.unwrap(),
        None,
        "第二次要失敗"
    );
    assert_eq!(password_resets::consume(&pool, "nope").await.unwrap(), None);
}

#[sqlx::test(migrations = "./migrations")]
async fn expired_token_is_rejected(pool: PgPool) {
    let uid = user_id(&pool).await;
    let raw = password_resets::create(&pool, uid).await.unwrap();
    sqlx::query("UPDATE password_resets SET expires_at = now() - interval '1 minute'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(password_resets::consume(&pool, &raw).await.unwrap(), None);
}
