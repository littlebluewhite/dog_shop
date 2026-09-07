use axum::{Json, Router, extract::State, routing::get};

use crate::{
    domain::cvs_stores::{self, CvsStore},
    error::{ApiError, ApiResult},
    extract::AppPath,
    state::AppState,
};

/// 計畫 4 會在這裡加 POST /api/checkout/cvs-map
pub fn router() -> Router<AppState> {
    Router::new().route("/api/checkout/cvs-store/{token}", get(cvs_store))
}

/// 結帳頁用 ?store=<token> 還原門市顯示；過期或不存在 404
async fn cvs_store(
    State(state): State<AppState>,
    AppPath(token): AppPath<String>,
) -> ApiResult<Json<CvsStore>> {
    cvs_stores::get_valid(&state.db, &token)
        .await?
        .map(Json)
        .ok_or(ApiError::NotFound)
}
