use chrono::{DateTime, FixedOffset, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::auth::tokens::generate_token;
pub use crate::domain::addresses::is_postal_code;
use crate::domain::cvs_stores::{self, CvsStore};
use crate::domain::jobs;
use crate::domain::settings::{self, PaymentMethods, ShippingSettings};
pub use crate::domain::users::is_tw_mobile;
use crate::domain::users::{User, is_valid_email, normalize_email};
use crate::error::{ApiError, FieldErrors, ShortItem};

// ───── 常數（字串欄位的合法值，與 migration 的 CHECK 一致）─────

pub const STATUS_PENDING_PAYMENT: &str = "pending_payment";
pub const STATUS_PAID: &str = "paid";
pub const STATUS_SHIPPED: &str = "shipped";
pub const STATUS_COMPLETED: &str = "completed";
pub const STATUS_CANCELLED: &str = "cancelled";
pub const STATUS_REFUNDED: &str = "refunded";

pub const SHIPPING_CVS: &str = "cvs";
pub const SHIPPING_HOME: &str = "home";

pub const PAYMENT_CREDIT: &str = "credit";
pub const PAYMENT_ATM: &str = "atm";
pub const PAYMENT_CVS_CODE: &str = "cvs_code";

pub const INVOICE_PERSONAL: &str = "personal";
pub const INVOICE_COMPANY: &str = "company";
pub const INVOICE_DONATION: &str = "donation";

/// 超商取貨商品小計上限（綠界 C2C 限制，規格 §7）
pub const CVS_SUBTOTAL_LIMIT: i32 = 20_000;
pub const MAX_QTY_PER_LINE: i32 = 99;
pub const MAX_LINES: usize = 50;

// ───── 輸入 ─────

#[derive(Debug, Clone, Deserialize)]
pub struct OrderItemInput {
    pub variant_id: Uuid,
    pub qty: i32,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct HomeAddress {
    pub postal_code: String,
    pub city: String,
    pub district: String,
    pub street: String,
}

/// 發票資料（規格 §7 第 4 點）。JSON 的 key 是 `type`，Rust 用 `kind`
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct InvoiceInput {
    #[serde(rename = "type")]
    pub kind: String,
    pub carrier_type: String,
    pub carrier_num: String,
    pub tax_id: String,
    pub title: String,
    pub address: String,
    pub love_code: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OrderInput {
    pub items: Vec<OrderItemInput>,
    pub email: String,
    pub recipient_name: String,
    pub recipient_phone: String,
    pub shipping_method: String,
    #[serde(default)]
    pub cvs_store_token: Option<String>,
    #[serde(default)]
    pub address: Option<HomeAddress>,
    #[serde(default)]
    pub invoice: InvoiceInput,
    pub payment_method: String,
    #[serde(default)]
    pub note: String,
}

impl OrderInput {
    /// 去頭尾空白、Email 轉小寫、載具號碼轉大寫
    pub fn normalized(mut self) -> Self {
        self.email = normalize_email(&self.email);
        for s in [
            &mut self.recipient_name,
            &mut self.recipient_phone,
            &mut self.shipping_method,
            &mut self.payment_method,
            &mut self.note,
            &mut self.invoice.kind,
            &mut self.invoice.carrier_type,
            &mut self.invoice.tax_id,
            &mut self.invoice.title,
            &mut self.invoice.address,
            &mut self.invoice.love_code,
        ] {
            *s = s.trim().to_string();
        }
        self.invoice.carrier_num = self.invoice.carrier_num.trim().to_ascii_uppercase();
        if let Some(t) = &mut self.cvs_store_token {
            *t = t.trim().to_string();
        }
        if let Some(a) = &mut self.address {
            for s in [
                &mut a.postal_code,
                &mut a.city,
                &mut a.district,
                &mut a.street,
            ] {
                *s = s.trim().to_string();
            }
        }
        self
    }
}

// ───── 格式檢查（前端 validation.ts 有一模一樣的規則）─────

/// 超商取貨收件人：2～5 個中文字（規格 §8.3）
pub fn is_cvs_recipient_name(name: &str) -> bool {
    let n = name.chars().count();
    (2..=5).contains(&n) && name.chars().all(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
}

/// 手機條碼載具：/ 開頭 + 7 碼（0-9、A-Z、+、-、.）
pub fn is_mobile_barcode(s: &str) -> bool {
    s.len() == 8
        && s.starts_with('/')
        && s[1..].bytes().all(|b| {
            b.is_ascii_digit() || b.is_ascii_uppercase() || matches!(b, b'+' | b'-' | b'.')
        })
}

/// 自然人憑證條碼：2 個大寫英文字母 + 14 碼數字
pub fn is_citizen_cert(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 16
        && b[..2].iter().all(u8::is_ascii_uppercase)
        && b[2..].iter().all(u8::is_ascii_digit)
}

/// 捐贈愛心碼：3～7 碼數字
pub fn is_love_code(s: &str) -> bool {
    (3..=7).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit())
}

/// 統一編號檢查（財政部規則）：8 碼數字各乘 1,2,1,2,1,2,4,1，
/// 每個乘積「十位數 + 個位數」相加後總和能被 5 整除即合法（2023 年起由 10 改 5）；
/// 第 7 碼是 7 時（乘積 28 → 10，可視為 1），總和 + 1 能被 5 整除也合法。全 0 不算。
pub fn is_tw_tax_id(s: &str) -> bool {
    if s.len() != 8 || !s.bytes().all(|b| b.is_ascii_digit()) || s == "00000000" {
        return false;
    }
    const WEIGHTS: [u32; 8] = [1, 2, 1, 2, 1, 2, 4, 1];
    let digits: Vec<u32> = s.bytes().map(|b| u32::from(b - b'0')).collect();
    let sum: u32 = digits
        .iter()
        .zip(WEIGHTS)
        .map(|(d, w)| {
            let p = d * w;
            p / 10 + p % 10
        })
        .sum();
    sum.is_multiple_of(5) || (digits[6] == 7 && (sum + 1).is_multiple_of(5))
}

// ───── 金額 ─────

/// 免運門檻 0 = 不免運；否則商品小計 >= 門檻就免運（與規格不同之處 16）
pub fn shipping_fee(shipping: &ShippingSettings, method: &str, subtotal: i32) -> i32 {
    if shipping.free_threshold > 0 && subtotal >= shipping.free_threshold {
        return 0;
    }
    if method == SHIPPING_CVS {
        shipping.cvs_fee
    } else {
        shipping.home_fee
    }
}

/// 同一個規格出現多次就合併數量；數量夾在 1..=MAX_QTY_PER_LINE；保留第一次出現的順序
pub fn merge_items(items: &[OrderItemInput]) -> Vec<(Uuid, i32)> {
    let mut merged: Vec<(Uuid, i32)> = Vec::new();
    for item in items {
        let qty = item.qty.max(1);
        match merged.iter_mut().find(|(id, _)| *id == item.variant_id) {
            Some((_, q)) => *q = (*q + qty).min(MAX_QTY_PER_LINE),
            None => merged.push((item.variant_id, qty.min(MAX_QTY_PER_LINE))),
        }
    }
    merged
}

fn len_between(value: &str, min: usize, max: usize) -> bool {
    let n = value.chars().count();
    n >= min && n <= max
}

/// 欄位層級驗證（已 normalized 的輸入）。門市 token 是否存在、庫存夠不夠在 create_order 裡查資料庫才知道。
pub fn validate_input(
    input: &OrderInput,
    payment_methods: &PaymentMethods,
) -> Result<(), ApiError> {
    let mut errors = FieldErrors::new();

    if input.items.is_empty() {
        errors.add("items", "購物車是空的");
    } else if input.items.len() > MAX_LINES {
        errors.add("items", "一次最多 50 種商品");
    } else if input
        .items
        .iter()
        .any(|i| i.qty < 1 || i.qty > MAX_QTY_PER_LINE)
    {
        errors.add("items", "數量要在 1 到 99 之間");
    }

    if !is_valid_email(&input.email) {
        errors.add("email", "Email 格式不正確");
    }
    if !is_tw_mobile(&input.recipient_phone) {
        errors.add("recipient_phone", "手機格式：09 開頭共 10 碼");
    }

    match input.shipping_method.as_str() {
        SHIPPING_CVS => {
            if !is_cvs_recipient_name(&input.recipient_name) {
                errors.add("recipient_name", "超商取貨收件人請填 2～5 個中文字的本名");
            }
        }
        SHIPPING_HOME => {
            if !len_between(&input.recipient_name, 1, 20) {
                errors.add("recipient_name", "必填，最多 20 字");
            }
            match &input.address {
                None => errors.add("address.street", "請填寫收件地址"),
                Some(a) => {
                    if !is_postal_code(&a.postal_code) {
                        errors.add("address.postal_code", "郵遞區號 3～5 碼數字");
                    }
                    if !len_between(&a.city, 1, 10) {
                        errors.add("address.city", "請選縣市");
                    }
                    if !len_between(&a.district, 1, 10) {
                        errors.add("address.district", "請選鄉鎮市區");
                    }
                    if !len_between(&a.street, 1, 100) {
                        errors.add("address.street", "必填，最多 100 字");
                    }
                }
            }
        }
        _ => errors.add("shipping_method", "請選擇取貨方式"),
    }

    let inv = &input.invoice;
    match inv.kind.as_str() {
        INVOICE_PERSONAL => match inv.carrier_type.as_str() {
            "1" => {}
            "2" => {
                if !is_citizen_cert(&inv.carrier_num) {
                    errors.add(
                        "invoice.carrier_num",
                        "自然人憑證條碼：2 個英文字母 + 14 碼數字",
                    );
                }
            }
            "3" => {
                if !is_mobile_barcode(&inv.carrier_num) {
                    errors.add("invoice.carrier_num", "手機條碼：/ 開頭共 8 碼");
                }
            }
            _ => errors.add("invoice.carrier_type", "請選擇載具"),
        },
        INVOICE_COMPANY => {
            if !is_tw_tax_id(&inv.tax_id) {
                errors.add("invoice.tax_id", "統一編號格式不正確");
            }
            if !len_between(&inv.title, 1, 60) {
                errors.add("invoice.title", "必填，最多 60 字");
            }
            if !len_between(&inv.address, 1, 100) {
                errors.add("invoice.address", "必填，最多 100 字");
            }
        }
        INVOICE_DONATION => {
            if !is_love_code(&inv.love_code) {
                errors.add("invoice.love_code", "愛心碼 3～7 碼數字");
            }
        }
        _ => errors.add("invoice.type", "請選擇發票類型"),
    }

    let enabled = match input.payment_method.as_str() {
        PAYMENT_CREDIT => payment_methods.credit,
        PAYMENT_ATM => payment_methods.atm,
        PAYMENT_CVS_CODE => payment_methods.cvs_code,
        _ => {
            errors.add("payment_method", "請選擇付款方式");
            true
        }
    };
    if !enabled {
        errors.add("payment_method", "這個付款方式目前沒有開放");
    }

    if input.note.chars().count() > 200 {
        errors.add("note", "最多 200 字");
    }

    errors.into_result()
}

// ───── 輸出 ─────

#[derive(Debug, Serialize)]
pub struct OrderCreated {
    pub order_id: Uuid,
    pub order_no: String,
    pub guest_token: String,
}

/// 誰在看訂單：登入會員（看自己的）或帶 guest_token 的訪客
#[derive(Debug, Clone)]
pub enum Viewer {
    User(Uuid),
    Guest(String),
}

impl Viewer {
    fn user_id(&self) -> Option<Uuid> {
        match self {
            Viewer::User(id) => Some(*id),
            Viewer::Guest(_) => None,
        }
    }

    fn token(&self) -> Option<&str> {
        match self {
            Viewer::User(_) => None,
            Viewer::Guest(t) => Some(t.as_str()),
        }
    }
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct OrderRow {
    pub id: Uuid,
    pub order_no: String,
    #[serde(skip)]
    pub user_id: Option<Uuid>,
    #[serde(skip)]
    pub guest_token: String,
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
    #[serde(skip)]
    pub needs_refund: bool,
    pub created_at: DateTime<Utc>,
    pub paid_at: Option<DateTime<Utc>>,
    pub shipped_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub cancelled_at: Option<DateTime<Utc>>,
    pub cancel_reason: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct OrderItemRow {
    pub product_name: String,
    pub variant_label: String,
    pub unit_price: i32,
    pub quantity: i32,
    pub line_total: i32,
    pub image_path: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ShipmentRow {
    pub method: String,
    pub cvs_sub_type: Option<String>,
    pub cvs_store_id: Option<String>,
    pub cvs_store_name: Option<String>,
    pub cvs_store_address: Option<String>,
    pub home_postal_code: Option<String>,
    pub home_city: Option<String>,
    pub home_district: Option<String>,
    pub home_street: Option<String>,
    pub status: String,
    pub carrier: Option<String>,
    pub tracking_no: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PaymentRow {
    pub method: String,
    pub status: String,
    pub amount: i32,
    pub atm_bank_code: Option<String>,
    pub atm_vaccount: Option<String>,
    pub cvs_payment_no: Option<String>,
    pub expire_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct OrderDetail {
    #[serde(flatten)]
    pub order: OrderRow,
    pub items: Vec<OrderItemRow>,
    pub shipment: Option<ShipmentRow>,
    pub payment: Option<PaymentRow>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct OrderSummary {
    pub id: Uuid,
    pub order_no: String,
    pub status: String,
    pub total: i32,
    pub item_count: i32,
    pub created_at: DateTime<Utc>,
    #[serde(skip)]
    pub total_count: i64,
}

// ───── 下單 ─────

/// 去掉易混淆的 I、O、0、1（與規格不同之處 21）
const ORDER_NO_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

/// DS + 台北日期 yyMMdd + 4 碼隨機（規格 §3）
fn generate_order_no() -> String {
    let taipei = Utc::now().with_timezone(&FixedOffset::east_opt(8 * 3600).expect("utc+8"));
    let mut rng = rand::rng();
    let suffix: String = (0..4)
        .map(|_| ORDER_NO_ALPHABET[rng.random_range(0..ORDER_NO_ALPHABET.len())] as char)
        .collect();
    format!("DS{}{}", taipei.format("%y%m%d"), suffix)
}

async fn unique_order_no(tx: &mut Transaction<'_, Postgres>) -> Result<String, ApiError> {
    for _ in 0..5 {
        let candidate = generate_order_no();
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM orders WHERE order_no = $1)")
                .bind(&candidate)
                .fetch_one(&mut **tx)
                .await?;
        if !exists {
            return Ok(candidate);
        }
    }
    Err(ApiError::Internal(anyhow::anyhow!(
        "order_no 連續撞號 5 次"
    )))
}

#[derive(sqlx::FromRow)]
struct SnapshotRow {
    variant_id: Uuid,
    product_name: String,
    option1_value: Option<String>,
    option2_value: Option<String>,
    price: i32,
    image_path: Option<String>,
}

/// 「雞肉 / S」；沒規格就是「預設」。Task 8 的 domain/cart.rs 也用
pub fn variant_label(option1: Option<&str>, option2: Option<&str>) -> String {
    let label: Vec<&str> = [option1, option2].into_iter().flatten().collect();
    if label.is_empty() {
        "預設".to_string()
    } else {
        label.join(" / ")
    }
}

/// 規格 §7 第 6 點：一個交易內驗證、重算金額與運費、扣庫存（規格 §5）、寫 orders / order_items /
/// shipments / payments(pending)，排 send_email:order_created job。任何失敗整筆 rollback。
pub async fn create_order(
    db: &PgPool,
    input: OrderInput,
    user: Option<&User>,
) -> Result<OrderCreated, ApiError> {
    let input = input.normalized();
    let settings = settings::get_all(db).await?;
    validate_input(&input, &settings.payment_methods)?;
    let items = merge_items(&input.items);

    let mut tx = db.begin().await?;

    // 超商門市先查，免得扣了庫存才發現沒門市（雖然 rollback 也會還回去）
    let cvs_store: Option<CvsStore> = if input.shipping_method == SHIPPING_CVS {
        let token = input.cvs_store_token.as_deref().unwrap_or("");
        Some(
            cvs_stores::get_valid(&mut *tx, token)
                .await?
                .ok_or(ApiError::CvsStoreRequired)?,
        )
    } else {
        None
    };

    // 扣庫存 + 快照：影響筆數 0 = 不夠（或已下架）。全部檢查完再一次回報（規格 §5）
    let mut lines: Vec<(SnapshotRow, i32)> = Vec::with_capacity(items.len());
    let mut short: Vec<ShortItem> = Vec::new();
    for (variant_id, qty) in &items {
        let row: Option<SnapshotRow> = sqlx::query_as(
            "UPDATE product_variants v SET stock = v.stock - $2
             FROM products p
             WHERE v.id = $1 AND v.product_id = p.id AND p.status = 'active' AND v.is_active AND v.stock >= $2
             RETURNING v.id AS variant_id, p.name AS product_name, v.option1_value, v.option2_value, v.price,
                       COALESCE((SELECT i.path FROM product_images i WHERE i.id = v.image_id),
                                (SELECT i.path FROM product_images i WHERE i.product_id = p.id ORDER BY i.sort_order LIMIT 1)) AS image_path",
        )
        .bind(variant_id)
        .bind(qty)
        .fetch_optional(&mut *tx)
        .await?;
        match row {
            Some(row) => lines.push((row, *qty)),
            None => {
                let available: Option<i32> = sqlx::query_scalar(
                    "SELECT CASE WHEN p.status = 'active' AND v.is_active THEN v.stock ELSE 0 END
                     FROM product_variants v JOIN products p ON p.id = v.product_id WHERE v.id = $1",
                )
                .bind(variant_id)
                .fetch_optional(&mut *tx)
                .await?;
                short.push(ShortItem {
                    variant_id: *variant_id,
                    available: available.unwrap_or(0),
                });
            }
        }
    }
    if !short.is_empty() {
        return Err(ApiError::OutOfStock(short)); // tx 被 drop → rollback
    }

    let subtotal: i32 = lines.iter().map(|(row, qty)| row.price * qty).sum();
    if input.shipping_method == SHIPPING_CVS && subtotal > CVS_SUBTOTAL_LIMIT {
        return Err(ApiError::CvsAmountLimit);
    }
    let shipping_fee = shipping_fee(&settings.shipping, &input.shipping_method, subtotal);
    let total = subtotal + shipping_fee;

    let order_id = Uuid::now_v7();
    let order_no = unique_order_no(&mut tx).await?;
    let guest_token = generate_token();
    let inv = &input.invoice;
    let opt = |s: &str| (!s.is_empty()).then(|| s.to_string());
    let (carrier_type, carrier_num) = if inv.kind == INVOICE_PERSONAL {
        (
            opt(&inv.carrier_type),
            if inv.carrier_type == "1" {
                None
            } else {
                opt(&inv.carrier_num)
            },
        )
    } else {
        (None, None)
    };
    sqlx::query(
        "INSERT INTO orders (id, order_no, user_id, guest_token, status, email, recipient_name, recipient_phone,
                             shipping_method, subtotal, shipping_fee, total, note, invoice_type, invoice_carrier_type,
                             invoice_carrier_num, invoice_tax_id, invoice_title, invoice_address, invoice_love_code)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20)",
    )
    .bind(order_id)
    .bind(&order_no)
    .bind(user.map(|u| u.id))
    .bind(&guest_token)
    .bind(STATUS_PENDING_PAYMENT)
    .bind(&input.email)
    .bind(&input.recipient_name)
    .bind(&input.recipient_phone)
    .bind(&input.shipping_method)
    .bind(subtotal)
    .bind(shipping_fee)
    .bind(total)
    .bind(&input.note)
    .bind(&inv.kind)
    .bind(carrier_type)
    .bind(carrier_num)
    .bind((inv.kind == INVOICE_COMPANY).then(|| inv.tax_id.clone()))
    .bind((inv.kind == INVOICE_COMPANY).then(|| inv.title.clone()))
    .bind((inv.kind == INVOICE_COMPANY).then(|| inv.address.clone()))
    .bind((inv.kind == INVOICE_DONATION).then(|| inv.love_code.clone()))
    .execute(&mut *tx)
    .await?;

    for (i, (row, qty)) in lines.iter().enumerate() {
        sqlx::query(
            "INSERT INTO order_items (id, order_id, variant_id, product_name, variant_label, unit_price, quantity, line_total, image_path, sort_order)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(Uuid::now_v7())
        .bind(order_id)
        .bind(row.variant_id)
        .bind(&row.product_name)
        .bind(variant_label(row.option1_value.as_deref(), row.option2_value.as_deref()))
        .bind(row.price)
        .bind(qty)
        .bind(row.price * qty)
        .bind(&row.image_path)
        .bind(i as i32)
        .execute(&mut *tx)
        .await?;
    }

    let address = input.address.as_ref();
    sqlx::query(
        "INSERT INTO shipments (id, order_id, method, cvs_sub_type, cvs_store_id, cvs_store_name, cvs_store_address, cvs_store_phone,
                                home_postal_code, home_city, home_district, home_street)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
    )
    .bind(Uuid::now_v7())
    .bind(order_id)
    .bind(&input.shipping_method)
    .bind(cvs_store.as_ref().map(|s| s.sub_type.clone()))
    .bind(cvs_store.as_ref().map(|s| s.store_id.clone()))
    .bind(cvs_store.as_ref().map(|s| s.store_name.clone()))
    .bind(cvs_store.as_ref().map(|s| s.store_address.clone()))
    .bind(cvs_store.as_ref().map(|s| s.store_phone.clone()))
    .bind(address.map(|a| a.postal_code.clone()))
    .bind(address.map(|a| a.city.clone()))
    .bind(address.map(|a| a.district.clone()))
    .bind(address.map(|a| a.street.clone()))
    .execute(&mut *tx)
    .await?;

    // 第一筆付款嘗試：merchant_trade_no = order_no + "01"（規格 §3）；綠界欄位計畫 3 填
    sqlx::query(
        "INSERT INTO payments (id, order_id, merchant_trade_no, method, amount) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::now_v7())
    .bind(order_id)
    .bind(format!("{order_no}01"))
    .bind(&input.payment_method)
    .bind(total)
    .execute(&mut *tx)
    .await?;

    jobs::enqueue(
        &mut tx,
        jobs::KIND_SEND_EMAIL,
        json!({ "template": "order_created", "order_id": order_id }),
        Some(&format!("email:order_created:{order_id}")),
    )
    .await?;

    tx.commit().await?;
    Ok(OrderCreated {
        order_id,
        order_no,
        guest_token,
    })
}

// ───── 查詢 ─────

const ORDER_COLUMNS: &str = "id, order_no, user_id, guest_token, status, email, recipient_name, recipient_phone, shipping_method,
     subtotal, shipping_fee, total, note, invoice_type, invoice_carrier_type, invoice_carrier_num, invoice_tax_id,
     invoice_title, invoice_address, invoice_love_code, needs_refund, created_at, paid_at, shipped_at, completed_at,
     cancelled_at, cancel_reason";

/// 會員看自己的、訪客用 guest_token；都不符回 None（→ 404，不用 403 免得被猜 id）
pub async fn get_for_viewer(
    db: &PgPool,
    id: Uuid,
    viewer: &Viewer,
) -> Result<Option<OrderDetail>, ApiError> {
    let sql = format!(
        "SELECT {ORDER_COLUMNS} FROM orders
         WHERE id = $1 AND ((user_id IS NOT NULL AND user_id = $2) OR ($3::text IS NOT NULL AND guest_token = $3))"
    );
    let Some(order) = sqlx::query_as::<_, OrderRow>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .bind(viewer.user_id())
        .bind(viewer.token())
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
    let shipment = sqlx::query_as::<_, ShipmentRow>(
        "SELECT method, cvs_sub_type, cvs_store_id, cvs_store_name, cvs_store_address, home_postal_code, home_city,
                home_district, home_street, status, carrier, tracking_no
         FROM shipments WHERE order_id = $1",
    )
    .bind(id)
    .fetch_optional(db)
    .await?;
    let payment = sqlx::query_as::<_, PaymentRow>(
        "SELECT method, status, amount, atm_bank_code, atm_vaccount, cvs_payment_no, expire_at
         FROM payments WHERE order_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(id)
    .fetch_optional(db)
    .await?;
    Ok(Some(OrderDetail {
        order,
        items,
        shipment,
        payment,
    }))
}

/// 會員中心的訂單列表（新到舊）
pub async fn list_for_user(
    db: &PgPool,
    user_id: Uuid,
    page: i64,
    per_page: i64,
) -> Result<crate::domain::products::Page<OrderSummary>, ApiError> {
    let rows = sqlx::query_as::<_, OrderSummary>(
        "SELECT o.id, o.order_no, o.status, o.total, o.created_at,
                (SELECT COALESCE(SUM(oi.quantity), 0) FROM order_items oi WHERE oi.order_id = o.id)::int AS item_count,
                COUNT(*) OVER () AS total_count
         FROM orders o WHERE o.user_id = $1
         ORDER BY o.created_at DESC LIMIT $2 OFFSET $3",
    )
    .bind(user_id)
    .bind(per_page)
    .bind((page - 1) * per_page)
    .fetch_all(db)
    .await?;
    let total = rows.first().map(|r| r.total_count).unwrap_or(0);
    Ok(crate::domain::products::Page {
        items: rows,
        total,
        page,
        per_page,
    })
}

// ───── 取消 ─────

/// 只有 pending_payment 能取消；成功就把 order_items 的數量加回庫存（規格 §4、§5）。
/// 回 Ok(false) 表示狀態不允許。計畫 3 的過期 job、計畫 4 的後台取消也用這個。
pub async fn cancel_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    order_id: Uuid,
    reason: &str,
) -> Result<bool, ApiError> {
    let updated = sqlx::query(
        "UPDATE orders SET status = $3, cancelled_at = now(), cancel_reason = $2 WHERE id = $1 AND status = $4",
    )
    .bind(order_id)
    .bind(reason)
    .bind(STATUS_CANCELLED)
    .bind(STATUS_PENDING_PAYMENT)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    if updated == 0 {
        return Ok(false);
    }
    sqlx::query(
        "UPDATE product_variants v SET stock = v.stock + oi.quantity
         FROM order_items oi WHERE oi.order_id = $1 AND v.id = oi.variant_id",
    )
    .bind(order_id)
    .execute(&mut **tx)
    .await?;
    Ok(true)
}

/// 買家取消（與規格不同之處 13）：看不到 → NotFound；狀態不對 → VALIDATION
pub async fn cancel(db: &PgPool, id: Uuid, viewer: &Viewer, reason: &str) -> Result<(), ApiError> {
    let mut tx = db.begin().await?;
    let visible: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM orders
                        WHERE id = $1 AND ((user_id IS NOT NULL AND user_id = $2) OR ($3::text IS NOT NULL AND guest_token = $3)))",
    )
    .bind(id)
    .bind(viewer.user_id())
    .bind(viewer.token())
    .fetch_one(&mut *tx)
    .await?;
    if !visible {
        return Err(ApiError::NotFound);
    }
    if !cancel_in_tx(&mut tx, id, reason).await? {
        return Err(ApiError::Validation {
            message: "這筆訂單已經不能取消".to_string(),
            details: Value::Null,
        });
    }
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home_input() -> OrderInput {
        OrderInput {
            items: vec![OrderItemInput {
                variant_id: Uuid::now_v7(),
                qty: 1,
            }],
            email: "a@b.co".to_string(),
            recipient_name: "王小明".to_string(),
            recipient_phone: "0912345678".to_string(),
            shipping_method: SHIPPING_HOME.to_string(),
            cvs_store_token: None,
            address: Some(HomeAddress {
                postal_code: "100".to_string(),
                city: "臺北市".to_string(),
                district: "中正區".to_string(),
                street: "重慶南路一段 122 號".to_string(),
            }),
            invoice: InvoiceInput {
                kind: INVOICE_PERSONAL.to_string(),
                carrier_type: "1".to_string(),
                ..Default::default()
            },
            payment_method: PAYMENT_CREDIT.to_string(),
            note: String::new(),
        }
    }

    fn fields(err: ApiError) -> serde_json::Value {
        match err {
            ApiError::Validation { details, .. } => details["fields"].clone(),
            other => panic!("expected validation, got {other:?}"),
        }
    }

    #[test]
    fn tax_id_checksum() {
        // 04595257：乘積 0,8,5,18,5,4,20,7 → 0+8+5+9+5+4+2+7 = 40 → 整除 5
        assert!(is_tw_tax_id("04595257"));
        // 10000004：1 + 4 = 5
        assert!(is_tw_tax_id("10000004"));
        // 12345675：1+4+3+8+5+3+10+5 = 39，第 7 碼是 7，39+1 = 40 → 合法
        assert!(is_tw_tax_id("12345675"));
        // 12345678：總和 42，第 7 碼是 7 但 43 也不整除 → 不合法
        assert!(!is_tw_tax_id("12345678"));
        // 12345674：總和 38、39 都不整除
        assert!(!is_tw_tax_id("12345674"));
        assert!(!is_tw_tax_id("1234567"));
        assert!(!is_tw_tax_id("1234567a"));
        assert!(!is_tw_tax_id("00000000"));
    }

    #[test]
    fn format_helpers() {
        assert!(is_cvs_recipient_name("王小明"));
        assert!(!is_cvs_recipient_name("王"));
        assert!(!is_cvs_recipient_name("John"));
        assert!(!is_cvs_recipient_name("王小明王小明"));
        assert!(is_mobile_barcode("/ABC+123"));
        assert!(!is_mobile_barcode("ABC+1234"));
        assert!(!is_mobile_barcode("/abc+123"));
        assert!(is_citizen_cert("AB12345678901234"));
        assert!(!is_citizen_cert("A123456789012345"));
        assert!(!is_citizen_cert("中A345678901234"));
        assert!(is_love_code("168"));
        assert!(!is_love_code("12"));
        assert!(!is_love_code("12345678"));
    }

    #[test]
    fn fee_rules() {
        let s = ShippingSettings {
            cvs_fee: 60,
            home_fee: 100,
            free_threshold: 0,
        };
        assert_eq!(shipping_fee(&s, SHIPPING_CVS, 99_999), 60);
        assert_eq!(shipping_fee(&s, SHIPPING_HOME, 1), 100);
        let s = ShippingSettings {
            free_threshold: 1000,
            ..s
        };
        assert_eq!(shipping_fee(&s, SHIPPING_HOME, 999), 100);
        assert_eq!(shipping_fee(&s, SHIPPING_HOME, 1000), 0);
        assert_eq!(shipping_fee(&s, SHIPPING_CVS, 1000), 0);
    }

    #[test]
    fn merge_items_sums_and_caps() {
        let a = Uuid::now_v7();
        let b = Uuid::now_v7();
        let merged = merge_items(&[
            OrderItemInput {
                variant_id: a,
                qty: 2,
            },
            OrderItemInput {
                variant_id: b,
                qty: 1,
            },
            OrderItemInput {
                variant_id: a,
                qty: 98,
            },
        ]);
        assert_eq!(merged, vec![(a, 99), (b, 1)]);
    }

    #[test]
    fn valid_home_input_passes() {
        assert!(validate_input(&home_input(), &PaymentMethods::default()).is_ok());
    }

    #[test]
    fn home_needs_address_and_cvs_needs_chinese_name() {
        let mut input = home_input();
        input.address = None;
        let f = fields(validate_input(&input, &PaymentMethods::default()).unwrap_err());
        assert_eq!(f["address.street"], "請填寫收件地址");

        let mut input = home_input();
        input.shipping_method = SHIPPING_CVS.to_string();
        input.recipient_name = "Amy".to_string();
        let f = fields(validate_input(&input, &PaymentMethods::default()).unwrap_err());
        assert!(f["recipient_name"].as_str().unwrap().contains("中文"));
    }

    #[test]
    fn invoice_rules() {
        let mut input = home_input();
        input.invoice = InvoiceInput {
            kind: INVOICE_COMPANY.to_string(),
            tax_id: "12345678".to_string(),
            ..Default::default()
        };
        let f = fields(validate_input(&input, &PaymentMethods::default()).unwrap_err());
        assert_eq!(f["invoice.tax_id"], "統一編號格式不正確");
        assert!(f.get("invoice.title").is_some());
        assert!(f.get("invoice.address").is_some());

        let mut input = home_input();
        input.invoice = InvoiceInput {
            kind: INVOICE_PERSONAL.to_string(),
            carrier_type: "3".to_string(),
            carrier_num: "/ABC+123".to_string(),
            ..Default::default()
        };
        assert!(validate_input(&input, &PaymentMethods::default()).is_ok());

        let mut input = home_input();
        input.invoice = InvoiceInput {
            kind: INVOICE_DONATION.to_string(),
            love_code: "x".to_string(),
            ..Default::default()
        };
        let f = fields(validate_input(&input, &PaymentMethods::default()).unwrap_err());
        assert_eq!(f["invoice.love_code"], "愛心碼 3～7 碼數字");
    }

    #[test]
    fn disabled_payment_method_is_rejected() {
        let mut input = home_input();
        input.payment_method = PAYMENT_ATM.to_string();
        let pm = PaymentMethods {
            atm: false,
            ..Default::default()
        };
        let f = fields(validate_input(&input, &pm).unwrap_err());
        assert_eq!(f["payment_method"], "這個付款方式目前沒有開放");
        input.payment_method = "bitcoin".to_string();
        let f = fields(validate_input(&input, &pm).unwrap_err());
        assert_eq!(f["payment_method"], "請選擇付款方式");
    }

    #[test]
    fn empty_items_and_long_note() {
        let mut input = home_input();
        input.items = vec![];
        input.note = "x".repeat(201);
        let f = fields(validate_input(&input, &PaymentMethods::default()).unwrap_err());
        assert_eq!(f["items"], "購物車是空的");
        assert_eq!(f["note"], "最多 200 字");
    }
}
