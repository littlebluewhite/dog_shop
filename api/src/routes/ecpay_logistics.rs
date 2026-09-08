//! 綠界物流回呼（規格 §7 第 2 點、§8.3、§11、§14）。map-reply 是買家瀏覽器 POST 過來的（沒有簽章，
//! 靠 token）；status 與 store-update 是綠界伺服器 POST（MD5 簽章）— Task 4。
//! 路徑前綴 `/api/ecpay/` 已在 auth/csrf.rs 的 EXEMPT_PREFIXES 內
use axum::{
    Router,
    extract::{DefaultBodyLimit, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::post,
};

use crate::{
    domain::cvs_stores::{self, CvsStore},
    ecpay::logistics,
    routes::ecpay_callback::{MAX_CALLBACK_FIELDS, parse_form},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/ecpay/logistics/map-reply", post(map_reply))
        // 回呼一律 < 2 KB；後掛的 layer 在內層，覆蓋 app.rs 給圖片上傳訂的 10 MB（計畫 3 修正波 #1）
        .layer(DefaultBodyLimit::max(64 * 1024))
}

fn redirect(location: String) -> Response {
    (StatusCode::SEE_OTHER, [(header::LOCATION, location)]).into_response()
}

/// 綠界地圖選完，用買家瀏覽器把門市 POST 回來。失敗不回 400 純文字（買家會卡在錯誤頁），
/// 改 303 回結帳頁帶 store_error（與規格不同之處 37）。token 不印進 log
async fn map_reply(State(state): State<AppState>, body: String) -> Response {
    let base = state.config.public_base_url.as_str();
    let fail = |reason: &str| redirect(format!("{base}/checkout?store_error={reason}"));
    let params = parse_form(&body);
    if params.len() > MAX_CALLBACK_FIELDS {
        tracing::warn!("map-reply 欄位太多");
        return fail("invalid");
    }
    let reply = match logistics::parse_map_reply(&params) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "map-reply 欄位不全");
            return fail("invalid");
        }
    };
    let registered = match cvs_stores::take_map_request(&state.db, &reply.token).await {
        Ok(Some(sub_type)) => sub_type,
        Ok(None) => {
            tracing::warn!("map-reply 的 token 不存在或已過期");
            return fail("expired");
        }
        Err(e) => {
            tracing::error!(error = %e, "map-reply 查 token 失敗");
            return fail("server");
        }
    };
    if registered != reply.sub_type {
        tracing::warn!(registered, replied = %reply.sub_type, "map-reply 的超商種類與登記不符");
        return fail("invalid");
    }
    let store = CvsStore {
        token: reply.token,
        sub_type: reply.sub_type,
        store_id: reply.store_id,
        store_name: reply.store_name,
        store_address: reply.store_address,
        store_phone: reply.store_phone,
    };
    if let Err(e) = cvs_stores::insert(&state.db, &store, cvs_stores::STORE_TTL_MINUTES).await {
        tracing::error!(error = %e, "map-reply 寫門市失敗");
        return fail("server");
    }
    tracing::info!(sub_type = %store.sub_type, store_id = %store.store_id, "map-reply 門市已存");
    redirect(format!("{base}/checkout?store={}", store.token))
}
