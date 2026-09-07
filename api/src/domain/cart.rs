use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::orders::{self, OrderItemInput};
use crate::error::ApiError;

/// 購物車一列經伺服器核對後的結果（規格 §6.1 /cart 重新向伺服器驗證價格與庫存）
#[derive(Debug, Serialize)]
pub struct CheckedLine {
    pub variant_id: Uuid,
    pub product_slug: String,
    pub product_name: String,
    pub variant_label: String,
    pub price: i32,
    pub image_thumb: Option<String>,
    pub stock: i32,
    /// 伺服器夾過的數量：min(要求, 庫存)；不可買時 0
    pub qty: i32,
    pub available: bool,
    /// null | "unavailable"（下架／不存在）| "sold_out" | "qty_reduced"
    pub reason: Option<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct CartCheck {
    pub items: Vec<CheckedLine>,
    /// 只算 available 的列（用夾過的數量）
    pub subtotal: i32,
}

#[derive(sqlx::FromRow)]
struct LineRow {
    product_slug: String,
    product_name: String,
    option1_value: Option<String>,
    option2_value: Option<String>,
    price: i32,
    stock: i32,
    sellable: bool,
    image_thumb: Option<String>,
}

pub async fn check(db: &PgPool, items: &[OrderItemInput]) -> Result<CartCheck, ApiError> {
    if items.len() > orders::MAX_LINES {
        return Err(ApiError::field("items", "一次最多 50 種商品"));
    }
    let merged = orders::merge_items(items);
    let mut checked = Vec::with_capacity(merged.len());
    let mut subtotal = 0;
    for (variant_id, wanted) in merged {
        let row: Option<LineRow> = sqlx::query_as(
            "SELECT p.slug AS product_slug, p.name AS product_name, v.option1_value, v.option2_value, v.price, v.stock,
                    (p.status = 'active' AND v.is_active) AS sellable,
                    COALESCE((SELECT i.thumb_path FROM product_images i WHERE i.id = v.image_id),
                             (SELECT i.thumb_path FROM product_images i WHERE i.product_id = p.id ORDER BY i.sort_order LIMIT 1)) AS image_thumb
             FROM product_variants v JOIN products p ON p.id = v.product_id
             WHERE v.id = $1",
        )
        .bind(variant_id)
        .fetch_optional(db)
        .await?;
        let line = match row {
            None => CheckedLine {
                variant_id,
                product_slug: String::new(),
                product_name: "已下架的商品".to_string(),
                variant_label: String::new(),
                price: 0,
                image_thumb: None,
                stock: 0,
                qty: 0,
                available: false,
                reason: Some("unavailable"),
            },
            Some(row) => {
                let (qty, available, reason) = if !row.sellable {
                    (0, false, Some("unavailable"))
                } else if row.stock <= 0 {
                    (0, false, Some("sold_out"))
                } else {
                    let qty = wanted.min(row.stock);
                    (qty, true, (qty < wanted).then_some("qty_reduced"))
                };
                if available {
                    subtotal += row.price * qty;
                }
                CheckedLine {
                    variant_id,
                    variant_label: orders::variant_label(
                        row.option1_value.as_deref(),
                        row.option2_value.as_deref(),
                    ),
                    product_slug: row.product_slug,
                    product_name: row.product_name,
                    price: row.price,
                    image_thumb: row.image_thumb,
                    stock: row.stock.max(0),
                    qty,
                    available,
                    reason,
                }
            }
        };
        checked.push(line);
    }
    Ok(CartCheck {
        items: checked,
        subtotal,
    })
}
