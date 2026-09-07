use serde_json::Value;
use sqlx::{Postgres, Transaction};

pub const KIND_SEND_EMAIL: &str = "send_email";
pub const KIND_ISSUE_INVOICE: &str = "issue_invoice";

/// outbox：和業務資料在同一個交易裡寫入，不會漏（規格 §9）。
/// dedupe_key 已存在就不再排（ON CONFLICT DO NOTHING）；None 不去重。worker 在計畫 3。
pub async fn enqueue(
    tx: &mut Transaction<'_, Postgres>,
    kind: &str,
    payload: Value,
    dedupe_key: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO jobs (kind, payload, dedupe_key) VALUES ($1, $2, $3)
         ON CONFLICT (dedupe_key) DO NOTHING",
    )
    .bind(kind)
    .bind(payload)
    .bind(dedupe_key)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
