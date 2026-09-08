mod common;

use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Request, StatusCode, header},
};
use dog_shop_api::{
    domain::{cvs_stores, settings},
    ecpay::mac,
    jobs::scheduled,
};
use serde_json::{Value, json};
use sqlx::PgPool;

/// stage 的物流憑證（Config::for_tests 用同一組）；Task 4 的狀態通知／門市更新測試才用得到
const KEY: &str = "XBERn1YOvpM9nfZc";
const IV: &str = "h1ONHk4P4yqbl5LK";

fn f(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// 模擬綠界（伺服器或買家瀏覽器）：form-urlencoded POST，沒有 Origin、沒有 X-Requested-With
async fn ecpay_post_raw(
    app: &Router,
    path: &str,
    fields: Vec<(String, String)>,
) -> (StatusCode, String, HeaderMap) {
    let body = form_urlencoded::Serializer::new(String::new())
        .extend_pairs(fields.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .finish();
    let request = Request::builder()
        .method("POST")
        .uri(path)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body))
        .unwrap();
    let (status, value, headers) = common::send(app, request).await;
    let text = match value {
        Value::String(s) => s,
        Value::Null => String::new(),
        other => other.to_string(),
    };
    (status, text, headers)
}

/// 算好 MD5 CheckMacValue 再送（狀態通知、更新門市通知用）；Task 4 才會用到
async fn ecpay_post(
    app: &Router,
    path: &str,
    mut fields: Vec<(String, String)>,
) -> (StatusCode, String) {
    let mac = mac::check_mac_value_md5(KEY, IV, &fields);
    fields.push(("CheckMacValue".to_string(), mac));
    let (status, text, _) = ecpay_post_raw(app, path, fields).await;
    (status, text)
}

