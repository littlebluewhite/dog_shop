mod common;

use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Request, StatusCode, header},
};
use dog_shop_api::{domain::cvs_stores, ecpay::mac, jobs::scheduled};
use serde_json::{Value, json};
use sqlx::PgPool;

/// stage 的物流憑證（Config::for_tests 用同一組）；Task 4 的狀態通知／門市更新測試才用得到
#[allow(dead_code)]
const KEY: &str = "XBERn1YOvpM9nfZc";
#[allow(dead_code)]
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
#[allow(dead_code)]
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
    // 超過 64 KB 的 body 直接被 DefaultBodyLimit 擋下
    let huge = "x".repeat(70 * 1024);
    let (status, _, _) = ecpay_post_raw(
        &app,
        "/api/ecpay/logistics/map-reply",
        f(&[("ExtraData", &huge)]),
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
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
