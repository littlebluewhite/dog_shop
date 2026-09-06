mod common;

use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrations = "./migrations")]
async fn login_me_logout(pool: PgPool) {
    let app = common::app(pool.clone());
    common::create_admin(&pool).await;

    // 沒登入 → 401
    let (status, body, _) =
        common::send(&app, common::req("GET", "/api/auth/me", None, None)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "UNAUTHORIZED");

    // 登入：Email 大小寫、空白都可以
    let (status, body, headers) = common::send(
        &app,
        common::req(
            "POST",
            "/api/auth/login",
            None,
            Some(json!({ "email": " Admin@Test.local ", "password": common::ADMIN_PASSWORD })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["user"]["email"], common::ADMIN_EMAIL);
    assert_eq!(body["user"]["role"], "admin");
    assert!(body["user"].get("password_hash").is_none());
    let set_cookie = headers.get("set-cookie").unwrap().to_str().unwrap();
    assert!(set_cookie.starts_with("sid="));
    assert!(set_cookie.contains("HttpOnly") && set_cookie.contains("SameSite=Lax"));
    assert!(
        !set_cookie.contains("Secure"),
        "測試設定 cookie_secure=false"
    );
    let cookie = set_cookie.split(';').next().unwrap().to_string();

    // me
    let (status, body, _) = common::send(
        &app,
        common::req("GET", "/api/auth/me", Some(&cookie), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["user"]["email"], common::ADMIN_EMAIL);

    // logout → 204 並清 cookie；之後 me 又是 401
    let (status, _, headers) = common::send(
        &app,
        common::req("POST", "/api/auth/logout", Some(&cookie), None),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(
        headers
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );
    let (status, _, _) = common::send(
        &app,
        common::req("GET", "/api/auth/me", Some(&cookie), None),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn wrong_password_and_unknown_email_are_401(pool: PgPool) {
    let app = common::app(pool.clone());
    common::create_admin(&pool).await;
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/auth/login",
            None,
            Some(json!({ "email": common::ADMIN_EMAIL, "password": "nope-nope" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "UNAUTHORIZED");
    let (status, _, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/auth/login",
            None,
            Some(json!({ "email": "ghost@test.local", "password": "nope-nope" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn missing_field_is_validation_error(pool: PgPool) {
    let app = common::app(pool);
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/auth/login",
            None,
            Some(json!({ "email": "x" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "VALIDATION");
}

#[sqlx::test(migrations = "./migrations")]
async fn login_is_rate_limited(pool: PgPool) {
    let app = common::app(pool);
    // burst 10：前 10 次都會被處理（401），第 11 次 429
    for _ in 0..10 {
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
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/auth/login",
            None,
            Some(json!({ "email": "a@b.co", "password": "x" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(body["error"]["code"], "RATE_LIMITED");
}

#[sqlx::test(migrations = "./migrations")]
async fn expired_session_is_401(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::admin_cookie(&app, &pool).await;
    sqlx::query("UPDATE sessions SET expires_at = now() - interval '1 day'")
        .execute(&pool)
        .await
        .unwrap();
    let (status, body, _) = common::send(
        &app,
        common::req("GET", "/api/auth/me", Some(&cookie), None),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "UNAUTHORIZED");
}

#[sqlx::test(migrations = "./migrations")]
async fn me_is_not_rate_limited(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::admin_cookie(&app, &pool).await;
    // /me 沒有掛 governor_layer，同一個 IP 打超過 burst_size(10) 次也不該 429
    for _ in 0..12 {
        let (status, _, _) = common::send(
            &app,
            common::req("GET", "/api/auth/me", Some(&cookie), None),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }
}
