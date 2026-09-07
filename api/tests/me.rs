mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;

fn address(name: &str, is_default: bool) -> Value {
    json!({
        "recipient_name": name, "phone": "0912345678", "postal_code": "100",
        "city": "臺北市", "district": "中正區", "street": "重慶南路一段 122 號", "is_default": is_default
    })
}

#[sqlx::test(migrations = "./migrations")]
async fn profile_update_and_password_change(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::register_cookie(&app, "p@test.local", "password123", "甲").await;

    let (status, body, _) = common::send(
        &app,
        common::req("GET", "/api/me/profile", Some(&cookie), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["user"]["name"], "甲");

    // 改名 + 手機
    let (status, body, _) = common::send(
        &app,
        common::req(
            "PUT",
            "/api/me/profile",
            Some(&cookie),
            Some(json!({ "name": "乙", "phone": "0987654321" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["user"]["name"], "乙");
    assert_eq!(body["user"]["phone"], "0987654321");

    // 目前密碼錯 → 欄位錯誤，名字也不會被改
    let (status, body, _) = common::send(
        &app,
        common::req(
            "PUT",
            "/api/me/profile",
            Some(&cookie),
            Some(json!({ "name": "丙", "current_password": "wrong", "new_password": "newpassword9" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body["error"]["details"]["fields"]["current_password"],
        "目前密碼錯誤"
    );
    let (_, body, _) = common::send(
        &app,
        common::req("GET", "/api/me/profile", Some(&cookie), None),
    )
    .await;
    assert_eq!(body["user"]["name"], "乙");

    // 改密碼成功 → 新密碼能登入
    let (status, _, _) = common::send(
        &app,
        common::req(
            "PUT",
            "/api/me/profile",
            Some(&cookie),
            Some(json!({ "name": "乙", "current_password": "password123", "new_password": "newpassword9" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    common::login(&app, "p@test.local", "newpassword9").await;

    // 沒登入 → 401
    let (status, _, _) =
        common::send(&app, common::req("GET", "/api/me/profile", None, None)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn address_crud_and_default_handling(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::register_cookie(&app, "a@test.local", "password123", "甲").await;
    let other = common::register_cookie(&app, "b@test.local", "password123", "乙").await;

    // 驗證
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/me/addresses",
            Some(&cookie),
            Some(json!({ "recipient_name": "", "phone": "1", "postal_code": "x", "city": "", "district": "", "street": "" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let fields = &body["error"]["details"]["fields"];
    for key in [
        "recipient_name",
        "phone",
        "postal_code",
        "city",
        "district",
        "street",
    ] {
        assert!(fields.get(key).is_some(), "缺 {key}");
    }

    // 第一筆自動預設
    let (status, first, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/me/addresses",
            Some(&cookie),
            Some(address("甲", false)),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{first}");
    assert_eq!(first["is_default"], true);
    // 第二筆指定預設 → 第一筆取消預設
    let (status, second, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/me/addresses",
            Some(&cookie),
            Some(address("乙", true)),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(second["is_default"], true);
    let (_, list, _) = common::send(
        &app,
        common::req("GET", "/api/me/addresses", Some(&cookie), None),
    )
    .await;
    let list = list.as_array().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["id"], second["id"], "預設排最前");
    assert_eq!(list[1]["is_default"], false);

    // 別人看不到、改不到、刪不到
    let first_id = first["id"].as_str().unwrap();
    let (_, other_list, _) = common::send(
        &app,
        common::req("GET", "/api/me/addresses", Some(&other), None),
    )
    .await;
    assert_eq!(other_list.as_array().unwrap().len(), 0);
    let (status, _, _) = common::send(
        &app,
        common::req(
            "PUT",
            &format!("/api/me/addresses/{first_id}"),
            Some(&other),
            Some(address("駭客", false)),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = common::send(
        &app,
        common::req(
            "DELETE",
            &format!("/api/me/addresses/{first_id}"),
            Some(&other),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // 自己改、自己刪
    let (status, updated, _) = common::send(
        &app,
        common::req(
            "PUT",
            &format!("/api/me/addresses/{first_id}"),
            Some(&cookie),
            Some(address("甲改", true)),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(updated["recipient_name"], "甲改");
    assert_eq!(updated["is_default"], true);
    let (status, _, _) = common::send(
        &app,
        common::req(
            "DELETE",
            &format!("/api/me/addresses/{first_id}"),
            Some(&cookie),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, list, _) = common::send(
        &app,
        common::req("GET", "/api/me/addresses", Some(&cookie), None),
    )
    .await;
    assert_eq!(list.as_array().unwrap().len(), 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn at_most_ten_addresses(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::register_cookie(&app, "m@test.local", "password123", "甲").await;
    for i in 0..10 {
        let (status, body, _) = common::send(
            &app,
            common::req(
                "POST",
                "/api/me/addresses",
                Some(&cookie),
                Some(address(&format!("第{i}"), false)),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
    }
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/me/addresses",
            Some(&cookie),
            Some(address("第11", false)),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body["error"]["details"]["fields"]["recipient_name"],
        "最多 10 筆常用地址"
    );
}
