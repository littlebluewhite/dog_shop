//! 綠界付款回呼（規格 §7 第 8 點、§8.2、§14）。回純文字：`1|OK`、`0|CheckMacValue Error`（400）、
//! `0|Missing Field`（400）、`0|Unknown MerchantTradeNo`（200）、`0|Server Error`（500，讓綠界重送）。
//! 路徑前綴 `/api/ecpay/` 已在 auth/csrf.rs 的 EXEMPT_PREFIXES 內（規格 §11：靠 CheckMacValue 驗）。
use axum::{
    Router,
    extract::{DefaultBodyLimit, State},
    http::StatusCode,
    response::Response,
    routing::post,
};

use crate::{
    domain::payments::{self, InfoOutcome, ReturnOutcome},
    ecpay::aio::{self, CallbackError},
    routes::ecpay_callback::{MAX_CALLBACK_FIELDS, callback_error, parse_form, server_error, text},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/ecpay/payment/return", post(payment_return))
        .route("/api/ecpay/payment/info", post(payment_info))
        // 回呼一律 < 2 KB；後掛的 layer 在內層，覆蓋 app.rs 給圖片上傳訂的 10 MB（修正波 #1）
        .layer(DefaultBodyLimit::max(64 * 1024))
}

/// 付款結果
async fn payment_return(State(state): State<AppState>, body: String) -> Response {
    let params = parse_form(&body);
    if params.len() > MAX_CALLBACK_FIELDS {
        return callback_error("return", CallbackError::BadMac);
    }
    let notification = match aio::parse_notification(&state.config.ecpay, &params) {
        Ok(n) => n,
        Err(e) => return callback_error("return", e),
    };
    match payments::apply_return(&state.db, &notification).await {
        Ok(ReturnOutcome::Unknown) => {
            tracing::warn!(merchant_trade_no = %notification.merchant_trade_no, "ReturnURL 找不到 MerchantTradeNo");
            text(StatusCode::OK, "0|Unknown MerchantTradeNo")
        }
        Ok(outcome) => {
            tracing::info!(merchant_trade_no = %notification.merchant_trade_no, rtn_code = notification.rtn_code, ?outcome, "ReturnURL 處理完成");
            text(StatusCode::OK, "1|OK")
        }
        Err(e) => server_error("return", e),
    }
}

/// ATM 虛擬帳號／超商代碼
async fn payment_info(State(state): State<AppState>, body: String) -> Response {
    let params = parse_form(&body);
    if params.len() > MAX_CALLBACK_FIELDS {
        return callback_error("info", CallbackError::BadMac);
    }
    let notification = match aio::parse_notification(&state.config.ecpay, &params) {
        Ok(n) => n,
        Err(e) => return callback_error("info", e),
    };
    match payments::apply_info(&state.db, &notification).await {
        Ok(InfoOutcome::Unknown) => {
            tracing::warn!(merchant_trade_no = %notification.merchant_trade_no, "PaymentInfoURL 找不到 MerchantTradeNo");
            text(StatusCode::OK, "0|Unknown MerchantTradeNo")
        }
        Ok(outcome) => {
            tracing::info!(merchant_trade_no = %notification.merchant_trade_no, rtn_code = notification.rtn_code, ?outcome, "PaymentInfoURL 處理完成");
            text(StatusCode::OK, "1|OK")
        }
        Err(e) => server_error("info", e),
    }
}
