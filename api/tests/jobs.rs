mod common;

use dog_shop_api::domain::jobs;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrations = "./migrations")]
async fn enqueue_dedupes_by_key(pool: PgPool) {
    let mut tx = pool.begin().await.unwrap();
    jobs::enqueue(
        &mut tx,
        jobs::KIND_SEND_EMAIL,
        json!({ "template": "x" }),
        Some("email:x:1"),
    )
    .await
    .unwrap();
    jobs::enqueue(
        &mut tx,
        jobs::KIND_SEND_EMAIL,
        json!({ "template": "x" }),
        Some("email:x:1"),
    )
    .await
    .unwrap();
    jobs::enqueue(
        &mut tx,
        jobs::KIND_SEND_EMAIL,
        json!({ "template": "y" }),
        None,
    )
    .await
    .unwrap();
    jobs::enqueue(
        &mut tx,
        jobs::KIND_SEND_EMAIL,
        json!({ "template": "y" }),
        None,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 3, "同 key 只排一次，None 不去重");
    let (kind, status, attempts): (String, String, i32) =
        sqlx::query_as("SELECT kind, status, attempts FROM jobs ORDER BY id LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        (kind.as_str(), status.as_str(), attempts),
        ("send_email", "queued", 0)
    );
}
