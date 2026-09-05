mod common;

use dog_shop_api::{
    auth::password::verify_password, cli::create_admin_with_password, domain::users,
};
use sqlx::PgPool;

#[sqlx::test(migrations = "./migrations")]
async fn creates_then_promotes_same_email(pool: PgPool) {
    let msg = create_admin_with_password(&pool, " Boss@Example.com ", "password123")
        .await
        .unwrap();
    assert!(msg.contains("已建立"), "{msg}");
    let user = users::find_by_email(&pool, "boss@example.com")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(user.role, "admin");
    assert_eq!(user.email, "boss@example.com");
    assert!(user.is_admin());

    // 同一個 email 再跑一次：不會多一個人，密碼被換掉
    let msg = create_admin_with_password(&pool, "boss@example.com", "newpassword9")
        .await
        .unwrap();
    assert!(msg.contains("設為管理員"), "{msg}");
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    let user = users::find_by_email(&pool, "boss@example.com")
        .await
        .unwrap()
        .unwrap();
    assert!(verify_password("newpassword9", &user.password_hash));
    assert!(!verify_password("password123", &user.password_hash));
}

#[sqlx::test(migrations = "./migrations")]
async fn rejects_short_password_and_bad_email(pool: PgPool) {
    let err = create_admin_with_password(&pool, "a@b.co", "short")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("至少"), "{err}");
    let err = create_admin_with_password(&pool, "not-an-email", "password123")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("Email"), "{err}");
}
