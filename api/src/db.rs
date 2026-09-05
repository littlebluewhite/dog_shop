use sqlx::{PgPool, migrate::Migrator, postgres::PgPoolOptions};

/// 內嵌 ./migrations 底下的 SQL；啟動時執行（規格 §16）
pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

pub async fn connect(database_url: &str) -> anyhow::Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await?;
    Ok(pool)
}

pub async fn migrate(pool: &PgPool) -> anyhow::Result<()> {
    MIGRATOR.run(pool).await?;
    Ok(())
}
