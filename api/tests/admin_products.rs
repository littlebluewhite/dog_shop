mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;

/// 兩層規格（口味 × 尺寸）、兩張圖的商品
fn sample_product(status: &str) -> Value {
    json!({
        "name": "雞肉狗糧",
        "description": "第一行\n第二行",
        "status": status,
        "option1_name": "口味",
        "option2_name": "尺寸",
        "images": [
            { "path": "/uploads/2026/09/a.jpg", "thumb_path": "/uploads/2026/09/a_thumb.jpg", "alt": "正面" },
            { "path": "/uploads/2026/09/b.jpg", "thumb_path": "/uploads/2026/09/b_thumb.jpg" }
        ],
        "variants": [
            { "option1_value": "雞肉", "option2_value": "S", "price": 300, "stock": 5, "sku": "CK-S", "image_path": "/uploads/2026/09/a.jpg" },
            { "option1_value": "雞肉", "option2_value": "L", "price": 500, "compare_at_price": 600, "stock": 0 },
            { "option1_value": "牛肉", "option2_value": "S", "price": 320, "stock": 2 },
            { "option1_value": "牛肉", "option2_value": "L", "price": 520, "stock": 1, "is_active": false }
        ]
    })
}

#[sqlx::test(migrations = "./migrations")]
async fn create_get_update_archive(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::admin_cookie(&app, &pool).await;

    // 建立
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/admin/products",
            Some(&cookie),
            Some(sample_product("draft")),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["name"], "雞肉狗糧");
    assert_eq!(body["status"], "draft");
    assert_eq!(body["slug"].as_str().unwrap().len(), 8);
    assert_eq!(body["variants"].as_array().unwrap().len(), 4);
    assert_eq!(body["images"].as_array().unwrap().len(), 2);
    assert_eq!(body["images"][0]["alt"], "正面");
    assert_eq!(body["images"][1]["sort_order"], 1);
    // 第一個規格的 image_id 指到第一張圖
    assert_eq!(body["variants"][0]["image_id"], body["images"][0]["id"]);
    assert!(body["variants"][1]["image_id"].is_null());
    assert_eq!(body["variants"][3]["is_active"], false);
    let id = body["id"].as_str().unwrap().to_string();
    let keep_variant_id = body["variants"][0]["id"].as_str().unwrap().to_string();

    // 取單筆
    let (status, body, _) = common::send(
        &app,
        common::req(
            "GET",
            &format!("/api/admin/products/{id}"),
            Some(&cookie),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], id);

    // 更新：改名、換 slug、上架、第一個規格改價、丟掉其他三個、新增一個、圖片只留第二張
    let update = json!({
        "name": "雞肉狗糧（新包裝）",
        "slug": "chicken-food",
        "status": "active",
        "option1_name": "口味",
        "option2_name": "尺寸",
        "images": [ { "path": "/uploads/2026/09/b.jpg", "thumb_path": "/uploads/2026/09/b_thumb.jpg" } ],
        "variants": [
            { "id": keep_variant_id, "option1_value": "雞肉", "option2_value": "S", "price": 310, "stock": 9, "image_path": "/uploads/2026/09/b.jpg" },
            { "option1_value": "鮭魚", "option2_value": "S", "price": 350, "stock": 3 }
        ]
    });
    let (status, body, _) = common::send(
        &app,
        common::req(
            "PUT",
            &format!("/api/admin/products/{id}"),
            Some(&cookie),
            Some(update),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["slug"], "chicken-food");
    assert_eq!(body["status"], "active");
    let variants = body["variants"].as_array().unwrap();
    assert_eq!(variants.len(), 2);
    assert_eq!(
        variants[0]["id"], keep_variant_id,
        "有 id 的規格要保留同一個 id"
    );
    assert_eq!(variants[0]["price"], 310);
    assert_eq!(variants[0]["image_id"], body["images"][0]["id"]);
    assert_eq!(variants[1]["option1_value"], "鮭魚");
    assert_eq!(body["images"].as_array().unwrap().len(), 1);

    // 資料庫裡真的只剩 2 個規格
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM product_variants")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 2);

    // 封存；再查 status 是 archived；封存不存在的 id 是 404
    let (status, _, _) = common::send(
        &app,
        common::req(
            "DELETE",
            &format!("/api/admin/products/{id}"),
            Some(&cookie),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, body, _) = common::send(
        &app,
        common::req(
            "GET",
            &format!("/api/admin/products/{id}"),
            Some(&cookie),
            None,
        ),
    )
    .await;
    assert_eq!(body["status"], "archived");
    let (status, _, _) = common::send(
        &app,
        common::req(
            "DELETE",
            "/api/admin/products/018f0000-0000-7000-8000-000000000000",
            Some(&cookie),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn validation_and_conflicts(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::admin_cookie(&app, &pool).await;

    // 沒有規格、名稱空白
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/admin/products",
            Some(&cookie),
            Some(json!({ "name": " ", "status": "draft", "variants": [] })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "VALIDATION");
    assert!(body["error"]["details"]["fields"]["name"].is_string());
    assert!(body["error"]["details"]["fields"]["variants"].is_string());

    // slug 重複
    let mut first = sample_product("draft");
    first["slug"] = json!("same-slug");
    let (status, _, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/admin/products",
            Some(&cookie),
            Some(first.clone()),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, body, _) = common::send(
        &app,
        common::req("POST", "/api/admin/products", Some(&cookie), Some(first)),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"]["details"]["fields"]["slug"].is_string());

    // 分類不存在
    let mut bad_cat = sample_product("draft");
    bad_cat["category_id"] = json!("018f0000-0000-7000-8000-000000000000");
    let (status, body, _) = common::send(
        &app,
        common::req("POST", "/api/admin/products", Some(&cookie), Some(bad_cat)),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        body["error"]["details"]["fields"]["category_id"],
        "分類不存在"
    );

    // 商品建好後分類不會有殘留：資料庫只有 1 個商品
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM products")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn list_filters_and_paging(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::admin_cookie(&app, &pool).await;

    // 分類
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

    for (name, status) in [
        ("A 草稿", "draft"),
        ("B 上架", "active"),
        ("C 上架", "active"),
        ("D 封存", "archived"),
    ] {
        let mut p = sample_product(status);
        p["name"] = json!(name);
        p["category_id"] = cat["id"].clone();
        let (s, b, _) = common::send(
            &app,
            common::req("POST", "/api/admin/products", Some(&cookie), Some(p)),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED, "{b}");
    }

    // 預設不含 archived
    let (status, body, _) = common::send(
        &app,
        common::req("GET", "/api/admin/products", Some(&cookie), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 3);
    assert_eq!(body["items"].as_array().unwrap().len(), 3);
    let item = &body["items"][0];
    assert_eq!(item["category_name"], "飼料");
    assert_eq!(item["price_min"], 300);
    assert_eq!(item["price_max"], 520);
    assert_eq!(item["stock_total"], 8);
    assert_eq!(item["image_thumb"], "/uploads/2026/09/a_thumb.jpg");

    // status=archived 只有 1
    let (_, body, _) = common::send(
        &app,
        common::req(
            "GET",
            "/api/admin/products?status=archived",
            Some(&cookie),
            None,
        ),
    )
    .await;
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["name"], "D 封存");

    // q 不分大小寫
    let (_, body, _) = common::send(
        &app,
        common::req(
            "GET",
            "/api/admin/products?q=%E4%B8%8A%E6%9E%B6",
            Some(&cookie),
            None,
        ),
    )
    .await;
    assert_eq!(body["total"], 2);

    // 分頁
    let (_, body, _) = common::send(
        &app,
        common::req(
            "GET",
            "/api/admin/products?per_page=2&page=2",
            Some(&cookie),
            None,
        ),
    )
    .await;
    assert_eq!(body["total"], 3);
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
    assert_eq!(body["page"], 2);
    assert_eq!(body["per_page"], 2);

    // 錯的 status
    let (status, _, _) = common::send(
        &app,
        common::req(
            "GET",
            "/api/admin/products?status=weird",
            Some(&cookie),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn customer_is_forbidden(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::customer_cookie(&app, &pool).await;
    let (status, _, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/admin/products",
            Some(&cookie),
            Some(sample_product("draft")),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _, _) = common::send(
        &app,
        common::req("GET", "/api/admin/products", Some(&cookie), None),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

/// 規格 §10：已有訂單的規格只能停用不能刪；沒訂單的照樣刪
#[sqlx::test(migrations = "./migrations")]
async fn variant_with_orders_is_deactivated_not_deleted(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::admin_cookie(&app, &pool).await;
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/admin/products",
            Some(&cookie),
            Some(sample_product("active")),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let id = body["id"].as_str().unwrap().to_string();
    let ordered_variant = body["variants"][0]["id"].as_str().unwrap().to_string();
    let free_variant = body["variants"][1]["id"].as_str().unwrap().to_string();
    let kept_variant = body["variants"][2].clone();

    // 直接塞一筆訂單引用第一個規格（訂單 API 在 Task 8 才有）
    let order_id = uuid::Uuid::now_v7();
    sqlx::query(
        "INSERT INTO orders (id, order_no, guest_token, email, recipient_name, recipient_phone, shipping_method,
                             subtotal, shipping_fee, total, invoice_type)
         VALUES ($1, 'DS260906TEST', 'tok', 'a@b.co', '王小明', '0912345678', 'home', 300, 100, 400, 'personal')",
    )
    .bind(order_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO order_items (id, order_id, variant_id, product_name, variant_label, unit_price, quantity, line_total)
         VALUES ($1, $2, $3, '雞肉狗糧', '雞肉 / S', 300, 1, 300)",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(order_id)
    .bind(uuid::Uuid::parse_str(&ordered_variant).unwrap())
    .execute(&pool)
    .await
    .unwrap();

    // PUT 只留第三個規格：第一個（有訂單）變 is_active=false 留著，第二個（沒訂單）真的被刪
    let update = json!({
        "name": "雞肉狗糧",
        "status": "active",
        "option1_name": "口味",
        "option2_name": "尺寸",
        "images": [],
        "variants": [ {
            "id": kept_variant["id"], "option1_value": "牛肉", "option2_value": "S", "price": 320, "stock": 2
        } ]
    });
    let (status, body, _) = common::send(
        &app,
        common::req(
            "PUT",
            &format!("/api/admin/products/{id}"),
            Some(&cookie),
            Some(update),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let variants = body["variants"].as_array().unwrap();
    assert_eq!(variants.len(), 2, "{body}");
    let ordered = variants
        .iter()
        .find(|v| v["id"] == ordered_variant)
        .expect("有訂單的規格還在");
    assert_eq!(ordered["is_active"], false);
    assert!(variants.iter().all(|v| v["id"] != free_variant));
    assert!(variants.iter().any(|v| v["id"] == kept_variant["id"]));
}
