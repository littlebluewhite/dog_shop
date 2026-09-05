use axum::{Json, Router, extract::State, routing::get};
use serde_json::{Value, json};

use crate::{domain::settings, error::ApiResult, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new().route("/api/settings/public", get(public_settings))
}

/// 公開的商店設定：目前只有 shop 這把 key（名稱、聯絡方式）。計畫 2 會加運費。
async fn public_settings(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let shop = settings::get(&state.db, settings::SHOP_KEY)
        .await?
        .unwrap_or_else(|| json!({}));
    Ok(Json(shop))
}
