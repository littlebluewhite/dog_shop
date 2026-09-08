mod common;

use axum::{Router, http::StatusCode};
use dog_shop_api::{domain::settings, ecpay::mac, jobs::worker, state::AppState};
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

/// 後台設定寄件人（建物流單必填）
async fn set_sender(pool: &PgPool, return_store: Option<(&str, &str)>) {
    let mut all = settings::get_all(pool).await.unwrap();
    all.sender.name = "狗狗商店".to_string();
    all.sender.phone = "0987654321".to_string();
    if let Some((sub_type, store_id)) = return_store {
        all.return_store.sub_type = sub_type.to_string();
        all.return_store.store_id = store_id.to_string();
        all.return_store.store_name = "退貨門市".to_string();
    }
    settings::put_all(pool, &all).await.unwrap();
}

async fn run_all_jobs(state: &AppState) {
    while worker::run_once(state).await.unwrap() > 0 {}
}

/// (orders.status, orders.shipped_at 有無, shipments.status, ecpay_merchant_trade_no, ecpay_logistics_id, last_status_code, last_status_msg)
async fn ship_snapshot(
    pool: &PgPool,
    id: Uuid,
) -> (
    String,
    bool,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    sqlx::query_as(
        "SELECT o.status, o.shipped_at IS NOT NULL, s.status, s.ecpay_merchant_trade_no, s.ecpay_logistics_id,
                s.last_status_code, s.last_status_msg
         FROM orders o JOIN shipments s ON s.order_id = o.id WHERE o.id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn ship_cvs_creates_logistics_order_ships_and_mails(pool: PgPool) {
    let (app, state) = common::app_with_state(pool.clone());
    let admin = common::admin_cookie(&app, &pool).await;
    set_sender(&pool, None).await;
    let (id, order_no, _) = place_order(&app, &pool, "cvs").await;
    mark_paid(&pool, id).await;

    let (status, d) = post(
        &app,
        &admin,
        &format!("/api/admin/orders/{id}/ship-cvs"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{d}");
    assert_eq!(d["status"], "shipped");
    assert!(d["shipped_at"].is_string());
    assert_eq!(d["shipment"]["status"], "created");
    assert_eq!(
        d["shipment"]["ecpay_merchant_trade_no"],
        format!("{order_no}L01")
    );
    assert_eq!(
        d["shipment"]["ecpay_logistics_id"],
        format!("FAKE{order_no}L01")
    );
    assert_eq!(d["shipment"]["cvs_payment_no"], "F0001234");
    assert_eq!(d["shipment"]["cvs_validation_no"], "1234");
    assert_eq!(d["shipment"]["last_status_code"], "300");

    let calls = common::fake_logistics(&state).calls();
    assert_eq!(calls.len(), 1);
    let (url, fields) = &calls[0];
    assert_eq!(url, "https://logistics-stage.ecpay.com.tw/Express/Create");
    assert_eq!(fields["MerchantID"], "2000933");
    assert_eq!(fields["MerchantTradeNo"], format!("{order_no}L01"));
    assert_eq!(fields["LogisticsSubType"], "UNIMARTC2C");
    assert_eq!(fields["GoodsAmount"], "600", "商品小計，不含運費");
    assert_eq!(fields["GoodsName"], "雞肉狗糧");
    assert_eq!(fields["SenderName"], "狗狗商店");
    assert_eq!(fields["SenderCellPhone"], "0987654321");
    assert_eq!(fields["ReceiverName"], "王小明");
    assert_eq!(fields["ReceiverCellPhone"], "0912345678");
    assert_eq!(fields["ReceiverEmail"], "buyer@test.local");
    assert_eq!(fields["ReceiverStoreID"], "131386");
    assert_eq!(
        fields["ServerReplyURL"],
        "http://localhost:5173/api/ecpay/logistics/status"
    );
    assert_eq!(
        fields["LogisticsC2CReplyURL"],
        "http://localhost:5173/api/ecpay/logistics/store-update"
    );
    assert!(!fields.contains_key("ReturnStoreID"), "沒設退貨門市");
    let params: Vec<(String, String)> =
        fields.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    assert!(mac::verify_md5(
        "XBERn1YOvpM9nfZc",
        "h1ONHk4P4yqbl5LK",
        &params
    ));

    let raw: Value = sqlx::query_scalar("SELECT raw FROM shipments WHERE order_id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        raw["create_request"]["MerchantTradeNo"],
        format!("{order_no}L01"),
        "先存請求再送"
    );
    assert_eq!(raw["create_response"]["RtnCode"], "300");

    // 出貨信排在同一個交易、worker 寄出
    let queued: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM jobs WHERE kind = 'send_email' AND dedupe_key = $1 AND status = 'queued'",
    )
    .bind(format!("email:order_shipped:{id}"))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(queued, 1);
    run_all_jobs(&state).await;
    let mail = common::sent_emails(&state)
        .into_iter()
        .find(|m| m.subject.contains("已出貨"))
        .expect("出貨信");
    assert!(mail.text.contains("測試門市"), "{}", mail.text);

    // 已出貨就不能再建
    let (status, body) = post(
        &app,
        &admin,
        &format!("/api/admin/orders/{id}/ship-cvs"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "VALIDATION");
    assert_eq!(common::fake_logistics(&state).calls().len(), 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn ship_cvs_sends_return_store_only_for_matching_sub_type(pool: PgPool) {
    let (app, state) = common::app_with_state(pool.clone());
    let admin = common::admin_cookie(&app, &pool).await;
    set_sender(&pool, Some(("FAMIC2C", "006598"))).await;
    let (id, _, _) = place_order(&app, &pool, "cvs").await; // UNIMARTC2C
    mark_paid(&pool, id).await;
    let (status, _) = post(
        &app,
        &admin,
        &format!("/api/admin/orders/{id}/ship-cvs"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        !common::fake_logistics(&state).calls()[0]
            .1
            .contains_key("ReturnStoreID"),
        "退貨門市是全家、訂單是 7-11：不帶"
    );

    set_sender(&pool, Some(("UNIMARTC2C", "991182"))).await;
    let (id2, _, _) = place_order(&app, &pool, "cvs").await;
    mark_paid(&pool, id2).await;
    let (status, _) = post(
        &app,
        &admin,
        &format!("/api/admin/orders/{id2}/ship-cvs"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        common::fake_logistics(&state).calls()[1].1["ReturnStoreID"],
        "991182"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn ship_cvs_rejection_keeps_order_paid_records_reason_and_retries_with_next_no(pool: PgPool) {
    let (app, state) = common::app_with_state(pool.clone());
    let admin = common::admin_cookie(&app, &pool).await;
    set_sender(&pool, None).await;
    let (id, order_no, _) = place_order(&app, &pool, "cvs").await;
    mark_paid(&pool, id).await;

    common::fake_logistics(&state).respond_with("0|收件人姓名格式錯誤");
    let (status, body) = post(
        &app,
        &admin,
        &format!("/api/admin/orders/{id}/ship-cvs"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
    assert_eq!(body["error"]["code"], "ECPAY_ERROR");
    assert!(
        !body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("收件人"),
        "綠界原文不進 message：{body}"
    );
    let (o, shipped, s, mtn, lid, code, msg) = ship_snapshot(&pool, id).await;
    assert_eq!(
        (o.as_str(), shipped, s.as_str()),
        ("paid", false, "pending")
    );
    assert_eq!(mtn.as_deref(), Some(format!("{order_no}L01").as_str()));
    assert!(lid.is_none());
    assert_eq!(code.as_deref(), Some("create_failed"));
    assert_eq!(msg.as_deref(), Some("收件人姓名格式錯誤"));
    let queued: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs WHERE dedupe_key = $1")
        .bind(format!("email:order_shipped:{id}"))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(queued, 0, "失敗不寄出貨信");

    // 連線失敗
    common::fake_logistics(&state).fail_next("connection reset");
    let (status, _) = post(
        &app,
        &admin,
        &format!("/api/admin/orders/{id}/ship-cvs"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    let (_, _, _, mtn, _, code, _) = ship_snapshot(&pool, id).await;
    assert_eq!(
        mtn.as_deref(),
        Some(format!("{order_no}L02").as_str()),
        "每次嘗試換新號"
    );
    assert_eq!(code.as_deref(), Some("create_error"));

    // 回應簽章不符：也當失敗，訊息說明
    let good = dog_shop_api::ecpay::logistics::fake_success_body(
        &common::fake_logistics(&state).calls()[0].1,
    );
    common::fake_logistics(&state).respond_with(&good.replace("RtnCode=300", "RtnCode=301"));
    let (status, _) = post(
        &app,
        &admin,
        &format!("/api/admin/orders/{id}/ship-cvs"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    let (_, _, _, _, _, code, msg) = ship_snapshot(&pool, id).await;
    assert_eq!(code.as_deref(), Some("create_failed"));
    assert!(msg.as_deref().unwrap().starts_with("回應簽章不符"));

    // 第四次成功：L04
    let (status, d) = post(
        &app,
        &admin,
        &format!("/api/admin/orders/{id}/ship-cvs"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{d}");
    assert_eq!(
        d["shipment"]["ecpay_merchant_trade_no"],
        format!("{order_no}L04")
    );
    assert_eq!(d["status"], "shipped");
    assert_eq!(common::fake_logistics(&state).calls().len(), 4);
}

#[sqlx::test(migrations = "./migrations")]
async fn ship_cvs_requires_paid_cvs_order_and_sender(pool: PgPool) {
    let (app, state) = common::app_with_state(pool.clone());
    let admin = common::admin_cookie(&app, &pool).await;
    let (pending, _, _) = place_order(&app, &pool, "cvs").await;
    let (status, body) = post(
        &app,
        &admin,
        &format!("/api/admin/orders/{pending}/ship-cvs"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"]["details"]["fields"]["status"].is_string());

    let (paid, _, _) = place_order(&app, &pool, "cvs").await;
    mark_paid(&pool, paid).await;
    let (status, body) = post(
        &app,
        &admin,
        &format!("/api/admin/orders/{paid}/ship-cvs"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["error"]["details"]["fields"]["sender"].is_string(),
        "沒設寄件人"
    );

    let (home, _, _) = place_order(&app, &pool, "home").await;
    mark_paid(&pool, home).await;
    set_sender(&pool, None).await;
    let (status, body) = post(
        &app,
        &admin,
        &format!("/api/admin/orders/{home}/ship-cvs"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"]["details"]["fields"]["shipping_method"].is_string());
    assert!(common::fake_logistics(&state).calls().is_empty());

    let (status, _) = post(
        &app,
        &admin,
        &format!("/api/admin/orders/{}/ship-cvs", Uuid::now_v7()),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn ship_cvs_finishes_interrupted_transition_without_calling_ecpay_again(pool: PgPool) {
    let (app, state) = common::app_with_state(pool.clone());
    let admin = common::admin_cookie(&app, &pool).await;
    set_sender(&pool, None).await;
    let (id, _, _) = place_order(&app, &pool, "cvs").await;
    mark_paid(&pool, id).await;
    let (status, _) = post(
        &app,
        &admin,
        &format!("/api/admin/orders/{id}/ship-cvs"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // 模擬「綠界建單成功、單號已存，但收尾交易失敗」：訂單還是 paid、shipments 還是 pending、但有綠界單號
    sqlx::query("UPDATE orders SET status = 'paid', shipped_at = NULL WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE shipments SET status = 'pending' WHERE order_id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();

    let (status, d) = post(
        &app,
        &admin,
        &format!("/api/admin/orders/{id}/ship-cvs"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{d}");
    assert_eq!(d["status"], "shipped");
    assert_eq!(d["shipment"]["status"], "created");
    assert_eq!(
        common::fake_logistics(&state).calls().len(),
        1,
        "不再打綠界"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn ship_home_sets_carrier_and_tracking_and_mails(pool: PgPool) {
    let (app, state) = common::app_with_state(pool.clone());
    let admin = common::admin_cookie(&app, &pool).await;
    let (id, _, _) = place_order(&app, &pool, "home").await;

    // 還沒付款
    let (status, _) = post(
        &app,
        &admin,
        &format!("/api/admin/orders/{id}/ship-home"),
        json!({ "carrier": "黑貓", "tracking_no": "900123456789" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    mark_paid(&pool, id).await;
    let (status, body) = post(
        &app,
        &admin,
        &format!("/api/admin/orders/{id}/ship-home"),
        json!({ "carrier": " ", "tracking_no": "" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"]["details"]["fields"]["carrier"].is_string());
    assert!(body["error"]["details"]["fields"]["tracking_no"].is_string());

    let (status, d) = post(
        &app,
        &admin,
        &format!("/api/admin/orders/{id}/ship-home"),
        json!({ "carrier": " 黑貓 ", "tracking_no": " 900123456789 " }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{d}");
    assert_eq!(d["status"], "shipped");
    assert_eq!(d["shipment"]["status"], "shipped");
    assert_eq!(d["shipment"]["carrier"], "黑貓");
    assert_eq!(d["shipment"]["tracking_no"], "900123456789");
    assert!(d["shipped_at"].is_string());

    run_all_jobs(&state).await;
    let mail = common::sent_emails(&state)
        .into_iter()
        .find(|m| m.subject.contains("已出貨"))
        .expect("出貨信");
    assert!(
        mail.text.contains("黑貓") && mail.text.contains("900123456789"),
        "{}",
        mail.text
    );

    // 超商訂單不能走宅配出貨
    let (cvs, _, _) = place_order(&app, &pool, "cvs").await;
    mark_paid(&pool, cvs).await;
    let (status, body) = post(
        &app,
        &admin,
        &format!("/api/admin/orders/{cvs}/ship-home"),
        json!({ "carrier": "黑貓", "tracking_no": "1" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"]["details"]["fields"]["shipping_method"].is_string());
}

#[sqlx::test(migrations = "./migrations")]
async fn print_label_returns_signed_form_after_shipping(pool: PgPool) {
    let app = common::app(pool.clone());
    let admin = common::admin_cookie(&app, &pool).await;
    set_sender(&pool, None).await;
    let (id, order_no, _) = place_order(&app, &pool, "cvs").await;
    mark_paid(&pool, id).await;

    let (status, body) = post(
        &app,
        &admin,
        &format!("/api/admin/orders/{id}/print-label"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "還沒建單：{body}");

    post(
        &app,
        &admin,
        &format!("/api/admin/orders/{id}/ship-cvs"),
        json!({}),
    )
    .await;
    let (status, form) = post(
        &app,
        &admin,
        &format!("/api/admin/orders/{id}/print-label"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{form}");
    assert_eq!(
        form["action"],
        "https://logistics-stage.ecpay.com.tw/Express/PrintUniMartC2COrderInfo"
    );
    assert_eq!(form["fields"]["MerchantID"], "2000933");
    assert_eq!(
        form["fields"]["AllPayLogisticsID"],
        format!("FAKE{order_no}L01")
    );
    assert_eq!(form["fields"]["CVSPaymentNo"], "F0001234");
    assert_eq!(form["fields"]["CVSValidationNo"], "1234");
    let params: Vec<(String, String)> = form["fields"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
        .collect();
    assert!(mac::verify_md5(
        "XBERn1YOvpM9nfZc",
        "h1ONHk4P4yqbl5LK",
        &params
    ));
}

#[sqlx::test(migrations = "./migrations")]
async fn complete_marks_shipped_order_completed(pool: PgPool) {
    let app = common::app(pool.clone());
    let admin = common::admin_cookie(&app, &pool).await;
    let (id, _, _) = place_order(&app, &pool, "home").await;
    mark_paid(&pool, id).await;
    let (status, _) = post(
        &app,
        &admin,
        &format!("/api/admin/orders/{id}/complete"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "只有已出貨能完成");

    post(
        &app,
        &admin,
        &format!("/api/admin/orders/{id}/ship-home"),
        json!({ "carrier": "黑貓", "tracking_no": "1" }),
    )
    .await;
    let (status, d) = post(
        &app,
        &admin,
        &format!("/api/admin/orders/{id}/complete"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{d}");
    assert_eq!(d["status"], "completed");
    assert!(d["completed_at"].is_string());
}

#[sqlx::test(migrations = "./migrations")]
async fn mutating_admin_order_routes_require_admin(pool: PgPool) {
    let app = common::app(pool.clone());
    let (id, _, _) = place_order(&app, &pool, "home").await;
    let (status, _, _) = common::send(
        &app,
        common::req(
            "POST",
            &format!("/api/admin/orders/{id}/ship-home"),
            None,
            Some(json!({ "carrier": "a", "tracking_no": "b" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let customer = common::customer_cookie(&app, &pool).await;
    let (status, _) = post(
        &app,
        &customer,
        &format!("/api/admin/orders/{id}/ship-cvs"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}
