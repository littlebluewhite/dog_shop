mod common;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use chrono::{DateTime, TimeZone, Utc};
use dog_shop_api::ecpay::mac;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

/// stage 的 AIO 憑證（Config::for_tests 用同一組）
const KEY: &str = "pwFHCqoQZGmho4w6";
const IV: &str = "EkRm7iFT261dpevs";

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

/// 建一筆 2 × 300 + 運費 100 = 700 的訪客訂單，回 (order_id, merchant_trade_no, guest_token)
async fn place_order(app: &Router, pool: &PgPool, method: &str) -> (Uuid, String, String) {
    let (variant, _) = common::active_product(pool, "雞肉狗糧", 300, 5).await;
    let (status, created, _) = common::send(
        app,
        common::req(
            "POST",
            "/api/orders",
            None,
            Some(order_body(&variant.to_string(), method)),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    (
        Uuid::parse_str(created["order_id"].as_str().unwrap()).unwrap(),
        created["ecpay"]["fields"]["MerchantTradeNo"]
            .as_str()
            .unwrap()
            .to_string(),
        created["guest_token"].as_str().unwrap().to_string(),
    )
}

fn f(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// 模擬綠界伺服器：算好 CheckMacValue，用 form-urlencoded POST（沒有 Origin、沒有 X-Requested-With）
async fn ecpay_post(
    app: &Router,
    path: &str,
    mut fields: Vec<(String, String)>,
) -> (StatusCode, String) {
    let mac = mac::check_mac_value(KEY, IV, &fields);
    fields.push(("CheckMacValue".to_string(), mac));
    ecpay_post_raw(app, path, fields).await
}

async fn ecpay_post_raw(
    app: &Router,
    path: &str,
    fields: Vec<(String, String)>,
) -> (StatusCode, String) {
    let body = form_urlencoded::Serializer::new(String::new())
        .extend_pairs(fields.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .finish();
    let request = Request::builder()
        .method("POST")
        .uri(path)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body))
        .unwrap();
    let (status, value, _) = common::send(app, request).await;
    let text = match value {
        Value::String(s) => s,
        other => other.to_string(),
    };
    (status, text)
}

/// ReturnURL 的欄位（文件）：付款成功
fn return_fields(mtn: &str, amount: &str) -> Vec<(String, String)> {
    f(&[
        ("MerchantID", "3002607"),
        ("MerchantTradeNo", mtn),
        ("StoreID", ""),
        ("RtnCode", "1"),
        ("RtnMsg", "交易成功"),
        ("TradeNo", "2609061530000001"),
        ("TradeAmt", amount),
        ("PaymentDate", "2026/09/06 15:30:23"),
        ("PaymentType", "Credit_CreditCard"),
        ("PaymentTypeChargeFee", "20"),
        ("TradeDate", "2026/09/06 15:28:00"),
        ("SimulatePaid", "0"),
        ("CustomField1", ""),
        ("CustomField2", ""),
        ("CustomField3", ""),
        ("CustomField4", ""),
    ])
}

async fn order_row(pool: &PgPool, id: Uuid) -> (String, Option<DateTime<Utc>>, bool) {
    sqlx::query_as("SELECT status, paid_at, needs_refund FROM orders WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn payment_row(
    pool: &PgPool,
    mtn: &str,
) -> (String, Option<String>, Option<String>, Option<Value>) {
    sqlx::query_as(
        "SELECT status, ecpay_trade_no, payment_type, raw FROM payments WHERE merchant_trade_no = $1",
    )
    .bind(mtn)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn jobs_of(pool: &PgPool, kind: &str) -> Vec<(Value, Option<String>)> {
    sqlx::query_as("SELECT payload, dedupe_key FROM jobs WHERE kind = $1 ORDER BY id")
        .bind(kind)
        .fetch_all(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn credit_return_marks_paid_enqueues_jobs_and_is_idempotent(pool: PgPool) {
    let app = common::app(pool.clone());
    let (order_id, mtn, token) = place_order(&app, &pool, "credit").await;

    let (status, text) = ecpay_post(
        &app,
        "/api/ecpay/payment/return",
        return_fields(&mtn, "700"),
    )
    .await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "1|OK"));

    let (o_status, paid_at, needs_refund) = order_row(&pool, order_id).await;
    assert_eq!(o_status, "paid");
    assert_eq!(
        paid_at,
        Some(Utc.with_ymd_and_hms(2026, 9, 6, 7, 30, 23).unwrap())
    );
    assert!(!needs_refund);
    let (p_status, trade_no, payment_type, raw) = payment_row(&pool, &mtn).await;
    assert_eq!(p_status, "paid");
    assert_eq!(trade_no.as_deref(), Some("2609061530000001"));
    assert_eq!(payment_type.as_deref(), Some("Credit_CreditCard"));
    assert_eq!(raw.unwrap()["RtnMsg"], "交易成功");

    let invoice_jobs = jobs_of(&pool, "issue_invoice").await;
    assert_eq!(invoice_jobs.len(), 1);
    assert_eq!(invoice_jobs[0].0["order_id"], order_id.to_string());
    assert_eq!(
        invoice_jobs[0].1.as_deref(),
        Some(format!("invoice:{order_id}").as_str())
    );
    let emails = jobs_of(&pool, "send_email").await;
    assert_eq!(emails.len(), 2, "order_created + payment_received");
    assert_eq!(emails[1].0["template"], "payment_received");

    // 綠界重送同一筆：回 1|OK、不重做
    let (status, text) = ecpay_post(
        &app,
        "/api/ecpay/payment/return",
        return_fields(&mtn, "700"),
    )
    .await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "1|OK"));
    assert_eq!(jobs_of(&pool, "issue_invoice").await.len(), 1);
    assert_eq!(jobs_of(&pool, "send_email").await.len(), 2);

    // 訂單頁看得到已付款
    let (_, detail, _) = common::send(
        &app,
        common::req(
            "GET",
            &format!("/api/orders/{order_id}?t={token}"),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(detail["status"], "paid");
    assert_eq!(detail["payment"]["status"], "paid");
}

#[sqlx::test(migrations = "./migrations")]
async fn bad_mac_is_400_and_changes_nothing(pool: PgPool) {
    let app = common::app(pool.clone());
    let (order_id, mtn, _) = place_order(&app, &pool, "credit").await;

    let mut fields = return_fields(&mtn, "700");
    fields.push(("CheckMacValue".to_string(), "0".repeat(64)));
    let (status, text) = ecpay_post_raw(&app, "/api/ecpay/payment/return", fields).await;
    assert_eq!(
        (status, text.as_str()),
        (StatusCode::BAD_REQUEST, "0|CheckMacValue Error")
    );

    // 沒帶 CheckMacValue 也是 400
    let (status, _) = ecpay_post_raw(
        &app,
        "/api/ecpay/payment/return",
        return_fields(&mtn, "700"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // 金額被改過（簽章是用原本欄位算的）
    let mut fields = return_fields(&mtn, "700");
    let mac = mac::check_mac_value(KEY, IV, &fields);
    fields.push(("CheckMacValue".to_string(), mac));
    fields.iter_mut().find(|(k, _)| k == "TradeAmt").unwrap().1 = "1".to_string();
    let (status, _) = ecpay_post_raw(&app, "/api/ecpay/payment/return", fields).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (o_status, _, _) = order_row(&pool, order_id).await;
    assert_eq!(o_status, "pending_payment");
    assert_eq!(payment_row(&pool, &mtn).await.0, "pending");
    assert!(jobs_of(&pool, "issue_invoice").await.is_empty());
}

#[sqlx::test(migrations = "./migrations")]
async fn unknown_merchant_trade_no_and_missing_fields(pool: PgPool) {
    let app = common::app(pool.clone());
    let (status, text) = ecpay_post(
        &app,
        "/api/ecpay/payment/return",
        return_fields("DS000000XXXX01", "700"),
    )
    .await;
    assert_eq!(
        (status, text.as_str()),
        (StatusCode::OK, "0|Unknown MerchantTradeNo")
    );

    let (status, text) = ecpay_post(
        &app,
        "/api/ecpay/payment/return",
        f(&[("MerchantID", "3002607"), ("RtnCode", "1")]),
    )
    .await;
    assert_eq!(
        (status, text.as_str()),
        (StatusCode::BAD_REQUEST, "0|Missing Field")
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn simulate_paid_only_logs(pool: PgPool) {
    let app = common::app(pool.clone());
    let (order_id, mtn, _) = place_order(&app, &pool, "credit").await;
    let mut fields = return_fields(&mtn, "700");
    fields
        .iter_mut()
        .find(|(k, _)| k == "SimulatePaid")
        .unwrap()
        .1 = "1".to_string();
    let (status, text) = ecpay_post(&app, "/api/ecpay/payment/return", fields).await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "1|OK"));
    assert_eq!(order_row(&pool, order_id).await.0, "pending_payment");
    assert_eq!(payment_row(&pool, &mtn).await.0, "pending");
    assert!(jobs_of(&pool, "issue_invoice").await.is_empty());
}

#[sqlx::test(migrations = "./migrations")]
async fn failed_rtn_code_marks_payment_failed_only(pool: PgPool) {
    let app = common::app(pool.clone());
    let (order_id, mtn, _) = place_order(&app, &pool, "credit").await;
    let mut fields = return_fields(&mtn, "700");
    fields.iter_mut().find(|(k, _)| k == "RtnCode").unwrap().1 = "10100058".to_string();
    fields.iter_mut().find(|(k, _)| k == "RtnMsg").unwrap().1 = "付款失敗".to_string();
    let (status, text) = ecpay_post(&app, "/api/ecpay/payment/return", fields).await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "1|OK"));
    assert_eq!(order_row(&pool, order_id).await.0, "pending_payment");
    let (p_status, _, _, raw) = payment_row(&pool, &mtn).await;
    assert_eq!(p_status, "failed");
    assert_eq!(raw.unwrap()["RtnCode"], "10100058");
    assert!(jobs_of(&pool, "issue_invoice").await.is_empty());

    // 同一個 MerchantTradeNo 之後成功（買家在綠界頁重刷）→ 正常付款
    let (status, text) = ecpay_post(
        &app,
        "/api/ecpay/payment/return",
        return_fields(&mtn, "700"),
    )
    .await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "1|OK"));
    assert_eq!(order_row(&pool, order_id).await.0, "paid");
    assert_eq!(payment_row(&pool, &mtn).await.0, "paid");
}

#[sqlx::test(migrations = "./migrations")]
async fn late_payment_after_cancel_sets_needs_refund(pool: PgPool) {
    let app = common::app(pool.clone());
    let (order_id, mtn, token) = place_order(&app, &pool, "credit").await;
    let (status, _, _) = common::send(
        &app,
        common::req(
            "POST",
            &format!("/api/orders/{order_id}/cancel?t={token}"),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, text) = ecpay_post(
        &app,
        "/api/ecpay/payment/return",
        return_fields(&mtn, "700"),
    )
    .await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "1|OK"));
    let (o_status, paid_at, needs_refund) = order_row(&pool, order_id).await;
    assert_eq!(o_status, "cancelled", "訂單狀態不動");
    assert!(paid_at.is_none());
    assert!(needs_refund, "遲到的付款要標 needs_refund（規格 §4）");
    assert_eq!(payment_row(&pool, &mtn).await.0, "paid");
    assert!(jobs_of(&pool, "issue_invoice").await.is_empty(), "不開發票");
    // 庫存不動（取消時已歸還）
    let stock: i32 = sqlx::query_scalar("SELECT stock FROM product_variants LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stock, 5);
}

#[sqlx::test(migrations = "./migrations")]
async fn amount_mismatch_is_treated_as_late(pool: PgPool) {
    let app = common::app(pool.clone());
    let (order_id, mtn, _) = place_order(&app, &pool, "credit").await;
    let (status, text) =
        ecpay_post(&app, "/api/ecpay/payment/return", return_fields(&mtn, "1")).await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "1|OK"));
    let (o_status, _, needs_refund) = order_row(&pool, order_id).await;
    assert_eq!(o_status, "pending_payment");
    assert!(needs_refund);
    assert_eq!(payment_row(&pool, &mtn).await.0, "paid");
    assert!(jobs_of(&pool, "issue_invoice").await.is_empty());
}

