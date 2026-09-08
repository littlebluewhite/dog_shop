//! 結帳輔助：門市選擇（規格 §7 第 2 點、§10）
use axum::{Json, Router, extract::State, routing::get};
use serde::Deserialize;

use crate::{
    auth::tokens,
    domain::cvs_stores::{self, CvsStore},
    ecpay::{aio::CheckoutForm, logistics},
    error::{ApiError, ApiResult},
    extract::{AppJson, AppPath},
    routes::rate_limit,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/checkout/cvs-map",
            rate_limit::anonymous_write(axum::routing::post(cvs_map)),
        )
        .route("/api/checkout/cvs-store/{token}", get(cvs_store))
}

#[derive(Deserialize)]
pub struct CvsMapInput {
    pub sub_type: String,
    /// 1 = 手機（綠界會用手機版地圖）；其他當桌機
    #[serde(default)]
    pub device: i32,
}

/// 登記一個 token，回送往綠界電子地圖的表單（頂層導頁，不用 iframe）
async fn cvs_map(
    State(state): State<AppState>,
    AppJson(input): AppJson<CvsMapInput>,
) -> ApiResult<Json<CheckoutForm>> {
    let sub_type = input.sub_type.trim().to_string();
    if !logistics::is_sub_type(&sub_type) {
        return Err(ApiError::field("sub_type", "請選擇超商"));
    }
    let token = tokens::generate_short_token();
    cvs_stores::insert_map_request(
        &state.db,
        &token,
        &sub_type,
        cvs_stores::MAP_REQUEST_TTL_MINUTES,
    )
    .await?;
    Ok(Json(logistics::map_form(
        &state.config.ecpay,
        &state.config.public_base_url,
        &token,
        &sub_type,
        input.device == 1,
    )))
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
