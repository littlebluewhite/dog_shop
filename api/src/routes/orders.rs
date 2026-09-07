use std::{sync::Arc, time::Duration};

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tower_governor::{
    GovernorError, GovernorLayer, governor::GovernorConfigBuilder,
    key_extractor::SmartIpKeyExtractor,
};
use uuid::Uuid;

use crate::{
    auth::extract::CurrentUser,
    domain::{
        orders::{self, OrderCreated, OrderDetail, OrderInput, Viewer},
        settings,
        users::User,
    },
    ecpay::aio::{self, CheckoutForm},
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

/// 規格 §7 第 6 點的回應：訂單資料 + 送往綠界的表單（與規格不同之處 23）。
/// `ecpay` 只有在訂單已經建立、但事後組表單失敗時才會是 null；這時前端要把買家導去訂單頁，
/// 用 Task 7 的重新付款
#[derive(Serialize)]
pub struct CreateOrderResponse {
    #[serde(flatten)]
    pub created: OrderCreated,
    pub ecpay: Option<CheckoutForm>,
}

/// 讀完整訂單與最新一筆付款，組綠界表單。create 與 repay（Task 7）共用。
/// guest_token 有值時 ClientBackURL 帶 ?t=（訪客回到訂單頁要靠它）
async fn checkout_form_for(
    state: &AppState,
    order_id: Uuid,
    guest_token: Option<&str>,
) -> ApiResult<CheckoutForm> {
    let detail = orders::get_detail(&state.db, order_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let payment = detail
        .payment
        .as_ref()
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("訂單 {order_id} 沒有 payments 列")))?;
    let shop = settings::get_all(&state.db).await?.shop;
    aio::checkout_form(
        &state.config.ecpay,
        &state.config.public_base_url,
        &shop.name,
        &detail,
        payment,
        guest_token,
        Utc::now(),
    )
    .map_err(ApiError::Internal)
}

/// 會員或訪客都能下單（規格 §7）；登入者的訂單掛在帳號下。回應含送往綠界的表單欄位；
/// 訂單一旦建立（庫存已扣、email job 已排），組表單失敗不能讓這次請求整個失敗——不然客戶端重試
/// 會再呼叫一次 create_order，庫存被扣兩次、多出一筆重複訂單。所以這裡失敗只記 log，ecpay 回
/// null，前端導去訂單頁用 Task 7 的重新付款
async fn create(
    CurrentUser(user): CurrentUser,
    State(state): State<AppState>,
    AppJson(input): AppJson<OrderInput>,
) -> ApiResult<(StatusCode, Json<CreateOrderResponse>)> {
    let created = orders::create_order(&state.db, input, user.as_ref()).await?;
    let guest_token = user.is_none().then_some(created.guest_token.as_str());
    let ecpay = match checkout_form_for(&state, created.order_id, guest_token).await {
        Ok(form) => Some(form),
        Err(e) => {
            tracing::error!(
                order_id = %created.order_id,
                error = %e,
                "下單成功但組綠界表單失敗，回傳訂單不帶表單"
            );
            None
        }
    };
    Ok((
        StatusCode::CREATED,
        Json(CreateOrderResponse { created, ecpay }),
    ))
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
