//! 綠界付款回呼（規格 §7 第 8 點、§8.2、§14）。回純文字：`1|OK`、`0|CheckMacValue Error`（400）、
//! `0|Missing Field`（400）、`0|Unknown MerchantTradeNo`（200）、`0|Server Error`（500，讓綠界重送）。
//! 路徑前綴 `/api/ecpay/` 已在 auth/csrf.rs 的 EXEMPT_PREFIXES 內（規格 §11：靠 CheckMacValue 驗）。
use axum::{
    Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};

use crate::{
    domain::payments::{self, InfoOutcome, ReturnOutcome},
    ecpay::aio::{self, CallbackError},
    error::ApiError,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/ecpay/payment/return", post(payment_return))
        .route("/api/ecpay/payment/info", post(payment_info))
}

/// 綠界送 application/x-www-form-urlencoded；不用 axum::Form，因為要拿到所有欄位重算簽章
fn parse_form(body: &str) -> Vec<(String, String)> {
    form_urlencoded::parse(body.as_bytes())
        .into_owned()
        .collect()
}

fn text(status: StatusCode, body: &'static str) -> Response {
    (status, body).into_response()
}

fn callback_error(path: &'static str, err: CallbackError) -> Response {
    tracing::warn!(path, error = %err, "綠界回呼簽章或欄位錯誤");
    match err {
        CallbackError::BadMac => text(StatusCode::BAD_REQUEST, "0|CheckMacValue Error"),
        CallbackError::Missing(_) => text(StatusCode::BAD_REQUEST, "0|Missing Field"),
    }
}

fn server_error(path: &'static str, err: ApiError) -> Response {
    tracing::error!(path, error = %err, "綠界回呼處理失敗，回 500 讓綠界重送");
    text(StatusCode::INTERNAL_SERVER_ERROR, "0|Server Error")
}

/// 付款結果
async fn payment_return(State(state): State<AppState>, body: String) -> Response {
    let params = parse_form(&body);
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
