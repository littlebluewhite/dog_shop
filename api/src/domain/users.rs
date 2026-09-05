use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

pub const ROLE_CUSTOMER: &str = "customer";
pub const ROLE_ADMIN: &str = "admin";

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub name: String,
    pub phone: Option<String>,
    pub role: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 給前端看的使用者資料（沒有 password_hash）
#[derive(Debug, Clone, Serialize)]
pub struct UserPublic {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub phone: Option<String>,
    pub role: String,
}

impl User {
    pub fn is_admin(&self) -> bool {
        self.role == ROLE_ADMIN
    }

    pub fn public(&self) -> UserPublic {
        UserPublic {
            id: self.id,
            email: self.email.clone(),
            name: self.name.clone(),
            phone: self.phone.clone(),
            role: self.role.clone(),
        }
    }
}

/// Email 一律去頭尾空白、轉小寫後再存、再查
pub fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

/// 很寬鬆的格式檢查：有一個 @，兩邊都有東西，domain 裡有 .，沒有空白
pub fn is_valid_email(email: &str) -> bool {
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !email.contains(char::is_whitespace)
        && !domain.contains('@')
}

pub async fn find_by_email(db: &PgPool, email: &str) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>("SELECT * FROM users WHERE lower(email) = $1")
        .bind(normalize_email(email))
        .fetch_optional(db)
        .await
}

pub async fn find_by_id(db: &PgPool, id: Uuid) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(db)
        .await
}

pub async fn create(
    db: &PgPool,
    email: &str,
    password_hash: &str,
    name: &str,
    role: &str,
) -> Result<User, sqlx::Error> {
    sqlx::query_as::<_, User>(
        "INSERT INTO users (id, email, password_hash, name, role) VALUES ($1, $2, $3, $4, $5) RETURNING *",
    )
    .bind(Uuid::now_v7())
    .bind(normalize_email(email))
    .bind(password_hash)
    .bind(name)
    .bind(role)
    .fetch_one(db)
    .await
}

pub async fn set_password_and_role(
    db: &PgPool,
    id: Uuid,
    password_hash: &str,
    role: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE users SET password_hash = $2, role = $3, updated_at = now() WHERE id = $1")
        .bind(id)
        .bind(password_hash)
        .bind(role)
        .execute(db)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_rules() {
        assert!(is_valid_email("a@b.co"));
        assert!(!is_valid_email("a@b"));
        assert!(!is_valid_email("@b.co"));
        assert!(!is_valid_email("a b@c.co"));
        assert_eq!(normalize_email("  Boss@Example.COM "), "boss@example.com");
    }
}
