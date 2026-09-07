//! 全方位金流（規格 §8.2）：建立付款表單。回呼解析在 Task 6 補在這個檔案下面。
use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::config::EcpayConfig;
use crate::domain::orders::{
    OrderDetail, PAYMENT_ATM, PAYMENT_CREDIT, PAYMENT_CVS_CODE, PaymentRow,
};
use crate::ecpay::{mac, time};

/// ATM 虛擬帳號幾天內有效（規格 §8.2）
pub const ATM_EXPIRE_DAYS: &str = "3";
/// 超商代碼幾分鐘內有效（規格 §8.2）
pub const CVS_STORE_EXPIRE_MINUTES: &str = "4320";
/// 綠界欄位長度上限
pub const TRADE_DESC_MAX: usize = 200;
pub const ITEM_NAME_MAX: usize = 400;

/// 給前端用隱藏表單 POST 的內容（規格 §7 第 6、7 點）
#[derive(Debug, Clone, Serialize)]
pub struct CheckoutForm {
    pub action: String,
    pub fields: BTreeMap<String, String>,
}

/// payments.method → ChoosePayment；cod 等不支援的回 None
pub fn choose_payment(method: &str) -> Option<&'static str> {
    match method {
        PAYMENT_CREDIT => Some("Credit"),
        PAYMENT_ATM => Some("ATM"),
        PAYMENT_CVS_CODE => Some("CVS"),
        _ => None,
    }
}

/// 取前 max 個字（不是 byte，中文不會切壞）
fn truncate_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// 品項用 `#` 串接，超長截斷（規格 §8.2）；名稱裡的 `#` 換成空白，免得被綠界當成分隔
pub fn item_name(order: &OrderDetail) -> String {
    let joined = order
        .items
        .iter()
        .map(|i| {
            format!(
                "{}({}) x {}",
                i.product_name.replace('#', " "),
                i.variant_label.replace('#', " "),
                i.quantity
            )
        })
        .collect::<Vec<_>>()
        .join("#");
    truncate_chars(&joined, ITEM_NAME_MAX)
}

/// 組出送往 AioCheckOut/V5 的欄位（規格 §8.2 的清單，一律全帶）。`now` 由呼叫者傳入，測試才能固定。
/// `guest_token` 有值時 ClientBackURL 帶 `?t=`（訪客回到訂單頁要靠它，規格 §7 第 8 點）。
pub fn checkout_form(
    cfg: &EcpayConfig,
    public_base_url: &str,
    shop_name: &str,
    order: &OrderDetail,
    payment: &PaymentRow,
    guest_token: Option<&str>,
    now: DateTime<Utc>,
) -> anyhow::Result<CheckoutForm> {
    let choose = choose_payment(&payment.method)
        .ok_or_else(|| anyhow::anyhow!("付款方式 {} 不能送綠界", payment.method))?;
    let order_id = order.order.id;
    let mut back_url = format!("{public_base_url}/orders/{order_id}");
    if let Some(token) = guest_token {
        back_url.push_str("?t=");
        back_url.push_str(token);
    }

    let mut fields = BTreeMap::new();
    let mut put = |k: &str, v: String| {
        fields.insert(k.to_string(), v);
    };
    put("MerchantID", cfg.aio.merchant_id.clone());
    put("MerchantTradeNo", payment.merchant_trade_no.clone());
    put("MerchantTradeDate", time::format_datetime(now));
    put("PaymentType", "aio".to_string());
    put("TotalAmount", payment.amount.to_string());
    put(
        "TradeDesc",
        truncate_chars(
            &format!("{shop_name} 訂單 {}", order.order.order_no),
            TRADE_DESC_MAX,
        ),
    );
    put("ItemName", item_name(order));
    put(
        "ReturnURL",
        format!("{public_base_url}/api/ecpay/payment/return"),
    );
    put("ChoosePayment", choose.to_string());
    put("ClientBackURL", back_url);
    put(
        "PaymentInfoURL",
        format!("{public_base_url}/api/ecpay/payment/info"),
    );
    put("ExpireDate", ATM_EXPIRE_DAYS.to_string());
    put("StoreExpireDate", CVS_STORE_EXPIRE_MINUTES.to_string());
    put("NeedExtraPaidInfo", "N".to_string());
    put("EncryptType", "1".to_string());
    put("CustomField1", order_id.to_string());

    let params: Vec<(String, String)> =
        fields.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    let mac = mac::check_mac_value(&cfg.aio.hash_key, &cfg.aio.hash_iv, &params);
    fields.insert("CheckMacValue".to_string(), mac);
    Ok(CheckoutForm {
        action: cfg.aio_checkout_url().to_string(),
        fields,
    })
}

