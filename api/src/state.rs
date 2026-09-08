use std::sync::Arc;

use sqlx::PgPool;

use crate::{config::Config, mail::Mailer};

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    /// Email 出口（SMTP／只記 log／測試擷取）
    pub mailer: Arc<Mailer>,
}
