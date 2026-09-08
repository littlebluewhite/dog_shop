use std::{net::SocketAddr, sync::Arc};

use anyhow::Context;
use dog_shop_api::{app, config::Config, ecpay, jobs, mail, state::AppState};
use dog_shop_api::{cli, db};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // reqwest 用 rustls-no-provider，整個行程要先裝好 crypto provider（與規格不同之處 32）；重複安裝會回 Err，忽略
    let _ = rustls::crypto::ring::default_provider().install_default();
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

    let mailer = Arc::new(mail::Mailer::from_config(&config)?);
    let invoices = Arc::new(ecpay::invoice::InvoiceGateway::ecpay(&config.ecpay)?);
    let logistics = Arc::new(ecpay::logistics::LogisticsGateway::ecpay()?);
    let state = AppState {
        db,
        config: config.clone(),
        mailer,
        invoices,
        logistics,
    };
    // 背景工作：jobs worker 與排程掃描（規格 §9），和 api 同一個行程、同一個連線池
    jobs::start(state.clone());
    let app = app::router(state);

    let listener = tokio::net::TcpListener::bind(&config.listen_addr).await?;
    tracing::info!(addr = %config.listen_addr, "api listening");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            // 註冊失敗時不能讓這個 future 完成，否則 select! 會立刻觸發、剛啟動就關機
            Err(e) => {
                tracing::error!(error = %e, "無法監聽 SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutting down");
}
