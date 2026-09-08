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
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

use crate::domain::orders::{STATUS_COMPLETED, STATUS_SHIPPED};
use crate::ecpay::logistics::{self, StatusNotification, StoreUpdate};
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
        "UPDATE shipments SET status = COALESCE($2, status), last_status_code = $3, last_status_msg = $4,
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

#[cfg(test)]
mod tests {
    use super::*;

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
