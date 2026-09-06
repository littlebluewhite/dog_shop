use std::{sync::Arc, time::Duration};

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode, header::SET_COOKIE},
    response::{AppendHeaders, IntoResponse},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use tower_governor::{
    GovernorError, GovernorLayer, governor::GovernorConfigBuilder,
    key_extractor::SmartIpKeyExtractor,
};
use uuid::Uuid;

use crate::{
    auth::{
        cookie::{self, SESSION_COOKIE},
        extract::AuthUser,
        password::{MIN_PASSWORD_CHARS, hash_password_async, verify_password_async},
        session,
    },
    domain::{jobs, password_resets, users},
    error::{ApiError, ApiResult, FieldErrors},
    extract::AppJson,
    state::AppState,
};

/// `/api/auth/login|register|forgot|reset` 共用一個限流器：每個 IP 每分鐘 10 次（規格 §11）：burst 10，每 6 秒補 1 個。
/// 只限制 login，/me 會被 SvelteKit SSR 每次請求呼叫，限制它會把同一 IP 的使用者鎖住。
/// key 用 SmartIpKeyExtractor：先看 X-Forwarded-For / X-Real-IP（正式環境前面是 Caddy），沒有才用連線 IP。
pub fn router() -> Router<AppState> {
    let governor_conf = Arc::new(
        GovernorConfigBuilder::default()
            .key_extractor(SmartIpKeyExtractor)
            .per_second(6)
            .burst_size(10)
            .finish()
            .expect("governor config"),
    );
    // 定期清掉沒在用的 IP 記錄，不然記憶體只會長
    let limiter = governor_conf.limiter().clone();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(60));
            limiter.retain_recent();
        }
    });
    // error_handler 是 GovernorLayer（不是 GovernorConfigBuilder）的方法（tower_governor 0.8）
    let governor_layer = GovernorLayer::new(governor_conf).error_handler(|err| match err {
        GovernorError::TooManyRequests { .. } => ApiError::RateLimited.into_response(),
        other => ApiError::Internal(anyhow::anyhow!("rate limiter: {other:?}")).into_response(),
    });

    Router::new()
        .route("/api/auth/login", post(login).layer(governor_layer.clone()))
        .route(
            "/api/auth/register",
            post(register).layer(governor_layer.clone()),
        )
        .route(
            "/api/auth/forgot",
            post(forgot).layer(governor_layer.clone()),
        )
        .route("/api/auth/reset", post(reset).layer(governor_layer))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/me", get(me))
}

#[derive(Deserialize)]
pub struct LoginBody {
    pub email: String,
    pub password: String,
}

async fn login(
    State(state): State<AppState>,
    AppJson(body): AppJson<LoginBody>,
) -> ApiResult<impl IntoResponse> {
    // 帳號不存在與密碼錯誤回同一個錯，不透露帳號存不存在
    let Some(user) = users::find_by_email(&state.db, &body.email).await? else {
        return Err(ApiError::Unauthorized("Email 或密碼錯誤"));
    };
    if !verify_password_async(body.password, user.password_hash.clone()).await? {
        return Err(ApiError::Unauthorized("Email 或密碼錯誤"));
    }
    let sid = session::create(&state.db, user.id).await?;
    let cookie = cookie::session_cookie(&sid.to_string(), state.config.cookie_secure);
    Ok((
        StatusCode::OK,
        AppendHeaders([(SET_COOKIE, cookie)]),
        Json(json!({ "user": user.public() })),
    ))
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<impl IntoResponse> {
    if let Some(sid) =
        cookie::get_cookie(&headers, SESSION_COOKIE).and_then(|raw| Uuid::parse_str(&raw).ok())
    {
        session::delete(&state.db, sid).await?;
    }
    let cookie = cookie::clear_session_cookie(state.config.cookie_secure);
    Ok((
        StatusCode::NO_CONTENT,
        AppendHeaders([(SET_COOKIE, cookie)]),
    ))
}

async fn me(AuthUser(user): AuthUser) -> Json<Value> {
    Json(json!({ "user": user.public() }))
}

#[derive(Deserialize)]
pub struct RegisterBody {
    pub email: String,
    pub password: String,
    pub name: String,
    pub phone: Option<String>,
}

/// 去空白；空字串當沒填
fn clean_opt(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

async fn register(
    State(state): State<AppState>,
    AppJson(body): AppJson<RegisterBody>,
) -> ApiResult<impl IntoResponse> {
    let mut errors = FieldErrors::new();
    let email = users::normalize_email(&body.email);
    if !users::is_valid_email(&email) {
        errors.add("email", "Email 格式不正確");
    }
    if body.password.chars().count() < MIN_PASSWORD_CHARS {
        errors.add("password", "密碼至少 8 碼");
    }
    let name = body.name.trim();
    let name_len = name.chars().count();
    if name_len == 0 || name_len > 50 {
        errors.add("name", "必填，最多 50 字");
    }
    let phone = clean_opt(&body.phone);
    if let Some(p) = &phone
        && !users::is_tw_mobile(p)
    {
        errors.add("phone", "手機格式：09 開頭共 10 碼");
    }
    errors.into_result()?;

    let hash = hash_password_async(body.password).await?;
    let user = match users::create_customer(&state.db, &email, &hash, name, phone.as_deref()).await
    {
        Ok(user) => user,
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            return Err(ApiError::field("email", "這個 Email 已經註冊過了"));
        }
        Err(e) => return Err(e.into()),
    };
    let sid = session::create(&state.db, user.id).await?;
    let cookie = cookie::session_cookie(&sid.to_string(), state.config.cookie_secure);
    Ok((
        StatusCode::CREATED,
        AppendHeaders([(SET_COOKIE, cookie)]),
        Json(json!({ "user": user.public() })),
    ))
}

#[derive(Deserialize)]
pub struct ForgotBody {
    pub email: String,
}

/// 永遠回 202，不透露帳號存不存在。有帳號才排寄信 job；token 由計畫 3 的 job 在寄出時產生（見與規格不同之處 11）
async fn forgot(
    State(state): State<AppState>,
    AppJson(body): AppJson<ForgotBody>,
) -> ApiResult<impl IntoResponse> {
    let email = users::normalize_email(&body.email);
    if !users::is_valid_email(&email) {
        return Err(ApiError::field("email", "Email 格式不正確"));
    }
    if let Some(user) = users::find_by_email(&state.db, &email).await? {
        let mut tx = state.db.begin().await?;
        jobs::enqueue(
            &mut tx,
            jobs::KIND_SEND_EMAIL,
            json!({ "template": "password_reset", "user_id": user.id }),
            None,
        )
        .await?;
        tx.commit().await?;
    }
    Ok((StatusCode::ACCEPTED, Json(json!({ "ok": true }))))
}

#[derive(Deserialize)]
pub struct ResetBody {
    pub token: String,
    pub password: String,
}

/// token 有效才換密碼；換完把該使用者所有 session 登出（規格 §11）
async fn reset(
    State(state): State<AppState>,
    AppJson(body): AppJson<ResetBody>,
) -> ApiResult<StatusCode> {
    if body.password.chars().count() < MIN_PASSWORD_CHARS {
        return Err(ApiError::field("password", "密碼至少 8 碼"));
    }
    let Some(user_id) = password_resets::consume(&state.db, body.token.trim()).await? else {
        return Err(ApiError::field("token", "重設連結無效或已過期"));
    };
    let hash = hash_password_async(body.password).await?;
    users::set_password(&state.db, user_id, &hash).await?;
    session::delete_all_for_user(&state.db, user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
