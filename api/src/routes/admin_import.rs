//! 商品匯入（規格 §10、§13；與規格不同之處 49、50）

use axum::{
    Json, Router,
    extract::{Multipart, State},
    routing::post,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{
    auth::extract::AdminUser,
    domain::products,
    error::{ApiError, ApiResult},
    extract::AppMultipart,
    import::{self, ImageFetcher, ImportResult, ParsedImport, parse_xlsx},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/import/preview", post(preview))
        .route("/api/admin/import/commit", post(commit))
}

#[derive(Serialize)]
pub struct PreviewResponse {
    pub fingerprint: String,
    pub product_count: usize,
    pub variant_count: usize,
    pub new_count: usize,
    pub update_count: usize,
    pub parsed: ParsedImport,
}

#[derive(Serialize)]
pub struct CommitResponse {
    pub fingerprint: String,
    pub result: ImportResult,
}

struct Upload {
    file: Vec<u8>,
    fingerprint_field: Option<String>,
}

/// 讀 multipart：file 必填（≤ MAX_XLSX_BYTES）、fingerprint 選填。非 multipart 請求（或缺
/// `file` 欄位以外的擷取錯誤）由 `AppMultipart` 擷取器本身轉成 VALIDATION（照 routes/uploads.rs）。
async fn read_upload(mut multipart: Multipart) -> Result<Upload, ApiError> {
    let mut file: Option<Vec<u8>> = None;
    let mut fingerprint_field: Option<String> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| ApiError::field("file", "上傳格式錯誤"))?
    {
        match field.name().unwrap_or("") {
            "file" => {
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|_| ApiError::field("file", "上傳格式錯誤"))?;
                if bytes.len() > import::MAX_XLSX_BYTES {
                    return Err(ApiError::field("file", "檔案超過 5 MB"));
                }
                file = Some(bytes.to_vec());
            }
            "fingerprint" => {
                fingerprint_field = Some(
                    field
                        .text()
                        .await
                        .map_err(|_| ApiError::field("fingerprint", "格式錯誤"))?
                        .trim()
                        .to_string(),
                );
            }
            _ => {}
        }
    }
    let file = file
        .filter(|f| !f.is_empty())
        .ok_or_else(|| ApiError::field("file", "請選擇 xlsx 檔案"))?;
    Ok(Upload {
        file,
        fingerprint_field,
    })
}

fn fingerprint(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

async fn parse_and_count(state: &AppState, bytes: &[u8]) -> Result<PreviewResponse, ApiError> {
    let parsed = parse_xlsx(bytes).map_err(|e| ApiError::field("file", &e.to_string()))?;
    let refs: Vec<String> = parsed
        .products
        .iter()
        .map(|p| p.external_ref.clone())
        .collect();
    let existing = products::existing_external_refs(&state.db, &refs).await?;
    let update_count = refs.iter().filter(|r| existing.contains(*r)).count();
    Ok(PreviewResponse {
        fingerprint: fingerprint(bytes),
        product_count: parsed.products.len(),
        variant_count: parsed.variant_count(),
        new_count: refs.len() - update_count,
        update_count,
        parsed,
    })
}

async fn preview(
    _admin: AdminUser,
    State(state): State<AppState>,
    AppMultipart(multipart): AppMultipart,
) -> ApiResult<Json<PreviewResponse>> {
    let upload = read_upload(multipart).await?;
    Ok(Json(parse_and_count(&state, &upload.file).await?))
}

async fn commit(
    _admin: AdminUser,
    State(state): State<AppState>,
    AppMultipart(multipart): AppMultipart,
) -> ApiResult<Json<CommitResponse>> {
    let upload = read_upload(multipart).await?;
    let expected = upload
        .fingerprint_field
        .clone()
        .ok_or_else(|| ApiError::field("fingerprint", "請先預覽"))?;
    let preview = parse_and_count(&state, &upload.file).await?;
    if preview.fingerprint != expected {
        return Err(ApiError::field("fingerprint", "檔案已變更，請重新預覽"));
    }
    if !preview.parsed.errors.is_empty() {
        return Err(ApiError::field(
            "rows",
            &format!(
                "檔案有 {} 列錯誤，請先修正再匯入",
                preview.parsed.errors.len()
            ),
        ));
    }
    let fetcher = ImageFetcher::new()?;
    let result = import::apply(
        &state.db,
        &state.config.upload_dir,
        &fetcher,
        &preview.parsed,
    )
    .await?;
    tracing::info!(
        created = result.created,
        updated = result.updated,
        warnings = result.warnings.len(),
        "商品匯入完成"
    );
    Ok(Json(CommitResponse {
        fingerprint: preview.fingerprint,
        result,
    }))
}
