mod common;

use axum::{Router, http::StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

fn order_body(variant: &str, method: &str, token: Option<&str>) -> Value {
    json!({
        "items": [{ "variant_id": variant, "qty": 2 }],
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

/// 建一筆 2 × 300 的訪客訂單（超商運費 60 → 660；宅配運費 100 → 700），回 (order_id, order_no, guest_token)
async fn place_order(app: &Router, pool: &PgPool, method: &str) -> (Uuid, String, String) {
    let (variant, _) = common::active_product(pool, "雞肉狗糧", 300, 5).await;
    let token = if method == "cvs" {
        Some(common::cvs_store_token(pool).await)
    } else {
        None
    };
    let (status, created, _) = common::send(
        app,
        common::req(
            "POST",
            "/api/orders",
            None,
            Some(order_body(&variant.to_string(), method, token.as_deref())),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    (
        Uuid::parse_str(created["order_id"].as_str().unwrap()).unwrap(),
        created["order_no"].as_str().unwrap().to_string(),
        created["guest_token"].as_str().unwrap().to_string(),
    )
}

/// 直接用 SQL 標成已付款（付款回呼的邏輯在 tests/ecpay_payment.rs 已測過）
async fn mark_paid(pool: &PgPool, id: Uuid) {
    sqlx::query("UPDATE orders SET status = 'paid', paid_at = now() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE payments SET status = 'paid', payment_date = now() WHERE order_id = $1")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
}

async fn get(app: &Router, cookie: &str, path: &str) -> (StatusCode, Value) {
    let (status, body, _) = common::send(app, common::req("GET", path, Some(cookie), None)).await;
    (status, body)
}

#[allow(dead_code)] // Task 6、7 才會呼叫
async fn post(app: &Router, cookie: &str, path: &str, body: Value) -> (StatusCode, Value) {
    let (status, value, _) =
        common::send(app, common::req("POST", path, Some(cookie), Some(body))).await;
    (status, value)
}

#[sqlx::test(migrations = "./migrations")]
async fn admin_order_routes_require_admin(pool: PgPool) {
    let app = common::app(pool.clone());
    let (id, _, _) = place_order(&app, &pool, "home").await;

    let (status, _, _) =
        common::send(&app, common::req("GET", "/api/admin/orders", None, None)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _, _) = common::send(
        &app,
        common::req("GET", &format!("/api/admin/orders/{id}"), None, None),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let customer = common::customer_cookie(&app, &pool).await;
    assert_eq!(
        get(&app, &customer, "/api/admin/orders").await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        get(&app, &customer, &format!("/api/admin/orders/{id}"))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn list_filters_by_status_flag_query_and_pages(pool: PgPool) {
    let app = common::app(pool.clone());
    let admin = common::admin_cookie(&app, &pool).await;
    let (pending, _, _) = place_order(&app, &pool, "home").await;
    let (refund, refund_no, _) = place_order(&app, &pool, "cvs").await;
    let (returned, _, _) = place_order(&app, &pool, "cvs").await;
    let (inv_failed, _, _) = place_order(&app, &pool, "home").await;
    mark_paid(&pool, refund).await;
    mark_paid(&pool, returned).await;
    mark_paid(&pool, inv_failed).await;
    sqlx::query("UPDATE orders SET needs_refund = true WHERE id = $1")
        .bind(refund)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE orders SET status = 'shipped', shipped_at = now() WHERE id = $1")
        .bind(returned)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE shipments SET status = 'returned' WHERE order_id = $1")
        .bind(returned)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE invoices SET status = 'failed', error = '綠界回錯' WHERE order_id = $1")
        .bind(inv_failed)
        .execute(&pool)
        .await
        .unwrap();

    let (status, page) = get(&app, &admin, "/api/admin/orders").await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(page["total"], 4);
    assert_eq!(page["items"].as_array().unwrap().len(), 4);
    let first = &page["items"][0];
    assert_eq!(first["id"], inv_failed.to_string(), "新到舊");
    assert_eq!(first["item_count"], 2);
    assert_eq!(first["shipment_status"], "pending");
    assert_eq!(first["invoice_status"], "failed");
    assert_eq!(first["needs_refund"], false);
    assert_eq!(first["shipping_method"], "home");
    assert_eq!(first["total"], 700);
    assert!(first["paid_at"].is_string());

    let (_, page) = get(&app, &admin, "/api/admin/orders?status=paid").await;
    assert_eq!(page["total"], 2);
    let ids: Vec<&str> = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap())
        .collect();
    assert!(
        ids.contains(&refund.to_string().as_str())
            && ids.contains(&inv_failed.to_string().as_str())
    );

    let (_, page) = get(&app, &admin, "/api/admin/orders?status=pending_payment").await;
    assert_eq!(page["items"][0]["id"], pending.to_string());

    let (_, page) = get(&app, &admin, "/api/admin/orders?flag=needs_refund").await;
    assert_eq!(page["total"], 1);
    assert_eq!(page["items"][0]["id"], refund.to_string());
    assert_eq!(page["items"][0]["needs_refund"], true);

    let (_, page) = get(&app, &admin, "/api/admin/orders?flag=cvs_returned").await;
    assert_eq!(page["total"], 1);
    assert_eq!(page["items"][0]["id"], returned.to_string());
    assert_eq!(page["items"][0]["shipment_status"], "returned");

    let (_, page) = get(&app, &admin, "/api/admin/orders?flag=invoice_failed").await;
    assert_eq!(page["total"], 1);
    assert_eq!(page["items"][0]["id"], inv_failed.to_string());

    let (_, page) = get(&app, &admin, &format!("/api/admin/orders?q={refund_no}")).await;
    assert_eq!(page["total"], 1);
    let (_, page) = get(
        &app,
        &admin,
        "/api/admin/orders?q=%E7%8E%8B%E5%B0%8F%E6%98%8E",
    )
    .await; // 王小明
    assert_eq!(page["total"], 4);
    let (_, page) = get(
        &app,
        &admin,
        "/api/admin/orders?q=buyer%40test.local&status=paid",
    )
    .await;
    assert_eq!(page["total"], 2);

    let (_, page) = get(&app, &admin, "/api/admin/orders?per_page=2&page=2").await;
    assert_eq!(page["total"], 4);
    assert_eq!(page["items"].as_array().unwrap().len(), 2);
    assert_eq!(page["page"], 2);

    let (status, body) = get(&app, &admin, "/api/admin/orders?status=bogus").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "VALIDATION");
    let (status, _) = get(&app, &admin, "/api/admin/orders?flag=bogus").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn detail_has_full_shipment_all_payments_and_no_guest_token(pool: PgPool) {
    let app = common::app(pool.clone());
    let admin = common::admin_cookie(&app, &pool).await;
    let (id, order_no, token) = place_order(&app, &pool, "cvs").await;
    // 買家重新付款一次 → 兩筆付款嘗試
    let (status, _, _) = common::send(
        &app,
        common::req(
            "POST",
            &format!("/api/orders/{id}/repay?t={token}"),
            None,
            Some(json!({ "payment_method": "atm" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, d) = get(&app, &admin, &format!("/api/admin/orders/{id}")).await;
    assert_eq!(status, StatusCode::OK, "{d}");
    assert_eq!(d["order_no"], order_no);
    assert_eq!(d["status"], "pending_payment");
    assert_eq!(d["needs_refund"], false);
    assert!(d["user_id"].is_null(), "訪客訂單 user_id 是 null");
    assert!(d.get("guest_token").is_none(), "後台明細不回 guest_token");
    assert_eq!(d["items"][0]["product_name"], "雞肉狗糧");
    assert_eq!(d["items"][0]["quantity"], 2);

    let s = &d["shipment"];
    assert_eq!(s["method"], "cvs");
    assert_eq!(s["status"], "pending");
    assert_eq!(s["cvs_sub_type"], "UNIMARTC2C");
    assert_eq!(s["cvs_store_id"], "131386");
    assert_eq!(s["cvs_store_name"], "測試門市");
    assert!(s["ecpay_logistics_id"].is_null());
    assert!(s["ecpay_merchant_trade_no"].is_null());
    assert!(s["last_status_code"].is_null());
    assert!(s.get("raw").is_none(), "raw 不給前端");
    assert!(s["created_at"].is_string());

    let payments = d["payments"].as_array().unwrap();
    assert_eq!(payments.len(), 2);
    assert_eq!(
        payments[0]["merchant_trade_no"],
        format!("{order_no}02"),
        "新到舊"
    );
    assert_eq!(payments[0]["method"], "atm");
    assert_eq!(payments[1]["merchant_trade_no"], format!("{order_no}01"));
    assert_eq!(payments[1]["status"], "pending");
    assert_eq!(payments[1]["amount"], 660);
    assert!(payments[1]["created_at"].is_string());

    assert_eq!(d["invoice"]["status"], "pending");
    assert!(d["invoice"]["error"].is_null());
    assert!(d["invoice"]["updated_at"].is_string());

    let (status, _) = get(
        &app,
        &admin,
        &format!("/api/admin/orders/{}", Uuid::now_v7()),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
