mod common;

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, Utc};
use dog_shop_api::domain::orders::{HomeAddress, InvoiceInput, OrderInput, OrderItemInput};
use dog_shop_api::domain::{jobs, orders};
use dog_shop_api::jobs::{scheduled, worker};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

async fn enqueue(pool: &PgPool, kind: &str, payload: Value) -> i64 {
    let mut tx = pool.begin().await.unwrap();
    jobs::enqueue(&mut tx, kind, payload, None).await.unwrap();
    tx.commit().await.unwrap();
    sqlx::query_scalar("SELECT max(id) FROM jobs")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn job_row(pool: &PgPool, id: i64) -> (String, i32, Option<String>, Value, DateTime<Utc>) {
    sqlx::query_as("SELECT status, attempts, last_error, payload, run_at FROM jobs WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn success_marks_done_and_clears_payload(pool: PgPool) {
    let id = enqueue(&pool, "test", json!({ "secret": "x" })).await;
    let seen = Arc::new(Mutex::new(Vec::<worker::Job>::new()));
    let sink = seen.clone();
    let n = worker::run_once_with(&pool, |job| {
        let sink = sink.clone();
        async move {
            sink.lock().unwrap().push(job);
            Ok(())
        }
    })
    .await
    .unwrap();
    assert_eq!(n, 1);
    {
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].id, id);
        assert_eq!(seen[0].kind, "test");
        assert_eq!(seen[0].attempts, 1, "認領時 +1");
        assert_eq!(seen[0].max_attempts, 5);
        assert_eq!(seen[0].payload["secret"], "x");
        assert!(!seen[0].is_last_attempt());
    }
    let (status, attempts, err, payload, _) = job_row(&pool, id).await;
    assert_eq!((status.as_str(), attempts, err), ("done", 1, None));
    assert_eq!(payload, json!({}), "做完不留個資");

    // 第二輪沒東西
    let n = worker::run_once_with(&pool, |_| async { Ok(()) })
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn failure_backs_off_then_fails_for_good(pool: PgPool) {
    let id = enqueue(&pool, "test", json!({ "keep": 1 })).await;
    sqlx::query("UPDATE jobs SET max_attempts = 2")
        .execute(&pool)
        .await
        .unwrap();
    let before = Utc::now();
    let n = worker::run_once_with(&pool, |_| async { Err(anyhow::anyhow!("boom")) })
        .await
        .unwrap();
    assert_eq!(n, 1);
    let (status, attempts, err, payload, run_at) = job_row(&pool, id).await;
    assert_eq!((status.as_str(), attempts), ("queued", 1));
    assert!(err.unwrap().contains("boom"));
    assert_eq!(payload, json!({ "keep": 1 }));
    // 2^1 = 2 分鐘後
    assert!(
        run_at >= before + Duration::minutes(1) && run_at <= Utc::now() + Duration::minutes(3),
        "{run_at}"
    );

    // run_at 還沒到 → 不會被認領
    let n = worker::run_once_with(&pool, |_| async { Ok(()) })
        .await
        .unwrap();
    assert_eq!(n, 0);
    sqlx::query("UPDATE jobs SET run_at = now()")
        .execute(&pool)
        .await
        .unwrap();

    // 第二次（= max_attempts）再失敗 → failed，payload 保留
    let n = worker::run_once_with(&pool, |job| async move {
        assert!(job.is_last_attempt());
        Err(anyhow::anyhow!("boom again"))
    })
    .await
    .unwrap();
    assert_eq!(n, 1);
    let (status, attempts, err, payload, _) = job_row(&pool, id).await;
    assert_eq!((status.as_str(), attempts), ("failed", 2));
    assert!(err.unwrap().contains("boom again"));
    assert_eq!(payload, json!({ "keep": 1 }));
    let n = worker::run_once_with(&pool, |_| async { Ok(()) })
        .await
        .unwrap();
    assert_eq!(n, 0, "failed 不會再被認領");
}

#[sqlx::test(migrations = "./migrations")]
async fn claims_in_id_order_up_to_batch(pool: PgPool) {
    for i in 0..12 {
        enqueue(&pool, "test", json!({ "i": i })).await;
    }
    let seen = Arc::new(Mutex::new(Vec::<i64>::new()));
    let sink = seen.clone();
    let n = worker::run_once_with(&pool, |job| {
        let sink = sink.clone();
        async move {
            sink.lock()
                .unwrap()
                .push(job.payload["i"].as_i64().unwrap());
            Ok(())
        }
    })
    .await
    .unwrap();
    assert_eq!(n, 10);
    assert_eq!(*seen.lock().unwrap(), (0..10).collect::<Vec<i64>>());
    let n = worker::run_once_with(&pool, |_| async { Ok(()) })
        .await
        .unwrap();
    assert_eq!(n, 2);
}

#[sqlx::test(migrations = "./migrations")]
async fn stale_running_is_requeued(pool: PgPool) {
    enqueue(&pool, "test", json!({})).await;
    sqlx::query("UPDATE jobs SET status = 'running', updated_at = now() - interval '11 minutes'")
        .execute(&pool)
        .await
        .unwrap();
    let n = worker::run_once_with(&pool, |_| async { Ok(()) })
        .await
        .unwrap();
    assert_eq!(n, 0, "running 不會被認領");
    assert_eq!(worker::requeue_stale(&pool).await.unwrap(), 1);
    let n = worker::run_once_with(&pool, |_| async { Ok(()) })
        .await
        .unwrap();
    assert_eq!(n, 1);

    // 剛開始跑的 running 不會被撿
    enqueue(&pool, "test", json!({})).await;
    sqlx::query("UPDATE jobs SET status = 'running', updated_at = now() WHERE status = 'queued'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(worker::requeue_stale(&pool).await.unwrap(), 0);
}

#[test]
fn backoff_doubles() {
    assert_eq!(worker::backoff_minutes(1), 2);
    assert_eq!(worker::backoff_minutes(2), 4);
    assert_eq!(worker::backoff_minutes(5), 32);
    assert_eq!(worker::backoff_minutes(99), 1024, "有上限");
}

#[sqlx::test(migrations = "./migrations")]
async fn panicking_handler_is_treated_as_failure_and_does_not_kill_the_batch(pool: PgPool) {
    let panics = enqueue(&pool, "test", json!({})).await;
    let healthy = enqueue(&pool, "test", json!({})).await;
    let before = Utc::now();
    // run_once_with 本身要 Ok：panic 被隔離、不會冒出來讓這次呼叫失敗
    let n = worker::run_once_with(&pool, move |job| async move {
        if job.id == panics {
            panic!("boom");
        }
        Ok(())
    })
    .await
    .unwrap();
    assert_eq!(n, 2, "同一批的另一筆健康 job 不受影響");

    let (status, attempts, err, payload, run_at) = job_row(&pool, panics).await;
    assert_eq!(
        (status.as_str(), attempts),
        ("queued", 1),
        "跟 Err 一樣走退避，不是卡在 running"
    );
    assert!(err.unwrap().contains("panicked"));
    assert_eq!(payload, json!({}));
    assert!(
        run_at >= before + Duration::minutes(1) && run_at <= Utc::now() + Duration::minutes(3),
        "{run_at}"
    );

    let (status, attempts, err, payload, _) = job_row(&pool, healthy).await;
    assert_eq!(
        (status.as_str(), attempts, err),
        ("done", 1, None),
        "同批的健康 job 照樣做完"
    );
    assert_eq!(payload, json!({}));
}

// ───── 排程工作 ─────

fn input(items: Vec<(Uuid, i32)>) -> OrderInput {
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
        payment_method: "atm".to_string(),
        note: String::new(),
    }
}

async fn stock_of(pool: &PgPool, variant_id: Uuid) -> i32 {
    sqlx::query_scalar("SELECT stock FROM product_variants WHERE id = $1")
        .bind(variant_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn order_status(pool: &PgPool, id: Uuid) -> (String, Option<String>) {
    sqlx::query_as("SELECT status, cancel_reason FROM orders WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn expires_orders_by_created_at_or_payment_expiry(pool: PgPool) {
    let (variant, _) = common::active_product(&pool, "A", 100, 10).await;
    let old = orders::create_order(&pool, input(vec![(variant, 2)]), None)
        .await
        .unwrap();
    let fresh = orders::create_order(&pool, input(vec![(variant, 1)]), None)
        .await
        .unwrap();
    let atm_expired = orders::create_order(&pool, input(vec![(variant, 1)]), None)
        .await
        .unwrap();
    let atm_alive = orders::create_order(&pool, input(vec![(variant, 1)]), None)
        .await
        .unwrap();
    assert_eq!(stock_of(&pool, variant).await, 5);

    // 沒有繳費期限、3 天多 → 過期
    sqlx::query("UPDATE orders SET created_at = now() - interval '3 days 1 minute' WHERE id = $1")
        .bind(old.order_id)
        .execute(&pool)
        .await
        .unwrap();
    // 繳費期限 3 小時前 → 過了 2 小時緩衝
    sqlx::query("UPDATE payments SET expire_at = now() - interval '3 hours' WHERE order_id = $1")
        .bind(atm_expired.order_id)
        .execute(&pool)
        .await
        .unwrap();
    // 繳費期限 1 小時前 → 還在緩衝內；就算 created_at 很久以前也以 expire_at 為準
    sqlx::query("UPDATE payments SET expire_at = now() - interval '1 hour' WHERE order_id = $1")
        .bind(atm_alive.order_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE orders SET created_at = now() - interval '10 days' WHERE id = $1")
        .bind(atm_alive.order_id)
        .execute(&pool)
        .await
        .unwrap();

    assert_eq!(scheduled::expire_unpaid_orders(&pool).await.unwrap(), 2);
    assert_eq!(
        order_status(&pool, old.order_id).await,
        ("cancelled".to_string(), Some("expired".to_string()))
    );
    assert_eq!(
        order_status(&pool, atm_expired.order_id).await,
        ("cancelled".to_string(), Some("expired".to_string()))
    );
    assert_eq!(
        order_status(&pool, fresh.order_id).await.0,
        "pending_payment"
    );
    assert_eq!(
        order_status(&pool, atm_alive.order_id).await.0,
        "pending_payment"
    );
    assert_eq!(stock_of(&pool, variant).await, 8, "5 + 2 + 1");
    let payment_status: String =
        sqlx::query_scalar("SELECT status FROM payments WHERE order_id = $1")
            .bind(old.order_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(payment_status, "expired");
    let alive_payment: String =
        sqlx::query_scalar("SELECT status FROM payments WHERE order_id = $1")
            .bind(atm_alive.order_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(alive_payment, "pending");

    // 再跑一次沒有東西
    assert_eq!(scheduled::expire_unpaid_orders(&pool).await.unwrap(), 0);
    assert_eq!(stock_of(&pool, variant).await, 8);
}

#[sqlx::test(migrations = "./migrations")]
async fn expire_one_rolls_back_payments_update_when_order_not_cancellable(pool: PgPool) {
    let (variant, _) = common::active_product(&pool, "A", 100, 10).await;
    let order = orders::create_order(&pool, input(vec![(variant, 1)]), None)
        .await
        .unwrap();
    // 訂單已經不是 pending_payment（例如剛好在這一刻付款成功了）；payments 還是 pending
    sqlx::query("UPDATE orders SET status = 'paid' WHERE id = $1")
        .bind(order.order_id)
        .execute(&pool)
        .await
        .unwrap();

    let cancelled = scheduled::expire_one(&pool, order.order_id).await.unwrap();
    assert!(!cancelled, "cancel_in_tx 回 false 就不算過期成功");

    let payment_status: String =
        sqlx::query_scalar("SELECT status FROM payments WHERE order_id = $1")
            .bind(order.order_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        payment_status, "pending",
        "payments 的 UPDATE 要跟著 cancel_in_tx 的 false 一起 rollback，不能留下 expired"
    );
    assert_eq!(order_status(&pool, order.order_id).await.0, "paid");
}

#[sqlx::test(migrations = "./migrations")]
async fn auto_completes_shipped_after_14_days_unless_returned(pool: PgPool) {
    let (variant, _) = common::active_product(&pool, "A", 100, 10).await;
    let a = orders::create_order(&pool, input(vec![(variant, 1)]), None)
        .await
        .unwrap();
    let b = orders::create_order(&pool, input(vec![(variant, 1)]), None)
        .await
        .unwrap();
    let c = orders::create_order(&pool, input(vec![(variant, 1)]), None)
        .await
        .unwrap();
    for (order, days, shipment_status) in [
        (&a, 15, "created"),
        (&b, 15, "returned"),
        (&c, 1, "created"),
    ] {
        sqlx::query("UPDATE orders SET status = 'shipped', paid_at = now(), shipped_at = now() - make_interval(days => $2) WHERE id = $1")
            .bind(order.order_id)
            .bind(days)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE shipments SET status = $2 WHERE order_id = $1")
            .bind(order.order_id)
            .bind(shipment_status)
            .execute(&pool)
            .await
            .unwrap();
    }
    assert_eq!(scheduled::auto_complete_shipped(&pool).await.unwrap(), 1);
    assert_eq!(order_status(&pool, a.order_id).await.0, "completed");
    assert_eq!(
        order_status(&pool, b.order_id).await.0,
        "shipped",
        "退回的不自動完成"
    );
    assert_eq!(
        order_status(&pool, c.order_id).await.0,
        "shipped",
        "還沒 14 天"
    );
    let completed_at: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT completed_at FROM orders WHERE id = $1")
            .bind(a.order_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(completed_at.is_some());
}

#[sqlx::test(migrations = "./migrations")]
async fn purge_removes_expired_rows(pool: PgPool) {
    let hash = dog_shop_api::auth::password::hash_password("password123").unwrap();
    let user =
        dog_shop_api::domain::users::create(&pool, "p@test.local", &hash, "小美", "customer")
            .await
            .unwrap();
    // 兩個 session：一個過期
    let expired_sid = dog_shop_api::auth::session::create(&pool, user.id)
        .await
        .unwrap();
    dog_shop_api::auth::session::create(&pool, user.id)
        .await
        .unwrap();
    sqlx::query("UPDATE sessions SET expires_at = now() - interval '1 minute' WHERE id = $1")
        .bind(expired_sid)
        .execute(&pool)
        .await
        .unwrap();
    // 重設 token：一個過期、一個用過、一個有效
    dog_shop_api::domain::password_resets::create(&pool, user.id)
        .await
        .unwrap();
    sqlx::query("UPDATE password_resets SET expires_at = now() - interval '1 minute'")
        .execute(&pool)
        .await
        .unwrap();
    let used = dog_shop_api::domain::password_resets::create(&pool, user.id)
        .await
        .unwrap();
    dog_shop_api::domain::password_resets::consume(&pool, &used)
        .await
        .unwrap();
    dog_shop_api::domain::password_resets::create(&pool, user.id)
        .await
        .unwrap();
    // 門市選擇：過期
    common::cvs_store_token(&pool).await;
    sqlx::query("UPDATE cvs_store_selections SET expires_at = now() - interval '1 minute'")
        .execute(&pool)
        .await
        .unwrap();
    // jobs：一個 31 天前做完、一個剛做完、一個 queued
    let old_done = enqueue(&pool, "test", json!({})).await;
    sqlx::query(
        "UPDATE jobs SET status = 'done', updated_at = now() - interval '31 days' WHERE id = $1",
    )
    .bind(old_done)
    .execute(&pool)
    .await
    .unwrap();
    let recent_done = enqueue(&pool, "test", json!({})).await;
    sqlx::query("UPDATE jobs SET status = 'done' WHERE id = $1")
        .bind(recent_done)
        .execute(&pool)
        .await
        .unwrap();
    enqueue(&pool, "test", json!({})).await;

    let n = scheduled::purge_expired(&pool).await.unwrap();
    assert_eq!(n, 5, "1 session + 2 resets + 1 store + 1 job");
    let sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    let resets: i64 = sqlx::query_scalar("SELECT count(*) FROM password_resets")
        .fetch_one(&pool)
        .await
        .unwrap();
    let stores: i64 = sqlx::query_scalar("SELECT count(*) FROM cvs_store_selections")
        .fetch_one(&pool)
        .await
        .unwrap();
    let jobs_left: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!((sessions, resets, stores, jobs_left), (1, 1, 0, 2));
}
