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
    domain::shipments::{self, StatusOutcome},
    ecpay::aio::CallbackError,
    ecpay::logistics,
    routes::ecpay_callback::{MAX_CALLBACK_FIELDS, callback_error, parse_form, server_error, text},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/ecpay/logistics/map-reply", post(map_reply))
        .route("/api/ecpay/logistics/status", post(status))
        .route("/api/ecpay/logistics/store-update", post(store_update))
        // 回呼一律 < 2 KB；後掛的 layer 在內層，覆蓋 app.rs 給圖片上傳訂的 10 MB（計畫 3 修正波 #1）
        .layer(DefaultBodyLimit::max(64 * 1024))
}

fn redirect(location: String) -> Response {
    (StatusCode::SEE_OTHER, [(header::LOCATION, location)]).into_response()
}

/// 綠界地圖選完，用買家瀏覽器把門市 POST 回來。失敗不回 400 純文字（買家會卡在錯誤頁），
/// 改 303 回結帳頁帶 store_error（與規格不同之處 37）。token 不印進 log。
/// body extractor 自己的拒絕（>64 KB、非 UTF-8）也要走同一條路，不能把裸錯誤丟給買家（審查 Minor 1）
async fn map_reply(
    State(state): State<AppState>,
    body: Result<String, axum::extract::rejection::StringRejection>,
) -> Response {
    let base = state.config.public_base_url.as_str();
    let fail = |reason: &str| redirect(format!("{base}/checkout?store_error={reason}"));
    let Ok(body) = body else {
        tracing::warn!("map-reply body 過大或不是 UTF-8");
        return fail("invalid");
    };
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

/// 物流狀態通知（綠界伺服器 POST；規格 §8.3、§14）
async fn status(State(state): State<AppState>, body: String) -> Response {
    let params = parse_form(&body);
    if params.len() > MAX_CALLBACK_FIELDS {
        return callback_error("logistics-status", CallbackError::BadMac);
    }
    let notification = match logistics::parse_status(&state.config.ecpay, &params) {
        Ok(n) => n,
        Err(e) => return callback_error("logistics-status", e),
    };
    match shipments::apply_status(&state.db, &notification).await {
        Ok(StatusOutcome::Unknown) => {
            tracing::warn!(merchant_trade_no = %notification.merchant_trade_no, logistics_id = %notification.logistics_id, "物流狀態通知找不到單");
            text(StatusCode::OK, "0|Unknown MerchantTradeNo")
        }
        Ok(outcome) => {
            tracing::info!(merchant_trade_no = %notification.merchant_trade_no, rtn_code = notification.rtn_code, ?outcome, "物流狀態通知處理完成");
            text(StatusCode::OK, "1|OK")
        }
        Err(e) => server_error("logistics-status", e),
    }
}

/// 更新門市通知（7-11 C2C；與規格不同之處 35）
async fn store_update(State(state): State<AppState>, body: String) -> Response {
    let params = parse_form(&body);
    if params.len() > MAX_CALLBACK_FIELDS {
        return callback_error("logistics-store-update", CallbackError::BadMac);
    }
    let update = match logistics::parse_store_update(&state.config.ecpay, &params) {
        Ok(u) => u,
        Err(e) => return callback_error("logistics-store-update", e),
    };
    match shipments::apply_store_update(&state.db, &update).await {
        Ok(false) => {
            tracing::warn!(logistics_id = %update.logistics_id, "更新門市通知找不到單");
            text(StatusCode::OK, "0|Unknown AllPayLogisticsID")
        }
        Ok(true) => {
            tracing::info!(logistics_id = %update.logistics_id, status = %update.status, store_type = %update.store_type, "更新門市通知已記錄");
            text(StatusCode::OK, "1|OK")
        }
        Err(e) => server_error("logistics-store-update", e),
    }
}
