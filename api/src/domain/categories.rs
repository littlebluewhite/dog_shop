use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{ApiError, FieldErrors};

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Category {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub sort_order: i32,
}

/// slug 規則：小寫英數字與 -，1～60 字，頭尾不是 -
pub fn is_valid_slug(slug: &str) -> bool {
    (1..=60).contains(&slug.len())
        && slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !slug.starts_with('-')
        && !slug.ends_with('-')
}

/// 8 碼隨機小寫英數字（規格 §3：取 UUID v4 的前 8 個 hex 字元）
pub fn random_slug() -> String {
    Uuid::new_v4().simple().to_string()[..8].to_string()
}

/// 使用者有給 slug 就檢查格式，沒給（None 或空白）就隨機
pub fn resolve_slug(input: Option<&str>) -> Result<String, ApiError> {
    match input.map(str::trim).filter(|s| !s.is_empty()) {
        Some(slug) if is_valid_slug(slug) => Ok(slug.to_string()),
        Some(_) => Err(ApiError::field(
            "slug",
            "網址代稱只能用小寫英文、數字和 -（1～60 字）",
        )),
        None => Ok(random_slug()),
    }
}

fn validate_name(name: &str) -> Result<(), ApiError> {
    let mut errors = FieldErrors::new();
    let len = name.trim().chars().count();
    if len == 0 || len > 50 {
        errors.add("name", "必填，最多 50 字");
    }
    errors.into_result()
}

/// slug 撞到 unique index → 欄位錯誤；其他錯誤照原樣往上丟
fn map_slug_conflict<T>(result: Result<T, sqlx::Error>) -> Result<T, ApiError> {
    match result {
        Ok(value) => Ok(value),
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            Err(ApiError::field("slug", "這個網址代稱已經有人用了"))
        }
        Err(e) => Err(e.into()),
    }
}

pub async fn list(db: &PgPool) -> Result<Vec<Category>, sqlx::Error> {
    sqlx::query_as::<_, Category>(
        "SELECT id, slug, name, sort_order FROM categories ORDER BY sort_order, name",
    )
    .fetch_all(db)
    .await
}

pub async fn create(
    db: &PgPool,
    slug: Option<&str>,
    name: &str,
    sort_order: i32,
) -> Result<Category, ApiError> {
    validate_name(name)?;
    let slug = resolve_slug(slug)?;
    let result = sqlx::query_as::<_, Category>(
        "INSERT INTO categories (id, slug, name, sort_order) VALUES ($1, $2, $3, $4) RETURNING id, slug, name, sort_order",
    )
    .bind(Uuid::now_v7())
    .bind(&slug)
    .bind(name.trim())
    .bind(sort_order)
    .fetch_one(db)
    .await;
    map_slug_conflict(result)
}

pub async fn update(
    db: &PgPool,
    id: Uuid,
    slug: Option<&str>,
    name: &str,
    sort_order: i32,
) -> Result<Category, ApiError> {
    validate_name(name)?;
    let slug = resolve_slug(slug)?;
    let result = sqlx::query_as::<_, Category>(
        "UPDATE categories SET slug = $2, name = $3, sort_order = $4 WHERE id = $1 RETURNING id, slug, name, sort_order",
    )
    .bind(id)
    .bind(&slug)
    .bind(name.trim())
    .bind(sort_order)
    .fetch_optional(db)
    .await;
    map_slug_conflict(result)?.ok_or(ApiError::NotFound)
}

/// 匯入用：依名稱找分類，同名取 sort_order／id 最小的；沒有就建一個（slug 隨機）
pub async fn find_or_create_by_name(db: &PgPool, name: &str) -> Result<Category, ApiError> {
    let name = name.trim();
    validate_name(name)?;
    let found = sqlx::query_as::<_, Category>(
        "SELECT id, slug, name, sort_order FROM categories WHERE name = $1 ORDER BY sort_order, id LIMIT 1",
    )
    .bind(name)
    .fetch_optional(db)
    .await?;
    match found {
        Some(c) => Ok(c),
        None => create(db, None, name, 0).await,
    }
}

/// 刪分類；商品的 category_id 會因 FK ON DELETE SET NULL 變成空。回 false 表示沒這個 id。
pub async fn delete(db: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM categories WHERE id = $1")
        .bind(id)
        .execute(db)
        .await?;
    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_rules() {
        assert!(is_valid_slug("dog-food"));
        assert!(is_valid_slug("a1"));
        assert!(!is_valid_slug(""));
        assert!(!is_valid_slug("Dog"));
        assert!(!is_valid_slug("-a"));
        assert!(!is_valid_slug("a b"));
        assert!(!is_valid_slug(&"x".repeat(61)));
    }

    #[test]
    fn random_slug_is_8_lowercase_alnum() {
        let slug = random_slug();
        assert_eq!(slug.len(), 8);
        assert!(is_valid_slug(&slug));
        assert_ne!(random_slug(), random_slug());
    }

    #[test]
    fn resolve_slug_cases() {
        assert_eq!(resolve_slug(Some(" food ")).unwrap(), "food");
        assert_eq!(resolve_slug(Some("   ")).unwrap().len(), 8);
        assert_eq!(resolve_slug(None).unwrap().len(), 8);
        assert!(resolve_slug(Some("Bad Slug")).is_err());
    }
}
