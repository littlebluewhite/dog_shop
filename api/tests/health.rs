mod common;

use axum::http::StatusCode;
use sqlx::postgres::PgPoolOptions;

#[tokio::test]
async fn health_returns_ok_with_request_id() {
    // health 不碰資料庫，用 lazy 連線就好（不會真的連）
    let pool = PgPoolOptions::new()
        .connect_lazy("postgres://dog_shop:dog_shop@localhost:5435/dog_shop")
        .unwrap();
    let app = common::app(pool);
    let (status, body, headers) =
        common::send(&app, common::req("GET", "/api/health", None, None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert!(headers.contains_key("x-request-id"));
}