fn info_fields(mtn: &str, extra: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut fields = f(&[
        ("MerchantID", "3002607"),
        ("MerchantTradeNo", mtn),
        ("StoreID", ""),
        ("RtnMsg", "Get VirtualAccount Succeeded"),
        ("TradeNo", "2609061530000002"),
        ("TradeAmt", "700"),
        ("TradeDate", "2026/09/06 15:28:00"),
        ("CustomField1", ""),
        ("CustomField2", ""),
        ("CustomField3", ""),
        ("CustomField4", ""),
    ]);
    fields.extend(f(extra));
    fields
}

#[sqlx::test(migrations = "./migrations")]
async fn atm_info_is_stored_and_emails_instructions(pool: PgPool) {
    let app = common::app(pool.clone());
    let (order_id, mtn, token) = place_order(&app, &pool, "atm").await;
    let fields = info_fields(
        &mtn,
        &[
            ("RtnCode", "2"),
            ("PaymentType", "ATM_TAISHIN"),
            ("BankCode", "812"),
            ("vAccount", "1234567890123456"),
            ("ExpireDate", "2026/09/09"),
        ],
    );
    let (status, text) = ecpay_post(&app, "/api/ecpay/payment/info", fields).await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "1|OK"));

    let (bank, vaccount, expire_at, p_status): (Option<String>, Option<String>, Option<DateTime<Utc>>, String) =
        sqlx::query_as("SELECT atm_bank_code, atm_vaccount, expire_at, status FROM payments WHERE merchant_trade_no = $1")
            .bind(&mtn)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(bank.as_deref(), Some("812"));
    assert_eq!(vaccount.as_deref(), Some("1234567890123456"));
    // 只有日期 → 該日台北 23:59:59 = UTC 15:59:59
    assert_eq!(
        expire_at,
        Some(Utc.with_ymd_and_hms(2026, 9, 9, 15, 59, 59).unwrap())
    );
    assert_eq!(p_status, "pending", "拿到帳號還沒付款");
    assert_eq!(order_row(&pool, order_id).await.0, "pending_payment");

    let emails = jobs_of(&pool, "send_email").await;
    assert_eq!(emails.len(), 2);
    assert_eq!(emails[1].0["template"], "payment_instructions");
    assert_eq!(emails[1].0["order_id"], order_id.to_string());
    let payment_id: Uuid =
        sqlx::query_scalar("SELECT id FROM payments WHERE merchant_trade_no = $1")
            .bind(&mtn)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(emails[1].0["payment_id"], payment_id.to_string());
    assert_eq!(
        emails[1].1.as_deref(),
        Some(format!("email:payment_instructions:{payment_id}").as_str())
    );

    // 訂單頁看得到帳號
    let (_, detail, _) = common::send(
        &app,
        common::req(
            "GET",
            &format!("/api/orders/{order_id}?t={token}"),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(detail["payment"]["atm_vaccount"], "1234567890123456");
    assert_eq!(detail["payment"]["atm_bank_code"], "812");
    assert_eq!(detail["payment"]["expire_at"], "2026-09-09T15:59:59Z");

    // 綠界重送同一筆 → 1|OK、不再排信
    let fields = info_fields(
        &mtn,
        &[
            ("RtnCode", "2"),
            ("PaymentType", "ATM_TAISHIN"),
            ("BankCode", "812"),
            ("vAccount", "1234567890123456"),
            ("ExpireDate", "2026/09/09"),
        ],
    );
    let (status, text) = ecpay_post(&app, "/api/ecpay/payment/info", fields).await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "1|OK"));
    assert_eq!(jobs_of(&pool, "send_email").await.len(), 2);
}

