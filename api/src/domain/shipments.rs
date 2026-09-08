//! shipments 表（規格 §3、§4）。本任務只有狀態常數與不倒退規則；Task 4 加完整列與狀態套用，Task 6 加出貨寫入
pub const SHIPMENT_PENDING: &str = "pending";
pub const SHIPMENT_CREATED: &str = "created";
pub const SHIPMENT_IN_TRANSIT: &str = "in_transit";
pub const SHIPMENT_ARRIVED: &str = "arrived";
pub const SHIPMENT_PICKED_UP: &str = "picked_up";
pub const SHIPMENT_RETURNED: &str = "returned";
/// 宅配：老闆填單號就是 shipped
pub const SHIPMENT_SHIPPED: &str = "shipped";

/// 狀態的先後（與規格不同之處 42）：arrived 與 returned 同一階（退回後可能重新配達）；
/// picked_up 與宅配的 shipped 是終態
pub fn status_rank(status: &str) -> u8 {
    match status {
        SHIPMENT_PENDING => 0,
        SHIPMENT_CREATED => 1,
        SHIPMENT_IN_TRANSIT => 2,
        SHIPMENT_ARRIVED | SHIPMENT_RETURNED => 3,
        SHIPMENT_PICKED_UP | SHIPMENT_SHIPPED => 4,
        _ => 0,
    }
}

