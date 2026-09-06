use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::domain::categories::{is_valid_slug, random_slug};
use crate::error::{ApiError, FieldErrors};

pub const STATUS_DRAFT: &str = "draft";
pub const STATUS_ACTIVE: &str = "active";
pub const STATUS_ARCHIVED: &str = "archived";
pub const MAX_IMAGES: usize = 9;
pub const MAX_VARIANTS: usize = 100;

// ───── 資料列 ─────

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ProductRow {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub category_id: Option<Uuid>,
    pub status: String,
    pub option1_name: Option<String>,
    pub option2_name: Option<String>,
    pub external_ref: Option<String>,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct VariantRow {
    pub id: Uuid,
    pub product_id: Uuid,
    pub option1_value: Option<String>,
    pub option2_value: Option<String>,
    pub sku: Option<String>,
    pub price: i32,
    pub compare_at_price: Option<i32>,
    pub stock: i32,
    pub is_active: bool,
    pub image_id: Option<Uuid>,
    pub sort_order: i32,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ImageRow {
    pub id: Uuid,
    pub product_id: Uuid,
    pub path: String,
    pub thumb_path: String,
    pub alt: String,
    pub sort_order: i32,
}

#[derive(Debug, Serialize)]
pub struct AdminProduct {
    #[serde(flatten)]
    pub product: ProductRow,
    pub variants: Vec<VariantRow>,
    pub images: Vec<ImageRow>,
}

#[derive(Debug, Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AdminListItem {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub status: String,
    pub category_name: Option<String>,
    pub price_min: Option<i32>,
    pub price_max: Option<i32>,
    pub stock_total: i64,
    pub image_thumb: Option<String>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip)]
    pub total: i64,
}

// ───── 輸入 ─────

#[derive(Debug, Deserialize)]
pub struct ProductInput {
    pub name: String,
    pub slug: Option<String>,
    pub description: Option<String>,
    pub category_id: Option<Uuid>,
    pub status: String,
    pub option1_name: Option<String>,
    pub option2_name: Option<String>,
    pub sort_order: Option<i32>,
    #[serde(default)]
    pub variants: Vec<VariantInput>,
    #[serde(default)]
    pub images: Vec<ImageInput>,
}