#[sqlx::test(migrations = "./migrations")]
async fn cvs_info_is_stored(pool: PgPool) {
    let app = common::app(pool.clone());
    let (_, mtn, _) = place_order(&app, &pool, "cvs_code").await;
    let fields = info_fields(
        &mtn,
        &[
            ("RtnCode", "10100073"),
            ("PaymentType", "CVS_CVS"),
            ("PaymentNo", "LLL26090612345"),
            ("ExpireDate", "2026/09/09 15:30:23"),
        ],
    );
    let (status, text) = ecpay_post(&app, "/api/ecpay/payment/info", fields).await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "1|OK"));
    let (payment_no, expire_at): (Option<String>, Option<DateTime<Utc>>) = sqlx::query_as(
        "SELECT cvs_payment_no, expire_at FROM payments WHERE merchant_trade_no = $1",
    )
    .bind(&mtn)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(payment_no.as_deref(), Some("LLL26090612345"));
    assert_eq!(
        expire_at,
        Some(Utc.with_ymd_and_hms(2026, 9, 9, 7, 30, 23).unwrap())
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn info_after_paid_or_unknown_is_harmless(pool: PgPool) {
    let app = common::app(pool.clone());
    let (_, mtn, _) = place_order(&app, &pool, "atm").await;
    let (status, text) = ecpay_post(
        &app,
        "/api/ecpay/payment/return",
        return_fields(&mtn, "700"),
    )
    .await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "1|OK"));

    let fields = info_fields(
        &mtn,
        &[
            ("RtnCode", "2"),
            ("PaymentType", "ATM_TAISHIN"),
            ("BankCode", "812"),
            ("vAccount", "1234567890123456"),
            ("ExpireDate", "2026/09/09"),
        ],
    );
    let (status, text) = ecpay_post(&app, "/api/ecpay/payment/info", fields).await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "1|OK"));
    let vaccount: Option<String> =
        sqlx::query_scalar("SELECT atm_vaccount FROM payments WHERE merchant_trade_no = $1")
            .bind(&mtn)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(vaccount.is_none(), "已付款的不再覆寫");

    let (status, text) = ecpay_post(
        &app,
        "/api/ecpay/payment/info",
        info_fields("DS000000XXXX01", &[("RtnCode", "2")]),
    )
    .await;
    assert_eq!(
        (status, text.as_str()),
        (StatusCode::OK, "0|Unknown MerchantTradeNo")
    );
}
