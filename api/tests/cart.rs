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
