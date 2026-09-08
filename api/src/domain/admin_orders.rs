//! 後台訂單（規格 §6.1、§10）：列表、明細；Task 7 加退款／完成／清除需退款，Task 8 加儀表板
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::orders::{
    OrderItemRow, STATUS_CANCELLED, STATUS_COMPLETED, STATUS_PAID, STATUS_PENDING_PAYMENT,
    STATUS_REFUNDED, STATUS_SHIPPED,
};
use crate::domain::payments::{self, Payment};
use crate::domain::products::Page;
use crate::domain::shipments::{self, Shipment};
use crate::error::ApiError;

pub const STATUSES: &[&str] = &[
    STATUS_PENDING_PAYMENT,
    STATUS_PAID,
    STATUS_SHIPPED,
    STATUS_COMPLETED,
    STATUS_CANCELLED,
    STATUS_REFUNDED,
];

/// 列表的特殊篩選（與規格不同之處 45）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flag {
    /// 遲到付款、金額不符（orders.needs_refund）
    NeedsRefund,
    /// 超商未取退回：訂單 shipped 且 shipments.status = returned
    CvsReturned,
    /// 發票開立失敗
    InvoiceFailed,
}

impl Flag {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "needs_refund" => Some(Self::NeedsRefund),
            "cvs_returned" => Some(Self::CvsReturned),
            "invoice_failed" => Some(Self::InvoiceFailed),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::NeedsRefund => "needs_refund",
            Self::CvsReturned => "cvs_returned",
            Self::InvoiceFailed => "invoice_failed",
        }
    }
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AdminOrderListItem {
    pub id: Uuid,
    pub order_no: String,
    pub status: String,
    pub email: String,
    pub recipient_name: String,
    pub shipping_method: String,
    pub total: i32,
    pub item_count: i32,
    pub needs_refund: bool,
    pub shipment_status: Option<String>,
    pub invoice_status: Option<String>,
    pub created_at: DateTime<Utc>,
    pub paid_at: Option<DateTime<Utc>>,
    #[serde(skip)]
    pub total_count: i64,
}

/// 後台列表：q 比對訂單編號、Email、收件人（不分大小寫、部分相符）；新到舊
pub async fn list(
    db: &PgPool,
    q: Option<&str>,
    status: Option<&str>,
    flag: Option<Flag>,
    page: i64,
    per_page: i64,
) -> Result<Page<AdminOrderListItem>, ApiError> {
    let rows = sqlx::query_as::<_, AdminOrderListItem>(
        "SELECT o.id, o.order_no, o.status, o.email, o.recipient_name, o.shipping_method, o.total, o.needs_refund,
                o.created_at, o.paid_at,
                (SELECT COALESCE(SUM(oi.quantity), 0) FROM order_items oi WHERE oi.order_id = o.id)::int AS item_count,
                s.status AS shipment_status,
                i.status AS invoice_status,
                COUNT(*) OVER () AS total_count
         FROM orders o
         LEFT JOIN shipments s ON s.order_id = o.id
         LEFT JOIN invoices i ON i.order_id = o.id
         WHERE ($1::text IS NULL OR o.order_no ILIKE '%' || $1 || '%' OR o.email ILIKE '%' || $1 || '%'
                OR o.recipient_name ILIKE '%' || $1 || '%')
           AND ($2::text IS NULL OR o.status = $2)
           AND ($3::text IS NULL
                OR ($3 = 'needs_refund' AND o.needs_refund)
                OR ($3 = 'cvs_returned' AND o.status = 'shipped' AND s.status = 'returned')
                OR ($3 = 'invoice_failed' AND i.status = 'failed'))
         ORDER BY o.created_at DESC, o.id DESC
         LIMIT $4 OFFSET $5",
    )
    .bind(q)
    .bind(status)
    .bind(flag.map(Flag::as_str))
    .bind(per_page)
    .bind((page - 1) * per_page)
    .fetch_all(db)
    .await?;
    let total = rows.first().map(|r| r.total_count).unwrap_or(0);
    Ok(Page {
        items: rows,
        total,
        page,
        per_page,
    })
}

