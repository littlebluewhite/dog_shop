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
        password::verify_password_async,
        session,
    },
    domain::users,
    error::{ApiError, ApiResult},
    extract::AppJson,
    state::AppState,
};

/// /api/auth/* 每個 IP 每分鐘 10 次（規格 §11）：burst 10，每 6 秒補 1 個。
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
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/me", get(me))
        .layer(governor_layer)
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
