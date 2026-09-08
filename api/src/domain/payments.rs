//! payments 表（規格 §3、§4、§7 第 8、9 點）：回呼寫入。重新付款在 Task 7 補在這個檔案下面
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::jobs;
use crate::domain::orders::{PaymentRow, STATUS_PAID, STATUS_PENDING_PAYMENT};
use crate::ecpay::aio::Notification;
use crate::error::ApiError;

pub const PAYMENT_PENDING: &str = "pending";
pub const PAYMENT_PAID: &str = "paid";
pub const PAYMENT_FAILED: &str = "failed";
pub const PAYMENT_EXPIRED: &str = "expired";

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Payment {
    pub id: Uuid,
    pub order_id: Uuid,
    pub merchant_trade_no: String,
    pub method: String,
    pub status: String,
    pub amount: i32,
    pub ecpay_trade_no: Option<String>,
    pub payment_type: Option<String>,
    pub payment_date: Option<DateTime<Utc>>,
    pub atm_bank_code: Option<String>,
    pub atm_vaccount: Option<String>,
    pub cvs_payment_no: Option<String>,
    pub expire_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// 剛建立的付款嘗試要拿去組綠界表單（routes::orders::repay）
impl From<Payment> for PaymentRow {
    fn from(p: Payment) -> Self {
        Self {
            id: p.id,
            merchant_trade_no: p.merchant_trade_no,
            method: p.method,
            status: p.status,
            amount: p.amount,
            atm_bank_code: p.atm_bank_code,
            atm_vaccount: p.atm_vaccount,
            cvs_payment_no: p.cvs_payment_no,
            expire_at: p.expire_at,
        }
    }
}

const PAYMENT_COLUMNS: &str =
    "id, order_id, merchant_trade_no, method, status, amount, ecpay_trade_no, payment_type,
     payment_date, atm_bank_code, atm_vaccount, cvs_payment_no, expire_at, created_at";

pub async fn get(db: &PgPool, id: Uuid) -> Result<Option<Payment>, sqlx::Error> {
    let sql = format!("SELECT {PAYMENT_COLUMNS} FROM payments WHERE id = $1");
    sqlx::query_as::<_, Payment>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(db)
        .await
}

/// 後台看全部付款嘗試（新到舊；OrderDetail.payment 只有最新一筆）
pub async fn list_for_order(db: &PgPool, order_id: Uuid) -> Result<Vec<Payment>, sqlx::Error> {
    let sql = format!(
        "SELECT {PAYMENT_COLUMNS} FROM payments WHERE order_id = $1 ORDER BY created_at DESC, id DESC"
    );
    sqlx::query_as::<_, Payment>(sqlx::AssertSqlSafe(sql))
        .bind(order_id)
        .fetch_all(db)
        .await
}

/// ReturnURL 的處理結果；除了 Unknown 都回綠界 `1|OK`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnOutcome {
    /// 找不到 MerchantTradeNo → `0|Unknown MerchantTradeNo`
    Unknown,
    /// 測試環境的模擬付款：只記 log
    Simulated,
    /// 這筆 payment 早就 paid：重複通知，不重做
    Duplicate,
    /// RtnCode ≠ 1：payment 標 failed，訂單不動
    NotPaid,
    /// 訂單已不是待付款、或金額不符：payment 標 paid、訂單 needs_refund（規格 §4）
    Late,
    /// 正常付款成功：訂單 paid，排發票與通知信
    Paid,
}

