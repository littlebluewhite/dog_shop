use axum::{Json, Router, extract::State, routing::get};

use crate::{
    domain::settings::{self, PublicSettings},
    error::ApiResult,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/api/settings/public", get(public_settings))
}

/// 前台需要的設定：商店資訊、運費、付款方式（不含寄件人與退貨門市）
async fn public_settings(State(state): State<AppState>) -> ApiResult<Json<PublicSettings>> {
    Ok(Json(settings::get_all(&state.db).await?.public()))
}
