mod common;

use dog_shop_api::domain::orders::{
    self, HomeAddress, InvoiceInput, OrderInput, OrderItemInput, Viewer,
};
use dog_shop_api::domain::settings;
use dog_shop_api::error::ApiError;
use sqlx::PgPool;
use uuid::Uuid;

fn input(items: Vec<(Uuid, i32)>, shipping_method: &str, token: Option<String>) -> OrderInput {
    OrderInput {
        items: items
            .into_iter()
            .map(|(variant_id, qty)| OrderItemInput { variant_id, qty })
            .collect(),
        email: "Buyer@Test.local".to_string(),
        recipient_name: "王小明".to_string(),
        recipient_phone: "0912345678".to_string(),
        shipping_method: shipping_method.to_string(),
        cvs_store_token: token,
        address: (shipping_method == "home").then(|| HomeAddress {
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
        payment_method: "credit".to_string(),
        note: " 請小心輕放 ".to_string(),
    }
}

async fn stock_of(pool: &PgPool, variant_id: Uuid) -> i32 {
    sqlx::query_scalar("SELECT stock FROM product_variants WHERE id = $1")
        .bind(variant_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn home_order_deducts_stock_and_writes_everything(pool: PgPool) {
    let (variant, _) = common::active_product(&pool, "雞肉狗糧", 300, 5).await;
    let created = orders::create_order(&pool, input(vec![(variant, 2)], "home", None), None)
        .await
        .unwrap();
    assert!(
        created.order_no.len() == 12 && created.order_no.starts_with("DS"),
        "{}",
        created.order_no
    );
    assert_eq!(created.guest_token.len(), 64);
    assert_eq!(stock_of(&pool, variant).await, 3);

    let detail = orders::get_for_viewer(
        &pool,
        created.order_id,
        &Viewer::Guest(created.guest_token.clone()),
    )
    .await
    .unwrap()
    .expect("guest token 看得到");
    assert_eq!(detail.order.status, "pending_payment");
    assert_eq!(detail.order.email, "buyer@test.local");
    assert_eq!(detail.order.note, "請小心輕放");
    assert_eq!(
        (
            detail.order.subtotal,
            detail.order.shipping_fee,
            detail.order.total
        ),
        (600, 100, 700)
    );
    assert_eq!(detail.items.len(), 1);
    assert_eq!(detail.items[0].variant_label, "預設");
    assert_eq!(detail.items[0].line_total, 600);
    let shipment = detail.shipment.unwrap();
    assert_eq!(shipment.method, "home");
    assert_eq!(shipment.home_city.as_deref(), Some("臺北市"));
    assert_eq!(shipment.status, "pending");
    let payment = detail.payment.unwrap();
    assert_eq!(
        (
            payment.method.as_str(),
            payment.status.as_str(),
            payment.amount
        ),
        ("credit", "pending", 700)
    );
    let mtn: String = sqlx::query_scalar("SELECT merchant_trade_no FROM payments")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(mtn, format!("{}01", created.order_no));
    let (kind, dedupe): (String, Option<String>) =
        sqlx::query_as("SELECT kind, dedupe_key FROM jobs")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(kind, "send_email");
    assert_eq!(
        dedupe.unwrap(),
        format!("email:order_created:{}", created.order_id)
    );

    // 錯的 token、沒登入的會員視角都看不到
    assert!(
        orders::get_for_viewer(&pool, created.order_id, &Viewer::Guest("nope".to_string()))
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        orders::get_for_viewer(&pool, created.order_id, &Viewer::User(Uuid::now_v7()))
            .await
            .unwrap()
            .is_none()
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn free_shipping_threshold_applies(pool: PgPool) {
    let mut all = settings::get_all(&pool).await.unwrap();
    all.shipping.free_threshold = 500;
    settings::put_all(&pool, &all).await.unwrap();
    let (variant, _) = common::active_product(&pool, "牛肉狗糧", 250, 5).await;
    let created = orders::create_order(&pool, input(vec![(variant, 2)], "home", None), None)
        .await
        .unwrap();
    let detail =
        orders::get_for_viewer(&pool, created.order_id, &Viewer::Guest(created.guest_token))
            .await
            .unwrap()
            .unwrap();
    assert_eq!(
        (
            detail.order.subtotal,
            detail.order.shipping_fee,
            detail.order.total
        ),
        (500, 0, 500)
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn out_of_stock_lists_shortages_and_rolls_back(pool: PgPool) {
    let (a, _) = common::active_product(&pool, "A", 100, 5).await;
    let (b, _) = common::active_product(&pool, "B", 100, 1).await;
    let err = orders::create_order(&pool, input(vec![(a, 2), (b, 3)], "home", None), None)
        .await
        .unwrap_err();
    let ApiError::OutOfStock(items) = err else {
        panic!("expected OutOfStock, got {err:?}");
    };
    assert_eq!(items.len(), 1);
    assert_eq!((items[0].variant_id, items[0].available), (b, 1));
    // A 的扣庫存被 rollback，沒有訂單
    assert_eq!(stock_of(&pool, a).await, 5);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM orders")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);

    // 未知規格 → available 0
    let ghost = Uuid::now_v7();
    let err = orders::create_order(&pool, input(vec![(ghost, 1)], "home", None), None)
        .await
        .unwrap_err();
    let ApiError::OutOfStock(items) = err else {
        panic!("expected OutOfStock");
    };
    assert_eq!((items[0].variant_id, items[0].available), (ghost, 0));
}

/// 規格 §15：並發下單不超賣。6 個人同時搶 3 件，正好 3 個成功。
#[sqlx::test(migrations = "./migrations")]
async fn concurrent_orders_do_not_oversell(pool: PgPool) {
    let (variant, _) = common::active_product(&pool, "限量", 100, 3).await;
    let mut set = tokio::task::JoinSet::new();
    for _ in 0..6 {
        let pool = pool.clone();
        set.spawn(async move {
            orders::create_order(&pool, input(vec![(variant, 1)], "home", None), None).await
        });
    }
    let mut ok = 0;
    let mut short = 0;
    while let Some(result) = set.join_next().await {
        match result.unwrap() {
            Ok(_) => ok += 1,
            Err(ApiError::OutOfStock(_)) => short += 1,
            Err(other) => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!((ok, short), (3, 3));
    assert_eq!(stock_of(&pool, variant).await, 0);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM orders")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 3);
}

#[sqlx::test(migrations = "./migrations")]
async fn cvs_order_needs_store_and_respects_limit(pool: PgPool) {
    let (variant, _) = common::active_product(&pool, "C", 60, 10).await;
    let err = orders::create_order(&pool, input(vec![(variant, 1)], "cvs", None), None)
        .await
        .unwrap_err();
    assert!(matches!(err, ApiError::CvsStoreRequired), "{err:?}");
    let err = orders::create_order(
        &pool,
        input(
            vec![(variant, 1)],
            "cvs",
            Some("expired-or-fake".to_string()),
        ),
        None,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, ApiError::CvsStoreRequired));
    assert_eq!(stock_of(&pool, variant).await, 10, "沒門市不該扣庫存");

    let token = common::cvs_store_token(&pool).await;
    // payload 也帶了宅配地址：超商訂單即使收到 address 也不該寫進 shipments 的 home_* 欄位（fix round 1）
    let created = orders::create_order(
        &pool,
        OrderInput {
            address: Some(HomeAddress {
                postal_code: "100".to_string(),
                city: "臺北市".to_string(),
                district: "中正區".to_string(),
                street: "重慶南路一段 122 號".to_string(),
            }),
            ..input(vec![(variant, 1)], "cvs", Some(token))
        },
        None,
    )
    .await
    .unwrap();
    let detail =
        orders::get_for_viewer(&pool, created.order_id, &Viewer::Guest(created.guest_token))
            .await
            .unwrap()
            .unwrap();
    assert_eq!(detail.order.shipping_fee, 60);
    let shipment = detail.shipment.unwrap();
    assert_eq!(shipment.cvs_sub_type.as_deref(), Some("UNIMARTC2C"));
    assert_eq!(shipment.cvs_store_name.as_deref(), Some("測試門市"));
    assert!(shipment.home_city.is_none());
    let home_city: Option<String> =
        sqlx::query_scalar("SELECT home_city FROM shipments WHERE order_id = $1")
            .bind(created.order_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(home_city.is_none(), "超商訂單不該寫入宅配地址");

    // 小計超過 20,000 → CVS_AMOUNT_LIMIT，庫存 rollback
    let (pricey, _) = common::active_product(&pool, "貴", 25_000, 2).await;
    let token = common::cvs_store_token(&pool).await;
    let err = orders::create_order(&pool, input(vec![(pricey, 1)], "cvs", Some(token)), None)
        .await
        .unwrap_err();
    assert!(matches!(err, ApiError::CvsAmountLimit), "{err:?}");
    assert_eq!(stock_of(&pool, pricey).await, 2);
}

#[sqlx::test(migrations = "./migrations")]
async fn cancel_returns_stock_once(pool: PgPool) {
    let (variant, _) = common::active_product(&pool, "D", 100, 5).await;
    let created = orders::create_order(&pool, input(vec![(variant, 4)], "home", None), None)
        .await
        .unwrap();
    assert_eq!(stock_of(&pool, variant).await, 1);
    let viewer = Viewer::Guest(created.guest_token.clone());

    let err = orders::cancel(
        &pool,
        created.order_id,
        &Viewer::Guest("wrong".to_string()),
        "buyer",
    )
    .await
    .unwrap_err();
    assert!(matches!(err, ApiError::NotFound));

    orders::cancel(&pool, created.order_id, &viewer, "buyer")
        .await
        .unwrap();
    assert_eq!(stock_of(&pool, variant).await, 5);
    let detail = orders::get_for_viewer(&pool, created.order_id, &viewer)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(detail.order.status, "cancelled");
    assert_eq!(detail.order.cancel_reason.as_deref(), Some("buyer"));
    assert!(detail.order.cancelled_at.is_some());

    let err = orders::cancel(&pool, created.order_id, &viewer, "buyer")
        .await
        .unwrap_err();
    assert!(matches!(err, ApiError::Validation { .. }), "{err:?}");
    assert_eq!(stock_of(&pool, variant).await, 5, "不能加兩次");
}

#[sqlx::test(migrations = "./migrations")]
async fn member_orders_are_listed_newest_first(pool: PgPool) {
    let app = common::app(pool.clone());
    common::register_cookie(&app, "m@test.local", "password123", "甲").await;
    let user = dog_shop_api::domain::users::find_by_email(&pool, "m@test.local")
        .await
        .unwrap()
        .unwrap();
    let (variant, _) = common::active_product(&pool, "E", 100, 10).await;
    let first = orders::create_order(&pool, input(vec![(variant, 1)], "home", None), Some(&user))
        .await
        .unwrap();
    let second = orders::create_order(&pool, input(vec![(variant, 2)], "home", None), Some(&user))
        .await
        .unwrap();
    let page = orders::list_for_user(&pool, user.id, 1, 20).await.unwrap();
    assert_eq!(page.total, 2);
    assert_eq!(page.items[0].order_no, second.order_no);
    assert_eq!(page.items[0].item_count, 2);
    assert_eq!(page.items[1].order_no, first.order_no);
    // 會員看自己的不用 token
    assert!(
        orders::get_for_viewer(&pool, first.order_id, &Viewer::User(user.id))
            .await
            .unwrap()
            .is_some()
    );
}