/// 綠界回呼的共同欄位（ReturnURL 與 PaymentInfoURL 都有；規格 §8.2）。其餘原樣留在 `raw`
#[derive(Debug, Clone)]
pub struct Notification {
    pub merchant_trade_no: String,
    pub rtn_code: i32,
    pub rtn_msg: String,
    pub trade_no: String,
    pub trade_amt: i32,
    pub payment_type: String,
    /// `yyyy/MM/dd HH:mm:ss`（台北）；ReturnURL 才有
    pub payment_date: Option<DateTime<Utc>>,
    /// 測試環境「模擬付款」會帶 1（規格 §8.2）
    pub simulate_paid: bool,
    /// 以下 PaymentInfoURL 才有
    pub bank_code: Option<String>,
    pub v_account: Option<String>,
    pub payment_no: Option<String>,
    /// ATM 是 `yyyy/MM/dd`（當天 23:59:59），超商是 `yyyy/MM/dd HH:mm:ss`
    pub expire_at: Option<DateTime<Utc>>,
    /// 所有欄位原樣（含 CheckMacValue），存進 payments.raw 對帳用
    pub raw: Value,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CallbackError {
    #[error("CheckMacValue Error")]
    BadMac,
    #[error("缺少欄位 {0}")]
    Missing(&'static str),
}

/// 取欄位值（key 不分大小寫）；空字串當沒有
fn get<'a>(params: &'a [(String, String)], key: &str) -> Option<&'a str> {
    params
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, v)| v.as_str())
        .filter(|v| !v.is_empty())
}

