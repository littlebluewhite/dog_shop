mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;

/// 用後台 API 建商品，回 slug
async fn seed(app: &axum::Router, cookie: &str, product: Value) -> String {
    let (status, body, _) = common::send(
        app,
        common::req("POST", "/api/admin/products", Some(cookie), Some(product)),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body["slug"].as_str().unwrap().to_string()
}

fn one_variant(
    name: &str,
    slug: &str,
    status: &str,
    price: i32,
    stock: i32,
    category_id: Option<&Value>,
) -> Value {
    json!({
        "name": name, "slug": slug, "status": status, "category_id": category_id,
        "images": [ { "path": format!("/uploads/2026/09/{slug}.jpg"), "thumb_path": format!("/uploads/2026/09/{slug}_thumb.jpg") } ],
        "variants": [ { "price": price, "stock": stock } ]
    })
}

async fn setup(pool: &PgPool) -> (axum::Router, String) {
    let app = common::app(pool.clone());
    let cookie = common::admin_cookie(&app, pool).await;
    let (_, cat, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/admin/categories",
            Some(&cookie),
            Some(json!({ "name": "飼料", "slug": "food" })),
        ),
    )
    .await;

    // 兩層規格、價格 100～300、庫存全 0、有一個停用的規格（價 999，不該算進範圍）
    seed(&app, &cookie, json!({
        "name": "綜合狗糧", "slug": "mix", "status": "active", "category_id": cat["id"],
        "option1_name": "口味",
        "images": [ { "path": "/uploads/2026/09/mix.jpg", "thumb_path": "/uploads/2026/09/mix_thumb.jpg", "alt": "mix" } ],
        "variants": [
            { "option1_value": "雞", "price": 100, "stock": 0, "image_path": "/uploads/2026/09/mix.jpg" },
            { "option1_value": "牛", "price": 300, "compare_at_price": 350, "stock": 0 },
            { "option1_value": "停用", "price": 999, "stock": 9, "is_active": false }
        ]
    })).await;
    seed(
        &app,
        &cookie,
        one_variant("潔牙骨", "bone", "active", 200, 5, None),
    )
    .await;
    seed(
        &app,
        &cookie,
        one_variant("草稿商品", "draft-item", "draft", 50, 5, Some(&cat["id"])),
    )
    .await;
    seed(
        &app,
        &cookie,
        one_variant("封存商品", "archived-item", "archived", 50, 5, None),
    )
    .await;
    (app, cookie)
}

#[sqlx::test(migrations = "./migrations")]
async fn list_only_active_with_ranges_and_sorting(pool: PgPool) {
    let (app, _) = setup(&pool).await;

    // 預設 newest：bone（後建）在前、mix 在後；草稿與封存不出現
    let (status, body, _) =
        common::send(&app, common::req("GET", "/api/products", None, None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 2);
    assert_eq!(body["per_page"], 24);
    let items = body["items"].as_array().unwrap();
    assert_eq!(items[0]["slug"], "bone");
    assert_eq!(items[1]["slug"], "mix");
    let mix = &items[1];
    assert_eq!(mix["price_min"], 100);
    assert_eq!(mix["price_max"], 300, "停用規格的 999 不算");
    assert_eq!(mix["in_stock"], false);
    assert_eq!(mix["image_thumb"], "/uploads/2026/09/mix_thumb.jpg");
    assert_eq!(items[0]["in_stock"], true);

    // price_asc：mix(100) 在前；price_desc：mix(300) 在前
    let (_, body, _) = common::send(
        &app,
        common::req("GET", "/api/products?sort=price_asc", None, None),
    )
    .await;
    assert_eq!(body["items"][0]["slug"], "mix");
    let (_, body, _) = common::send(
        &app,
        common::req("GET", "/api/products?sort=price_desc", None, None),
    )
    .await;
    assert_eq!(body["items"][0]["slug"], "mix");

    // 錯的 sort
    let (status, _, _) = common::send(
        &app,
        common::req("GET", "/api/products?sort=random", None, None),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn list_filters_and_paging(pool: PgPool) {
    let (app, _) = setup(&pool).await;

    // 分類 food 只有 mix（草稿不算）
    let (_, body, _) = common::send(
        &app,
        common::req("GET", "/api/products?category=food", None, None),
    )
    .await;
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["slug"], "mix");

    // 不存在的分類 → 空
    let (_, body, _) = common::send(
        &app,
        common::req("GET", "/api/products?category=nope", None, None),
    )
    .await;
    assert_eq!(body["total"], 0);

    // 關鍵字（潔牙）
    let (_, body, _) = common::send(
        &app,
        common::req("GET", "/api/products?q=%E6%BD%94%E7%89%99", None, None),
    )
    .await;
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["slug"], "bone");

    // per_page 上限 60；分頁
    let (_, body, _) = common::send(
        &app,
        common::req("GET", "/api/products?per_page=500", None, None),
    )
    .await;
    assert_eq!(body["per_page"], 60);
    let (_, body, _) = common::send(
        &app,
        common::req("GET", "/api/products?per_page=1&page=2", None, None),
    )
    .await;
    assert_eq!(body["total"], 2);
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
    assert_eq!(body["items"][0]["slug"], "mix");
}

#[sqlx::test(migrations = "./migrations")]
async fn detail_returns_active_variants_only(pool: PgPool) {
    let (app, _) = setup(&pool).await;

    let (status, body, _) =
        common::send(&app, common::req("GET", "/api/products/mix", None, None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "綜合狗糧");
    assert_eq!(body["category"]["slug"], "food");
    assert_eq!(body["option1_name"], "口味");
    assert!(body["option2_name"].is_null());
    assert_eq!(body["images"][0]["alt"], "mix");
    let variants = body["variants"].as_array().unwrap();
    assert_eq!(variants.len(), 2, "停用的規格不回");
    assert_eq!(variants[0]["option1_value"], "雞");
    assert_eq!(variants[0]["image_path"], "/uploads/2026/09/mix.jpg");
    assert!(variants[1]["image_path"].is_null());
    assert_eq!(variants[1]["compare_at_price"], 350);
    assert!(body.get("external_ref").is_none(), "公開頁不吐內部欄位");

    // 草稿、封存、不存在 → 404
    for slug in ["draft-item", "archived-item", "nothing-here"] {
        let (status, body, _) = common::send(
            &app,
            common::req("GET", &format!("/api/products/{slug}"), None, None),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{slug}");
        assert_eq!(body["error"]["code"], "NOT_FOUND");
    }
}
