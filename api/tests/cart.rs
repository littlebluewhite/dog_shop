mod common;

use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrations = "./migrations")]
async fn validate_reports_price_stock_and_availability(pool: PgPool) {
    let app = common::app(pool.clone());
    let (a, slug_a) = common::active_product(&pool, "A", 300, 5).await;
    let (b, _) = common::active_product(&pool, "B", 200, 0).await;
    let (c, _) = common::active_product(&pool, "C", 100, 9).await;
    let product_c: uuid::Uuid =
        sqlx::query_scalar("SELECT product_id FROM product_variants WHERE id = $1")
            .bind(c)
            .fetch_one(&pool)
            .await
            .unwrap();
    dog_shop_api::domain::products::archive(&pool, product_c)
        .await
        .unwrap();
    let ghost = uuid::Uuid::now_v7();

    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/cart/validate",
            None,
            Some(json!({ "items": [
                { "variant_id": a, "qty": 7 },
                { "variant_id": b, "qty": 1 },
                { "variant_id": c, "qty": 1 },
                { "variant_id": ghost, "qty": 2 }
            ] })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 4);
    assert_eq!(items[0]["product_slug"], slug_a);
    assert_eq!(items[0]["variant_label"], "預設");
    assert_eq!(items[0]["qty"], 5);
    assert_eq!(items[0]["stock"], 5);
    assert_eq!(items[0]["available"], true);
    assert_eq!(items[0]["reason"], "qty_reduced");
    assert_eq!(items[1]["available"], false);
    assert_eq!(items[1]["reason"], "sold_out");
    assert_eq!(items[2]["reason"], "unavailable");
    assert_eq!(items[3]["reason"], "unavailable");
    assert_eq!(items[3]["product_name"], "已下架的商品");
    assert_eq!(body["subtotal"], 1500);
    assert_eq!(body["shipping"]["cvs_fee"], 60);
    assert_eq!(body["cvs_limit_exceeded"], false);

    // 空購物車也是 200
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/cart/validate",
            None,
            Some(json!({ "items": [] })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"].as_array().unwrap().len(), 0);
    assert_eq!(body["subtotal"], 0);

    // 小計超過 20,000 → cvs_limit_exceeded
    let (pricey, _) = common::active_product(&pool, "貴", 25_000, 1).await;
    let (_, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/cart/validate",
            None,
            Some(json!({ "items": [ { "variant_id": pricey, "qty": 1 } ] })),
        ),
    )
    .await;
    assert_eq!(body["cvs_limit_exceeded"], true);
}

#[sqlx::test(migrations = "./migrations")]
async fn cvs_store_lookup(pool: PgPool) {
    let app = common::app(pool.clone());
    let token = common::cvs_store_token(&pool).await;
    let (status, body, _) = common::send(
        &app,
        common::req(
            "GET",
            &format!("/api/checkout/cvs-store/{token}"),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["store_name"], "測試門市");
    assert_eq!(body["sub_type"], "UNIMARTC2C");
    let (status, _, _) = common::send(
        &app,
        common::req("GET", "/api/checkout/cvs-store/nope", None, None),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    sqlx::query("UPDATE cvs_store_selections SET expires_at = now() - interval '1 minute'")
        .execute(&pool)
        .await
        .unwrap();
    let (status, _, _) = common::send(
        &app,
        common::req(
            "GET",
            &format!("/api/checkout/cvs-store/{token}"),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// 品項數上限要在合併（O(n²)）之前擋掉，免得沒登入的呼叫端用超大 body 卡住 worker
#[sqlx::test(migrations = "./migrations")]
async fn validate_rejects_too_many_lines(pool: PgPool) {
    let app = common::app(pool.clone());
    let items: Vec<_> = (0..51)
        .map(|_| json!({ "variant_id": uuid::Uuid::now_v7(), "qty": 1 }))
        .collect();

    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/cart/validate",
            None,
            Some(json!({ "items": items })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "VALIDATION");
    assert_eq!(
        body["error"]["details"]["fields"]["items"],
        "一次最多 50 種商品"
    );
}

/// 未登入的 /cart/validate 不會走 validate_input 的 1..=99 檢查，merge_items 要在相加前就夾住
/// 數量，不然兩列同規格的巨大 qty 相加會整數溢位（見 fix wave item 1）
#[sqlx::test(migrations = "./migrations")]
async fn validate_clamps_huge_quantities(pool: PgPool) {
    let app = common::app(pool.clone());
    let (variant, _) = common::active_product(&pool, "夾限測試", 300, 5).await;

    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/cart/validate",
            None,
            Some(json!({ "items": [
                { "variant_id": variant, "qty": 2147483647 },
                { "variant_id": variant, "qty": 2147483647 }
            ] })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["qty"], 5);
    assert_eq!(items[0]["reason"], "qty_reduced");
}
