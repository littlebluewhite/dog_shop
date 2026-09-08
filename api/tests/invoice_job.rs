mod common;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use chrono::{DateTime, TimeZone, Utc};
use dog_shop_api::domain::jobs;
use dog_shop_api::domain::orders::{
    self, HomeAddress, InvoiceInput, OrderInput, OrderItemInput, Viewer,
};
use dog_shop_api::ecpay::mac;
use dog_shop_api::jobs::worker;
use dog_shop_api::state::AppState;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

fn company_invoice() -> InvoiceInput {
    InvoiceInput {
        kind: "company".to_string(),
        tax_id: "04595257".to_string(),
        title: "測試公司".to_string(),
        address: "台北市信義區市府路 1 號".to_string(),
        ..Default::default()
    }
}

fn personal_invoice() -> InvoiceInput {
    InvoiceInput {
        kind: "personal".to_string(),
        carrier_type: "1".to_string(),
        ..Default::default()
    }
}

fn input(items: Vec<(Uuid, i32)>, invoice: InvoiceInput) -> OrderInput {
    OrderInput {
        items: items
            .into_iter()
            .map(|(variant_id, qty)| OrderItemInput { variant_id, qty })
            .collect(),
        email: "buyer@test.local".to_string(),
        recipient_name: "王小明".to_string(),
        recipient_phone: "0912345678".to_string(),
        shipping_method: "home".to_string(),
        cvs_store_token: None,
        address: Some(HomeAddress {
            postal_code: "100".to_string(),
            city: "臺北市".to_string(),
            district: "中正區".to_string(),
            street: "重慶南路一段 122 號".to_string(),
        }),
        invoice,
        payment_method: "credit".to_string(),
        note: String::new(),
    }
}

async fn run_all(state: &AppState) {
    while worker::run_once(state).await.unwrap() > 0 {}
}

async fn mark_paid(pool: &PgPool, order_id: Uuid) {
    sqlx::query("UPDATE orders SET status = 'paid', paid_at = now() WHERE id = $1")
        .bind(order_id)
        .execute(pool)
        .await
        .unwrap();
}