/// 綠界通知晚到、重複、亂序都不能讓狀態倒退：新狀態的階要 >= 目前的、而且不同才套用；終態不再改
pub fn should_apply(current: &str, next: &str) -> bool {
    current != next
        && current != SHIPMENT_PICKED_UP
        && current != SHIPMENT_SHIPPED
        && status_rank(next) >= status_rank(current)
}

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::{PgExecutor, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::domain::jobs;
use crate::domain::orders::{SHIPPING_HOME, STATUS_COMPLETED, STATUS_PAID, STATUS_SHIPPED};
use crate::ecpay::logistics::{self, CreateOk, StatusNotification, StoreUpdate};
use crate::error::ApiError;

/// 後台與回呼用的完整 shipments 列（訂單頁的 orders::ShipmentRow 是子集合）。raw 不給前端
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Shipment {
    pub id: Uuid,
    pub order_id: Uuid,
    pub method: String,
    pub cvs_sub_type: Option<String>,
    pub cvs_store_id: Option<String>,
    pub cvs_store_name: Option<String>,
    pub cvs_store_address: Option<String>,
    pub cvs_store_phone: Option<String>,
    pub home_postal_code: Option<String>,
    pub home_city: Option<String>,
    pub home_district: Option<String>,
    pub home_street: Option<String>,
    pub status: String,
    pub ecpay_logistics_id: Option<String>,
    pub ecpay_merchant_trade_no: Option<String>,
    pub cvs_payment_no: Option<String>,
    pub cvs_validation_no: Option<String>,
    pub carrier: Option<String>,
    pub tracking_no: Option<String>,
    pub last_status_code: Option<String>,
    pub last_status_msg: Option<String>,
    #[serde(skip)]
    pub raw: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub const SHIPMENT_COLUMNS: &str = "id, order_id, method, cvs_sub_type, cvs_store_id, cvs_store_name, cvs_store_address, cvs_store_phone,
     home_postal_code, home_city, home_district, home_street, status, ecpay_logistics_id, ecpay_merchant_trade_no,
     cvs_payment_no, cvs_validation_no, carrier, tracking_no, last_status_code, last_status_msg, raw, created_at, updated_at";

pub async fn get_by_order<'e, E: PgExecutor<'e>>(
    exec: E,
    order_id: Uuid,
) -> Result<Option<Shipment>, sqlx::Error> {
    let sql = format!("SELECT {SHIPMENT_COLUMNS} FROM shipments WHERE order_id = $1");
    sqlx::query_as::<_, Shipment>(sqlx::AssertSqlSafe(sql))
        .bind(order_id)
        .fetch_optional(exec)
        .await
}

/// 狀態通知的處理結果；除了 Unknown 都回綠界 `1|OK`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusOutcome {
    /// MerchantTradeNo 與 AllPayLogisticsID 都找不到 → `0|Unknown MerchantTradeNo`
    Unknown,
    /// shipments.status 改了；picked_up 且訂單是 shipped 時一併 completed
    Updated {
        status: String,
        order_completed: bool,
    },
    /// 代碼不在對照表、或不能倒退／重複：只記代碼、訊息與 raw
    Recorded,
}

/// 套用一則狀態通知（規格 §8.3、與規格不同之處 42）。一個交易：先鎖 orders 列、再鎖 shipments 列
/// （計畫 4 的鎖序 orders → shipments）。一律更新 last_status_code／last_status_msg／raw.last_notification；
/// 有對照且不倒退才改 status；picked_up 且訂單是 shipped → completed。重複通知是 no-op
pub async fn apply_status(db: &PgPool, n: &StatusNotification) -> Result<StatusOutcome, ApiError> {
    let mut tx = db.begin().await?;
    let mut found: Option<(Uuid,)> =
        sqlx::query_as("SELECT order_id FROM shipments WHERE ecpay_merchant_trade_no = $1")
            .bind(&n.merchant_trade_no)
            .fetch_optional(&mut *tx)
            .await?;
    if found.is_none() && !n.logistics_id.is_empty() {
        found = sqlx::query_as("SELECT order_id FROM shipments WHERE ecpay_logistics_id = $1")
            .bind(&n.logistics_id)
            .fetch_optional(&mut *tx)
            .await?;
    }
    let Some((order_id,)) = found else {
        return Ok(StatusOutcome::Unknown);
    };
    let order_status: String =
        sqlx::query_scalar("SELECT status FROM orders WHERE id = $1 FOR UPDATE")
            .bind(order_id)
            .fetch_one(&mut *tx)
            .await?;
    let current: String =
        sqlx::query_scalar("SELECT status FROM shipments WHERE order_id = $1 FOR UPDATE")
            .bind(order_id)
            .fetch_one(&mut *tx)
            .await?;
    let next =
        logistics::shipment_status_for(n.rtn_code).filter(|next| should_apply(&current, next));
    sqlx::query(
        // 認領中（'creating'）不能被通知的代碼換掉，否則 claim_create 的守衛會失效（審查 I2）
        "UPDATE shipments SET status = COALESCE($2, status),
                last_status_code = CASE WHEN last_status_code = 'creating' THEN last_status_code ELSE $3 END,
                last_status_msg = $4,
                raw = COALESCE(raw, '{}'::jsonb) || $5, updated_at = now()
         WHERE order_id = $1",
    )
    .bind(order_id)
    .bind(next)
    .bind(n.rtn_code.to_string())
    .bind(&n.rtn_msg)
    .bind(json!({ "last_notification": n.raw }))
    .execute(&mut *tx)
    .await?;
    let mut order_completed = false;
    if next == Some(SHIPMENT_PICKED_UP) && order_status == STATUS_SHIPPED {
        sqlx::query("UPDATE orders SET status = $2, completed_at = now() WHERE id = $1")
            .bind(order_id)
            .bind(STATUS_COMPLETED)
            .execute(&mut *tx)
            .await?;
        order_completed = true;
    }
    tx.commit().await?;
    Ok(match next {
        Some(status) => StatusOutcome::Updated {
            status: status.to_string(),
            order_completed,
        },
        None => StatusOutcome::Recorded,
    })
}

/// 更新門市通知（與規格不同之處 35）：只記一句話與 raw.store_updates（追加），不改狀態。
/// 回 false = 找不到 AllPayLogisticsID
pub async fn apply_store_update(db: &PgPool, u: &StoreUpdate) -> Result<bool, ApiError> {
    let n = sqlx::query(
        "UPDATE shipments SET last_status_msg = $2,
                raw = jsonb_set(COALESCE(raw, '{}'::jsonb), '{store_updates}',
                                COALESCE(raw -> 'store_updates', '[]'::jsonb) || $3::jsonb),
                updated_at = now()
         WHERE ecpay_logistics_id = $1",
    )
    .bind(&u.logistics_id)
    .bind(logistics::store_update_message(u))
    .bind(json!([u.raw]))
    .execute(db)
    .await?
    .rows_affected();
    Ok(n > 0)
}

/// last_status_code 的三個特殊值（不是綠界代碼）：建單中、綠界拒絕、連線失敗（與規格不同之處 41）
pub const CREATING: &str = "creating";
pub const CREATE_FAILED: &str = "create_failed";
pub const CREATE_ERROR: &str = "create_error";

/// 物流單流水：order_no + "L" + 兩碼，每次嘗試換新號（規格 §8.3）；上次的號從 ecpay_merchant_trade_no 讀。
/// order_no 本身可能含 L，所以從右邊取最後一段
pub fn next_merchant_trade_no(order_no: &str, previous: Option<&str>) -> Result<String, ApiError> {
    let last = previous
        .and_then(|p| p.strip_prefix(order_no))
        .and_then(|rest| rest.strip_prefix('L'))
        .and_then(|seq| seq.parse::<u32>().ok())
        .unwrap_or(0);
    let seq = last + 1;
    if seq > 99 {
        return Err(ApiError::field(
            "shipment",
            "物流單重試次數過多，請聯絡綠界客服",
        ));
    }
    let result = format!("{order_no}L{seq:02}");
    // 綠界的 MerchantTradeNo 上限 20 字（擱置 31）
    if result.chars().count() > 20 {
        return Err(ApiError::field("shipment", "訂單編號過長，無法建立物流單"));
    }
    Ok(result)
}

/// 建單前的認領：只有 pending、還沒有綠界單號、且不是別人正在建（或上次卡住超過 2 分鐘）才能建；
/// 同時寫入這次的 MerchantTradeNo 與請求欄位（規格 §14 先存再送）。回 false = 別人正在建或已建過。
/// `raw.create_request` 是最後一次，`raw.create_requests` 累積歷次 —— 逾時重試時第一張單才有跡可循（審查 I1）
pub async fn claim_create(
    db: &PgPool,
    order_id: Uuid,
    merchant_trade_no: &str,
    request_fields: &Value,
) -> Result<bool, sqlx::Error> {
    let n = sqlx::query(
        "UPDATE shipments SET ecpay_merchant_trade_no = $2, last_status_code = $3, last_status_msg = NULL,
                raw = COALESCE(raw, '{}'::jsonb)
                      || jsonb_build_object('create_request', $4::jsonb,
                                            'create_requests',
                                            COALESCE(raw -> 'create_requests', '[]'::jsonb) || jsonb_build_array($4::jsonb)),
                updated_at = now()
         WHERE order_id = $1 AND status = $5 AND ecpay_logistics_id IS NULL
           AND (last_status_code IS DISTINCT FROM $3 OR updated_at < now() - interval '2 minutes')",
    )
    .bind(order_id)
    .bind(merchant_trade_no)
    .bind(CREATING)
    .bind(request_fields)
    .bind(SHIPMENT_PENDING)
    .execute(db)
    .await?
    .rows_affected();
    Ok(n > 0)
}

/// 綠界拒絕或連不上：記原因（給老闆看），訂單維持 paid、shipments 維持 pending
pub async fn record_create_failure(
    db: &PgPool,
    order_id: Uuid,
    code: &str,
    msg: &str,
    response_text: Option<&str>,
) -> Result<(), sqlx::Error> {
    let msg: String = msg.chars().take(200).collect();
    let patch = match response_text {
        Some(t) => json!({ "create_response_text": t.chars().take(2000).collect::<String>() }),
        None => json!({}),
    };
    sqlx::query(
        "UPDATE shipments SET last_status_code = $2, last_status_msg = $3,
                raw = COALESCE(raw, '{}'::jsonb) || $4, updated_at = now()
         WHERE order_id = $1",
    )
    .bind(order_id)
    .bind(code)
    .bind(msg)
    .bind(patch)
    .execute(db)
    .await?;
    Ok(())
}

/// 綠界成功：先把單號存起來（自己一句 UPDATE，不在收尾的交易裡），收尾失敗時下次不會重複建單
pub async fn record_create_ok(
    db: &PgPool,
    order_id: Uuid,
    ok: &CreateOk,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE shipments SET ecpay_logistics_id = $2, cvs_payment_no = NULLIF($3, ''), cvs_validation_no = NULLIF($4, ''),
                last_status_code = $5, last_status_msg = $6,
                raw = COALESCE(raw, '{}'::jsonb) || $7, updated_at = now()
         WHERE order_id = $1",
    )
    .bind(order_id)
    .bind(&ok.logistics_id)
    .bind(&ok.cvs_payment_no)
    .bind(&ok.cvs_validation_no)
    .bind(ok.rtn_code.to_string())
    .bind(&ok.rtn_msg)
    .bind(json!({ "create_response": ok.raw }))
    .execute(db)
    .await?;
    Ok(())
}

