mod common;

use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrations = "./migrations")]
async fn admin_crud_and_public_list(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::admin_cookie(&app, &pool).await;

    // 建立（slug 自動）
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/admin/categories",
            Some(&cookie),
            Some(json!({ "name": "飼料" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["name"], "飼料");
    assert_eq!(body["slug"].as_str().unwrap().len(), 8);
    let id = body["id"].as_str().unwrap().to_string();

    // 建立（指定 slug 與排序）
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/admin/categories",
            Some(&cookie),
            Some(json!({ "name": "零食", "slug": "snacks", "sort_order": 1 })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["slug"], "snacks");

    // slug 重複 → VALIDATION，欄位 slug
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/admin/categories",
            Some(&cookie),
            Some(json!({ "name": "別的", "slug": "snacks" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "VALIDATION");
    assert!(body["error"]["details"]["fields"]["slug"].is_string());

    // 更新
    let (status, body, _) = common::send(
        &app,
        common::req(
            "PUT",
            &format!("/api/admin/categories/{id}"),
            Some(&cookie),
            Some(json!({ "name": "狗飼料", "slug": "food", "sort_order": 0 })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["name"], "狗飼料");
    assert_eq!(body["slug"], "food");

    // 公開列表（不用登入），依 sort_order 排
    let (status, body, _) =
        common::send(&app, common::req("GET", "/api/categories", None, None)).await;
    assert_eq!(status, StatusCode::OK);
    let items = body.as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["slug"], "food");
    assert_eq!(items[1]["slug"], "snacks");

    // 刪除；再刪一次是 404
    let (status, _, _) = common::send(
        &app,
        common::req(
            "DELETE",
            &format!("/api/admin/categories/{id}"),
            Some(&cookie),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _, _) = common::send(
        &app,
        common::req(
            "DELETE",
            &format!("/api/admin/categories/{id}"),
            Some(&cookie),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn customer_and_guest_cannot_manage_categories(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::customer_cookie(&app, &pool).await;
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/admin/categories",
            Some(&cookie),
            Some(json!({ "name": "x" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "FORBIDDEN");
    let (status, _, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/admin/categories",
            None,
            Some(json!({ "name": "x" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn name_is_required(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::admin_cookie(&app, &pool).await;
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/admin/categories",
            Some(&cookie),
            Some(json!({ "name": "   " })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body["error"]["details"]["fields"]["name"],
        "必填，最多 50 字"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn bad_uuid_in_path_is_validation_error(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::admin_cookie(&app, &pool).await;
    let (status, body, _) = common::send(
        &app,
        common::req(
            "DELETE",
            "/api/admin/categories/not-a-uuid",
            Some(&cookie),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "VALIDATION");
}