#[derive(Debug, Deserialize)]
pub struct VariantInput {
    /// 有 id 且屬於這個商品 → 更新；否則新增
    pub id: Option<Uuid>,
    pub option1_value: Option<String>,
    pub option2_value: Option<String>,
    pub sku: Option<String>,
    pub price: i32,
    pub compare_at_price: Option<i32>,
    pub stock: i32,
    pub is_active: Option<bool>,
    /// 對應 images[].path；伺服器換成 image_id
    pub image_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ImageInput {
    pub path: String,
    pub thumb_path: String,
    pub alt: Option<String>,
}

/// 去頭尾空白；空字串當作沒填
pub fn clean(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// page 夾在 1..=10_000（上限避免 (page - 1) * per_page 溢位或變負數）；per_page 夾在 1..=max
pub fn clamp_paging(
    page: Option<i64>,
    per_page: Option<i64>,
    default_per_page: i64,
    max_per_page: i64,
) -> (i64, i64) {
    (
        page.unwrap_or(1).clamp(1, 10_000),
        per_page.unwrap_or(default_per_page).clamp(1, max_per_page),
    )
}

pub fn validate(input: &ProductInput) -> Result<(), ApiError> {
    let mut errors = FieldErrors::new();
    let name_len = input.name.trim().chars().count();
    if name_len == 0 || name_len > 120 {
        errors.add("name", "必填，最多 120 字");
    }
    if !matches!(
        input.status.as_str(),
        STATUS_DRAFT | STATUS_ACTIVE | STATUS_ARCHIVED
    ) {
        errors.add("status", "狀態只能是 draft、active 或 archived");
    }
    if let Some(slug) = clean(&input.slug)
        && !is_valid_slug(&slug)
    {
        errors.add("slug", "網址代稱只能用小寫英文、數字和 -（1～60 字）");
    }
    if input.variants.is_empty() {
        errors.add("variants", "至少要有一個規格");
    }
    if input.variants.len() > MAX_VARIANTS {
        errors.add("variants", "規格最多 100 個");
    }
    if input.images.len() > MAX_IMAGES {
        errors.add("images", "圖片最多 9 張");
    }
    let has_opt1 = clean(&input.option1_name).is_some();
    let has_opt2 = clean(&input.option2_name).is_some();
    if has_opt2 && !has_opt1 {
        errors.add("option2_name", "要先有規格 1 才能有規格 2");
    }
    if !has_opt1 && input.variants.len() > 1 {
        errors.add("variants", "沒有規格名稱時只能有一個規格");
    }
    let mut seen: HashSet<(String, String)> = HashSet::new();
    for (i, v) in input.variants.iter().enumerate() {
        if v.price < 0 {
            errors.add(&format!("variants.{i}.price"), "價格不能是負數");
        }
        if v.stock < 0 {
            errors.add(&format!("variants.{i}.stock"), "庫存不能是負數");
        }
        if v.compare_at_price.is_some_and(|p| p < 0) {
            errors.add(&format!("variants.{i}.compare_at_price"), "原價不能是負數");
        }
        let v1 = clean(&v.option1_value);
        let v2 = clean(&v.option2_value);
        if has_opt1 && v1.is_none() {
            errors.add(&format!("variants.{i}.option1_value"), "必填");
        }
        if has_opt2 && v2.is_none() {
            errors.add(&format!("variants.{i}.option2_value"), "必填");
        }
        if !seen.insert((v1.unwrap_or_default(), v2.unwrap_or_default())) {
            errors.add(&format!("variants.{i}"), "規格重複");
        }
        if let Some(path) = v.image_path.as_deref()
            && !input.images.iter().any(|img| img.path == path)
        {
            errors.add(
                &format!("variants.{i}.image_path"),
                "圖片不在這個商品的圖片清單裡",
            );
        }
    }
    for (i, img) in input.images.iter().enumerate() {
        if !img.path.starts_with("/uploads/") || !img.thumb_path.starts_with("/uploads/") {
            errors.add(&format!("images.{i}"), "圖片路徑不正確");
        }
    }
    errors.into_result()
}

/// 資料庫錯誤 → 我們的錯誤：slug 撞 unique、category 不存在（FK）；其他往上丟
fn map_product_db_error(err: sqlx::Error) -> ApiError {
    if let sqlx::Error::Database(e) = &err {
        if e.is_unique_violation() {
            return ApiError::field("slug", "這個網址代稱已經有人用了");
        }
        if e.is_foreign_key_violation() {
            return ApiError::field("category_id", "分類不存在");
        }
    }
    err.into()
}

pub async fn create(db: &PgPool, input: ProductInput) -> Result<AdminProduct, ApiError> {
    validate(&input)?;
    let id = Uuid::now_v7();
    let slug = clean(&input.slug).unwrap_or_else(random_slug);
    let mut tx = db.begin().await?;
    sqlx::query(
        "INSERT INTO products (id, slug, name, description, category_id, status, option1_name, option2_name, sort_order)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(id)
    .bind(&slug)
    .bind(input.name.trim())
    .bind(input.description.as_deref().unwrap_or(""))
    .bind(input.category_id)
    .bind(&input.status)
    .bind(clean(&input.option1_name))
    .bind(clean(&input.option2_name))
    .bind(input.sort_order.unwrap_or(0))
    .execute(&mut *tx)
    .await
    .map_err(map_product_db_error)?;
    write_images_and_variants(&mut tx, id, &input).await?;
    tx.commit().await?;
    get_admin(db, id).await?.ok_or(ApiError::NotFound)
}

pub async fn update(db: &PgPool, id: Uuid, input: ProductInput) -> Result<AdminProduct, ApiError> {
    validate(&input)?;
    let slug = clean(&input.slug).unwrap_or_else(random_slug);
    let mut tx = db.begin().await?;
    let result = sqlx::query(
        "UPDATE products SET slug = $2, name = $3, description = $4, category_id = $5, status = $6,
                option1_name = $7, option2_name = $8, sort_order = $9, updated_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .bind(&slug)
    .bind(input.name.trim())
    .bind(input.description.as_deref().unwrap_or(""))
    .bind(input.category_id)
    .bind(&input.status)
    .bind(clean(&input.option1_name))
    .bind(clean(&input.option2_name))
    .bind(input.sort_order.unwrap_or(0))
    .execute(&mut *tx)
    .await
    .map_err(map_product_db_error)?;
    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    write_images_and_variants(&mut tx, id, &input).await?;
    tx.commit().await?;
    get_admin(db, id).await?.ok_or(ApiError::NotFound)
}

/// 圖片整組重建；規格有 id 的更新、沒有的新增、沒出現的刪除。
/// 計畫 2 建了 order_items 之後，這裡的刪除要改成：有訂單引用就 is_active=false，沒有才刪。
async fn write_images_and_variants(
    tx: &mut Transaction<'_, Postgres>,
    product_id: Uuid,
    input: &ProductInput,
) -> Result<(), ApiError> {
    // 圖片：先清掉再依順序寫入（variants.image_id 會因 FK ON DELETE SET NULL 先變空，下面再補回）
    sqlx::query("DELETE FROM product_images WHERE product_id = $1")
        .bind(product_id)
        .execute(&mut **tx)
        .await?;
    let mut image_id_by_path: HashMap<&str, Uuid> = HashMap::new();
    for (i, img) in input.images.iter().enumerate() {
        let image_id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO product_images (id, product_id, path, thumb_path, alt, sort_order) VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(image_id)
        .bind(product_id)
        .bind(&img.path)
        .bind(&img.thumb_path)
        .bind(img.alt.as_deref().unwrap_or(""))
        .bind(i as i32)
        .execute(&mut **tx)
        .await?;
        image_id_by_path.insert(img.path.as_str(), image_id);
    }

    // 規格
    let has_opt1 = clean(&input.option1_name).is_some();
    let has_opt2 = clean(&input.option2_name).is_some();
    let existing: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM product_variants WHERE product_id = $1")
            .bind(product_id)
            .fetch_all(&mut **tx)
            .await?;
    let mut kept: Vec<Uuid> = Vec::with_capacity(input.variants.len());
    for (i, v) in input.variants.iter().enumerate() {
        let image_id = v
            .image_path
            .as_deref()
            .and_then(|p| image_id_by_path.get(p))
            .copied();
        let option1_value = if has_opt1 {
            clean(&v.option1_value)
        } else {
            None
        };
        let option2_value = if has_opt2 {
            clean(&v.option2_value)
        } else {
            None
        };
        let sku = clean(&v.sku);
        let is_active = v.is_active.unwrap_or(true);
        let variant_id = match v.id.filter(|id| existing.contains(id)) {
            Some(id) => {
                sqlx::query(
                    "UPDATE product_variants SET option1_value = $2, option2_value = $3, sku = $4, price = $5,
                            compare_at_price = $6, stock = $7, is_active = $8, image_id = $9, sort_order = $10
                     WHERE id = $1",
                )
                .bind(id)
                .bind(&option1_value)
                .bind(&option2_value)
                .bind(&sku)
                .bind(v.price)
                .bind(v.compare_at_price)
                .bind(v.stock)
                .bind(is_active)
                .bind(image_id)
                .bind(i as i32)
                .execute(&mut **tx)
                .await?;
                id
            }
            None => {
                let id = Uuid::now_v7();
                sqlx::query(
                    "INSERT INTO product_variants (id, product_id, option1_value, option2_value, sku, price,
                            compare_at_price, stock, is_active, image_id, sort_order)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
                )
                .bind(id)
                .bind(product_id)
                .bind(&option1_value)
                .bind(&option2_value)
                .bind(&sku)
                .bind(v.price)
                .bind(v.compare_at_price)
                .bind(v.stock)
                .bind(is_active)
                .bind(image_id)
                .bind(i as i32)
                .execute(&mut **tx)
                .await?;
                id
            }
        };
        kept.push(variant_id);
    }
    for old in existing.iter().filter(|id| !kept.contains(id)) {
        sqlx::query("DELETE FROM product_variants WHERE id = $1")
            .bind(old)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

pub async fn get_admin(db: &PgPool, id: Uuid) -> Result<Option<AdminProduct>, ApiError> {
    let Some(product) = sqlx::query_as::<_, ProductRow>("SELECT * FROM products WHERE id = $1")
        .bind(id)
        .fetch_optional(db)
        .await?
    else {
        return Ok(None);
    };
    let variants = sqlx::query_as::<_, VariantRow>(
        "SELECT * FROM product_variants WHERE product_id = $1 ORDER BY sort_order, id",
    )
    .bind(id)
    .fetch_all(db)
    .await?;
    let images = sqlx::query_as::<_, ImageRow>(
        "SELECT * FROM product_images WHERE product_id = $1 ORDER BY sort_order, id",
    )
    .bind(id)
    .fetch_all(db)
    .await?;
    Ok(Some(AdminProduct {
        product,
        variants,
        images,
    }))
}

/// 刪除 = 封存（規格 §3）。回 false 表示沒這個商品。
pub async fn archive(db: &PgPool, id: Uuid) -> Result<bool, ApiError> {
    let result =
        sqlx::query("UPDATE products SET status = 'archived', updated_at = now() WHERE id = $1")
            .bind(id)
            .execute(db)
            .await?;
    Ok(result.rows_affected() > 0)
}

/// 後台列表。沒給 status 時不含 archived；q 比對名稱（不分大小寫、部分相符）。
pub async fn list_admin(
    db: &PgPool,
    q: Option<&str>,
    status: Option<&str>,
    page: i64,
    per_page: i64,
) -> Result<Page<AdminListItem>, ApiError> {
    let items = sqlx::query_as::<_, AdminListItem>(
        "SELECT p.id, p.slug, p.name, p.status, c.name AS category_name,
                v.price_min, v.price_max, v.stock_total,
                (SELECT i.thumb_path FROM product_images i WHERE i.product_id = p.id ORDER BY i.sort_order, i.id LIMIT 1) AS image_thumb,
                p.updated_at,
                COUNT(*) OVER () AS total
         FROM products p
         LEFT JOIN categories c ON c.id = p.category_id
         LEFT JOIN LATERAL (
            SELECT MIN(pv.price) AS price_min, MAX(pv.price) AS price_max, COALESCE(SUM(pv.stock), 0)::bigint AS stock_total
            FROM product_variants pv WHERE pv.product_id = p.id
         ) v ON true
         WHERE ($1::text IS NULL OR p.name ILIKE '%' || $1 || '%')
           AND ($2::text IS NULL OR p.status = $2)
           AND ($2::text IS NOT NULL OR p.status <> 'archived')
         ORDER BY p.updated_at DESC, p.id DESC
         LIMIT $3 OFFSET $4",
    )
    .bind(q)
    .bind(status)
    .bind(per_page)
    .bind((page - 1) * per_page)
    .fetch_all(db)
    .await?;
    let total = items.first().map(|i| i.total).unwrap_or(0);
    Ok(Page {
        items,
        total,
        page,
        per_page,
    })
}

// ───── 公開（買家）查詢 ─────

pub const SORT_OPTIONS: &[&str] = &["newest", "price_asc", "price_desc"];

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PublicListItem {
    pub slug: String,
    pub name: String,
    pub price_min: i32,
    pub price_max: i32,
    pub image_thumb: Option<String>,
    pub in_stock: bool,
    #[serde(skip)]
    pub total: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct CategoryRef {
    pub slug: String,
    pub name: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PublicImage {
    pub path: String,
    pub thumb_path: String,
    pub alt: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PublicVariant {
    pub id: Uuid,
    pub option1_value: Option<String>,
    pub option2_value: Option<String>,
    pub price: i32,
    pub compare_at_price: Option<i32>,
    pub stock: i32,
    pub image_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PublicProduct {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub category: Option<CategoryRef>,
    pub option1_name: Option<String>,
    pub option2_name: Option<String>,
    pub images: Vec<PublicImage>,
    pub variants: Vec<PublicVariant>,
}

/// 買家列表：只有 active 且至少一個啟用規格的商品。價格範圍與庫存只算啟用的規格。
/// sort 只接受 SORT_OPTIONS 裡的值（route 先驗證過），這裡用白名單組 ORDER BY。
pub async fn list_public(
    db: &PgPool,
    q: Option<&str>,
    category_slug: Option<&str>,
    sort: &str,
    page: i64,
    per_page: i64,
) -> Result<Page<PublicListItem>, ApiError> {
    let order = match sort {
        "price_asc" => "v.price_min ASC, p.created_at DESC",
        "price_desc" => "v.price_max DESC, p.created_at DESC",
        _ => "p.created_at DESC",
    };
    let sql = format!(
        "SELECT p.slug, p.name, v.price_min, v.price_max, (v.stock_total > 0) AS in_stock,
                (SELECT i.thumb_path FROM product_images i WHERE i.product_id = p.id ORDER BY i.sort_order, i.id LIMIT 1) AS image_thumb,
                COUNT(*) OVER () AS total
         FROM products p
         JOIN LATERAL (
            SELECT MIN(pv.price) AS price_min, MAX(pv.price) AS price_max, COALESCE(SUM(pv.stock), 0)::bigint AS stock_total
            FROM product_variants pv WHERE pv.product_id = p.id AND pv.is_active
         ) v ON true
         WHERE p.status = 'active'
           AND v.price_min IS NOT NULL
           AND ($1::text IS NULL OR p.name ILIKE '%' || $1 || '%')
           AND ($2::text IS NULL OR p.category_id = (SELECT c.id FROM categories c WHERE c.slug = $2))
         ORDER BY {order}, p.id DESC
         LIMIT $3 OFFSET $4"
    );
    // sqlx 0.9 只接受 &'static str 或 AssertSqlSafe 包住的字串；order 來自上面的白名單，沒有使用者輸入
    let items = sqlx::query_as::<_, PublicListItem>(sqlx::AssertSqlSafe(sql))
        .bind(q)
        .bind(category_slug)
        .bind(per_page)
        .bind((page - 1) * per_page)
        .fetch_all(db)
        .await?;
    let total = items.first().map(|i| i.total).unwrap_or(0);
    Ok(Page {
        items,
        total,
        page,
        per_page,
    })
}

/// 買家商品頁：只回 active 商品、啟用的規格；規格的 image_path 由 image_id 對出來
pub async fn get_public(db: &PgPool, slug: &str) -> Result<Option<PublicProduct>, ApiError> {
    let Some(p) = sqlx::query_as::<_, ProductRow>(
        "SELECT * FROM products WHERE slug = $1 AND status = 'active'",
    )
    .bind(slug)
    .fetch_optional(db)
    .await?
    else {
        return Ok(None);
    };
    let category = match p.category_id {
        Some(category_id) => {
            sqlx::query_as::<_, CategoryRef>("SELECT slug, name FROM categories WHERE id = $1")
                .bind(category_id)
                .fetch_optional(db)
                .await?
        }
        None => None,
    };
    let images = sqlx::query_as::<_, PublicImage>(
        "SELECT path, thumb_path, alt FROM product_images WHERE product_id = $1 ORDER BY sort_order, id",
    )
    .bind(p.id)
    .fetch_all(db)
    .await?;
    let variants = sqlx::query_as::<_, PublicVariant>(
        "SELECT v.id, v.option1_value, v.option2_value, v.price, v.compare_at_price, v.stock, i.path AS image_path
         FROM product_variants v
         LEFT JOIN product_images i ON i.id = v.image_id
         WHERE v.product_id = $1 AND v.is_active
         ORDER BY v.sort_order, v.id",
    )
    .bind(p.id)
    .fetch_all(db)
    .await?;
    Ok(Some(PublicProduct {
        id: p.id,
        slug: p.slug,
        name: p.name,
        description: p.description,
        category,
        option1_name: p.option1_name,
        option2_name: p.option2_name,
        images,
        variants,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_input() -> ProductInput {
        ProductInput {
            name: "狗糧".to_string(),
            slug: None,
            description: None,
            category_id: None,
            status: STATUS_DRAFT.to_string(),
            option1_name: None,
            option2_name: None,
            sort_order: None,
            variants: vec![VariantInput {
                id: None,
                option1_value: None,
                option2_value: None,
                sku: None,
                price: 100,
                compare_at_price: None,
                stock: 1,
                is_active: None,
                image_path: None,
            }],
            images: vec![],
        }
    }

    fn fields(err: ApiError) -> serde_json::Value {
        match err {
            ApiError::Validation { details, .. } => details["fields"].clone(),
            other => panic!("expected validation error, got {other:?}"),
        }
    }

    #[test]
    fn minimal_product_is_valid() {
        assert!(validate(&base_input()).is_ok());
    }

    #[test]
    fn needs_name_status_and_a_variant() {
        let mut input = base_input();
        input.name = "  ".to_string();
        input.status = "gone".to_string();
        input.variants.clear();
        let f = fields(validate(&input).unwrap_err());
        assert!(f["name"].is_string());
        assert!(f["status"].is_string());
        assert!(f["variants"].is_string());
    }

    #[test]
    fn options_must_be_consistent() {
        let mut input = base_input();
        input.option2_name = Some("尺寸".to_string());
        let f = fields(validate(&input).unwrap_err());
        assert!(f["option2_name"].is_string());

        let mut input = base_input();
        input.option1_name = Some("口味".to_string());
        input.variants.push(VariantInput {
            option1_value: Some("雞".to_string()),
            ..base_input().variants.remove(0)
        });
        let f = fields(validate(&input).unwrap_err());
        assert_eq!(f["variants.0.option1_value"], "必填");
    }

    #[test]
    fn duplicate_variants_and_negative_numbers() {
        let mut input = base_input();
        input.option1_name = Some("口味".to_string());
        let first = VariantInput {
            option1_value: Some("雞".to_string()),
            price: -1,
            stock: -2,
            ..base_input().variants.remove(0)
        };
        let second = VariantInput {
            option1_value: Some("雞".to_string()),
            ..base_input().variants.remove(0)
        };
        input.variants = vec![first, second];
        let f = fields(validate(&input).unwrap_err());
        assert!(f["variants.0.price"].is_string());
        assert!(f["variants.0.stock"].is_string());
        assert_eq!(f["variants.1"], "規格重複");
    }

    #[test]
    fn image_rules() {
        let mut input = base_input();
        input.images = (0..10)
            .map(|i| ImageInput {
                path: format!("/uploads/a{i}.jpg"),
                thumb_path: format!("/uploads/a{i}_t.jpg"),
                alt: None,
            })
            .collect();
        input.variants[0].image_path = Some("/uploads/nope.jpg".to_string());
        let f = fields(validate(&input).unwrap_err());
        assert_eq!(f["images"], "圖片最多 9 張");
        assert!(f["variants.0.image_path"].is_string());

        let mut input = base_input();
        input.images = vec![ImageInput {
            path: "http://evil/x.jpg".to_string(),
            thumb_path: "/uploads/t.jpg".to_string(),
            alt: None,
        }];
        let f = fields(validate(&input).unwrap_err());
        assert_eq!(f["images.0"], "圖片路徑不正確");
    }

    #[test]
    fn paging_clamps() {
        assert_eq!(clamp_paging(None, None, 24, 60), (1, 24));
        assert_eq!(clamp_paging(Some(0), Some(999), 24, 60), (1, 60));
        assert_eq!(clamp_paging(Some(3), Some(0), 24, 60), (3, 1));
    }
}