async fn enqueue_invoice(pool: &PgPool, order_id: Uuid, dedupe: Option<&str>) {
    let mut tx = pool.begin().await.unwrap();
    jobs::enqueue(
        &mut tx,
        jobs::KIND_ISSUE_INVOICE,
        json!({ "order_id": order_id }),
        dedupe,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

type InvoiceRow = (
    String,
    Option<String>,
    Option<DateTime<Utc>>,
    Option<String>,
    Option<Value>,
    Option<Value>,
    Option<String>,
);

async fn invoice_row(pool: &PgPool, order_id: Uuid) -> InvoiceRow {
    sqlx::query_as(
        "SELECT status, invoice_no, invoice_date, random_number, request, response, error FROM invoices WHERE order_id = $1",
    )
    .bind(order_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn job_rows(pool: &PgPool, kind: &str) -> Vec<(String, i32, Option<String>)> {
    sqlx::query_as("SELECT status, attempts, last_error FROM jobs WHERE kind = $1 ORDER BY id")
        .bind(kind)
        .fetch_all(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn paid_order_gets_invoice_and_email(pool: PgPool) {
    let (app, state) = common::app_with_state(pool.clone());
    let (variant, _) = common::active_product(&pool, "雞肉狗糧", 300, 5).await;
    let created = orders::create_order(&pool, input(vec![(variant, 2)], company_invoice()), None)
        .await
        .unwrap();
    mark_paid(&pool, created.order_id).await;
    enqueue_invoice(
        &pool,
        created.order_id,
        Some(&format!("invoice:{}", created.order_id)),
    )
    .await;
    run_all(&state).await;

    let (status, invoice_no, invoice_date, random_number, request, response, error) =
        invoice_row(&pool, created.order_id).await;
    assert_eq!(status, "issued");
    assert_eq!(invoice_no.as_deref(), Some("AB12345678"));
    assert_eq!(random_number.as_deref(), Some("1234"));
    assert_eq!(
        invoice_date,
        Some(Utc.with_ymd_and_hms(2026, 9, 6, 7, 30, 23).unwrap()),
        "台北 15:30:23"
    );
    assert!(error.is_none());
    let request = request.expect("先存請求再送（規格 §14）");
    assert_eq!(request["MerchantID"], "2000132");
    assert_eq!(request["RelateNumber"], created.order_no);
    assert_eq!(request["CustomerIdentifier"], "04595257");
    assert_eq!(request["CustomerName"], "測試公司");
    assert_eq!(request["Print"], "1");
    assert_eq!(request["SalesAmount"], 700);
    assert_eq!(request["Items"].as_array().unwrap().len(), 2);
    assert_eq!(request["Items"][1]["ItemName"], "運費");
    assert_eq!(response.unwrap()["RtnCode"], 1);

    let calls = common::fake_invoices(&state).calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].relate_number, created.order_no);

    let emails = common::sent_emails(&state);
    assert_eq!(emails.len(), 2, "order_created + invoice_issued");
    assert!(
        emails[1].subject.contains("電子發票"),
        "{}",
        emails[1].subject
    );
    assert!(emails[1].text.contains("AB12345678") && emails[1].text.contains("1234"));

    // 再排一次（不同 dedupe）→ 已開立就略過，不再打綠界、不再寄信
    enqueue_invoice(&pool, created.order_id, None).await;
    run_all(&state).await;
    assert_eq!(common::fake_invoices(&state).calls().len(), 1);
    assert_eq!(common::sent_emails(&state).len(), 2);
    let jobs = job_rows(&pool, "issue_invoice").await;
    assert_eq!(jobs.len(), 2);
    assert!(jobs.iter().all(|j| j.0 == "done"), "{jobs:?}");

    // 訂單頁看得到發票
    let (status, detail, _) = common::send(
        &app,
        common::req(
            "GET",
            &format!("/api/orders/{}?t={}", created.order_id, created.guest_token),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["invoice"]["status"], "issued");
    assert_eq!(detail["invoice"]["invoice_no"], "AB12345678");
    assert_eq!(detail["invoice"]["random_number"], "1234");
}

#[sqlx::test(migrations = "./migrations")]
async fn ecpay_errors_are_recorded_retried_then_marked_failed(pool: PgPool) {
    let state = common::state(pool.clone());
    let (variant, _) = common::active_product(&pool, "A", 300, 5).await;
    let created = orders::create_order(&pool, input(vec![(variant, 1)], personal_invoice()), None)
        .await
        .unwrap();
    mark_paid(&pool, created.order_id).await;
    let fake = common::fake_invoices(&state);

    fake.fail_next_with_rtn(1000007, "RelateNumber 重複");
    enqueue_invoice(
        &pool,
        created.order_id,
        Some(&format!("invoice:{}", created.order_id)),
    )
    .await;
    assert_eq!(
        worker::run_once(&state).await.unwrap(),
        2,
        "order_created 信 + issue_invoice"
    );
    let (status, invoice_no, _, _, _, response, error) = invoice_row(&pool, created.order_id).await;
    assert_eq!(status, "pending", "還會重試");
    assert!(invoice_no.is_none());
    assert!(error.unwrap().contains("1000007"));
    assert_eq!(response.unwrap()["RtnMsg"], "RelateNumber 重複");
    let jobs = job_rows(&pool, "issue_invoice").await;
    assert_eq!((jobs[0].0.as_str(), jobs[0].1), ("queued", 1));

    // 讓下一次成為最後一次；這次連線層失敗 → invoices 標 failed
    sqlx::query("UPDATE jobs SET run_at = now(), max_attempts = 2 WHERE kind = 'issue_invoice'")
        .execute(&pool)
        .await
        .unwrap();
    fake.fail_next_with_error("connect timeout");
    assert_eq!(worker::run_once(&state).await.unwrap(), 1);
    let (status, _, _, _, _, _, error) = invoice_row(&pool, created.order_id).await;
    assert_eq!(status, "failed");
    assert!(error.unwrap().contains("connect timeout"));
    let jobs = job_rows(&pool, "issue_invoice").await;
    assert_eq!((jobs[0].0.as_str(), jobs[0].1), ("failed", 2));
    assert_eq!(fake.calls().len(), 2);
    assert_eq!(
        common::sent_emails(&state).len(),
        1,
        "沒有 invoice_issued 信"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn unpaid_or_cancelled_order_is_skipped(pool: PgPool) {
    let state = common::state(pool.clone());
    let (variant, _) = common::active_product(&pool, "A", 300, 5).await;
    let created = orders::create_order(&pool, input(vec![(variant, 1)], personal_invoice()), None)
        .await
        .unwrap();

    enqueue_invoice(
        &pool,
        created.order_id,
        Some(&format!("invoice:{}", created.order_id)),
    )
    .await;
    run_all(&state).await;
    assert_eq!(common::fake_invoices(&state).calls().len(), 0, "待付款不開");
    assert_eq!(invoice_row(&pool, created.order_id).await.0, "pending");

    orders::cancel(
        &pool,
        created.order_id,
        &Viewer::Guest(created.guest_token.clone()),
        "buyer",
    )
    .await
    .unwrap();
    enqueue_invoice(&pool, created.order_id, None).await;
    run_all(&state).await;
    assert_eq!(common::fake_invoices(&state).calls().len(), 0, "取消的不開");
    let jobs = job_rows(&pool, "issue_invoice").await;
    assert!(jobs.iter().all(|j| j.0 == "done"), "略過算完成：{jobs:?}");
}

/// 模擬綠界伺服器的 ReturnURL（同 tests/ecpay_payment.rs）
async fn ecpay_return(app: &Router, mtn: &str) -> (StatusCode, String) {
    let mut fields: Vec<(String, String)> = [
        ("MerchantID", "3002607"),
        ("MerchantTradeNo", mtn),
        ("RtnCode", "1"),
        ("RtnMsg", "交易成功"),
        ("TradeNo", "2609061530000001"),
        ("TradeAmt", "700"),
        ("PaymentDate", "2026/09/06 15:30:23"),
        ("PaymentType", "Credit_CreditCard"),
        ("TradeDate", "2026/09/06 15:28:00"),
        ("SimulatePaid", "0"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    let mac = mac::check_mac_value("pwFHCqoQZGmho4w6", "EkRm7iFT261dpevs", &fields);
    fields.push(("CheckMacValue".to_string(), mac));
    let body = form_urlencoded::Serializer::new(String::new())
        .extend_pairs(fields.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .finish();
    let request = Request::builder()
        .method("POST")
        .uri("/api/ecpay/payment/return")
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

#[sqlx::test(migrations = "./migrations")]
async fn full_flow_from_return_callback(pool: PgPool) {
    let (app, state) = common::app_with_state(pool.clone());
    let (variant, _) = common::active_product(&pool, "雞肉狗糧", 300, 5).await;
    let (status, created, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/orders",
            None,
            Some(json!({
                "items": [{ "variant_id": variant, "qty": 2 }],
                "email": "buyer@test.local",
                "recipient_name": "王小明",
                "recipient_phone": "0912345678",
                "shipping_method": "home",
                "address": { "postal_code": "100", "city": "臺北市", "district": "中正區", "street": "重慶南路一段 122 號" },
                "invoice": { "type": "company", "tax_id": "04595257", "title": "測試公司", "address": "台北市信義區市府路 1 號" },
                "payment_method": "credit",
                "note": ""
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let order_id = Uuid::parse_str(created["order_id"].as_str().unwrap()).unwrap();
    let mtn = created["ecpay"]["fields"]["MerchantTradeNo"]
        .as_str()
        .unwrap()
        .to_string();

    let (status, text) = ecpay_return(&app, &mtn).await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "1|OK"));
    run_all(&state).await;

    let (inv_status, invoice_no, ..) = invoice_row(&pool, order_id).await;
    assert_eq!(inv_status, "issued");
    assert_eq!(invoice_no.as_deref(), Some("AB12345678"));
    assert_eq!(common::fake_invoices(&state).calls().len(), 1);
    let subjects: Vec<String> = common::sent_emails(&state)
        .into_iter()
        .map(|m| m.subject)
        .collect();
    assert_eq!(subjects.len(), 3, "{subjects:?}");
    assert!(subjects[0].contains("已成立"));
    assert!(subjects[1].contains("已收到款項"));
    assert!(subjects[2].contains("電子發票"));
    let all_jobs: Vec<(String, String)> =
        sqlx::query_as("SELECT kind, status FROM jobs ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(all_jobs.iter().all(|(_, s)| s == "done"), "{all_jobs:?}");
    assert_eq!(
        all_jobs.len(),
        4,
        "order_created、issue_invoice、payment_received、invoice_issued"
    );
}