/// ReturnURL（付款結果）。一個交易內鎖 payments 與 orders 列；重複通知是 no-op（規格 §7 第 8 點、§14）
pub async fn apply_return(db: &PgPool, n: &Notification) -> Result<ReturnOutcome, ApiError> {
    if n.simulate_paid {
        tracing::info!(merchant_trade_no = %n.merchant_trade_no, rtn_code = n.rtn_code, "綠界模擬付款通知，只記 log 不改狀態");
        return Ok(ReturnOutcome::Simulated);
    }
    let mut tx = db.begin().await?;
    let sql =
        format!("SELECT {PAYMENT_COLUMNS} FROM payments WHERE merchant_trade_no = $1 FOR UPDATE");
    let Some(payment) = sqlx::query_as::<_, Payment>(sqlx::AssertSqlSafe(sql))
        .bind(&n.merchant_trade_no)
        .fetch_optional(&mut *tx)
        .await?
    else {
        return Ok(ReturnOutcome::Unknown);
    };
    if payment.status == PAYMENT_PAID {
        return Ok(ReturnOutcome::Duplicate);
    }
    if n.rtn_code != 1 {
        // 失敗或待確認（信用卡 10300066）：記下來，訂單不動；買家可以重新付款
        sqlx::query(
            "UPDATE payments SET status = $2, ecpay_trade_no = $3, payment_type = $4, raw = $5, updated_at = now() WHERE id = $1",
        )
        .bind(payment.id)
        .bind(PAYMENT_FAILED)
        .bind(&n.trade_no)
        .bind(&n.payment_type)
        .bind(&n.raw)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        return Ok(ReturnOutcome::NotPaid);
    }

    // 成功：先鎖訂單列再決定是正常付款還是遲到（規格 §4）
    let order_status: String =
        sqlx::query_scalar("SELECT status FROM orders WHERE id = $1 FOR UPDATE")
            .bind(payment.order_id)
            .fetch_one(&mut *tx)
            .await?;
    let paid_at = n.payment_date.unwrap_or_else(Utc::now);
    sqlx::query(
        "UPDATE payments SET status = $2, ecpay_trade_no = $3, payment_type = $4, payment_date = $5, raw = $6, updated_at = now() WHERE id = $1",
    )
    .bind(payment.id)
    .bind(PAYMENT_PAID)
    .bind(&n.trade_no)
    .bind(&n.payment_type)
    .bind(paid_at)
    .bind(&n.raw)
    .execute(&mut *tx)
    .await?;

    let on_time = order_status == STATUS_PENDING_PAYMENT && n.trade_amt == payment.amount;
    if !on_time {
        tracing::warn!(
            order_id = %payment.order_id,
            merchant_trade_no = %n.merchant_trade_no,
            order_status = %order_status,
            trade_amt = n.trade_amt,
            expected = payment.amount,
            "遲到或金額不符的付款：只標 payment paid 並記 needs_refund（規格 §4）"
        );
        sqlx::query("UPDATE orders SET needs_refund = true WHERE id = $1")
            .bind(payment.order_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        return Ok(ReturnOutcome::Late);
    }

    let order_id = payment.order_id;
    sqlx::query("UPDATE orders SET status = $2, paid_at = $3 WHERE id = $1")
        .bind(order_id)
        .bind(STATUS_PAID)
        .bind(paid_at)
        .execute(&mut *tx)
        .await?;
    jobs::enqueue(
        &mut tx,
        jobs::KIND_ISSUE_INVOICE,
        json!({ "order_id": order_id }),
        Some(&format!("invoice:{order_id}")),
    )
    .await?;
    jobs::enqueue(
        &mut tx,
        jobs::KIND_SEND_EMAIL,
        json!({ "template": "payment_received", "order_id": order_id }),
        Some(&format!("email:payment_received:{order_id}")),
    )
    .await?;
    tx.commit().await?;
    Ok(ReturnOutcome::Paid)
}

/// PaymentInfoURL 的處理結果；除了 Unknown 都回綠界 `1|OK`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InfoOutcome {
    Unknown,
    /// payment 已不是 pending，或這次通知沒帶繳費資訊：只存 raw（或不動）
    Ignored,
    /// 存好帳號／代碼與期限，排 payment_instructions 信
    Stored,
}

