mod common;

use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrations = "./migrations")]
async fn public_settings_are_nested_with_defaults(pool: PgPool) {
    let app = common::app(pool);
    let (status, body, _) =
        common::send(&app, common::req("GET", "/api/settings/public", None, None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["shop"]["name"], "dog_shop");
    assert_eq!(body["shipping"]["cvs_fee"], 60);
    assert_eq!(body["shipping"]["home_fee"], 100);
    assert_eq!(body["shipping"]["free_threshold"], 0);
    assert_eq!(body["payment_methods"]["credit"], true);
    assert!(body.get("sender").is_none(), "寄件人不公開");
    assert!(body.get("return_store").is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn admin_settings_require_admin(pool: PgPool) {
    let app = common::app(pool.clone());
    let (status, _, _) =
        common::send(&app, common::req("GET", "/api/admin/settings", None, None)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let customer = common::customer_cookie(&app, &pool).await;
    let (status, _, _) = common::send(
        &app,
        common::req("GET", "/api/admin/settings", Some(&customer), None),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "./migrations")]
async fn admin_can_read_validate_and_update(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::admin_cookie(&app, &pool).await;

    let (status, body, _) = common::send(
        &app,
        common::req("GET", "/api/admin/settings", Some(&cookie), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["sender"]["name"], "");
    assert_eq!(body["return_store"]["sub_type"], "");

    // 驗證失敗：整組回欄位錯誤（key 用點分路徑）
    let mut bad = body.clone();
    bad["shop"]["name"] = json!("");
    bad["payment_methods"] = json!({ "credit": false, "atm": false, "cvs_code": false });
    let (status, err, _) = common::send(
        &app,
        common::req("PUT", "/api/admin/settings", Some(&cookie), Some(bad)),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        err["error"]["details"]["fields"]["shop.name"],
        "必填，最多 60 字"
    );
    assert_eq!(
        err["error"]["details"]["fields"]["payment_methods"],
        "至少要開一種付款方式"
    );

    // 成功：回整組，public 也跟著變
    let mut good = body.clone();
    good["shop"]["name"] = json!(" 汪汪商店 ");
    good["shipping"]["free_threshold"] = json!(1000);
    good["payment_methods"]["cvs_code"] = json!(false);
    good["sender"] = json!({ "name": "老闆", "phone": "0912345678" });
    good["return_store"] =
        json!({ "sub_type": "UNIMARTC2C", "store_id": "123456", "store_name": "測試門市" });
    let (status, saved, _) = common::send(
        &app,
        common::req("PUT", "/api/admin/settings", Some(&cookie), Some(good)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(saved["shop"]["name"], "汪汪商店");
    assert_eq!(saved["shipping"]["free_threshold"], 1000);
    assert_eq!(saved["payment_methods"]["cvs_code"], false);
    assert_eq!(saved["return_store"]["store_name"], "測試門市");

    let (_, public, _) =
        common::send(&app, common::req("GET", "/api/settings/public", None, None)).await;
    assert_eq!(public["shop"]["name"], "汪汪商店");
    assert_eq!(public["shipping"]["free_threshold"], 1000);
    assert_eq!(public["payment_methods"]["cvs_code"], false);
}
