//! invoices 表（規格 §3、§8.4）。本任務只有 pending 列與查詢；開立相關的函式在 Task 10 補
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

pub const STATUS_PENDING: &str = "pending";
pub const STATUS_ISSUED: &str = "issued";
pub const STATUS_FAILED: &str = "failed";

/// 給訂單頁看的發票狀態（規格 §3 invoices 的子集合）
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct InvoiceRow {
    pub status: String,
    pub invoice_no: Option<String>,
    pub invoice_date: Option<DateTime<Utc>>,
    pub random_number: Option<String>,
}

/// 下單交易內建一列 pending（規格 §7 第 6 點）；RelateNumber = order_no（規格 §8.4）
pub async fn insert_pending_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    order_id: Uuid,
    order_no: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO invoices (id, order_id, relate_number) VALUES ($1, $2, $3)")
        .bind(Uuid::now_v7())
        .bind(order_id)
        .bind(order_no)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn get_by_order(db: &PgPool, order_id: Uuid) -> Result<Option<InvoiceRow>, sqlx::Error> {
    sqlx::query_as::<_, InvoiceRow>(
        "SELECT status, invoice_no, invoice_date, random_number FROM invoices WHERE order_id = $1",
    )
    .bind(order_id)
    .fetch_optional(db)
    .await
}

/// 先存請求再送（規格 §14）
pub async fn record_request(
    db: &PgPool,
    order_id: Uuid,
    request: &Value,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE invoices SET request = $2, updated_at = now() WHERE order_id = $1")
        .bind(order_id)
        .bind(request)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn mark_issued(
    db: &PgPool,
    order_id: Uuid,
    invoice_no: &str,
    invoice_date: Option<DateTime<Utc>>,
    random_number: &str,
    response: &Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE invoices SET status = $2, invoice_no = $3, invoice_date = $4, random_number = $5, response = $6,
                error = NULL, updated_at = now()
         WHERE order_id = $1",
    )
    .bind(order_id)
    .bind(STATUS_ISSUED)
    .bind(invoice_no)
    .bind(invoice_date)
    .bind(random_number)
    .bind(response)
    .execute(db)
    .await?;
    Ok(())
}

/// 記錄一次失敗；最後一次嘗試時把狀態標 failed（後台顯示、重試在計畫 4）
pub async fn record_failure(
    db: &PgPool,
    order_id: Uuid,
    response: Option<&Value>,
    error: &str,
    final_attempt: bool,
) -> Result<(), sqlx::Error> {
    let error: String = error.chars().take(1000).collect();
    sqlx::query(
        "UPDATE invoices SET status = CASE WHEN $4 THEN $5 ELSE status END, response = COALESCE($2, response),
                error = $3, updated_at = now()
         WHERE order_id = $1",
    )
    .bind(order_id)
    .bind(response)
    .bind(error)
    .bind(final_attempt)
    .bind(STATUS_FAILED)
    .execute(db)
    .await?;
    Ok(())
}
