use std::{net::SocketAddr, sync::Arc};

use anyhow::Context;
use dog_shop_api::{app, config::Config, state::AppState};
use dog_shop_api::{cli, db};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,tower_http=info")),
        )
        .init();

    let config = Arc::new(Config::from_env()?);
    let db = db::connect(&config.database_url).await?;
    db::migrate(&db).await?;

    // 子指令：cargo run -- create-admin <email>
    let mut args = std::env::args().skip(1);
    if let Some(command) = args.next() {
        return match command.as_str() {
            "create-admin" => {
                let email = args
                    .next()
                    .context("用法：cargo run -- create-admin <email>")?;
                cli::create_admin_interactive(&db, &email).await
            }
            other => anyhow::bail!("未知指令：{other}（可用：create-admin <email>）"),
        };
    }

    let state = AppState {
        db,
        config: config.clone(),
    };
    let app = app::router(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    tracing::info!("api listening on http://0.0.0.0:8080");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutting down");
}
