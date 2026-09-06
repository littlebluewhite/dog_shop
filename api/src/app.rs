use axum::{Router, extract::DefaultBodyLimit};
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::TraceLayer;

use crate::{auth, routes, state::AppState};

/// 上傳上限 10 MB，多留一點給 multipart 邊界與 JSON
pub const BODY_LIMIT_BYTES: usize = 10 * 1024 * 1024 + 64 * 1024;

/// 組出整個 API。layer 的順序：後加的在外層，所以由外到內是
/// DefaultBodyLimit → SetRequestId → PropagateRequestId → Trace → require_same_origin → handler。
pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(routes::health::router())
        .merge(routes::settings::router())
        .merge(routes::auth::router())
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::csrf::require_same_origin,
        ))
        .layer(TraceLayer::new_for_http())
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(DefaultBodyLimit::max(BODY_LIMIT_BYTES))
        .with_state(state)
}
