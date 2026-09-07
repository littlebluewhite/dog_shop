mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;

fn order_body(variant: &str, qty: i32, method: &str, token: Option<&str>) -> Value {
    json!({
        "items": [{ "variant_id": variant, "qty": qty }],
        "email": "buyer@test.local",
        "recipient_name": "王小明",
        "recipient_phone": "0912345678",
        "shipping_method": method,
        "cvs_store_token": token,
        "address": if method == "home" {
            json!({ "postal_code": "100", "city": "臺北市", "district": "中正區", "street": "重慶南路一段 122 號" })
        } else { Value::Null },
        "invoice": { "type": "personal", "carrier_type": "1" },
        "payment_method": "credit",
        "note": ""
    })
}

#[sqlx::test(migrations = "./migrations")]
async fn guest_checkout_view_and_cancel(pool: PgPool) {
    let app = common::app(pool.clone());
    let (variant, _) = common::active_product(&pool, "雞肉狗糧", 300, 5).await;
    let (status, created, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/orders",
            None,
            Some(order_body(&variant.to_string(), 2, "home", None)),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let id = created["order_id"].as_str().unwrap().to_string();
    let token = created["guest_token"].as_str().unwrap().to_string();
    assert!(created["order_no"].as_str().unwrap().starts_with("DS"));

    // 沒 token 看不到
    let (status, _, _) = common::send(
        &app,
        common::req("GET", &format!("/api/orders/{id}"), None, None),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = common::send(
        &app,
        common::req("GET", &format!("/api/orders/{id}?t=wrong"), None, None),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, detail, _) = common::send(
        &app,
        common::req("GET", &format!("/api/orders/{id}?t={token}"), None, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{detail}");
    assert_eq!(detail["status"], "pending_payment");
    assert_eq!(detail["total"], 700);
    assert_eq!(detail["items"][0]["product_name"], "雞肉狗糧");
    assert_eq!(detail["shipment"]["home_street"], "重慶南路一段 122 號");
    assert_eq!(detail["payment"]["method"], "credit");
    assert!(detail.get("guest_token").is_none(), "回應不含 guest_token");
    assert!(detail.get("user_id").is_none());

    // 取消 → 204，狀態變 cancelled，再取消 400
    let (status, _, _) = common::send(
        &app,
        common::req(
            "POST",
            &format!("/api/orders/{id}/cancel?t={token}"),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, detail, _) = common::send(
        &app,
        common::req("GET", &format!("/api/orders/{id}?t={token}"), None, None),
    )
    .await;
    assert_eq!(detail["status"], "cancelled");
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            &format!("/api/orders/{id}/cancel?t={token}"),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["message"], "這筆訂單已經不能取消");
    let (status, _, _) = common::send(
        &app,
        common::req(
            "POST",
            &format!("/api/orders/{id}/cancel?t=wrong"),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn member_checkout_and_order_list(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::register_cookie(&app, "m@test.local", "password123", "甲").await;
    let other = common::register_cookie(&app, "o@test.local", "password123", "乙").await;
    let (variant, _) = common::active_product(&pool, "E", 100, 10).await;

    let (status, created, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/orders",
            Some(&cookie),
            Some(order_body(&variant.to_string(), 1, "home", None)),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let id = created["order_id"].as_str().unwrap().to_string();

    // 自己看不用 token；別人看不到
    let (status, detail, _) = common::send(
        &app,
        common::req("GET", &format!("/api/orders/{id}"), Some(&cookie), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["order_no"], created["order_no"]);
    let (status, _, _) = common::send(
        &app,
        common::req("GET", &format!("/api/orders/{id}"), Some(&other), None),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // 列表
    let (status, page, _) = common::send(
        &app,
        common::req("GET", "/api/me/orders", Some(&cookie), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(page["total"], 1);
    assert_eq!(page["items"][0]["order_no"], created["order_no"]);
    assert_eq!(page["items"][0]["item_count"], 1);
    let (status, _, _) = common::send(&app, common::req("GET", "/api/me/orders", None, None)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn order_errors_over_http(pool: PgPool) {
    let app = common::app(pool.clone());
    let (variant, _) = common::active_product(&pool, "F", 300, 2).await;

    // 欄位驗證
    let mut bad = order_body(&variant.to_string(), 1, "home", None);
    bad["email"] = json!("nope");
    bad["recipient_phone"] = json!("123");
    bad["invoice"] = json!({ "type": "" });
    let (status, body, _) =
        common::send(&app, common::req("POST", "/api/orders", None, Some(bad))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let fields = &body["error"]["details"]["fields"];
    assert_eq!(fields["email"], "Email 格式不正確");
    assert_eq!(fields["recipient_phone"], "手機格式：09 開頭共 10 碼");
    assert_eq!(fields["invoice.type"], "請選擇發票類型");

    // 庫存不足 → 409
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/orders",
            None,
            Some(order_body(&variant.to_string(), 3, "home", None)),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "OUT_OF_STOCK");
    assert_eq!(
        body["error"]["details"]["items"][0]["variant_id"],
        variant.to_string()
    );
    assert_eq!(body["error"]["details"]["items"][0]["available"], 2);

    // 超商沒門市 → 400 CVS_STORE_REQUIRED
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/orders",
            None,
            Some(order_body(&variant.to_string(), 1, "cvs", None)),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "CVS_STORE_REQUIRED");

    // 超商小計超過 20,000 → 400 CVS_AMOUNT_LIMIT
    let (pricey, _) = common::active_product(&pool, "貴", 25_000, 1).await;
    let token = common::cvs_store_token(&pool).await;
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/orders",
            None,
            Some(order_body(&pricey.to_string(), 1, "cvs", Some(&token))),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "CVS_AMOUNT_LIMIT");

    // JSON 壞掉 → VALIDATION（details.detail）
    let (status, body, _) = common::send(
        &app,
        common::req("POST", "/api/orders", None, Some(json!({ "items": "x" }))),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "VALIDATION");
}

/// 會員訂單也會拿到 guest_token（下單回應一律回），但這個 token 不能拿來當訪客憑證用：
/// 沒有 session 時用它看／取消一律 404；有 session 才看得到（安全性修正，見 Fix round 1）。
#[sqlx::test(migrations = "./migrations")]
async fn member_order_rejects_guest_token(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::register_cookie(&app, "g@test.local", "password123", "丙").await;
    let (variant, _) = common::active_product(&pool, "G", 150, 3).await;

    let (status, created, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/orders",
            Some(&cookie),
            Some(order_body(&variant.to_string(), 1, "home", None)),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let id = created["order_id"].as_str().unwrap().to_string();
    let token = created["guest_token"].as_str().unwrap().to_string();

    // 沒帶 session，只帶會員訂單的 guest_token → 看不到、也不能取消
    let (status, _, _) = common::send(
        &app,
        common::req("GET", &format!("/api/orders/{id}?t={token}"), None, None),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = common::send(
        &app,
        common::req(
            "POST",
            &format!("/api/orders/{id}/cancel?t={token}"),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // 帶 session 還是看得到
    let (status, _, _) = common::send(
        &app,
        common::req("GET", &format!("/api/orders/{id}"), Some(&cookie), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}
