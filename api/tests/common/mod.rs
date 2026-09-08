#![allow(dead_code)]

use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Request, StatusCode, header},
};
use dog_shop_api::{
    app,
    config::Config,
    mail::{Email, Mailer},
    state::AppState,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

pub const TEST_ORIGIN: &str = "http://localhost:5173";

/// 每個測試一個獨立的上傳目錄；設定用 Config::for_tests；Email 用 Mailer::Capture（用 sent_emails 讀）
pub fn state(pool: PgPool) -> AppState {
    let upload_dir = std::env::temp_dir().join(format!("dog_shop_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&upload_dir).unwrap();
    let (mailer, _) = Mailer::capture();
    AppState {
        db: pool,
        config: Arc::new(Config::for_tests(upload_dir)),
        mailer: Arc::new(mailer),
    }
}

pub fn app(pool: PgPool) -> Router {
    app::router(state(pool))
}

/// 同時要打 API 又要看 job／信件的測試用這個
pub fn app_with_state(pool: PgPool) -> (Router, AppState) {
    let state = state(pool);
    (app::router(state.clone()), state)
}

/// 測試裡寄出的信（Mailer::Capture）
pub fn sent_emails(state: &AppState) -> Vec<Email> {
    match &*state.mailer {
        Mailer::Capture(sink) => sink.lock().unwrap().clone(),
        _ => Vec::new(),
    }
}

/// 建一個像瀏覽器 fetch 送出的請求：帶 Origin、X-Requested-With、X-Forwarded-For（速率限制用）
pub fn req(method: &str, uri: &str, cookie: Option<&str>, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::ACCEPT, "application/json")
        .header(header::ORIGIN, TEST_ORIGIN)
        .header("x-requested-with", "fetch")
        .header("x-forwarded-for", "127.0.0.1");
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    match body {
        Some(json) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    }
}

/// 送出請求，回 (狀態碼, JSON body（空 body 是 Null；不是 JSON 就包成字串）, 回應 headers)
pub async fn send(app: &Router, request: Request<Body>) -> (StatusCode, Value, HeaderMap) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()))
    };
    (status, json, headers)
}

pub const ADMIN_EMAIL: &str = "admin@test.local";
pub const ADMIN_PASSWORD: &str = "password123";

pub async fn create_admin(pool: &PgPool) {
    dog_shop_api::cli::create_admin_with_password(pool, ADMIN_EMAIL, ADMIN_PASSWORD)
        .await
        .unwrap();
}

/// 登入並回傳 "sid=<uuid>"，之後直接放進 Cookie header
pub async fn login(app: &Router, email: &str, password: &str) -> String {
    let (status, body, headers) = send(
        app,
        req(
            "POST",
            "/api/auth/login",
            None,
            Some(json!({ "email": email, "password": password })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "login failed: {body}");
    let set_cookie = headers
        .get(header::SET_COOKIE)
        .expect("set-cookie")
        .to_str()
        .unwrap();
    set_cookie.split(';').next().unwrap().to_string()
}

/// 建 admin 並登入，回 cookie
pub async fn admin_cookie(app: &Router, pool: &PgPool) -> String {
    create_admin(pool).await;
    login(app, ADMIN_EMAIL, ADMIN_PASSWORD).await
}

/// 建一個一般會員並登入（還沒有註冊 API，直接寫 DB）
pub async fn customer_cookie(app: &Router, pool: &PgPool) -> String {
    let hash = dog_shop_api::auth::password::hash_password("password123").unwrap();
    dog_shop_api::domain::users::create(pool, "user@test.local", &hash, "小明", "customer")
        .await
        .unwrap();
    login(app, "user@test.local", "password123").await
}

/// 用註冊 API 建一個會員並回 cookie（Task 3 起可用）
pub async fn register_cookie(app: &Router, email: &str, password: &str, name: &str) -> String {
    let (status, body, headers) = send(
        app,
        req(
            "POST",
            "/api/auth/register",
            None,
            Some(json!({ "email": email, "password": password, "name": name })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "register failed: {body}");
    headers
        .get(header::SET_COOKIE)
        .expect("set-cookie")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string()
}

/// 建一個上架商品（單一預設規格），回 (variant_id, slug)
pub async fn active_product(
    pool: &PgPool,
    name: &str,
    price: i32,
    stock: i32,
) -> (uuid::Uuid, String) {
    use dog_shop_api::domain::products::{self, ProductInput, VariantInput};
    let product = products::create(
        pool,
        ProductInput {
            name: name.to_string(),
            slug: None,
            description: None,
            category_id: None,
            status: "active".to_string(),
            option1_name: None,
            option2_name: None,
            sort_order: None,
            variants: vec![VariantInput {
                id: None,
                option1_value: None,
                option2_value: None,
                sku: None,
                price,
                compare_at_price: None,
                stock,
                is_active: None,
                image_path: None,
            }],
            images: vec![],
        },
    )
    .await
    .unwrap();
    (product.variants[0].id, product.product.slug.clone())
}

/// 塞一筆有效的門市選擇，回 token
pub async fn cvs_store_token(pool: &PgPool) -> String {
    use dog_shop_api::domain::cvs_stores::{self, CvsStore};
    let token = dog_shop_api::auth::tokens::generate_token();
    cvs_stores::insert(
        pool,
        &CvsStore {
            token: token.clone(),
            sub_type: "UNIMARTC2C".to_string(),
            store_id: "131386".to_string(),
            store_name: "測試門市".to_string(),
            store_address: "台北市中正區重慶南路一段 122 號".to_string(),
            store_phone: "0223456789".to_string(),
        },
        60,
    )
    .await
    .unwrap();
    token
}
