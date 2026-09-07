use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use serde::Deserialize;
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

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/orders", post(create))
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
