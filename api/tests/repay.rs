mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;

fn order_body(variant: &str, method: &str) -> Value {
    json!({
        "items": [{ "variant_id": variant, "qty": 2 }],
        "email": "buyer@test.local",
        "recipient_name": "王小明",
        "recipient_phone": "0912345678",
        "shipping_method": "home",
        "address": { "postal_code": "100", "city": "臺北市", "district": "中正區", "street": "重慶南路一段 122 號" },
        "invoice": { "type": "personal", "carrier_type": "1" },
        "payment_method": method,
        "note": ""
    })
}

/// 回 (order_id, order_no, guest_token)
async fn place_order(
    app: &axum::Router,
    pool: &PgPool,
    cookie: Option<&str>,
) -> (String, String, String) {
    let (variant, _) = common::active_product(pool, "雞肉狗糧", 300, 5).await;
    let (status, created, _) = common::send(
        app,
        common::req(
            "POST",
            "/api/orders",
            cookie,
            Some(order_body(&variant.to_string(), "credit")),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    (
        created["order_id"].as_str().unwrap().to_string(),
        created["order_no"].as_str().unwrap().to_string(),
        created["guest_token"].as_str().unwrap().to_string(),
    )
}

fn uuid(s: &str) -> uuid::Uuid {
    uuid::Uuid::parse_str(s).unwrap()
}

async fn payment_count(pool: &PgPool, order_id: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM payments WHERE order_id = $1")
        .bind(uuid(order_id))
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn repay_creates_new_payment_and_form(pool: PgPool) {
    let app = common::app(pool.clone());
    let (id, order_no, token) = place_order(&app, &pool, None).await;

    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            &format!("/api/orders/{id}/repay?t={token}"),
            None,
            Some(json!({ "payment_method": "atm" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let fields = &body["ecpay"]["fields"];
    assert_eq!(fields["MerchantTradeNo"], format!("{order_no}02"));
    assert_eq!(fields["ChoosePayment"], "ATM");
    assert_eq!(fields["TotalAmount"], "700");
    assert_eq!(
        fields["ClientBackURL"],
        format!("{}/orders/{id}?t={token}", common::TEST_ORIGIN)
    );
    assert_eq!(payment_count(&pool, &id).await, 2);

    // 訂單頁的 payment 是最新那筆
    let (_, detail, _) = common::send(
        &app,
        common::req("GET", &format!("/api/orders/{id}?t={token}"), None, None),
    )
    .await;
    assert_eq!(detail["payment"]["method"], "atm");
    assert_eq!(detail["payment"]["status"], "pending");

    // 沒帶 payment_method → 沿用最近一筆（atm）；流水 03
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            &format!("/api/orders/{id}/repay?t={token}"),
            None,
            Some(json!({})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["ecpay"]["fields"]["MerchantTradeNo"],
        format!("{order_no}03")
    );
    assert_eq!(body["ecpay"]["fields"]["ChoosePayment"], "ATM");
    assert_eq!(payment_count(&pool, &id).await, 3);
}

#[sqlx::test(migrations = "./migrations")]
async fn repay_rejects_non_pending_and_disabled_method(pool: PgPool) {
    let app = common::app(pool.clone());
    let (id, _, token) = place_order(&app, &pool, None).await;

    // 關掉 ATM
    sqlx::query("UPDATE settings SET value = '{\"credit\": true, \"atm\": false, \"cvs_code\": true}' WHERE key = 'payment_methods'")
        .execute(&pool)
        .await
        .unwrap();
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            &format!("/api/orders/{id}/repay?t={token}"),
            None,
            Some(json!({ "payment_method": "atm" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "VALIDATION");
    assert_eq!(
        body["error"]["details"]["fields"]["payment_method"],
        "這個付款方式目前沒有開放"
    );

    // 已付款（直接改 DB 模擬）→ ORDER_NOT_PAYABLE
    sqlx::query("UPDATE orders SET status = 'paid', paid_at = now() WHERE id = $1")
        .bind(uuid(&id))
        .execute(&pool)
        .await
        .unwrap();
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            &format!("/api/orders/{id}/repay?t={token}"),
            None,
            Some(json!({})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "ORDER_NOT_PAYABLE");
    assert_eq!(payment_count(&pool, &id).await, 1);

    // 取消的也不行
    sqlx::query("UPDATE orders SET status = 'cancelled', paid_at = NULL WHERE id = $1")
        .bind(uuid(&id))
        .execute(&pool)
        .await
        .unwrap();
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            &format!("/api/orders/{id}/repay?t={token}"),
            None,
            Some(json!({})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "ORDER_NOT_PAYABLE");
}

#[sqlx::test(migrations = "./migrations")]
async fn repay_needs_viewer(pool: PgPool) {
    let app = common::app(pool.clone());
    let (id, _, _) = place_order(&app, &pool, None).await;
    let (status, _, _) = common::send(
        &app,
        common::req(
            "POST",
            &format!("/api/orders/{id}/repay"),
            None,
            Some(json!({})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = common::send(
        &app,
        common::req(
            "POST",
            &format!("/api/orders/{id}/repay?t=wrong"),
            None,
            Some(json!({})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(payment_count(&pool, &id).await, 1);

    // 會員用 cookie；ClientBackURL 不帶 ?t=
    let cookie = common::register_cookie(&app, "m@test.local", "password123", "甲").await;
    let (mid, order_no, _) = place_order(&app, &pool, Some(&cookie)).await;
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            &format!("/api/orders/{mid}/repay"),
            Some(&cookie),
            Some(json!({})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["ecpay"]["fields"]["MerchantTradeNo"],
        format!("{order_no}02")
    );
    assert_eq!(
        body["ecpay"]["fields"]["ClientBackURL"],
        format!("{}/orders/{mid}", common::TEST_ORIGIN)
    );
}
