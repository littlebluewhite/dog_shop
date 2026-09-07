use serde::Deserialize;
use uuid::Uuid;

pub use crate::domain::addresses::is_postal_code;
use crate::domain::settings::{PaymentMethods, ShippingSettings};
pub use crate::domain::users::is_tw_mobile;
use crate::domain::users::{is_valid_email, normalize_email};
use crate::error::{ApiError, FieldErrors};

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
