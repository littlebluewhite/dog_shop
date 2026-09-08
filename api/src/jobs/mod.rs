//! 背景工作（規格 §9）：worker 認領 jobs 表的工作並執行；scheduled 是定時掃描（不走 jobs 表）
pub mod handlers;
pub mod scheduled;
pub mod worker;

use std::time::Duration;

use crate::state::AppState;

/// worker 每 2 秒看一次 jobs 表（規格 §9）
pub const POLL_INTERVAL: Duration = Duration::from_secs(2);
/// 過期未付款每 10 分鐘（規格 §9）
pub const EXPIRE_INTERVAL: Duration = Duration::from_secs(10 * 60);
/// 出貨 14 天自動完成每小時（規格 §9）
pub const AUTO_COMPLETE_INTERVAL: Duration = Duration::from_secs(60 * 60);
/// 清理每天（規格 §9 purge_expired_sessions，擴大到其他過期資料）
pub const PURGE_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// 在 main 裡呼叫一次：開四個 tokio task，各自無限迴圈；錯誤只記 log 不會停
pub fn start(state: AppState) {
    tokio::spawn(worker::run(state.clone()));
    tokio::spawn(scheduled::run_every(
        EXPIRE_INTERVAL,
        "expire_unpaid_orders",
        state.clone(),
        |s| async move {
            // 順便把當機殘留的 running job 撿回來（與規格不同之處 27）
            worker::requeue_stale(&s.db).await?;
            scheduled::expire_unpaid_orders(&s.db).await
        },
    ));
    tokio::spawn(scheduled::run_every(
        AUTO_COMPLETE_INTERVAL,
        "auto_complete_shipped",
        state.clone(),
        |s| async move { scheduled::auto_complete_shipped(&s.db).await },
    ));
    tokio::spawn(scheduled::run_every(
        PURGE_INTERVAL,
        "purge_expired",
        state,
        |s| async move { scheduled::purge_expired(&s.db).await },
    ));
    tracing::info!("jobs worker 與排程工作已啟動");
}
