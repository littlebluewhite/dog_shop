use axum::{
    extract::{Request, State},
    http::{Method, header::ORIGIN},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::{error::ApiError, state::AppState};

/// 綠界伺服器回呼與門市回傳走這些前綴，帶不了我們的 header（路由本身在計畫 3、4 才會加）
const EXEMPT_PREFIXES: &[&str] = &["/api/ecpay/"];

/// 會改資料的請求（POST/PUT/PATCH/DELETE）必須（規格 §11）：
/// 1. 帶 `X-Requested-With: fetch`（跨站的 <form> 送不出自訂 header）
/// 2. 如果有 Origin header，必須等於 PUBLIC_BASE_URL 的 origin
pub async fn require_same_origin(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let mutating = matches!(
        *request.method(),
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    );
    let exempt = EXEMPT_PREFIXES
        .iter()
        .any(|prefix| request.uri().path().starts_with(prefix));
    if mutating && !exempt {
        let has_marker = request
            .headers()
            .get("x-requested-with")
            .and_then(|v| v.to_str().ok())
            == Some("fetch");
        if !has_marker {
            return ApiError::Forbidden("缺少 X-Requested-With header").into_response();
        }
        if let Some(origin) = request.headers().get(ORIGIN).and_then(|v| v.to_str().ok())
            && origin.trim_end_matches('/') != state.config.public_origin()
        {
            return ApiError::Forbidden("Origin 不符").into_response();
        }
    }
    next.run(request).await
}
