//! 共用的匿名寫入限流器（規格 §11）：只給允許匿名呼叫、又會寫一筆 DB 記錄的端點用
//! （目前是 `POST /api/orders`、`POST /api/checkout/cvs-map`）。每個 IP 突發 10 次，
//! 之後每 12 秒補 1 次（約 5 次/分鐘）；每個呼叫端各自獨立配額，不共用。
//! `/api/auth/*` 的限流器數字不同、獨立維護，見 routes/auth.rs。
use std::{sync::Arc, time::Duration};

use axum::{response::IntoResponse, routing::MethodRouter};
use tower_governor::{
    GovernorError, GovernorLayer, governor::GovernorConfigBuilder,
    key_extractor::SmartIpKeyExtractor,
};

use crate::{error::ApiError, state::AppState};

/// 把限流器掛到一個 `MethodRouter` 上：每個 IP 突發 10 次，之後每 12 秒補 1 次。
/// 呼叫一次就會另外起一支清理執行緒（定期丟掉沒在用的 IP 記錄），只在組路由時呼叫，不要每個請求呼叫。
pub(crate) fn anonymous_write(route: MethodRouter<AppState>) -> MethodRouter<AppState> {
    let governor_conf = Arc::new(
        GovernorConfigBuilder::default()
            .key_extractor(SmartIpKeyExtractor)
            .per_second(12)
            .burst_size(10)
            .finish()
            .expect("governor config"),
    );
    // 定期清掉沒在用的 IP 記錄，不然記憶體只會長
    let limiter = governor_conf.limiter().clone();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(60));
            limiter.retain_recent();
        }
    });
    let governor_layer = GovernorLayer::new(governor_conf).error_handler(|err| match err {
        GovernorError::TooManyRequests { .. } => ApiError::RateLimited.into_response(),
        other => ApiError::Internal(anyhow::anyhow!("rate limiter: {other:?}")).into_response(),
    });
    route.layer(governor_layer)
}
