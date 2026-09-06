use axum::{
    Json, Router,
    extract::{Multipart, State},
    http::StatusCode,
    routing::post,
};
use serde_json::json;

use crate::{
    auth::extract::AdminUser,
    error::{ApiError, ApiResult},
    state::AppState,
    storage::{self, ALLOWED_MIME, MAX_UPLOAD_BYTES, StorageError, StoredImage},
};

pub fn router() -> Router<AppState> {
    Router::new().route("/api/admin/uploads", post(upload))
}

/// multipart，欄位名 `file`。只收 jpeg/png/webp/gif、≤ 10 MB（規格 §10、§11）
async fn upload(
    _admin: AdminUser,
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> ApiResult<(StatusCode, Json<StoredImage>)> {
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::Validation {
            message: "上傳內容格式錯誤".to_string(),
            details: json!({ "detail": e.body_text() }),
        })?
    {
        if field.name() != Some("file") {
            continue;
        }
        let content_type = field.content_type().unwrap_or("").to_string();
        if !ALLOWED_MIME.contains(&content_type.as_str()) {
            return Err(ApiError::field("file", "只接受 JPEG、PNG、WebP、GIF"));
        }
        let bytes = field.bytes().await.map_err(|e| ApiError::Validation {
            message: "讀取上傳檔失敗".to_string(),
            details: json!({ "detail": e.body_text() }),
        })?;
        if bytes.len() > MAX_UPLOAD_BYTES {
            return Err(ApiError::field("file", "檔案不能超過 10 MB"));
        }
        let stored = storage::save(&state.config.upload_dir, bytes.to_vec())
            .await
            .map_err(|e| match e {
                StorageError::Decode(_) => ApiError::field("file", "圖片無法讀取"),
                StorageError::Io(io) => ApiError::Internal(io.into()),
            })?;
        return Ok((StatusCode::CREATED, Json(stored)));
    }
    Err(ApiError::field("file", "缺少 file 欄位"))
}
