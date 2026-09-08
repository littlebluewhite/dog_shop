mod common;

use dog_shop_api::domain::orders::{
    self, HomeAddress, InvoiceInput, OrderInput, OrderItemInput, Viewer,
};
use dog_shop_api::domain::{jobs, password_resets, users};
use dog_shop_api::jobs::worker;
use dog_shop_api::state::AppState;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

fn home_input(items: Vec<(Uuid, i32)>, method: &str) -> OrderInput {
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
        invoice: InvoiceInput {
            kind: "personal".to_string(),
            carrier_type: "1".to_string(),
            ..Default::default()
        },
        payment_method: method.to_string(),
        note: String::new(),
    }
}

fn cvs_input(items: Vec<(Uuid, i32)>, token: String) -> OrderInput {
    OrderInput {
        shipping_method: "cvs".to_string(),
        cvs_store_token: Some(token),
        address: None,
        ..home_input(items, "credit")
    }
}

/// 一直跑到沒有可認領的 job
async fn run_all(state: &AppState) {
    while worker::run_once(state).await.unwrap() > 0 {}
}

async fn enqueue(pool: &PgPool, payload: Value) {
    let mut tx = pool.begin().await.unwrap();
    jobs::enqueue(&mut tx, jobs::KIND_SEND_EMAIL, payload, None)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

async fn job_rows(pool: &PgPool) -> Vec<(String, i32, Value, Option<String>)> {
    sqlx::query_as("SELECT status, attempts, payload, last_error FROM jobs ORDER BY id")
        .fetch_all(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn order_created_email_goes_to_guest_with_token_link(pool: PgPool) {
    let state = common::state(pool.clone());
    let (variant, _) = common::active_product(&pool, "雞肉狗糧", 300, 5).await;
    let created = orders::create_order(&pool, home_input(vec![(variant, 2)], "credit"), None)
        .await
        .unwrap();
    run_all(&state).await;

    let emails = common::sent_emails(&state);
    assert_eq!(emails.len(), 1);
    let m = &emails[0];
    assert_eq!(m.to, "buyer@test.local");
    assert!(
        m.subject.contains(&created.order_no) && m.subject.contains("已成立"),
        "{}",
        m.subject
    );
    let link = format!(
        "http://localhost:5173/orders/{}?t={}",
        created.order_id, created.guest_token
    );
    assert!(m.text.contains(&link), "{}", m.text);
    assert!(m.html.contains(&link));
    assert!(m.text.contains("雞肉狗糧（預設）× 2"), "{}", m.text);
    assert!(m.text.contains("NT$ 600") && m.text.contains("NT$ 100") && m.text.contains("NT$ 700"));
    assert!(m.text.contains("信用卡"));
    assert!(
        m.text.contains("宅配：100臺北市中正區重慶南路一段 122 號"),
        "{}",
        m.text
    );
    assert!(m.html.contains("<!doctype html>") && m.html.contains("<strong>"));

    let jobs = job_rows(&pool).await;
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].0, "done");
    assert_eq!(jobs[0].2, json!({}), "做完不留個資");
}

#[sqlx::test(migrations = "./migrations")]
async fn member_order_link_has_no_token(pool: PgPool) {
    let state = common::state(pool.clone());
    let hash = dog_shop_api::auth::password::hash_password("password123").unwrap();
    let user = users::create(&pool, "m@test.local", &hash, "甲", "customer")
        .await
        .unwrap();
    let (variant, _) = common::active_product(&pool, "A", 100, 5).await;
    let created =
        orders::create_order(&pool, home_input(vec![(variant, 1)], "credit"), Some(&user))
            .await
            .unwrap();
    run_all(&state).await;
    let emails = common::sent_emails(&state);
    assert_eq!(emails.len(), 1);
    assert_eq!(emails[0].to, "buyer@test.local", "寄到訂單填的 Email");
    assert!(emails[0].text.contains(&format!(
        "http://localhost:5173/orders/{}\n",
        created.order_id
    )));
    assert!(!emails[0].text.contains("?t="));
}

/// 排入繳費資訊信之後、worker 跑到之前訂單被取消（或已由另一筆 attempt 付清）：
/// 不能再叫客人去繳一筆已取消的訂單
#[sqlx::test(migrations = "./migrations")]
async fn payment_instructions_not_sent_after_order_cancelled(pool: PgPool) {
    let state = common::state(pool.clone());
    let (variant, _) = common::active_product(&pool, "A", 300, 5).await;
    let created = orders::create_order(&pool, home_input(vec![(variant, 1)], "atm"), None)
        .await
        .unwrap();
    let payment_id: Uuid = sqlx::query_scalar("SELECT id FROM payments WHERE order_id = $1")
        .bind(created.order_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    enqueue(
        &pool,
        json!({ "template": "payment_instructions", "order_id": created.order_id, "payment_id": payment_id }),
    )
    .await;
    orders::cancel(
        &pool,
        created.order_id,
        &Viewer::Guest(created.guest_token.clone()),
        "buyer",
    )
    .await
    .unwrap();
    run_all(&state).await;

    let emails = common::sent_emails(&state);
    assert!(
        !emails.iter().any(|m| m.subject.contains("繳費資訊")),
        "訂單已取消還是寄了繳費資訊信：{:?}",
        emails.iter().map(|m| &m.subject).collect::<Vec<_>>()
    );
    assert_eq!(emails.len(), 1, "只有 order_created");
    let jobs = job_rows(&pool).await;
    assert!(
        jobs.iter().all(|j| j.0 == "done"),
        "略過不算失敗，job 要標 done：{jobs:?}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn payment_instructions_email_has_atm_or_cvs_details(pool: PgPool) {
    let state = common::state(pool.clone());
    let (variant, _) = common::active_product(&pool, "A", 300, 5).await;
    let created = orders::create_order(&pool, home_input(vec![(variant, 2)], "atm"), None)
        .await
        .unwrap();
    let payment_id: Uuid = sqlx::query_scalar("SELECT id FROM payments WHERE order_id = $1")
        .bind(created.order_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE payments SET atm_bank_code = '812', atm_vaccount = '1234567890123456', expire_at = '2026-09-09T15:59:59Z' WHERE id = $1",
    )
    .bind(payment_id)
    .execute(&pool)
    .await
    .unwrap();
    enqueue(
        &pool,
        json!({ "template": "payment_instructions", "order_id": created.order_id, "payment_id": payment_id }),
    )
    .await;
    run_all(&state).await;
    let emails = common::sent_emails(&state);
    assert_eq!(emails.len(), 2, "order_created + payment_instructions");
    let m = &emails[1];
    assert!(m.subject.contains("繳費資訊"), "{}", m.subject);
    for needle in [
        "ATM 轉帳",
        "812",
        "1234567890123456",
        "2026/09/09 23:59:59",
        "NT$ 700",
    ] {
        assert!(m.text.contains(needle), "純文字缺 {needle}：{}", m.text);
        assert!(m.html.contains(needle), "HTML 缺 {needle}");
    }
    assert!(!m.text.contains("超商繳費代碼"));

    // 超商代碼
    sqlx::query(
        "UPDATE payments SET method = 'cvs_code', atm_bank_code = NULL, atm_vaccount = NULL, cvs_payment_no = 'LLL26090612345', expire_at = '2026-09-09T07:30:23Z' WHERE id = $1",
    )
    .bind(payment_id)
    .execute(&pool)
    .await
    .unwrap();
    enqueue(
        &pool,
        json!({ "template": "payment_instructions", "order_id": created.order_id, "payment_id": payment_id }),
    )
    .await;
    run_all(&state).await;
    let emails = common::sent_emails(&state);
    assert_eq!(emails.len(), 3);
    let m = &emails[2];
    assert!(
        m.text.contains("超商代碼繳費")
            && m.text.contains("LLL26090612345")
            && m.text.contains("2026/09/09 15:30:23"),
        "{}",
        m.text
    );
    assert!(!m.text.contains("虛擬帳號"));
}

#[sqlx::test(migrations = "./migrations")]
async fn status_emails_render(pool: PgPool) {
    let state = common::state(pool.clone());
    let (variant, _) = common::active_product(&pool, "A", 300, 10).await;
    let created = orders::create_order(&pool, home_input(vec![(variant, 2)], "credit"), None)
        .await
        .unwrap();
    let id = created.order_id;

    sqlx::query(
        "UPDATE orders SET status = 'paid', paid_at = '2026-09-06T07:30:23Z' WHERE id = $1",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    enqueue(
        &pool,
        json!({ "template": "payment_received", "order_id": id }),
    )
    .await;
    run_all(&state).await;
    let m = common::sent_emails(&state).into_iter().nth(1).unwrap();
    assert!(m.subject.contains("已收到款項"), "{}", m.subject);
    assert!(
        m.text.contains("2026/09/06 15:30:23") && m.text.contains("NT$ 700"),
        "{}",
        m.text
    );

    sqlx::query(
        "UPDATE invoices SET status = 'issued', invoice_no = 'AB12345678', random_number = '1234', invoice_date = '2026-09-06T07:31:00Z' WHERE order_id = $1",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    enqueue(
        &pool,
        json!({ "template": "invoice_issued", "order_id": id }),
    )
    .await;
    run_all(&state).await;
    let m = common::sent_emails(&state).into_iter().nth(2).unwrap();
    assert!(m.subject.contains("電子發票"), "{}", m.subject);
    assert!(
        m.text.contains("AB12345678")
            && m.text.contains("1234")
            && m.text.contains("2026/09/06 15:31:00"),
        "{}",
        m.text
    );

    sqlx::query("UPDATE orders SET status = 'shipped', shipped_at = now() WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE shipments SET status = 'shipped', carrier = '黑貓宅急便', tracking_no = '9001234567' WHERE order_id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    enqueue(
        &pool,
        json!({ "template": "order_shipped", "order_id": id }),
    )
    .await;
    run_all(&state).await;
    let m = common::sent_emails(&state).into_iter().nth(3).unwrap();
    assert!(m.subject.contains("已出貨"), "{}", m.subject);
    assert!(
        m.text.contains("黑貓宅急便") && m.text.contains("9001234567"),
        "{}",
        m.text
    );

    // 超商取貨的出貨信：門市名稱與地址
    let token = common::cvs_store_token(&pool).await;
    let cvs = orders::create_order(&pool, cvs_input(vec![(variant, 1)], token), None)
        .await
        .unwrap();
    enqueue(
        &pool,
        json!({ "template": "order_shipped", "order_id": cvs.order_id }),
    )
    .await;
    run_all(&state).await;
    let m = common::sent_emails(&state).last().unwrap().clone();
    assert!(m.subject.contains("已出貨"));
    assert!(
        m.text.contains("測試門市") && m.text.contains("台北市中正區重慶南路一段 122 號"),
        "{}",
        m.text
    );
    assert!(m.text.contains("超商簡訊"));
}

#[sqlx::test(migrations = "./migrations")]
async fn password_reset_creates_token_throttles_and_skips_unknown_user(pool: PgPool) {
    let state = common::state(pool.clone());
    let hash = dog_shop_api::auth::password::hash_password("password123").unwrap();
    let user = users::create(&pool, "reset@test.local", &hash, "小美", "customer")
        .await
        .unwrap();

    enqueue(
        &pool,
        json!({ "template": "password_reset", "user_id": user.id }),
    )
    .await;
    run_all(&state).await;
    let emails = common::sent_emails(&state);
    assert_eq!(emails.len(), 1);
    let m = &emails[0];
    assert_eq!(m.to, "reset@test.local");
    assert!(m.subject.contains("重設密碼"), "{}", m.subject);
    assert!(
        m.text.contains("小美") && m.text.contains("60 分鐘"),
        "{}",
        m.text
    );
    let start = m
        .text
        .find("http://localhost:5173/reset/")
        .expect("有重設連結")
        + "http://localhost:5173/reset/".len();
    let token = &m.text[start..start + 64];
    assert!(token.chars().all(|c| c.is_ascii_hexdigit()), "{token}");
    assert!(m.html.contains(token));
    // token 真的能用
    assert_eq!(
        password_resets::consume(&pool, token).await.unwrap(),
        Some(user.id)
    );

    // 10 分鐘內再要一次：job done、沒有新信、沒有新 token（與規格不同之處 25）
    enqueue(
        &pool,
        json!({ "template": "password_reset", "user_id": user.id }),
    )
    .await;
    run_all(&state).await;
    assert_eq!(common::sent_emails(&state).len(), 1);
    let tokens: i64 = sqlx::query_scalar("SELECT count(*) FROM password_resets WHERE user_id = $1")
        .bind(user.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(tokens, 1);

    // 使用者不存在：done、沒信
    enqueue(
        &pool,
        json!({ "template": "password_reset", "user_id": Uuid::now_v7() }),
    )
    .await;
    run_all(&state).await;
    assert_eq!(common::sent_emails(&state).len(), 1);
    let jobs = job_rows(&pool).await;
    assert_eq!(jobs.len(), 3);
    assert!(jobs.iter().all(|j| j.0 == "done"), "{jobs:?}");
}

#[sqlx::test(migrations = "./migrations")]
async fn bad_jobs_fail_and_retry(pool: PgPool) {
    let state = common::state(pool.clone());
    let (variant, _) = common::active_product(&pool, "A", 300, 10).await;
    let created = orders::create_order(&pool, home_input(vec![(variant, 1)], "credit"), None)
        .await
        .unwrap();
    // 發票還沒開就要寄 invoice_issued → 失敗重試
    enqueue(
        &pool,
        json!({ "template": "invoice_issued", "order_id": created.order_id }),
    )
    .await;
    // 不認識的模板
    enqueue(
        &pool,
        json!({ "template": "nope", "order_id": created.order_id }),
    )
    .await;
    // 訂單不存在 → 略過（done）
    enqueue(
        &pool,
        json!({ "template": "order_created", "order_id": Uuid::now_v7() }),
    )
    .await;
    // 不認識的 kind
    let mut tx = pool.begin().await.unwrap();
    jobs::enqueue(&mut tx, "weird", json!({}), None)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    run_all(&state).await;
    let jobs = job_rows(&pool).await;
    assert_eq!(jobs.len(), 5);
    assert_eq!(jobs[0].0, "done", "order_created");
    assert_eq!((jobs[1].0.as_str(), jobs[1].1), ("queued", 1));
    assert!(jobs[1].3.as_deref().unwrap().contains("還沒開立"));
    assert_eq!((jobs[2].0.as_str(), jobs[2].1), ("queued", 1));
    assert!(jobs[2].3.as_deref().unwrap().contains("未知的 email 模板"));
    assert_eq!(jobs[3].0, "done");
    assert_eq!((jobs[4].0.as_str(), jobs[4].1), ("queued", 1));
    assert!(jobs[4].3.as_deref().unwrap().contains("未知的 job kind"));
    assert_eq!(
        common::sent_emails(&state).len(),
        1,
        "只有 order_created 寄出"
    );
}