/// orders → shipped、shipped_at、排出貨信（超商與宅配共用；規格 §12、計畫 3 交接 3）。呼叫者已鎖住訂單列
async fn ship_order_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    order_id: Uuid,
) -> Result<(), ApiError> {
    sqlx::query("UPDATE orders SET status = $2, shipped_at = now() WHERE id = $1")
        .bind(order_id)
        .bind(STATUS_SHIPPED)
        .execute(&mut **tx)
        .await?;
    jobs::enqueue(
        tx,
        jobs::KIND_SEND_EMAIL,
        json!({ "template": "order_shipped", "order_id": order_id }),
        Some(&format!("email:order_shipped:{order_id}")),
    )
    .await?;
    Ok(())
}

/// 超商建單的收尾：一個交易內 shipments → created、orders → shipped、排出貨信（鎖序 orders → shipments）。
/// 回 false = 訂單不是 paid（例如同時被標退款）；已經 shipped 回 true（冪等）
pub async fn finalize_cvs(db: &PgPool, order_id: Uuid) -> Result<bool, ApiError> {
    let mut tx = db.begin().await?;
    let status: Option<String> =
        sqlx::query_scalar("SELECT status FROM orders WHERE id = $1 FOR UPDATE")
            .bind(order_id)
            .fetch_optional(&mut *tx)
            .await?;
    match status.as_deref() {
        Some(STATUS_PAID) => {}
        Some(STATUS_SHIPPED) => {
            tx.rollback().await?;
            return Ok(true);
        }
        _ => {
            tx.rollback().await?;
            return Ok(false);
        }
    }
    sqlx::query(
        "UPDATE shipments SET status = $2, updated_at = now() WHERE order_id = $1 AND status = $3",
    )
    .bind(order_id)
    .bind(SHIPMENT_CREATED)
    .bind(SHIPMENT_PENDING)
    .execute(&mut *tx)
    .await?;
    ship_order_in_tx(&mut tx, order_id).await?;
    tx.commit().await?;
    Ok(true)
}

