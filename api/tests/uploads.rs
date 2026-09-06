mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use sqlx::PgPool;

fn png(width: u32, height: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(width, height, image::Rgba([10, 120, 200, 255]));
    let mut buffer = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut buffer, image::ImageFormat::Png)
        .unwrap();
    buffer.into_inner()
}

/// 手工組 multipart（沒有 reqwest）
fn multipart(cookie: &str, filename: &str, content_type: &str, data: &[u8]) -> Request<Body> {
    let boundary = "XxDogShopBoundaryxX";
    let mut body = Vec::new();
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: {content_type}\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    Request::builder()
        .method("POST")
        .uri("/api/admin/uploads")
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .header(header::ORIGIN, common::TEST_ORIGIN)
        .header("x-requested-with", "fetch")
        .header("x-forwarded-for", "127.0.0.1")
        .header(header::COOKIE, cookie)
        .body(Body::from(body))
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn upload_then_serve(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::admin_cookie(&app, &pool).await;

    let (status, body, _) = common::send(
        &app,
        multipart(&cookie, "a.png", "image/png", &png(2000, 1000)),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["width"], 1600);
    assert_eq!(body["height"], 800);
    let path = body["path"].as_str().unwrap();
    let thumb = body["thumb_path"].as_str().unwrap();
    assert!(
        path.starts_with("/uploads/") && path.ends_with(".jpg"),
        "{path}"
    );
    assert!(thumb.ends_with("_thumb.jpg"));

    // 靜態檔服務：兩張都拿得到，是 JPEG，帶長快取
    for p in [path, thumb] {
        let request = Request::builder()
            .method("GET")
            .uri(p)
            .body(Body::empty())
            .unwrap();
        let response = tower::ServiceExt::oneshot(app.clone(), request)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{p}");
        assert_eq!(response.headers()[header::CONTENT_TYPE], "image/jpeg");
        assert_eq!(
            response.headers()[header::CACHE_CONTROL],
            "public, max-age=31536000, immutable"
        );
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn rejects_wrong_type_and_garbage(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::admin_cookie(&app, &pool).await;

    let (status, body, _) =
        common::send(&app, multipart(&cookie, "a.txt", "text/plain", b"hello")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body["error"]["details"]["fields"]["file"],
        "只接受 JPEG、PNG、WebP、GIF"
    );

    let (status, body, _) = common::send(
        &app,
        multipart(&cookie, "a.png", "image/png", b"not really a png"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["details"]["fields"]["file"], "圖片無法讀取");
}

#[sqlx::test(migrations = "./migrations")]
async fn customer_cannot_upload(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::customer_cookie(&app, &pool).await;
    let (status, _, _) =
        common::send(&app, multipart(&cookie, "a.png", "image/png", &png(10, 10))).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}
