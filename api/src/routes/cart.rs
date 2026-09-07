use axum::{Json, Router, extract::State, routing::post};
use serde::{Deserialize, Serialize};

use crate::{
    domain::{
        cart::{self, CheckedLine},
        orders::{CVS_SUBTOTAL_LIMIT, OrderItemInput},
        settings::{self, ShippingSettings},
    },
    error::ApiResult,
    extract::AppJson,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/api/cart/validate", post(validate))
}

#[derive(Deserialize)]
pub struct ValidateBody {
    #[serde(default)]
    pub items: Vec<OrderItemInput>,
}

#[derive(Serialize)]
pub struct CartValidateResponse {
    pub items: Vec<CheckedLine>,
    pub subtotal: i32,
    pub shipping: ShippingSettings,
    pub cvs_limit_exceeded: bool,
}

/// 購物車頁與結帳頁用：核對價格、庫存、上下架，並附上運費設定讓前端算運費
async fn validate(
    State(state): State<AppState>,
    AppJson(body): AppJson<ValidateBody>,
) -> ApiResult<Json<CartValidateResponse>> {
    let check = cart::check(&state.db, &body.items).await?;
    let settings = settings::get_all(&state.db).await?;
    Ok(Json(CartValidateResponse {
        cvs_limit_exceeded: check.subtotal > CVS_SUBTOTAL_LIMIT,
        items: check.items,
        subtotal: check.subtotal,
        shipping: settings.shipping,
    }))
}