/// PaymentInfoURL（ATM 虛擬帳號／超商代碼；規格 §7 第 8 點）。只在 payment 仍 pending 時寫入
pub async fn apply_info(db: &PgPool, n: &Notification) -> Result<InfoOutcome, ApiError> {
    let mut tx = db.begin().await?;
    let sql =
        format!("SELECT {PAYMENT_COLUMNS} FROM payments WHERE merchant_trade_no = $1 FOR UPDATE");
    let Some(payment) = sqlx::query_as::<_, Payment>(sqlx::AssertSqlSafe(sql))
        .bind(&n.merchant_trade_no)
        .fetch_optional(&mut *tx)
        .await?
    else {
        return Ok(InfoOutcome::Unknown);
    };
    if payment.status != PAYMENT_PENDING {
        return Ok(InfoOutcome::Ignored);
    }
    // 買家在綠界取號後、這個回呼抵達前把訂單取消掉：訂單已不是待付款就不寫入繳費資訊、
    // 也不寄「繳費資訊」信，免得叫客人去付一筆已取消的訂單（Minor 1）
    let order_status: String =
        sqlx::query_scalar("SELECT status FROM orders WHERE id = $1 FOR UPDATE")
            .bind(payment.order_id)
            .fetch_one(&mut *tx)
            .await?;
    if order_status != STATUS_PENDING_PAYMENT {
        tracing::warn!(merchant_trade_no = %n.merchant_trade_no, order_status = %order_status, "訂單已不是待付款，PaymentInfoURL 不寫入繳費資訊也不寄信");
        return Ok(InfoOutcome::Ignored);
    }
    // RtnCode 2 = ATM 取號成功、10100073 = 超商代碼取號成功（綠界文件）；其他碼只存 raw
    let got_info =
        matches!(n.rtn_code, 2 | 10100073) && (n.v_account.is_some() || n.payment_no.is_some());
    if !got_info {
        tracing::warn!(merchant_trade_no = %n.merchant_trade_no, rtn_code = n.rtn_code, rtn_msg = %n.rtn_msg, "PaymentInfoURL 沒帶繳費資訊");
        sqlx::query("UPDATE payments SET raw = $2, updated_at = now() WHERE id = $1")
            .bind(payment.id)
            .bind(&n.raw)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        return Ok(InfoOutcome::Ignored);
    }
    sqlx::query(
        "UPDATE payments SET ecpay_trade_no = $2, payment_type = $3, atm_bank_code = $4, atm_vaccount = $5,
                cvs_payment_no = $6, expire_at = $7, raw = $8, updated_at = now()
         WHERE id = $1",
    )
    .bind(payment.id)
    .bind(&n.trade_no)
    .bind(&n.payment_type)
    .bind(&n.bank_code)
    .bind(&n.v_account)
    .bind(&n.payment_no)
    .bind(n.expire_at)
    .bind(&n.raw)
    .execute(&mut *tx)
    .await?;
    jobs::enqueue(
        &mut tx,
        jobs::KIND_SEND_EMAIL,
        json!({ "template": "payment_instructions", "order_id": payment.order_id, "payment_id": payment.id }),
        Some(&format!("email:payment_instructions:{}", payment.id)),
    )
    .await?;
    tx.commit().await?;
    Ok(InfoOutcome::Stored)
}

/// 重新付款（規格 §7 第 9 點）：新列、新 merchant_trade_no（order_no + 兩碼流水，規格 §3）。
/// 只有 pending_payment 能重付（ORDER_NOT_PAYABLE）。鎖訂單列，兩個同時重付不會拿到同一個流水
pub async fn create_repayment(
    db: &PgPool,
    order_id: Uuid,
    method: &str,
) -> Result<Payment, ApiError> {
    let mut tx = db.begin().await?;
    let row: Option<(String, String, i32)> =
        sqlx::query_as("SELECT order_no, status, total FROM orders WHERE id = $1 FOR UPDATE")
            .bind(order_id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((order_no, status, total)) = row else {
        return Err(ApiError::NotFound);
    };
    if status != STATUS_PENDING_PAYMENT {
        return Err(ApiError::OrderNotPayable);
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM payments WHERE order_id = $1")
        .bind(order_id)
        .fetch_one(&mut *tx)
        .await?;
    let seq = count + 1;
    if seq > 99 {
        return Err(ApiError::OrderNotPayable);
    }
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO payments (id, order_id, merchant_trade_no, method, amount) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(order_id)
    .bind(format!("{order_no}{seq:02}"))
    .bind(method)
    .bind(total)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    get(db, id)
        .await?
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("剛建立的 payment {id} 不見了")))
}