/// 買家按「選擇門市」：回綠界表單裡的 token（ExtraData）
async fn map_token(app: &Router, sub_type: &str) -> String {
    let (status, form, _) = common::send(
        app,
        common::req(
            "POST",
            "/api/checkout/cvs-map",
            None,
            Some(json!({ "sub_type": sub_type, "device": 1 })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{form}");
    form["fields"]["ExtraData"].as_str().unwrap().to_string()
}

fn map_reply_fields(token: &str, sub_type: &str) -> Vec<(String, String)> {
    f(&[
        ("MerchantID", "2000933"),
        ("MerchantTradeNo", token),
        ("LogisticsSubType", sub_type),
        ("CVSStoreID", "006598"),
        ("CVSStoreName", "全家測試店"),
        ("CVSAddress", "台北市中正區重慶南路一段 122 號"),
        ("CVSTelephone", "0223456789"),
        ("CVSOutSide", "0"),
        ("ExtraData", token),
    ])
}

fn location(headers: &HeaderMap) -> String {
    headers
        .get(header::LOCATION)
        .expect("location")
        .to_str()
        .unwrap()
        .to_string()
}

#[sqlx::test(migrations = "./migrations")]
async fn cvs_map_returns_form_and_registers_token(pool: PgPool) {
    let app = common::app(pool.clone());
    let (status, form, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/checkout/cvs-map",
            None,
            Some(json!({ "sub_type": "FAMIC2C", "device": 1 })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{form}");
    assert_eq!(
        form["action"],
        "https://logistics-stage.ecpay.com.tw/Express/map"
    );
    let fields = &form["fields"];
    assert_eq!(fields["MerchantID"], "2000933");
    assert_eq!(fields["LogisticsType"], "CVS");
    assert_eq!(fields["LogisticsSubType"], "FAMIC2C");
    assert_eq!(fields["IsCollection"], "N");
    assert_eq!(fields["Device"], "1");
    assert_eq!(
        fields["ServerReplyURL"],
        "http://localhost:5173/api/ecpay/logistics/map-reply"
    );
    let token = fields["ExtraData"].as_str().unwrap();
    assert_eq!(token.len(), 20);
    assert!(token.chars().all(|c| c.is_ascii_alphanumeric()));
    assert_eq!(fields["MerchantTradeNo"], token);
    assert!(fields.get("CheckMacValue").is_none(), "電子地圖不需簽章");

    let registered: Option<String> =
        sqlx::query_scalar("SELECT sub_type FROM cvs_map_requests WHERE token = $1")
            .bind(token)
            .fetch_optional(&pool)
            .await
            .unwrap();
    assert_eq!(registered.as_deref(), Some("FAMIC2C"));
}

/// 擱置 14：cvs-map 不在 CSRF 豁免前綴內，沒有 X-Requested-With 就 403（`tests/csrf.rs` 同一條規則）
#[sqlx::test(migrations = "./migrations")]
async fn cvs_map_requires_the_csrf_marker(pool: PgPool) {
    let app = common::app(pool.clone());
    let request = Request::builder()
        .method("POST")
        .uri("/api/checkout/cvs-map")
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-forwarded-for", "127.0.0.1")
        .body(Body::from(
            json!({ "sub_type": "UNIMARTC2C", "device": 1 }).to_string(),
        ))
        .unwrap();
    let (status, body, _) = common::send(&app, request).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"]["code"], "FORBIDDEN");
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM cvs_map_requests")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 0, "沒有登記 token");
}

#[sqlx::test(migrations = "./migrations")]
async fn cvs_map_rejects_bad_sub_type(pool: PgPool) {
    let app = common::app(pool);
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/checkout/cvs-map",
            None,
            Some(json!({ "sub_type": "TCAT" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "VALIDATION");
    assert!(body["error"]["details"]["fields"]["sub_type"].is_string());
}

/// POST /api/checkout/cvs-map 掛的是獨立 governor（不跟 /api/orders 共用配額）：burst 10，
/// 前 10 次都會成功（200），第 11 次 429。
#[sqlx::test(migrations = "./migrations")]
async fn cvs_map_is_rate_limited(pool: PgPool) {
    let app = common::app(pool);
    for _ in 0..10 {
        let (status, body, _) = common::send(
            &app,
            common::req(
                "POST",
                "/api/checkout/cvs-map",
                None,
                Some(json!({ "sub_type": "FAMIC2C", "device": 1 })),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/checkout/cvs-map",
            None,
            Some(json!({ "sub_type": "FAMIC2C", "device": 1 })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(body["error"]["code"], "RATE_LIMITED");
}

#[sqlx::test(migrations = "./migrations")]
async fn map_reply_stores_selection_redirects_and_is_single_use(pool: PgPool) {
    let app = common::app(pool.clone());
    let token = map_token(&app, "FAMIC2C").await;

    let (status, _, headers) = ecpay_post_raw(
        &app,
        "/api/ecpay/logistics/map-reply",
        map_reply_fields(&token, "FAMIC2C"),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        location(&headers),
        format!("http://localhost:5173/checkout?store={token}")
    );

    let (status, store, _) = common::send(
        &app,
        common::req(
            "GET",
            &format!("/api/checkout/cvs-store/{token}"),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{store}");
    assert_eq!(store["sub_type"], "FAMIC2C");
    assert_eq!(store["store_id"], "006598");
    assert_eq!(store["store_name"], "全家測試店");
    assert_eq!(store["store_address"], "台北市中正區重慶南路一段 122 號");
    assert_eq!(store["store_phone"], "0223456789");

    // 登記已用掉：同一個 token 再回傳一次就當過期
    assert!(
        cvs_stores::take_map_request(&pool, &token)
            .await
            .unwrap()
            .is_none()
    );
    let (status, _, headers) = ecpay_post_raw(
        &app,
        "/api/ecpay/logistics/map-reply",
        map_reply_fields(&token, "FAMIC2C"),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        location(&headers),
        "http://localhost:5173/checkout?store_error=expired"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn map_reply_without_phone_is_fine_for_unimart(pool: PgPool) {
    let app = common::app(pool.clone());
    let token = map_token(&app, "UNIMARTC2C").await;
    let mut fields = map_reply_fields(&token, "UNIMARTC2C");
    fields.retain(|(k, _)| k != "CVSTelephone");
    let (status, _, headers) = ecpay_post_raw(&app, "/api/ecpay/logistics/map-reply", fields).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert!(location(&headers).ends_with(&format!("?store={token}")));
    let store = cvs_stores::get_valid(&pool, &token).await.unwrap().unwrap();
    assert_eq!(store.store_phone, "");
}

#[sqlx::test(migrations = "./migrations")]
async fn map_reply_rejects_unknown_expired_mismatched_or_incomplete(pool: PgPool) {
    let app = common::app(pool.clone());

    // 沒登記過的 token
    let (status, _, headers) = ecpay_post_raw(
        &app,
        "/api/ecpay/logistics/map-reply",
        map_reply_fields("NoSuchTokenAbcdefghij", "FAMIC2C"),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert!(location(&headers).ends_with("/checkout?store_error=expired"));

    // 登記過但過期
    cvs_stores::insert_map_request(&pool, "ExpiredTokenAbcdefgh", "FAMIC2C", -1)
        .await
        .unwrap();
    let (_, _, headers) = ecpay_post_raw(
        &app,
        "/api/ecpay/logistics/map-reply",
        map_reply_fields("ExpiredTokenAbcdefgh", "FAMIC2C"),
    )
    .await;
    assert!(location(&headers).ends_with("/checkout?store_error=expired"));

    // 超商種類與登記的不符
    let token = map_token(&app, "UNIMARTC2C").await;
    let (_, _, headers) = ecpay_post_raw(
        &app,
        "/api/ecpay/logistics/map-reply",
        map_reply_fields(&token, "FAMIC2C"),
    )
    .await;
    assert!(location(&headers).ends_with("/checkout?store_error=invalid"));
    assert!(
        cvs_stores::get_valid(&pool, &token)
            .await
            .unwrap()
            .is_none(),
        "不符就不寫門市"
    );

    // 欄位不全（沒有 CVSStoreID）
    let token = map_token(&app, "FAMIC2C").await;
    let mut fields = map_reply_fields(&token, "FAMIC2C");
    fields.retain(|(k, _)| k != "CVSStoreID");
    let (_, _, headers) = ecpay_post_raw(&app, "/api/ecpay/logistics/map-reply", fields).await;
    assert!(location(&headers).ends_with("/checkout?store_error=invalid"));
    assert!(
        cvs_stores::take_map_request(&pool, &token)
            .await
            .unwrap()
            .is_some(),
        "欄位不全時登記還在，買家可以再試"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn map_reply_ignores_csrf_headers_but_limits_body(pool: PgPool) {
    let app = common::app(pool);
    // 超過 64 KB 的 body 被 DefaultBodyLimit 擋下；買家看到的仍是 303 回結帳頁（審查 Minor 1）
    let huge = "x".repeat(70 * 1024);
    let (status, _, headers) = ecpay_post_raw(
        &app,
        "/api/ecpay/logistics/map-reply",
        f(&[("ExtraData", &huge)]),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let location = location(&headers);
    assert!(location.ends_with("store_error=invalid"), "{location}");
}

#[sqlx::test(migrations = "./migrations")]
async fn purge_removes_expired_map_requests(pool: PgPool) {
    cvs_stores::insert_map_request(&pool, "OldTokenAbcdefghijkl", "FAMIC2C", -5)
        .await
        .unwrap();
    cvs_stores::insert_map_request(&pool, "NewTokenAbcdefghijkl", "FAMIC2C", 60)
        .await
        .unwrap();
    scheduled::purge_expired(&pool).await.unwrap();
    let left: Vec<String> = sqlx::query_scalar("SELECT token FROM cvs_map_requests ORDER BY token")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(left, vec!["NewTokenAbcdefghijkl".to_string()]);
}

use uuid::Uuid;

fn cvs_order_body(variant: &str, token: &str) -> Value {
    json!({
        "items": [{ "variant_id": variant, "qty": 2 }],
        "email": "buyer@test.local",
        "recipient_name": "王小明",
        "recipient_phone": "0912345678",
        "shipping_method": "cvs",
        "cvs_store_token": token,
        "invoice": { "type": "personal", "carrier_type": "1" },
        "payment_method": "credit",
        "note": ""
    })
}

/// 建一筆超商訂單（訪客），用 SQL 直接標成已付款＋已出貨、物流單已建立（Task 6 的建單流程在這裡跳過）
async fn shipped_cvs_order(app: &Router, pool: &PgPool, mtn: &str, logistics_id: &str) -> Uuid {
    let (variant, _) = common::active_product(pool, "雞肉狗糧", 300, 5).await;
    let token = common::cvs_store_token(pool).await;
    let (status, created, _) = common::send(
        app,
        common::req(
            "POST",
            "/api/orders",
            None,
            Some(cvs_order_body(&variant.to_string(), &token)),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let id = Uuid::parse_str(created["order_id"].as_str().unwrap()).unwrap();
    sqlx::query(
        "UPDATE orders SET status = 'shipped', paid_at = now(), shipped_at = now() WHERE id = $1",
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE shipments SET status = 'created', ecpay_merchant_trade_no = $2, ecpay_logistics_id = $3 WHERE order_id = $1",
    )
    .bind(id)
    .bind(mtn)
    .bind(logistics_id)
    .execute(pool)
    .await
    .unwrap();
    id
}

fn status_fields(mtn: &str, logistics_id: &str, code: &str, msg: &str) -> Vec<(String, String)> {
    f(&[
        ("MerchantID", "2000933"),
        ("MerchantTradeNo", mtn),
        ("RtnCode", code),
        ("RtnMsg", msg),
        ("AllPayLogisticsID", logistics_id),
        ("LogisticsType", "CVS"),
        ("LogisticsSubType", "UNIMARTC2C"),
        ("GoodsAmount", "600"),
        ("UpdateStatusDate", "2026/09/10 18:30:00"),
        ("ReceiverName", "王小明"),
        ("ReceiverCellPhone", "0912345678"),
        ("CVSPaymentNo", "F0001234"),
        ("CVSValidationNo", "1234"),
    ])
}

/// (shipments.status, orders.status, last_status_code, last_status_msg, orders.completed_at 有無)
async fn snapshot(
    pool: &PgPool,
    id: Uuid,
) -> (String, String, Option<String>, Option<String>, bool) {
    sqlx::query_as(
        "SELECT s.status, o.status, s.last_status_code, s.last_status_msg, o.completed_at IS NOT NULL
         FROM shipments s JOIN orders o ON o.id = s.order_id WHERE s.order_id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn status_callback_rejects_bad_mac_and_missing_fields(pool: PgPool) {
    let app = common::app(pool.clone());
    let id = shipped_cvs_order(&app, &pool, "DS260908AAAAL01", "10035").await;

    let mut fields = status_fields("DS260908AAAAL01", "10035", "2030", "物流中心驗收成功");
    fields.push(("CheckMacValue".to_string(), "0".repeat(32)));
    let (status, text, _) = ecpay_post_raw(&app, "/api/ecpay/logistics/status", fields).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(text, "0|CheckMacValue Error");

    let mut fields = status_fields("DS260908AAAAL01", "10035", "2030", "x");
    fields.retain(|(k, _)| k != "RtnCode");
    let (status, text) = ecpay_post(&app, "/api/ecpay/logistics/status", fields).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(text, "0|Missing Field");

    let (s, o, code, _, _) = snapshot(&pool, id).await;
    assert_eq!((s.as_str(), o.as_str(), code), ("created", "shipped", None));
}

#[sqlx::test(migrations = "./migrations")]
async fn status_callback_unknown_trade_no_answers_0(pool: PgPool) {
    let app = common::app(pool);
    let (status, text) = ecpay_post(
        &app,
        "/api/ecpay/logistics/status",
        status_fields("DS000000ZZZZL01", "99999", "2030", "x"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(text, "0|Unknown MerchantTradeNo");
}

#[sqlx::test(migrations = "./migrations")]
async fn status_callback_moves_forward_completes_on_pickup_and_never_regresses(pool: PgPool) {
    let app = common::app(pool.clone());
    let id = shipped_cvs_order(&app, &pool, "DS260908BBBBL01", "10036").await;
    let post = |code: &'static str, msg: &'static str| {
        let app = app.clone();
        async move {
            ecpay_post(
                &app,
                "/api/ecpay/logistics/status",
                status_fields("DS260908BBBBL01", "10036", code, msg),
            )
            .await
        }
    };

    assert_eq!(
        post("2030", "物流中心驗收成功").await,
        (StatusCode::OK, "1|OK".to_string())
    );
    let (s, o, code, msg, _) = snapshot(&pool, id).await;
    assert_eq!((s.as_str(), o.as_str()), ("in_transit", "shipped"));
    assert_eq!(code.as_deref(), Some("2030"));
    assert_eq!(msg.as_deref(), Some("物流中心驗收成功"));

    assert_eq!(post("2073", "商品配達買家取貨門市").await.1, "1|OK");
    assert_eq!(snapshot(&pool, id).await.0, "arrived");

    // 晚到的物流中心通知：不倒退，但代碼照記
    assert_eq!(post("2030", "物流中心驗收成功").await.1, "1|OK");
    let (s, _, code, _, _) = snapshot(&pool, id).await;
    assert_eq!((s.as_str(), code.as_deref()), ("arrived", Some("2030")));

    assert_eq!(post("2067", "消費者成功取件").await.1, "1|OK");
    let (s, o, _, _, completed) = snapshot(&pool, id).await;
    assert_eq!(
        (s.as_str(), o.as_str(), completed),
        ("picked_up", "completed", true)
    );

    // 重複通知：no-op 仍回 1|OK；之後任何代碼都不改終態
    assert_eq!(post("2067", "消費者成功取件").await.1, "1|OK");
    assert_eq!(post("2074", "消費者七天未取").await.1, "1|OK");
    let (s, o, _, _, _) = snapshot(&pool, id).await;
    assert_eq!((s.as_str(), o.as_str()), ("picked_up", "completed"));

    let raw: Value = sqlx::query_scalar("SELECT raw FROM shipments WHERE order_id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(raw["last_notification"]["RtnCode"], "2074");
    assert!(
        raw["last_notification"]["CheckMacValue"].is_string(),
        "原始 payload 整包存進 raw"
    );
}

/// 擱置 17：取件通知只有在訂單是 shipped 時才把訂單轉 completed。
/// 訂單被改回 paid（例如出貨後又被回退）之後，晚到的 2067 只動 shipments
#[sqlx::test(migrations = "./migrations")]
async fn pickup_does_not_complete_an_order_that_is_not_shipped(pool: PgPool) {
    let app = common::app(pool.clone());
    let id = shipped_cvs_order(&app, &pool, "DS260908FFFFL01", "10041").await;

    ecpay_post(
        &app,
        "/api/ecpay/logistics/status",
        status_fields("DS260908FFFFL01", "10041", "2073", "商品配達買家取貨門市"),
    )
    .await;
    assert_eq!(snapshot(&pool, id).await.0, "arrived");

    sqlx::query("UPDATE orders SET status = 'paid', completed_at = NULL WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();

    let (status, text) = ecpay_post(
        &app,
        "/api/ecpay/logistics/status",
        status_fields("DS260908FFFFL01", "10041", "2067", "消費者成功取件"),
    )
    .await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "1|OK"));
    let (s, o, _, _, completed) = snapshot(&pool, id).await;
    assert_eq!(
        (s.as_str(), o.as_str(), completed),
        ("picked_up", "paid", false),
        "訂單不是已出貨就不轉已完成"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn status_callback_returned_keeps_order_shipped_and_can_be_redelivered(pool: PgPool) {
    let app = common::app(pool.clone());
    let id = shipped_cvs_order(&app, &pool, "DS260908CCCCL01", "10037").await;
    let mut fields = status_fields(
        "DS260908CCCCL01",
        "10037",
        "3018",
        "到店尚未取貨，簡訊通知取件",
    );
    fields[6].1 = "FAMIC2C".to_string();
    ecpay_post(&app, "/api/ecpay/logistics/status", fields).await;
    assert_eq!(snapshot(&pool, id).await.0, "arrived");

    ecpay_post(
        &app,
        "/api/ecpay/logistics/status",
        status_fields("DS260908CCCCL01", "10037", "3020", "貨件未取退回物流中心"),
    )
    .await;
    let (s, o, _, _, completed) = snapshot(&pool, id).await;
    assert_eq!(
        (s.as_str(), o.as_str(), completed),
        ("returned", "shipped", false)
    );

    // 重新配達取件門市 → 回到 arrived
    ecpay_post(
        &app,
        "/api/ecpay/logistics/status",
        status_fields("DS260908CCCCL01", "10037", "2098", "包裹重新配達取件門市"),
    )
    .await;
    assert_eq!(snapshot(&pool, id).await.0, "arrived");
}

#[sqlx::test(migrations = "./migrations")]
async fn status_callback_unknown_code_only_records_and_falls_back_to_logistics_id(pool: PgPool) {
    let app = common::app(pool.clone());
    let id = shipped_cvs_order(&app, &pool, "DS260908DDDDL01", "10038").await;

    // 門市關轉店：不在對照表，只記代碼與訊息
    let (status, text) = ecpay_post(
        &app,
        "/api/ecpay/logistics/status",
        status_fields("DS260908DDDDL01", "10038", "2101", "門市關轉店"),
    )
    .await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "1|OK"));
    let (s, _, code, msg, _) = snapshot(&pool, id).await;
    assert_eq!(
        (s.as_str(), code.as_deref(), msg.as_deref()),
        ("created", Some("2101"), Some("門市關轉店"))
    );

    // MerchantTradeNo 對不上（例如綠界自己補的號）但 AllPayLogisticsID 對得上
    let (status, text) = ecpay_post(
        &app,
        "/api/ecpay/logistics/status",
        status_fields("SOMETHING_ELSE", "10038", "2030", "物流中心驗收成功"),
    )
    .await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "1|OK"));
    assert_eq!(snapshot(&pool, id).await.0, "in_transit");
}

/// 審查 I2：`apply_status` 不能把建單認領用的 'creating' 標記換成綠界代碼 ——
/// 代碼沒對照時 status 仍是 pending、ecpay_logistics_id 仍是 NULL，認領守衛就會整個失效，
/// 同一秒內的第二次點擊會用 L02 再建一張真的物流單
#[sqlx::test(migrations = "./migrations")]
async fn status_notification_does_not_release_the_create_claim(pool: PgPool) {
    let (app, state) = common::app_with_state(pool.clone());
    let admin = common::admin_cookie(&app, &pool).await;
    let mut all = settings::get_all(&pool).await.unwrap();
    all.sender.name = "狗狗商店".to_string();
    all.sender.phone = "0987654321".to_string();
    settings::put_all(&pool, &all).await.unwrap();

    let (variant, _) = common::active_product(&pool, "雞肉狗糧", 300, 5).await;
    let token = common::cvs_store_token(&pool).await;
    let (status, created, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/orders",
            None,
            Some(cvs_order_body(&variant.to_string(), &token)),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let id = Uuid::parse_str(created["order_id"].as_str().unwrap()).unwrap();
    let order_no: String = sqlx::query_scalar(
        "UPDATE orders SET status = 'paid', paid_at = now() WHERE id = $1 RETURNING order_no",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // 建單認領中：request 還在往返途中
    let mtn = format!("{order_no}L01");
    sqlx::query(
        "UPDATE shipments SET status = 'pending', last_status_code = 'creating',
                ecpay_merchant_trade_no = $2, updated_at = now() WHERE order_id = $1",
    )
    .bind(id)
    .bind(&mtn)
    .execute(&pool)
    .await
    .unwrap();

    // 這時候綠界推一則未對照代碼的通知進來
    let (status, text) = ecpay_post(
        &app,
        "/api/ecpay/logistics/status",
        status_fields(&mtn, "10040", "2101", "門市關轉店"),
    )
    .await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "1|OK"));
    let (s, _, code, msg, _) = snapshot(&pool, id).await;
    assert_eq!(
        (s.as_str(), code.as_deref()),
        ("pending", Some("creating")),
        "認領標記不能被通知洗掉"
    );
    assert_eq!(msg.as_deref(), Some("門市關轉店"), "訊息照舊更新");
    let raw: Value = sqlx::query_scalar("SELECT raw FROM shipments WHERE order_id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        raw["last_notification"]["RtnCode"], "2101",
        "代碼仍完整保存"
    );

    // 另一個分頁按下「建立物流單」：認領守衛仍擋得住，不會再對綠界建一張
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            &format!("/api/admin/orders/{id}/ship-cvs"),
            Some(&admin),
            Some(json!({})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["error"]["details"]["fields"]["status"].is_string(),
        "{body}"
    );
    assert!(
        common::fake_logistics(&state).calls().is_empty(),
        "沒有第二次建單"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn store_update_records_message_without_changing_status(pool: PgPool) {
    let app = common::app(pool.clone());
    let id = shipped_cvs_order(&app, &pool, "DS260908EEEEL01", "10039").await;
    let fields = f(&[
        ("MerchantID", "2000933"),
        ("AllPayLogisticsID", "10039"),
        ("GoodsName", "雞肉狗糧"),
        ("GoodsAmount", "600"),
        ("StoreType", "01"),
        ("Status", "01"),
        ("StoreID", "991182"),
    ]);
    let (status, text) =
        ecpay_post(&app, "/api/ecpay/logistics/store-update", fields.clone()).await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "1|OK"));
    let (s, o, _, msg, _) = snapshot(&pool, id).await;
    assert_eq!((s.as_str(), o.as_str()), ("created", "shipped"));
    assert_eq!(msg.as_deref(), Some("取件門市異動：門市關轉店（991182）"));
    let raw: Value = sqlx::query_scalar("SELECT raw FROM shipments WHERE order_id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(raw["store_updates"].as_array().unwrap().len(), 1);

    // 第二次追加、不覆蓋
    ecpay_post(&app, "/api/ecpay/logistics/store-update", fields).await;
    let raw: Value = sqlx::query_scalar("SELECT raw FROM shipments WHERE order_id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(raw["store_updates"].as_array().unwrap().len(), 2);

    let mut bad = f(&[
        ("MerchantID", "2000933"),
        ("AllPayLogisticsID", "nope"),
        ("StoreType", "01"),
        ("Status", "01"),
        ("StoreID", "1"),
    ]);
    let mac = mac::check_mac_value_md5(KEY, IV, &bad);
    bad.push(("CheckMacValue".to_string(), mac));
    let (status, text, _) = ecpay_post_raw(&app, "/api/ecpay/logistics/store-update", bad).await;
    assert_eq!(
        (status, text.as_str()),
        (StatusCode::OK, "0|Unknown AllPayLogisticsID")
    );

    let (status, text, _) = ecpay_post_raw(
        &app,
        "/api/ecpay/logistics/store-update",
        f(&[("AllPayLogisticsID", "10039"), ("CheckMacValue", "bad")]),
    )
    .await;
    assert_eq!(
        (status, text.as_str()),
        (StatusCode::BAD_REQUEST, "0|CheckMacValue Error")
    );
}
