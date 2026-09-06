mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use serde_json::json;
use sqlx::PgPool;

fn login_body() -> Body {
    Body::from(json!({ "email": "a@b.co", "password": "x" }).to_string())
}

#[sqlx::test(migrations = "./migrations")]
async fn post_without_marker_is_forbidden(pool: PgPool) {
    let app = common::app(pool);
    let request = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-forwarded-for", "127.0.0.1")
        .body(login_body())
        .unwrap();
    let (status, body, _) = common::send(&app, request).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "FORBIDDEN");
}

#[sqlx::test(migrations = "./migrations")]
async fn post_from_other_origin_is_forbidden(pool: PgPool) {
    let app = common::app(pool);
    let request = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-requested-with", "fetch")
        .header(header::ORIGIN, "https://evil.example")
        .header("x-forwarded-for", "127.0.0.1")
        .body(login_body())
        .unwrap();
    let (status, _, _) = common::send(&app, request).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "./migrations")]
async fn post_with_marker_and_same_origin_passes_through(pool: PgPool) {
    let app = common::app(pool);
    // common::req 會帶正確的 header；沒有這個帳號所以是 401，而不是 403
    let (status, _, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/auth/login",
            None,
            Some(json!({ "email": "a@b.co", "password": "x" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn get_without_marker_is_fine(pool: PgPool) {
    let app = common::app(pool);
    let request = Request::builder()
        .method("GET")
        .uri("/api/health")
        .body(Body::empty())
        .unwrap();
    let (status, _, _) = common::send(&app, request).await;
    assert_eq!(status, StatusCode::OK);
}

#[sqlx::test(migrations = "./migrations")]
async fn exempt_ecpay_path_skips_check(pool: PgPool) {
    let app = common::app(pool);
    let request = Request::builder()
        .method("POST")
        .uri("/api/ecpay/x")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ORIGIN, "https://evil.example")
        .header("x-forwarded-for", "127.0.0.1")
        .body(login_body())
        .unwrap();
    let (status, _, _) = common::send(&app, request).await;
    assert_ne!(status, StatusCode::FORBIDDEN);
    assert_eq!(status, StatusCode::NOT_FOUND);
}