/// 先驗簽章再解析。順序重要：沒過簽章什麼都不信
pub fn parse_notification(
    cfg: &EcpayConfig,
    params: &[(String, String)],
) -> Result<Notification, CallbackError> {
    if !mac::verify(&cfg.aio.hash_key, &cfg.aio.hash_iv, params) {
        return Err(CallbackError::BadMac);
    }
    let raw: Map<String, Value> = params
        .iter()
        .map(|(k, v)| (k.clone(), Value::String(v.clone())))
        .collect();
    Ok(Notification {
        merchant_trade_no: get(params, "MerchantTradeNo")
            .ok_or(CallbackError::Missing("MerchantTradeNo"))?
            .to_string(),
        rtn_code: get(params, "RtnCode")
            .and_then(|v| v.parse().ok())
            .ok_or(CallbackError::Missing("RtnCode"))?,
        rtn_msg: get(params, "RtnMsg").unwrap_or("").to_string(),
        trade_no: get(params, "TradeNo").unwrap_or("").to_string(),
        trade_amt: get(params, "TradeAmt")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
        payment_type: get(params, "PaymentType").unwrap_or("").to_string(),
        payment_date: get(params, "PaymentDate").and_then(time::parse_datetime),
        simulate_paid: get(params, "SimulatePaid") == Some("1"),
        bank_code: get(params, "BankCode").map(str::to_string),
        v_account: get(params, "vAccount").map(str::to_string),
        payment_no: get(params, "PaymentNo").map(str::to_string),
        expire_at: get(params, "ExpireDate").and_then(time::parse_expire),
        raw: Value::Object(raw),
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use chrono::TimeZone;
    use uuid::Uuid;

    use super::*;
    use crate::config::Config;
    use crate::domain::orders::{OrderItemRow, OrderRow};

    fn sample() -> (OrderDetail, PaymentRow) {
        let order = OrderRow {
            id: Uuid::parse_str("019a0000-0000-7000-8000-000000000001").unwrap(),
            order_no: "DS260906ABCD".to_string(),
            user_id: None,
            guest_token: "t".repeat(64),
            status: "pending_payment".to_string(),
            email: "a@b.co".to_string(),
            recipient_name: "王小明".to_string(),
            recipient_phone: "0912345678".to_string(),
            shipping_method: "home".to_string(),
            subtotal: 600,
            shipping_fee: 100,
            total: 700,
            note: String::new(),
            invoice_type: "personal".to_string(),
            invoice_carrier_type: Some("1".to_string()),
            invoice_carrier_num: None,
            invoice_tax_id: None,
            invoice_title: None,
            invoice_address: None,
            invoice_love_code: None,
            needs_refund: false,
            created_at: Utc::now(),
            paid_at: None,
            shipped_at: None,
            completed_at: None,
            cancelled_at: None,
            cancel_reason: None,
        };
        let items = vec![OrderItemRow {
            product_name: "雞肉狗糧 #1".to_string(),
            variant_label: "S".to_string(),
            unit_price: 300,
            quantity: 2,
            line_total: 600,
            image_path: None,
        }];
        let payment = PaymentRow {
            id: Uuid::now_v7(),
            merchant_trade_no: "DS260906ABCD01".to_string(),
            method: "credit".to_string(),
            status: "pending".to_string(),
            amount: 700,
            atm_bank_code: None,
            atm_vaccount: None,
            cvs_payment_no: None,
            expire_at: None,
        };
        (
            OrderDetail {
                order,
                items,
                shipment: None,
                payment: None,
                invoice: None,
            },
            payment,
        )
    }

    #[test]
    fn choose_payment_maps_methods() {
        assert_eq!(choose_payment("credit"), Some("Credit"));
        assert_eq!(choose_payment("atm"), Some("ATM"));
        assert_eq!(choose_payment("cvs_code"), Some("CVS"));
        assert_eq!(choose_payment("cod"), None);
    }

    #[test]
    fn item_name_joins_and_truncates() {
        let (mut order, _) = sample();
        assert_eq!(item_name(&order), "雞肉狗糧  1(S) x 2");
        order.items[0].product_name = "狗".repeat(500);
        assert_eq!(item_name(&order).chars().count(), 400);
    }

    #[test]
    fn checkout_form_has_every_field_and_valid_mac() {
        let cfg = Config::for_tests(PathBuf::from("/tmp")).ecpay;
        let (order, payment) = sample();
        let now = Utc.with_ymd_and_hms(2026, 9, 6, 7, 30, 23).unwrap();
        let form = checkout_form(
            &cfg,
            "https://shop.example.com",
            "狗狗商店",
            &order,
            &payment,
            Some("tok"),
            now,
        )
        .unwrap();
        assert_eq!(
            form.action,
            "https://payment-stage.ecpay.com.tw/Cashier/AioCheckOut/V5"
        );
        let f = &form.fields;
        assert_eq!(f["MerchantID"], "3002607");
        assert_eq!(f["MerchantTradeNo"], "DS260906ABCD01");
        assert_eq!(f["MerchantTradeDate"], "2026/09/06 15:30:23");
        assert_eq!(f["PaymentType"], "aio");
        assert_eq!(f["TotalAmount"], "700");
        assert_eq!(f["TradeDesc"], "狗狗商店 訂單 DS260906ABCD");
        assert_eq!(f["ItemName"], "雞肉狗糧  1(S) x 2");
        assert_eq!(
            f["ReturnURL"],
            "https://shop.example.com/api/ecpay/payment/return"
        );
        assert_eq!(f["ChoosePayment"], "Credit");
        assert_eq!(
            f["ClientBackURL"],
            format!("https://shop.example.com/orders/{}?t=tok", order.order.id)
        );
        assert_eq!(
            f["PaymentInfoURL"],
            "https://shop.example.com/api/ecpay/payment/info"
        );
        assert_eq!(f["ExpireDate"], "3");
        assert_eq!(f["StoreExpireDate"], "4320");
        assert_eq!(f["NeedExtraPaidInfo"], "N");
        assert_eq!(f["EncryptType"], "1");
        assert_eq!(f["CustomField1"], order.order.id.to_string());
        assert_eq!(f.len(), 17, "16 個欄位 + CheckMacValue");
        let params: Vec<(String, String)> = f.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        assert!(mac::verify(&cfg.aio.hash_key, &cfg.aio.hash_iv, &params));

        // 會員：ClientBackURL 不帶 ?t=
        let form = checkout_form(
            &cfg,
            "https://shop.example.com",
            "狗狗商店",
            &order,
            &payment,
            None,
            now,
        )
        .unwrap();
        assert_eq!(
            form.fields["ClientBackURL"],
            format!("https://shop.example.com/orders/{}", order.order.id)
        );

        // 不能送綠界的付款方式
        let (_, mut cod) = sample();
        cod.method = "cod".to_string();
        assert!(
            checkout_form(
                &cfg,
                "https://shop.example.com",
                "狗狗商店",
                &order,
                &cod,
                None,
                now
            )
            .is_err()
        );
    }

    #[test]
    fn parse_notification_requires_mac_then_reads_fields() {
        let cfg = Config::for_tests(PathBuf::from("/tmp")).ecpay;
        let mut params: Vec<(String, String)> = [
            ("MerchantID", "3002607"),
            ("MerchantTradeNo", "DS260906ABCD01"),
            ("RtnCode", "2"),
            ("RtnMsg", "Get VirtualAccount Succeeded"),
            ("TradeNo", "2609061530000002"),
            ("TradeAmt", "700"),
            ("PaymentType", "ATM_TAISHIN"),
            ("BankCode", "812"),
            ("vAccount", "1234567890123456"),
            ("ExpireDate", "2026/09/09"),
            ("SimulatePaid", "0"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        assert_eq!(
            parse_notification(&cfg, &params).unwrap_err(),
            CallbackError::BadMac
        );
        let mac = mac::check_mac_value(&cfg.aio.hash_key, &cfg.aio.hash_iv, &params);
        params.push(("CheckMacValue".to_string(), mac));
        let n = parse_notification(&cfg, &params).unwrap();
        assert_eq!(n.merchant_trade_no, "DS260906ABCD01");
        assert_eq!(n.rtn_code, 2);
        assert_eq!(n.trade_amt, 700);
        assert_eq!(n.bank_code.as_deref(), Some("812"));
        assert_eq!(n.v_account.as_deref(), Some("1234567890123456"));
        assert_eq!(
            n.expire_at,
            Some(Utc.with_ymd_and_hms(2026, 9, 9, 15, 59, 59).unwrap())
        );
        assert!(n.payment_date.is_none());
        assert!(!n.simulate_paid);
        assert_eq!(n.raw["vAccount"], "1234567890123456");
        assert!(n.raw.get("CheckMacValue").is_some());

        // 缺 MerchantTradeNo（簽章對）
        let mut short: Vec<(String, String)> = vec![("RtnCode".to_string(), "1".to_string())];
        let mac = mac::check_mac_value(&cfg.aio.hash_key, &cfg.aio.hash_iv, &short);
        short.push(("CheckMacValue".to_string(), mac));
        assert_eq!(
            parse_notification(&cfg, &short).unwrap_err(),
            CallbackError::Missing("MerchantTradeNo")
        );
    }
}
