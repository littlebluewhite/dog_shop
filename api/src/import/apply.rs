//! 把解析結果寫進商品（規格 §13；與規格不同之處 51、54）。
//! 一個商品一個交易（既有 products::create/update）；圖片在交易外先下載

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::categories;
use crate::domain::products::{
    self, AdminProduct, ImageInput, ProductInput, STATUS_DRAFT, VariantInput, clean,
};
use crate::error::{ApiError, FieldErrors};
use crate::import::images::{ImageFetcher, fetch_all};
use crate::import::parse::{ImportProduct, ParsedImport};

#[derive(Debug, Clone, Serialize)]
pub struct ImportWarning {
    pub external_ref: String,
    pub row: u32,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportedProduct {
    pub external_ref: String,
    pub id: Uuid,
    pub name: String,
    pub created: bool,
    pub images: usize,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ImportResult {
    pub created: usize,
    pub updated: usize,
    pub products: Vec<ImportedProduct>,
    pub warnings: Vec<ImportWarning>,
}

/// 全部商品逐一寫入。呼叫者保證 parsed.errors 是空的。
pub async fn apply(
    db: &PgPool,
    upload_dir: &Path,
    fetcher: &ImageFetcher,
    parsed: &ParsedImport,
) -> Result<ImportResult, ApiError> {
    let mut result = ImportResult::default();
    let mut category_cache: HashMap<String, Uuid> = HashMap::new();
    for p in &parsed.products {
        let category_id = match &p.category {
            None => None,
            Some(name) => match category_cache.get(name) {
                Some(id) => Some(*id),
                None => {
                    let c = categories::find_or_create_by_name(db, name).await?;
                    category_cache.insert(name.clone(), c.id);
                    Some(c.id)
                }
            },
        };
        let existing = products::find_by_external_ref(db, &p.external_ref).await?;

        // 圖片：先下載，失敗只記警告；更新時全部失敗（或沒填）就保留原圖
        let mut images: Vec<ImageInput> = Vec::new();
        for (url, r) in fetch_all(fetcher, upload_dir, &p.image_urls).await {
            match r {
                Ok(stored) => images.push(ImageInput {
                    path: stored.path,
                    thumb_path: stored.thumb_path,
                    alt: Some(p.name.clone()),
                }),
                Err(e) => result.warnings.push(ImportWarning {
                    external_ref: p.external_ref.clone(),
                    row: p.first_row,
                    message: format!("圖片 {url} 沒有匯入：{e}"),
                }),
            }
        }
        if images.is_empty()
            && let Some(ex) = &existing
        {
            images = ex
                .images
                .iter()
                .map(|i| ImageInput {
                    path: i.path.clone(),
                    thumb_path: i.thumb_path.clone(),
                    alt: Some(i.alt.clone()),
                })
                .collect();
        }

        let input = build_input(p, existing.as_ref(), category_id, images);
        let (saved, created) = match &existing {
            Some(ex) => (
                products::update(db, ex.product.id, input)
                    .await
                    .map_err(|e| annotate(e, p))?,
                false,
            ),
            None => (
                products::create(db, input)
                    .await
                    .map_err(|e| annotate(e, p))?,
                true,
            ),
        };
        if created {
            result.created += 1
        } else {
            result.updated += 1
        }
        result.products.push(ImportedProduct {
            external_ref: p.external_ref.clone(),
            id: saved.product.id,
            name: saved.product.name.clone(),
            created,
            images: saved.images.len(),
        });
    }
    Ok(result)
}

/// 既有商品當底，只覆蓋工作表有填的欄位（與規格不同之處 51）
fn build_input(
    p: &ImportProduct,
    existing: Option<&AdminProduct>,
    category_id: Option<Uuid>,
    images: Vec<ImageInput>,
) -> ProductInput {
    let ex = existing.map(|e| &e.product);
    let variants = p
        .variants
        .iter()
        .map(|v| {
            let matched = existing.and_then(|e| {
                e.variants.iter().find(|r| {
                    clean(&r.option1_value) == clean(&v.option1_value)
                        && clean(&r.option2_value) == clean(&v.option2_value)
                })
            });
            VariantInput {
                id: matched.map(|m| m.id),
                option1_value: v.option1_value.clone(),
                option2_value: v.option2_value.clone(),
                sku: v
                    .sku
                    .clone()
                    .or_else(|| matched.and_then(|m| m.sku.clone())),
                price: v.price,
                compare_at_price: matched.and_then(|m| m.compare_at_price),
                stock: v.stock.or_else(|| matched.map(|m| m.stock)).unwrap_or(0),
                is_active: matched.map(|m| m.is_active),
                image_path: None,
            }
        })
        .collect();
    ProductInput {
        name: p.name.clone(),
        slug: ex.map(|e| e.slug.clone()),
        description: p
            .description
            .clone()
            .or_else(|| ex.map(|e| e.description.clone())),
        category_id: category_id.or_else(|| ex.and_then(|e| e.category_id)),
        status: ex
            .map(|e| e.status.clone())
            .unwrap_or_else(|| STATUS_DRAFT.to_string()),
        option1_name: p.option1_name.clone(),
        option2_name: p.option2_name.clone(),
        sort_order: ex.map(|e| e.sort_order),
        variants,
        images,
        external_ref: Some(p.external_ref.clone()),
    }
}

/// products::validate／DB 撞唯一鍵的欄位錯誤加上商品編號，老闆才知道是哪一列。
/// `ApiError::Validation` 存的是已經序列化好的 `details: Value`（不是 `FieldErrors`），
/// 所以這裡直接從 `details.fields` 這個 JSON object 重建，不需要動 error.rs。
fn annotate(err: ApiError, p: &ImportProduct) -> ApiError {
    match err {
        ApiError::Validation { details, .. } => {
            let mut out = FieldErrors::new();
            if let Some(fields) = details.get("fields").and_then(|v| v.as_object()) {
                for (k, v) in fields {
                    if let Some(msg) = v.as_str() {
                        out.add(&format!("{}.{k}", p.external_ref), msg);
                    }
                }
            }
            out.into_error()
        }
        other => other,
    }
}
