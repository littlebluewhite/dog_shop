mod common;

use axum::{
    Router,
    body::{Body, Bytes},
    http::{HeaderValue, Request, StatusCode, header},
    response::IntoResponse,
    routing::get,
};
use rust_xlsxwriter::Workbook;
use serde_json::json;
use sqlx::PgPool;
use std::net::SocketAddr;

fn png(width: u32, height: u32) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(width, height, image::Rgb([10, 200, 90]));
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut out, image::ImageFormat::Png)
        .unwrap();
    out.into_inner()
}

/// 本機假圖床：/ok.png 與 /500
async fn image_server() -> String {
    async fn ok() -> impl IntoResponse {
        (
            [(header::CONTENT_TYPE, HeaderValue::from_static("image/png"))],
            Bytes::from(png(40, 30)),
        )
    }
    async fn fail() -> impl IntoResponse {
        (StatusCode::INTERNAL_SERVER_ERROR, "boom")
    }
    let app = Router::new()
        .route("/ok.png", get(ok))
        .route("/500", get(fail));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

fn xlsx(rows: &[Vec<String>]) -> Vec<u8> {
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    for (r, row) in rows.iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            ws.write_string(r as u32, c as u16, cell).unwrap();
        }
    }
    wb.save_to_buffer().unwrap()
}

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

/// multipart：file=<xlsx> 再加零個或一個文字欄位
fn multipart(cookie: &str, path: &str, data: &[u8], extra: Option<(&str, &str)>) -> Request<Body> {
    let boundary = "XxDogShopBoundaryxX";
    let mut body: Vec<u8> = Vec::new();
    if let Some((name, value)) = extra {
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
            )
            .as_bytes(),
        );
    }
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"import.xlsx\"\r\nContent-Type: application/vnd.openxmlformats-officedocument.spreadsheetml.sheet\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    Request::builder()
        .method("POST")
        .uri(path)
        .header("cookie", cookie)
        .header("x-requested-with", "fetch")
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap()
}

const HEADER: &[&str] = &[
    "商品編號",
    "商品名稱",
    "商品描述",
    "分類",
    "規格名稱1",
    "規格選項1",
    "規格名稱2",
    "規格選項2",
    "價格",
    "庫存",
    "SKU",
    "圖片網址",
];

