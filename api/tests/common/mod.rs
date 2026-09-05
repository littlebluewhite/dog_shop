#![allow(dead_code)]

use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Request, StatusCode, header},
};
use dog_shop_api::{app, config::Config, state::AppState};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt;

pub const TEST_ORIGIN: &str = "http://localhost:5173";

/// 每個測試一個獨立的上傳目錄，避免互相干擾
pub fn state(pool: PgPool) -> AppState {
    let upload_dir = std::env::temp_dir().join(format!("dog_shop_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&upload_dir).unwrap();
    AppState {
        db: pool,
        config: Arc::new(Config {
            database_url: String::new(),
            public_base_url: TEST_ORIGIN.to_string(),
            cookie_secure: false,
            upload_dir,
        }),
    }
}

pub fn app(pool: PgPool) -> Router {
    app::router(state(pool))
}

/// 建一個像瀏覽器 fetch 送出的請求：帶 Origin、X-Requested-With、X-Forwarded-For（速率限制用）
pub fn req(method: &str, uri: &str, cookie: Option<&str>, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::ACCEPT, "application/json")
        .header(header::ORIGIN, TEST_ORIGIN)
        .header("x-requested-with", "fetch")
        .header("x-forwarded-for", "127.0.0.1");
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    match body {
        Some(json) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    }
}

/// 送出請求，回 (狀態碼, JSON body（空 body 是 Null；不是 JSON 就包成字串）, 回應 headers)
pub async fn send(app: &Router, request: Request<Body>) -> (StatusCode, Value, HeaderMap) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()))
    };
    (status, json, headers)
}