/// 後台看的訂單主檔：比 orders::OrderRow 多 user_id 與 needs_refund；不含 guest_token（規格 §11、與規格不同之處 48）
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AdminOrderRow {
    pub id: Uuid,
    pub order_no: String,
    pub user_id: Option<Uuid>,
    pub status: String,
    pub email: String,
    pub recipient_name: String,
    pub recipient_phone: String,
    pub shipping_method: String,
    pub subtotal: i32,
    pub shipping_fee: i32,
    pub total: i32,
    pub note: String,
    pub invoice_type: String,
    pub invoice_carrier_type: Option<String>,
    pub invoice_carrier_num: Option<String>,
    pub invoice_tax_id: Option<String>,
    pub invoice_title: Option<String>,
    pub invoice_address: Option<String>,
    pub invoice_love_code: Option<String>,
    pub needs_refund: bool,
    pub created_at: DateTime<Utc>,
    pub paid_at: Option<DateTime<Utc>>,
    pub shipped_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub cancelled_at: Option<DateTime<Utc>>,
    pub cancel_reason: Option<String>,
}

const ADMIN_ORDER_COLUMNS: &str = "id, order_no, user_id, status, email, recipient_name, recipient_phone, shipping_method,
     subtotal, shipping_fee, total, note, invoice_type, invoice_carrier_type, invoice_carrier_num, invoice_tax_id,
     invoice_title, invoice_address, invoice_love_code, needs_refund, created_at, paid_at, shipped_at, completed_at,
     cancelled_at, cancel_reason";

/// 後台看的發票：多 error 與 updated_at
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AdminInvoiceRow {
    pub status: String,
    pub invoice_no: Option<String>,
    pub invoice_date: Option<DateTime<Utc>>,
    pub random_number: Option<String>,
    pub error: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct AdminOrderDetail {
    #[serde(flatten)]
    pub order: AdminOrderRow,
    pub items: Vec<OrderItemRow>,
    pub shipment: Option<Shipment>,
    /// 全部付款嘗試，新到舊
    pub payments: Vec<Payment>,
    pub invoice: Option<AdminInvoiceRow>,
}

pub async fn get_detail(db: &PgPool, id: Uuid) -> Result<Option<AdminOrderDetail>, ApiError> {
    let sql = format!("SELECT {ADMIN_ORDER_COLUMNS} FROM orders WHERE id = $1");
    let Some(order) = sqlx::query_as::<_, AdminOrderRow>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(db)
        .await?
    else {
        return Ok(None);
    };
    let items = sqlx::query_as::<_, OrderItemRow>(
        "SELECT product_name, variant_label, unit_price, quantity, line_total, image_path
         FROM order_items WHERE order_id = $1 ORDER BY sort_order",
    )
    .bind(id)
    .fetch_all(db)
    .await?;
    let shipment = shipments::get_by_order(db, id).await?;
    let payments = payments::list_for_order(db, id).await?;
    let invoice = sqlx::query_as::<_, AdminInvoiceRow>(
        "SELECT status, invoice_no, invoice_date, random_number, error, updated_at FROM invoices WHERE order_id = $1",
    )
    .bind(id)
    .fetch_optional(db)
    .await?;
    Ok(Some(AdminOrderDetail {
        order,
        items,
        shipment,
        payments,
        invoice,
    }))
}

/// 後台標記完成（規格 §4）：shipped → completed，不看 shipments.status（與規格不同之處 47）
pub async fn complete(db: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let n = sqlx::query(
        "UPDATE orders SET status = $2, completed_at = now() WHERE id = $1 AND status = $3",
    )
    .bind(id)
    .bind(STATUS_COMPLETED)
    .bind(STATUS_SHIPPED)
    .execute(db)
    .await?
    .rows_affected();
    Ok(n > 0)
}
