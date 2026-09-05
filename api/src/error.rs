use std::collections::BTreeMap;

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};

/// 所有 handler 的錯誤型別。回應格式固定為
/// `{ "error": { "code", "message", "details" } }`（規格 §10）。
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{message}")]
    Validation { message: String, details: Value },
    #[error("{0}")]
    Unauthorized(&'static str),
    #[error("{0}")]
    Forbidden(&'static str),
    #[error("找不到資料")]
    NotFound,
    #[error("請求太頻繁，請稍後再試")]
    RateLimited,
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

pub type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
    /// 單一欄位的驗證錯誤
    pub fn field(field: &str, message: &str) -> Self {
        let mut errors = FieldErrors::new();
        errors.add(field, message);
        errors.into_error()
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::Validation { .. } => "VALIDATION",
            Self::Unauthorized(_) => "UNAUTHORIZED",
            Self::Forbidden(_) => "FORBIDDEN",
            Self::NotFound => "NOT_FOUND",
            Self::RateLimited => "RATE_LIMITED",
            Self::Internal(_) => "INTERNAL",
        }
    }

    pub fn status(&self) -> StatusCode {
        match self {
            Self::Validation { .. } => StatusCode::BAD_REQUEST,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        let code = self.code();
        let (message, details) = match &self {
            Self::Validation { message, details } => (message.clone(), details.clone()),
            Self::Internal(err) => {
                // 細節只進 log；request id 在回應 header x-request-id
                tracing::error!(error = ?err, "internal error");
                ("伺服器發生錯誤，請稍後再試".to_string(), Value::Null)
            }
            other => (other.to_string(), Value::Null),
        };
        let body = json!({ "error": { "code": code, "message": message, "details": details } });
        (status, Json(body)).into_response()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => Self::NotFound,
            other => Self::Internal(other.into()),
        }
    }
}

impl From<std::io::Error> for ApiError {
    fn from(err: std::io::Error) -> Self {
        Self::Internal(err.into())
    }
}

impl From<tokio::task::JoinError> for ApiError {
    fn from(err: tokio::task::JoinError) -> Self {
        Self::Internal(err.into())
    }
}

/// 收集欄位錯誤，最後變成一個 VALIDATION 錯誤：
/// `details = { "fields": { "name": "必填" } }`。同一欄位只留第一個訊息。
#[derive(Debug, Default)]
pub struct FieldErrors(BTreeMap<String, String>);

impl FieldErrors {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, field: &str, message: &str) {
        self.0
            .entry(field.to_string())
            .or_insert_with(|| message.to_string());
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn into_error(self) -> ApiError {
        ApiError::Validation {
            message: "輸入資料有誤".to_string(),
            details: json!({ "fields": self.0 }),
        }
    }

    /// 沒錯誤回 Ok(())，有錯誤回 Err(VALIDATION)
    pub fn into_result(self) -> Result<(), ApiError> {
        if self.is_empty() {
            Ok(())
        } else {
            Err(self.into_error())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn body_json(response: Response) -> Value {
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn not_found_envelope() {
        let response = ApiError::NotFound.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let v = body_json(response).await;
        assert_eq!(v["error"]["code"], "NOT_FOUND");
        assert_eq!(v["error"]["message"], "找不到資料");
        assert!(v["error"]["details"].is_null());
    }

    #[tokio::test]
    async fn field_errors_envelope() {
        let mut errors = FieldErrors::new();
        errors.add("name", "必填");
        errors.add("name", "第二個訊息會被忽略");
        let response = errors.into_error().into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let v = body_json(response).await;
        assert_eq!(v["error"]["code"], "VALIDATION");
        assert_eq!(v["error"]["details"]["fields"]["name"], "必填");
    }

    #[test]
    fn empty_field_errors_is_ok() {
        assert!(FieldErrors::new().into_result().is_ok());
    }
}
