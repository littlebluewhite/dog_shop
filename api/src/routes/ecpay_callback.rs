//! 綠界回呼共用的小工具：付款（ecpay_payment.rs）與物流（ecpay_logistics.rs）都用。
//! 回純文字：`1|OK`、`0|CheckMacValue Error`（400）、`0|Missing Field`（400）、`0|Server Error`（500，讓綠界重送）
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

use crate::{ecpay::aio::CallbackError, error::ApiError};

/// 綠界回呼的欄位筆數上限：`ReturnURL` 最多 30 幾個欄位，100 很寬鬆（計畫 3 修正波 #1）
pub const MAX_CALLBACK_FIELDS: usize = 100;

/// 綠界送 application/x-www-form-urlencoded；不用 axum::Form，因為要拿到所有欄位重算簽章
pub fn parse_form(body: &str) -> Vec<(String, String)> {
    form_urlencoded::parse(body.as_bytes())
        .into_owned()
        .collect()
}

pub fn text(status: StatusCode, body: &'static str) -> Response {
    (status, body).into_response()
}

pub fn callback_error(path: &'static str, err: CallbackError) -> Response {
    tracing::warn!(path, error = %err, "綠界回呼簽章或欄位錯誤");
    match err {
        CallbackError::BadMac => text(StatusCode::BAD_REQUEST, "0|CheckMacValue Error"),
        CallbackError::Missing(_) => text(StatusCode::BAD_REQUEST, "0|Missing Field"),
    }
}

pub fn server_error(path: &'static str, err: ApiError) -> Response {
    tracing::error!(path, error = %err, "綠界回呼處理失敗，回 500 讓綠界重送");
    text(StatusCode::INTERNAL_SERVER_ERROR, "0|Server Error")
}
