use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::users::is_tw_mobile;
use crate::error::{ApiError, FieldErrors};

pub const MAX_ADDRESSES: i64 = 10;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Address {
    pub id: Uuid,
    pub recipient_name: String,
    pub phone: String,
    pub postal_code: String,
    pub city: String,
    pub district: String,
    pub street: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AddressInput {
    pub recipient_name: String,
    pub phone: String,
    pub postal_code: String,
    pub city: String,
    pub district: String,
    pub street: String,
    #[serde(default)]
    pub is_default: bool,
}

/// 郵遞區號 3～5 碼數字（前端從靜態 JSON 帶入，這裡只驗格式，不驗對不對）
pub fn is_postal_code(code: &str) -> bool {
    (3..=5).contains(&code.len()) && code.bytes().all(|b| b.is_ascii_digit())
}

fn len_between(value: &str, min: usize, max: usize) -> bool {
    let n = value.chars().count();
    n >= min && n <= max
}

impl AddressInput {
    pub fn trimmed(mut self) -> Self {
        for s in [
            &mut self.recipient_name,
            &mut self.phone,
            &mut self.postal_code,
            &mut self.city,
            &mut self.district,
            &mut self.street,
        ] {
            *s = s.trim().to_string();
        }
        self
    }
}

pub fn validate(input: &AddressInput) -> Result<(), ApiError> {
    let mut errors = FieldErrors::new();
    if !len_between(&input.recipient_name, 1, 20) {
        errors.add("recipient_name", "必填，最多 20 字");
    }
    if !is_tw_mobile(&input.phone) {
        errors.add("phone", "手機格式：09 開頭共 10 碼");
    }
    if !is_postal_code(&input.postal_code) {
        errors.add("postal_code", "郵遞區號 3～5 碼數字");
    }
    if !len_between(&input.city, 1, 10) {
        errors.add("city", "請選縣市");
    }
    if !len_between(&input.district, 1, 10) {
        errors.add("district", "請選鄉鎮市區");
    }
    if !len_between(&input.street, 1, 100) {
        errors.add("street", "必填，最多 100 字");
    }
    errors.into_result()
}

// 三個查詢的 SELECT / RETURNING 欄位順序都要和 Address 的欄位一致
pub async fn list(db: &PgPool, user_id: Uuid) -> Result<Vec<Address>, sqlx::Error> {
    sqlx::query_as::<_, Address>(
        "SELECT id, recipient_name, phone, postal_code, city, district, street, is_default
         FROM addresses WHERE user_id = $1 ORDER BY is_default DESC, created_at",
    )
    .bind(user_id)
    .fetch_all(db)
    .await
}

/// 第一筆自動成為預設；is_default = true 會把其他筆取消預設。最多 MAX_ADDRESSES 筆
pub async fn create(db: &PgPool, user_id: Uuid, input: AddressInput) -> Result<Address, ApiError> {
    let input = input.trimmed();
    validate(&input)?;
    let mut tx = db.begin().await?;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM addresses WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(&mut *tx)
        .await?;
    if count >= MAX_ADDRESSES {
        return Err(ApiError::field("recipient_name", "最多 10 筆常用地址"));
    }
    let make_default = input.is_default || count == 0;
    if make_default {
        sqlx::query("UPDATE addresses SET is_default = false WHERE user_id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
    }
    let address = sqlx::query_as::<_, Address>(
        "INSERT INTO addresses (id, user_id, recipient_name, phone, postal_code, city, district, street, is_default)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         RETURNING id, recipient_name, phone, postal_code, city, district, street, is_default",
    )
    .bind(Uuid::now_v7())
    .bind(user_id)
    .bind(&input.recipient_name)
    .bind(&input.phone)
    .bind(&input.postal_code)
    .bind(&input.city)
    .bind(&input.district)
    .bind(&input.street)
    .bind(make_default)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(address)
}

/// 只能改自己的；找不到回 NotFound
pub async fn update(
    db: &PgPool,
    user_id: Uuid,
    id: Uuid,
    input: AddressInput,
) -> Result<Address, ApiError> {
    let input = input.trimmed();
    validate(&input)?;
    let mut tx = db.begin().await?;
    if input.is_default {
        sqlx::query("UPDATE addresses SET is_default = false WHERE user_id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
    }
    let address = sqlx::query_as::<_, Address>(
        "UPDATE addresses SET recipient_name = $3, phone = $4, postal_code = $5, city = $6, district = $7,
                street = $8, is_default = $9, updated_at = now()
         WHERE id = $1 AND user_id = $2
         RETURNING id, recipient_name, phone, postal_code, city, district, street, is_default",
    )
    .bind(id)
    .bind(user_id)
    .bind(&input.recipient_name)
    .bind(&input.phone)
    .bind(&input.postal_code)
    .bind(&input.city)
    .bind(&input.district)
    .bind(&input.street)
    .bind(input.is_default)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(ApiError::NotFound)?;
    tx.commit().await?;
    Ok(address)
}

pub async fn delete(db: &PgPool, user_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM addresses WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user_id)
        .execute(db)
        .await?;
    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postal_code_rules() {
        assert!(is_postal_code("100"));
        assert!(is_postal_code("10058"));
        assert!(!is_postal_code("10"));
        assert!(!is_postal_code("100a"));
        assert!(!is_postal_code("100058"));
    }
}
