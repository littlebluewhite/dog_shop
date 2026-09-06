mod common;

use axum::http::StatusCode;
use dog_shop_api::domain::password_resets;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrations = "./migrations")]
async fn register_logs_in_and_stores_phone(pool: PgPool) {
    let app = common::app(pool.clone());
    let (status, body, headers) = common::send(
        &app,
        common::req(
            "POST",
            "/api/auth/register",
            None,
            Some(json!({
                "email": " New@Test.local ", "password": "password123", "name": " 小新 ", "phone": "0912345678"
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["user"]["email"], "new@test.local");
    assert_eq!(body["user"]["name"], "小新");
    assert_eq!(body["user"]["phone"], "0912345678");
    assert_eq!(body["user"]["role"], "customer");
    assert!(body["user"].get("password_hash").is_none());
    let cookie = headers
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    assert!(cookie.starts_with("sid="));
    let (status, body, _) = common::send(
        &app,
        common::req("GET", "/api/auth/me", Some(&cookie), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["user"]["email"], "new@test.local");
}

#[sqlx::test(migrations = "./migrations")]
async fn register_validation_and_duplicate(pool: PgPool) {
    let app = common::app(pool.clone());
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/auth/register",
            None,
            Some(json!({ "email": "bad", "password": "short", "name": "", "phone": "123" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let fields = &body["error"]["details"]["fields"];
    assert_eq!(fields["email"], "Email 格式不正確");
    assert_eq!(fields["password"], "密碼至少 8 碼");
    assert_eq!(fields["name"], "必填，最多 50 字");
    assert_eq!(fields["phone"], "手機格式：09 開頭共 10 碼");

    common::register_cookie(&app, "dup@test.local", "password123", "甲").await;
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/auth/register",
            None,
            Some(json!({ "email": "DUP@test.local", "password": "password123", "name": "乙" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body["error"]["details"]["fields"]["email"],
        "這個 Email 已經註冊過了"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn forgot_is_quiet_and_enqueues_only_for_known_email(pool: PgPool) {
    let app = common::app(pool.clone());
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/auth/forgot",
            None,
            Some(json!({ "email": "ghost@test.local" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(body["ok"], true);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);

    common::register_cookie(&app, "known@test.local", "password123", "甲").await;
    let (status, _, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/auth/forgot",
            None,
            Some(json!({ "email": "Known@test.local" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let (kind, payload): (String, serde_json::Value) =
        sqlx::query_as("SELECT kind, payload FROM jobs ORDER BY id DESC LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(kind, "send_email");
    assert_eq!(payload["template"], "password_reset");
    assert!(payload["user_id"].is_string());
    assert!(payload.get("token").is_none(), "payload 不能有 token");

    let (status, _, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/auth/forgot",
            None,
            Some(json!({ "email": "bad" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn reset_changes_password_and_logs_out_everywhere(pool: PgPool) {
    let app = common::app(pool.clone());
    let old_cookie = common::register_cookie(&app, "r@test.local", "password123", "甲").await;
    let user_id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM users WHERE email = 'r@test.local'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let raw = password_resets::create(&pool, user_id).await.unwrap();

    // 太短
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/auth/reset",
            None,
            Some(json!({ "token": raw, "password": "short" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body["error"]["details"]["fields"]["password"],
        "密碼至少 8 碼"
    );

    let (status, _, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/auth/reset",
            None,
            Some(json!({ "token": raw, "password": "newpassword9" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // 舊 session 全部失效
    let (status, _, _) = common::send(
        &app,
        common::req("GET", "/api/auth/me", Some(&old_cookie), None),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    // 舊密碼不能登入、新密碼可以
    let (status, _, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/auth/login",
            None,
            Some(json!({ "email": "r@test.local", "password": "password123" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    common::login(&app, "r@test.local", "newpassword9").await;

    // token 用過就失效
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/auth/reset",
            None,
            Some(json!({ "token": raw, "password": "newpassword9" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body["error"]["details"]["fields"]["token"],
        "重設連結無效或已過期"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn register_shares_the_login_rate_limit(pool: PgPool) {
    let app = common::app(pool);
    // 同一個 IP：register 與 login 加起來 burst 10，第 11 次 429
    for i in 0..10 {
        let path = if i % 2 == 0 {
            "/api/auth/register"
        } else {
            "/api/auth/login"
        };
        let (status, _, _) = common::send(
            &app,
            common::req(
                "POST",
                path,
                None,
                Some(json!({ "email": "bad", "password": "x", "name": "" })),
            ),
        )
        .await;
        assert_ne!(status, StatusCode::TOO_MANY_REQUESTS, "第 {i} 次不該被限");
    }
    let (status, _, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/auth/forgot",
            None,
            Some(json!({ "email": "a@b.co" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
}
