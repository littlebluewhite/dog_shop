use anyhow::Context;
use sqlx::PgPool;

use crate::auth::password::{MIN_PASSWORD_CHARS, hash_password};
use crate::domain::users::{self, ROLE_ADMIN, is_valid_email, normalize_email};

/// `api create-admin <email>`：互動輸入密碼兩次（規格 §11）。
/// 沒有終端機可以互動時（腳本、CI）可以改用環境變數 ADMIN_PASSWORD。
pub async fn create_admin_interactive(db: &PgPool, email: &str) -> anyhow::Result<()> {
    let password = match std::env::var("ADMIN_PASSWORD") {
        Ok(from_env) => from_env,
        Err(_) => {
            let password = rpassword::prompt_password(format!(
                "{email} 的密碼（至少 {MIN_PASSWORD_CHARS} 碼）: "
            ))?;
            let confirm = rpassword::prompt_password("再輸入一次: ")?;
            anyhow::ensure!(password == confirm, "兩次輸入的密碼不一樣");
            password
        }
    };
    let outcome = create_admin_with_password(db, email, &password).await?;
    println!("{outcome}");
    Ok(())
}

/// 建立 admin；email 已存在就把那個帳號升成 admin 並換密碼。回傳給人看的訊息。測試直接呼叫這個。
pub async fn create_admin_with_password(
    db: &PgPool,
    email: &str,
    password: &str,
) -> anyhow::Result<String> {
    let email = normalize_email(email);
    anyhow::ensure!(is_valid_email(&email), "Email 格式不正確：{email}");
    anyhow::ensure!(
        password.chars().count() >= MIN_PASSWORD_CHARS,
        "密碼至少 {MIN_PASSWORD_CHARS} 碼"
    );
    let hash = hash_password(password)?;
    match users::find_by_email(db, &email)
        .await
        .context("查詢使用者")?
    {
        Some(user) => {
            users::set_password_and_role(db, user.id, &hash, ROLE_ADMIN)
                .await
                .context("更新使用者")?;
            Ok(format!("已把 {email} 設為管理員並更新密碼"))
        }
        None => {
            users::create(db, &email, &hash, "管理員", ROLE_ADMIN)
                .await
                .context("建立使用者")?;
            Ok(format!("已建立管理員 {email}"))
        }
    }
}
