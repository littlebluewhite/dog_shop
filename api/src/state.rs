use std::sync::Arc;

use sqlx::PgPool;

use crate::{
    config::Config,
    ecpay::{invoice::InvoiceGateway, logistics::LogisticsGateway},
    mail::Mailer,
};

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    /// Email 出口（SMTP／只記 log／測試擷取）
    pub mailer: Arc<Mailer>,
    /// 電子發票出口（綠界／測試 Fake）
    pub invoices: Arc<InvoiceGateway>,
    /// 物流建單出口（綠界／測試 Fake）
    pub logistics: Arc<LogisticsGateway>,
}