/// 宅配出貨（規格 §4、§6.1）：填貨運公司與單號 → shipments shipped、orders shipped、排出貨信。
/// 回 false = 訂單不是 paid 或不是宅配
pub async fn ship_home(
    db: &PgPool,
    order_id: Uuid,
    carrier: &str,
    tracking_no: &str,
) -> Result<bool, ApiError> {
    let mut tx = db.begin().await?;
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT status, shipping_method FROM orders WHERE id = $1 FOR UPDATE")
            .bind(order_id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((status, method)) = row else {
        tx.rollback().await?;
        return Ok(false);
    };
    if status != STATUS_PAID || method != SHIPPING_HOME {
        tx.rollback().await?;
        return Ok(false);
    }
    sqlx::query(
        "UPDATE shipments SET status = $2, carrier = $3, tracking_no = $4, updated_at = now() WHERE order_id = $1",
    )
    .bind(order_id)
    .bind(SHIPMENT_SHIPPED)
    .bind(carrier)
    .bind(tracking_no)
    .execute(&mut *tx)
    .await?;
    ship_order_in_tx(&mut tx, order_id).await?;
    tx.commit().await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merchant_trade_no_increments_per_attempt() {
        assert_eq!(
            next_merchant_trade_no("DS260908ABLD", None).unwrap(),
            "DS260908ABLDL01"
        );
        assert_eq!(
            next_merchant_trade_no("DS260908ABLD", Some("DS260908ABLDL01")).unwrap(),
            "DS260908ABLDL02"
        );
        assert_eq!(
            next_merchant_trade_no("DS260908ABLD", Some("DS260908ABLDL09")).unwrap(),
            "DS260908ABLDL10"
        );
        assert_eq!(
            next_merchant_trade_no("DS260908ABLD", Some("garbage")).unwrap(),
            "DS260908ABLDL01"
        );
        assert!(next_merchant_trade_no("DS260908ABLD", Some("DS260908ABLDL99")).is_err());
        // 綠界的 MerchantTradeNo 上限 20 字（擱置 31）
        assert!(next_merchant_trade_no(&"A".repeat(20), None).is_err());
    }

    #[test]
    fn forward_moves_apply_backward_moves_do_not() {
        assert!(should_apply(SHIPMENT_PENDING, SHIPMENT_CREATED));
        assert!(should_apply(SHIPMENT_CREATED, SHIPMENT_IN_TRANSIT));
        assert!(should_apply(SHIPMENT_IN_TRANSIT, SHIPMENT_ARRIVED));
        assert!(should_apply(SHIPMENT_ARRIVED, SHIPMENT_PICKED_UP));
        assert!(
            !should_apply(SHIPMENT_ARRIVED, SHIPMENT_IN_TRANSIT),
            "晚到的物流中心通知不能倒退"
        );
        assert!(!should_apply(SHIPMENT_IN_TRANSIT, SHIPMENT_CREATED));
        assert!(
            !should_apply(SHIPMENT_ARRIVED, SHIPMENT_ARRIVED),
            "同狀態不算變更"
        );
    }

    #[test]
    fn returned_and_arrived_can_swap_but_picked_up_is_final() {
        assert!(should_apply(SHIPMENT_ARRIVED, SHIPMENT_RETURNED));
        assert!(
            should_apply(SHIPMENT_RETURNED, SHIPMENT_ARRIVED),
            "退回後重新配達"
        );
        assert!(!should_apply(SHIPMENT_PICKED_UP, SHIPMENT_RETURNED));
        assert!(!should_apply(SHIPMENT_PICKED_UP, SHIPMENT_ARRIVED));
        assert!(
            !should_apply(SHIPMENT_SHIPPED, SHIPMENT_ARRIVED),
            "宅配不會收到超商通知"
        );
    }
}
