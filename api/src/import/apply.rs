//! 把解析結果寫進商品（規格 §13；與規格不同之處 51、54）。
//! 一個商品一個交易（既有 products::create/update）；圖片在交易外先下載

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;
use serde_json::Value;
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
///
/// 先全部驗證再寫入：`validate_all` 用「乾跑」（不建分類、不下載圖片、不觸碰資料庫寫入）
/// 把每個商品的 `products::validate` 與分類名稱都跑過一次，任何一個失敗就整批擋下、什麼都
/// 不寫——驗證錯誤不會造成部分匯入。DB 唯一鍵衝突等只有真的下 INSERT/UPDATE 才查得出來的
/// 錯誤，仍可能讓前面幾個商品已經 commit（那是既有「一個商品一個交易」設計下的極少數例外）。
///
/// 乾跑讀到的既有商品**只用來驗證**，不拿來當寫入的底：整批圖片下載可能跑很久，那段時間
/// 客人下單扣掉的庫存不能被寫回去。所以每個商品都在圖片下載完、真的要寫入之前才重讀一次
/// （`find_by_external_ref`），殘餘的競爭窗口只剩「重讀到 UPDATE」之間的毫秒級。
pub async fn apply(
    db: &PgPool,
    upload_dir: &Path,
    fetcher: &ImageFetcher,
    parsed: &ParsedImport,
) -> Result<ImportResult, ApiError> {
    validate_all(db, &parsed.products).await?;

    let mut result = ImportResult::default();
    let mut category_cache: HashMap<String, Uuid> = HashMap::new();
    for p in parsed.products.iter() {
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
        // 寫入前才重讀庫存快照：下載期間客人下單扣掉的庫存不能被寫回去
        let existing = products::find_by_external_ref(db, &p.external_ref).await?;
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

/// 乾跑：對每個商品用 `products::validate` 檢查一次（分類給 `None`——`validate` 不看
/// `category_id`；圖片給空陣列，所以乾跑不檢查圖片張數上限——那條規則 parse 已經擋過
/// （每個商品的圖片網址 ≤ MAX_IMAGES 個），保留原圖的情況也只是重用既有、已經驗過的圖片列）。
/// 工作表填的**分類名稱**也在這裡先驗一次（`categories::validate_name`），不然名稱太長要等到
/// 寫入迴圈裡 `find_or_create_by_name` 才會失敗，前面的商品早就寫進去了。
/// 任何一個商品沒過就把全部欄位錯誤（各自加上商品編號前綴）收進同一個 `FieldErrors`，整批
/// 擋下、不建分類、不下載圖片、不寫任何一列。這裡讀到的既有商品**只用來乾跑驗證**，不回傳
/// 給寫入用——寫入前會重讀（見 `apply` 的說明）。
async fn validate_all(db: &PgPool, products_in: &[ImportProduct]) -> Result<(), ApiError> {
    let mut errors = FieldErrors::new();
    for p in products_in {
        let existing = products::find_by_external_ref(db, &p.external_ref).await?;
        let dry_input = build_input(p, existing.as_ref(), None, Vec::new());
        if let Err(ApiError::Validation { details, .. }) = products::validate(&dry_input) {
            add_prefixed_fields(&mut errors, &details, &p.external_ref);
        }
        if let Some(name) = &p.category
            && let Err(ApiError::Validation { details, .. }) = categories::validate_name(name)
        {
            add_category_error(&mut errors, &details, &p.external_ref);
        }
    }
    errors.into_result()
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
                // 規格在後台指定的那張圖：只有當它的 path 也在這次要寫入的 images 裡才留得住
                // （整組換新圖時新 path 不同，就變成 None——換圖後要回後台重指定）
                image_path: matched
                    .and_then(|m| m.image_id)
                    .and_then(|id| existing.and_then(|e| e.images.iter().find(|i| i.id == id)))
                    .map(|i| i.path.clone())
                    .filter(|path| images.iter().any(|i| i.path == *path)),
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

/// `ApiError::Validation` 的 `details.fields`（已經序列化好的 JSON object，不是 `FieldErrors`——
/// 所以這裡直接讀 JSON，不需要動 error.rs）逐一加上 `{external_ref}.` 前綴、收進 acc。
/// `annotate` 與 `validate_all` 共用，欄位錯誤不管來自哪個商品都合併得起來。
fn add_prefixed_fields(acc: &mut FieldErrors, details: &Value, external_ref: &str) {
    if let Some(fields) = details.get("fields").and_then(|v| v.as_object()) {
        for (k, v) in fields {
            if let Some(msg) = v.as_str() {
                acc.add(&format!("{external_ref}.{k}"), msg);
            }
        }
    }
}

/// `categories::validate_name` 回的欄位名是 `name`，直接前綴會跟商品名稱的錯誤撞鍵
/// （`FieldErrors::add` 只留先進來的那一個），所以分類的錯誤明確掛在 `{external_ref}.category`，
/// 訊息沿用 `validate_name` 的文案。
fn add_category_error(acc: &mut FieldErrors, details: &Value, external_ref: &str) {
    if let Some(fields) = details.get("fields").and_then(|v| v.as_object())
        && let Some(msg) = fields.values().find_map(|v| v.as_str())
    {
        acc.add(&format!("{external_ref}.category"), msg);
    }
}

/// products::validate／DB 撞唯一鍵的欄位錯誤加上商品編號，老闆才知道是哪一列。非欄位錯誤
/// （例如資料庫連線問題的 `Internal`）原樣往上丟，不能被吞成假的 400。
fn annotate(err: ApiError, p: &ImportProduct) -> ApiError {
    match err {
        ApiError::Validation { details, .. } => {
            let mut out = FieldErrors::new();
            add_prefixed_fields(&mut out, &details, &p.external_ref);
            out.into_error()
        }
        other => other,
    }
}