#[sqlx::test(migrations = "./migrations")]
async fn preview_then_commit_creates_products_with_images(pool: PgPool) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let base = image_server().await;
    let (app, state) = common::app_with_state(pool.clone());
    let cookie = common::admin_cookie(&app, &pool).await;
    let file = xlsx(&[
        s(HEADER),
        s(&[
            "A1",
            "狗糧 5kg",
            "很好吃",
            "狗糧",
            "口味",
            "雞肉",
            "",
            "",
            "1200",
            "10",
            "DOG-C",
            &format!("{base}/ok.png, {base}/500"),
        ]),
        s(&[
            "A1", "", "", "", "", "牛肉", "", "", "1300", "", "DOG-B", "",
        ]),
        s(&["B2", "玩具球", "", "", "", "", "", "", "99", "0", "", ""]),
    ]);

    let (status, body, _) = common::send(
        &app,
        multipart(&cookie, "/api/admin/import/preview", &file, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["product_count"], json!(2));
    assert_eq!(body["variant_count"], json!(3));
    assert_eq!(body["new_count"], json!(2));
    assert_eq!(body["update_count"], json!(0));
    assert_eq!(body["parsed"]["errors"], json!([]));
    assert_eq!(body["parsed"]["products"][0]["external_ref"], json!("A1"));
    let fingerprint = body["fingerprint"].as_str().unwrap().to_string();
    assert_eq!(fingerprint.len(), 64);
    // 預覽不寫資料庫
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM products")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0);

    let (status, body, _) = common::send(
        &app,
        multipart(
            &cookie,
            "/api/admin/import/commit",
            &file,
            Some(("fingerprint", &fingerprint)),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["created"], json!(2));
    assert_eq!(body["result"]["updated"], json!(0));
    assert_eq!(body["result"]["products"][0]["created"], json!(true));
    assert_eq!(body["result"]["products"][0]["images"], json!(1));
    let warnings = body["result"]["warnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0]["external_ref"], json!("A1"));
    assert!(
        warnings[0]["message"]
            .as_str()
            .unwrap()
            .contains("HTTP 500"),
        "{warnings:?}"
    );

    let a = dog_shop_api::domain::products::find_by_external_ref(&pool, "A1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(a.product.status, "draft");
    assert_eq!(a.product.name, "狗糧 5kg");
    assert_eq!(a.product.description, "很好吃");
    assert!(a.product.category_id.is_some());
    assert_eq!(a.product.option1_name.as_deref(), Some("口味"));
    assert_eq!(a.variants.len(), 2);
    assert_eq!(a.variants[0].stock, 10);
    assert_eq!(a.variants[1].stock, 0, "庫存空白的新商品是 0");
    assert_eq!(a.images.len(), 1);
    assert!(
        state
            .config
            .upload_dir
            .join(a.images[0].path.trim_start_matches("/uploads/"))
            .exists()
    );
    let cat: (String,) = sqlx::query_as("SELECT name FROM categories WHERE id = $1")
        .bind(a.product.category_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(cat.0, "狗糧");
}

#[sqlx::test(migrations = "./migrations")]
async fn reimport_updates_in_place_and_keeps_what_the_sheet_leaves_blank(pool: PgPool) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let base = image_server().await;
    let (app, _state) = common::app_with_state(pool.clone());
    let cookie = common::admin_cookie(&app, &pool).await;
    let first = xlsx(&[
        s(HEADER),
        s(&[
            "A1",
            "狗糧",
            "描述一",
            "狗糧",
            "口味",
            "雞肉",
            "",
            "",
            "1200",
            "10",
            "",
            &format!("{base}/ok.png"),
        ]),
        s(&["A1", "", "", "", "", "牛肉", "", "", "1300", "5", "", ""]),
    ]);
    let (_, body, _) = common::send(
        &app,
        multipart(&cookie, "/api/admin/import/preview", &first, None),
    )
    .await;
    let fp = body["fingerprint"].as_str().unwrap().to_string();
    let (status, _, _) = common::send(
        &app,
        multipart(
            &cookie,
            "/api/admin/import/commit",
            &first,
            Some(("fingerprint", &fp)),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let before = dog_shop_api::domain::products::find_by_external_ref(&pool, "A1")
        .await
        .unwrap()
        .unwrap();
    // 老闆上架、改 slug、賣掉一些
    sqlx::query(
        "UPDATE products SET status = 'active', slug = 'dog-food', sort_order = 7 WHERE id = $1",
    )
    .bind(before.product.id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE product_variants SET stock = 3, compare_at_price = 1500 WHERE id = $1")
        .bind(before.variants[0].id)
        .execute(&pool)
        .await
        .unwrap();

    // 第二次：只改價格、雞肉庫存空白、牛肉庫存 8、描述與分類空白、圖片網址全部壞掉
    let second = xlsx(&[
        s(HEADER),
        s(&[
            "A1",
            "狗糧（新包裝）",
            "",
            "",
            "口味",
            "雞肉",
            "",
            "",
            "1250",
            "",
            "",
            &format!("{base}/500"),
        ]),
        s(&["A1", "", "", "", "", "牛肉", "", "", "1350", "8", "", ""]),
        s(&["A1", "", "", "", "", "魚肉", "", "", "1400", "2", "", ""]),
    ]);
    let (_, body, _) = common::send(
        &app,
        multipart(&cookie, "/api/admin/import/preview", &second, None),
    )
    .await;
    assert_eq!(body["new_count"], json!(0));
    assert_eq!(body["update_count"], json!(1));
    let fp = body["fingerprint"].as_str().unwrap().to_string();
    let (status, body, _) = common::send(
        &app,
        multipart(
            &cookie,
            "/api/admin/import/commit",
            &second,
            Some(("fingerprint", &fp)),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["updated"], json!(1));
    assert_eq!(body["result"]["warnings"].as_array().unwrap().len(), 1);

    let after = dog_shop_api::domain::products::find_by_external_ref(&pool, "A1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.product.id, before.product.id);
    assert_eq!(after.product.name, "狗糧（新包裝）");
    assert_eq!(after.product.status, "active", "狀態保留");
    assert_eq!(after.product.slug, "dog-food", "slug 保留");
    assert_eq!(after.product.sort_order, 7);
    assert_eq!(after.product.description, "描述一", "描述空白就保留");
    assert_eq!(
        after.product.category_id, before.product.category_id,
        "分類空白就保留"
    );
    assert_eq!(after.images.len(), 1, "圖片全部失敗 → 保留原圖");
    assert_eq!(after.images[0].path, before.images[0].path);
    let chicken = after
        .variants
        .iter()
        .find(|v| v.option1_value.as_deref() == Some("雞肉"))
        .unwrap();
    assert_eq!(chicken.id, before.variants[0].id, "規格對回既有 id");
    assert_eq!(chicken.price, 1250);
    assert_eq!(chicken.stock, 3, "庫存空白 → 保留現值");
    assert_eq!(chicken.compare_at_price, Some(1500), "原價保留");
    let beef = after
        .variants
        .iter()
        .find(|v| v.option1_value.as_deref() == Some("牛肉"))
        .unwrap();
    assert_eq!(beef.id, before.variants[1].id);
    assert_eq!(beef.stock, 8);
    assert!(
        after
            .variants
            .iter()
            .any(|v| v.option1_value.as_deref() == Some("魚肉")),
        "新規格新增"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn commit_is_refused_with_row_errors_or_a_stale_fingerprint(pool: PgPool) {
    let (app, _state) = common::app_with_state(pool.clone());
    let cookie = common::admin_cookie(&app, &pool).await;
    let bad = xlsx(&[
        s(HEADER),
        s(&["A1", "", "", "", "", "", "", "", "abc", "", "", ""]),
    ]);
    let (status, body, _) = common::send(
        &app,
        multipart(&cookie, "/api/admin/import/preview", &bad, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["parsed"]["errors"].as_array().unwrap().len(), 2);
    let fp = body["fingerprint"].as_str().unwrap().to_string();
    let (status, body, _) = common::send(
        &app,
        multipart(
            &cookie,
            "/api/admin/import/commit",
            &bad,
            Some(("fingerprint", &fp)),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body["error"]["details"]["fields"]["rows"],
        json!("檔案有 2 列錯誤，請先修正再匯入")
    );

    let good = xlsx(&[
        s(HEADER),
        s(&["A1", "ok", "", "", "", "", "", "", "10", "", "", ""]),
    ]);
    let (status, body, _) = common::send(
        &app,
        multipart(
            &cookie,
            "/api/admin/import/commit",
            &good,
            Some(("fingerprint", "0000")),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body["error"]["details"]["fields"]["fingerprint"],
        json!("檔案已變更，請重新預覽")
    );
    let (status, body, _) = common::send(
        &app,
        multipart(&cookie, "/api/admin/import/commit", &good, None),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM products")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn bad_files_and_missing_columns_are_validation_errors(pool: PgPool) {
    let (app, _state) = common::app_with_state(pool.clone());
    let cookie = common::admin_cookie(&app, &pool).await;
    let (status, body, _) = common::send(
        &app,
        multipart(&cookie, "/api/admin/import/preview", b"not an xlsx", None),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body["error"]["details"]["fields"]["file"],
        json!("檔案不是 xlsx 或已損壞")
    );
    let no_ref = xlsx(&[s(&["商品名稱", "價格", "庫存"]), s(&["x", "1", "1"])]);
    let (status, body, _) = common::send(
        &app,
        multipart(&cookie, "/api/admin/import/preview", &no_ref, None),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body["error"]["details"]["fields"]["file"],
        json!("缺少必要欄位：商品編號")
    );
    // 沒有 file 欄位
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/admin/import/preview",
            Some(&cookie),
            Some(json!({})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    // 超過 5 MB
    let huge = vec![0u8; dog_shop_api::import::MAX_XLSX_BYTES + 1];
    let (status, body, _) = common::send(
        &app,
        multipart(&cookie, "/api/admin/import/preview", &huge, None),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body["error"]["details"]["fields"]["file"],
        json!("檔案超過 5 MB")
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn import_routes_require_admin(pool: PgPool) {
    let app = common::app(pool.clone());
    let file = xlsx(&[
        s(HEADER),
        s(&["A1", "x", "", "", "", "", "", "", "1", "", "", ""]),
    ]);
    // 產生一次即可：customer_cookie 在 DB 裡建的帳號 email 是固定值，兩條路徑各建一次會撞
    // users_email_lower_idx 的 unique index 而 panic（簡報原本把它放在迴圈裡）。
    let customer = common::customer_cookie(&app, &pool).await;
    for path in ["/api/admin/import/preview", "/api/admin/import/commit"] {
        let (status, _, _) = common::send(&app, multipart("", path, &file, None)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{path}");
        let (status, _, _) = common::send(&app, multipart(&customer, path, &file, None)).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}");
    }
}
