mod common;

use axum::http::StatusCode;
use sqlx::PgPool;

#[sqlx::test(migrations = "./migrations")]
async fn public_settings_returns_shop(pool: PgPool) {
    let app = common::app(pool);
    let (status, body, _) =
        common::send(&app, common::req("GET", "/api/settings/public", None, None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "dog_shop");
    assert_eq!(body["contact_email"], "");
}

#[sqlx::test(migrations = "./migrations")]
async fn migration_creates_catalog_tables(pool: PgPool) {
    for table in [
        "users",
        "sessions",
        "categories",
        "products",
        "product_variants",
        "product_images",
    ] {
        // sqlx 0.9 的 query_scalar 要求 SqlSafeStr；table 只來自上面固定的字面字串陣列，
        // 不是外部輸入，用 AssertSqlSafe 手動核可這個動態組出來的 SQL。
        let count: i64 =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 0, "{table} 應該是空的");
    }
}
