use std::{sync::Arc, time::Duration};

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use serde::Deserialize;
use tower_governor::{
    GovernorError, GovernorLayer, governor::GovernorConfigBuilder,
    key_extractor::SmartIpKeyExtractor,
};
use uuid::Uuid;

use crate::{
    auth::extract::CurrentUser,
    domain::{
        orders::{self, OrderCreated, OrderDetail, OrderInput, Viewer},
        users::User,
    },
    error::{ApiError, ApiResult},
    extract::{AppJson, AppPath, AppQuery},
    state::AppState,
};

/// `POST /api/orders` 是唯一開放給匿名者的寫入端點，未登入就能扣庫存；掛一個獨立的
/// GovernorLayer 當減速帶（不與 auth 共用配額）：每個 IP 突發 10 次，之後每 12 秒補 1 次（約 5 次/分）。
/// GET /api/orders/{id}、cancel 不限（結構同 routes/auth.rs:33-58）。
pub fn router() -> Router<AppState> {
    let governor_conf = Arc::new(
        GovernorConfigBuilder::default()
            .key_extractor(SmartIpKeyExtractor)
            .per_second(12)
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
    let governor_layer = GovernorLayer::new(governor_conf).error_handler(|err| match err {
        GovernorError::TooManyRequests { .. } => ApiError::RateLimited.into_response(),
        other => ApiError::Internal(anyhow::anyhow!("rate limiter: {other:?}")).into_response(),
    });

    Router::new()
        .route("/api/orders", post(create).layer(governor_layer))
        .route("/api/orders/{id}", get(detail))
        .route("/api/orders/{id}/cancel", post(cancel))
}

/// 會員或訪客都能下單（規格 §7）；登入者的訂單掛在帳號下
async fn create(
    CurrentUser(user): CurrentUser,
    State(state): State<AppState>,
    AppJson(input): AppJson<OrderInput>,
) -> ApiResult<(StatusCode, Json<OrderCreated>)> {
    let created = orders::create_order(&state.db, input, user.as_ref()).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

#[derive(Deserialize)]
pub struct ViewerQuery {
    /// 訪客的 guest_token（規格 §6.1：訪客要帶 ?t=）
    pub t: Option<String>,
}

/// 先用帳號看（會員看自己的），看不到再用 ?t=；都不行 404（不用 403，免得被拿來猜訂單 id）
async fn find_order(
    state: &AppState,
    id: Uuid,
    user: Option<&User>,
    token: Option<&str>,
) -> ApiResult<(OrderDetail, Viewer)> {
    if let Some(user) = user {
        let viewer = Viewer::User(user.id);
        if let Some(detail) = orders::get_for_viewer(&state.db, id, &viewer).await? {
            return Ok((detail, viewer));
        }
    }
    if let Some(token) = token.map(str::trim).filter(|t| !t.is_empty()) {
        let viewer = Viewer::Guest(token.to_string());
        if let Some(detail) = orders::get_for_viewer(&state.db, id, &viewer).await? {
            return Ok((detail, viewer));
        }
    }
    Err(ApiError::NotFound)
}

async fn detail(
    CurrentUser(user): CurrentUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<Uuid>,
    AppQuery(query): AppQuery<ViewerQuery>,
) -> ApiResult<Json<OrderDetail>> {
    let (detail, _) = find_order(&state, id, user.as_ref(), query.t.as_deref()).await?;
    Ok(Json(detail))
}

/// 買家取消（與規格不同之處 13）
async fn cancel(
    CurrentUser(user): CurrentUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<Uuid>,
    AppQuery(query): AppQuery<ViewerQuery>,
) -> ApiResult<StatusCode> {
    let (_, viewer) = find_order(&state, id, user.as_ref(), query.t.as_deref()).await?;
    orders::cancel(&state.db, id, &viewer, "buyer").await?;
    Ok(StatusCode::NO_CONTENT)
}
