//! 把 axum 內建擷取器的錯誤轉成我們的 JSON 錯誤格式。
//! handler 一律用 AppJson / AppQuery / AppPath，不要直接用 axum::Json 等。
use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::extract::{FromRequest, FromRequestParts};
use serde_json::json;

use crate::error::ApiError;

#[derive(FromRequest)]
#[from_request(via(axum::Json), rejection(ApiError))]
pub struct AppJson<T>(pub T);

#[derive(FromRequestParts)]
#[from_request(via(axum::extract::Query), rejection(ApiError))]
pub struct AppQuery<T>(pub T);

#[derive(FromRequestParts)]
#[from_request(via(axum::extract::Path), rejection(ApiError))]
pub struct AppPath<T>(pub T);

impl From<JsonRejection> for ApiError {
    fn from(rejection: JsonRejection) -> Self {
        ApiError::Validation {
            message: "JSON 格式錯誤".to_string(),
            details: json!({ "detail": rejection.body_text() }),
        }
    }
}

impl From<QueryRejection> for ApiError {
    fn from(rejection: QueryRejection) -> Self {
        ApiError::Validation {
            message: "查詢參數格式錯誤".to_string(),
            details: json!({ "detail": rejection.body_text() }),
        }
    }
}

impl From<PathRejection> for ApiError {
    fn from(rejection: PathRejection) -> Self {
        ApiError::Validation {
            message: "網址參數格式錯誤".to_string(),
            details: json!({ "detail": rejection.body_text() }),
        }
    }
}
