//! 電子發票 B2C 開立（規格 §8.4）：組 Data、加密送 /B2CInvoice/Issue、解回應。
//! `InvoiceGateway` 讓測試換成 Fake，不打網路
use std::sync::Mutex;

use anyhow::Context;
use chrono::Utc;
use serde::Serialize;
use serde_json::{Value, json};

use crate::config::EcpayConfig;
use crate::domain::orders::{INVOICE_COMPANY, INVOICE_DONATION, OrderDetail};
use crate::ecpay::aes;

/// 綠界欄位長度上限
pub const ITEM_NAME_MAX: usize = 100;
pub const CUSTOMER_NAME_MAX: usize = 60;
pub const CUSTOMER_ADDR_MAX: usize = 100;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct IssueItem {
    #[serde(rename = "ItemSeq")]
    pub item_seq: i32,
    #[serde(rename = "ItemName")]
    pub item_name: String,
    #[serde(rename = "ItemCount")]
    pub item_count: i32,
    #[serde(rename = "ItemWord")]
    pub item_word: String,
    #[serde(rename = "ItemPrice")]
    pub item_price: i32,
    #[serde(rename = "ItemTaxType")]
    pub item_tax_type: String,
    #[serde(rename = "ItemAmount")]
    pub item_amount: i32,
    #[serde(rename = "ItemRemark")]
    pub item_remark: String,
}

/// 內層 Data（欄位名照綠界文件）
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct IssueRequest {
    #[serde(rename = "MerchantID")]
    pub merchant_id: String,
    #[serde(rename = "RelateNumber")]
    pub relate_number: String,
    #[serde(rename = "CustomerID")]
    pub customer_id: String,
    #[serde(rename = "CustomerIdentifier")]
    pub customer_identifier: String,
    #[serde(rename = "CustomerName")]
    pub customer_name: String,
    #[serde(rename = "CustomerAddr")]
    pub customer_addr: String,
    #[serde(rename = "CustomerPhone")]
    pub customer_phone: String,
    #[serde(rename = "CustomerEmail")]
    pub customer_email: String,
    #[serde(rename = "ClearanceMark")]
    pub clearance_mark: String,
    #[serde(rename = "Print")]
    pub print: String,
    #[serde(rename = "Donation")]
    pub donation: String,
    #[serde(rename = "LoveCode")]
    pub love_code: String,
    #[serde(rename = "CarrierType")]
    pub carrier_type: String,
    #[serde(rename = "CarrierNum")]
    pub carrier_num: String,
    #[serde(rename = "TaxType")]
    pub tax_type: String,
    #[serde(rename = "SalesAmount")]
    pub sales_amount: i32,
    #[serde(rename = "InvoiceRemark")]
    pub invoice_remark: String,
    #[serde(rename = "Items")]
    pub items: Vec<IssueItem>,
    #[serde(rename = "InvType")]
    pub inv_type: String,
    #[serde(rename = "vat")]
    pub vat: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueResponse {
    pub rtn_code: i32,
    pub rtn_msg: String,
    pub invoice_no: String,
    /// `yyyy-MM-dd HH:mm:ss`（台北）
    pub invoice_date: String,
    pub random_number: String,
    /// 解密後的整個 Data，存進 invoices.response
    pub raw: Value,
}

impl IssueResponse {
    pub fn is_ok(&self) -> bool {
        self.rtn_code == 1
    }
}

fn truncate_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// 依訂單的發票資料組 Data（規格 §8.4、與規格不同之處 29）。
/// 個人：CarrierType 1／2／3（1 時 CarrierNum 空）；公司：統編 + Print=1 + 抬頭 + 地址、不帶載具；
/// 捐贈：Donation=1 + 愛心碼。Items 每個 order_item 一列，運費 > 0 多一列「運費」；SalesAmount = total
pub fn build_issue_request(merchant_id: &str, order: &OrderDetail) -> IssueRequest {
    let o = &order.order;
    let mut items: Vec<IssueItem> = order
        .items
        .iter()
        .enumerate()
        .map(|(i, it)| IssueItem {
            item_seq: i as i32 + 1,
            item_name: truncate_chars(
                &format!("{}（{}）", it.product_name, it.variant_label),
                ITEM_NAME_MAX,
            ),
            item_count: it.quantity,
            item_word: "件".to_string(),
            item_price: it.unit_price,
            item_tax_type: "1".to_string(),
            item_amount: it.line_total,
            item_remark: String::new(),
        })
        .collect();
    if o.shipping_fee > 0 {
        items.push(IssueItem {
            item_seq: items.len() as i32 + 1,
            item_name: "運費".to_string(),
            item_count: 1,
            item_word: "式".to_string(),
            item_price: o.shipping_fee,
            item_tax_type: "1".to_string(),
            item_amount: o.shipping_fee,
            item_remark: String::new(),
        });
    }
    let empty = String::new;
    let (
        customer_identifier,
        customer_name,
        customer_addr,
        print,
        donation,
        love_code,
        carrier_type,
        carrier_num,
    ) = match o.invoice_type.as_str() {
        INVOICE_COMPANY => (
            o.invoice_tax_id.clone().unwrap_or_default(),
            o.invoice_title.clone().unwrap_or_default(),
            o.invoice_address.clone().unwrap_or_default(),
            "1",
            "0",
            empty(),
            empty(),
            empty(),
        ),
        INVOICE_DONATION => (
            empty(),
            o.recipient_name.clone(),
            empty(),
            "0",
            "1",
            o.invoice_love_code.clone().unwrap_or_default(),
            empty(),
            empty(),
        ),
        _ => (
            empty(),
            o.recipient_name.clone(),
            empty(),
            "0",
            "0",
            empty(),
            o.invoice_carrier_type
                .clone()
                .unwrap_or_else(|| "1".to_string()),
            o.invoice_carrier_num.clone().unwrap_or_default(),
        ),
    };
    IssueRequest {
        merchant_id: merchant_id.to_string(),
        relate_number: o.order_no.clone(),
        customer_id: empty(),
        customer_identifier,
        customer_name: truncate_chars(&customer_name, CUSTOMER_NAME_MAX),
        customer_addr: truncate_chars(&customer_addr, CUSTOMER_ADDR_MAX),
        customer_phone: o.recipient_phone.clone(),
        customer_email: o.email.clone(),
        clearance_mark: empty(),
        print: print.to_string(),
        donation: donation.to_string(),
        love_code,
        carrier_type,
        carrier_num,
        tax_type: "1".to_string(),
        sales_amount: o.total,
        invoice_remark: empty(),
        items,
        inv_type: "07".to_string(),
        vat: "1".to_string(),
    }
}

/// 解密後的 Data → IssueResponse
pub fn parse_issue_response(inner: Value) -> IssueResponse {
    let text = |k: &str| {
        inner
            .get(k)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    let rtn_code = inner.get("RtnCode").and_then(Value::as_i64).unwrap_or(0) as i32;
    let rtn_msg = text("RtnMsg");
    let invoice_no = text("InvoiceNo");
    let invoice_date = text("InvoiceDate");
    let random_number = text("RandomNumber");
    IssueResponse {
        rtn_code,
        rtn_msg,
        invoice_no,
        invoice_date,
        random_number,
        raw: inner,
    }
}

/// 真的打綠界
pub struct EcpayInvoiceClient {
    client: reqwest::Client,
    url: String,
    merchant_id: String,
    key: [u8; 16],
    iv: [u8; 16],
}

impl EcpayInvoiceClient {
    pub fn new(cfg: &EcpayConfig) -> anyhow::Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(20))
                .build()
                .context("建立 HTTP client")?,
            url: cfg.invoice_issue_url().to_string(),
            merchant_id: cfg.invoice.merchant_id.clone(),
            key: aes::key16(&cfg.invoice.hash_key)?,
            iv: aes::key16(&cfg.invoice.hash_iv)?,
        })
    }

    pub fn merchant_id(&self) -> &str {
        &self.merchant_id
    }

    pub async fn issue(&self, req: &IssueRequest) -> anyhow::Result<IssueResponse> {
        let data = aes::encode_data(&self.key, &self.iv, &serde_json::to_value(req)?);
        // Timestamp 是 Unix epoch 秒，沒有時區（規格 §8）
        let envelope = json!({
            "MerchantID": self.merchant_id,
            "RqHeader": { "Timestamp": Utc::now().timestamp() },
            "Data": data,
        });
        let response: Value = self
            .client
            .post(&self.url)
            .json(&envelope)
            .send()
            .await
            .context("連線綠界發票 API")?
            .error_for_status()
            .context("綠界發票 API HTTP 錯誤")?
            .json()
            .await
            .context("綠界發票回應不是 JSON")?;
        let trans_code = response
            .get("TransCode")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        if trans_code != 1 {
            anyhow::bail!(
                "綠界發票 TransCode {trans_code}：{}",
                response
                    .get("TransMsg")
                    .and_then(Value::as_str)
                    .unwrap_or("")
            );
        }
        let encoded = response
            .get("Data")
            .and_then(Value::as_str)
            .context("綠界發票回應缺 Data")?;
        Ok(parse_issue_response(aes::decode_data(
            &self.key, &self.iv, encoded,
        )?))
    }
}

/// 測試用：記錄請求、回預設或指定的結果
#[derive(Default)]
pub struct FakeInvoiceGateway {
    calls: Mutex<Vec<IssueRequest>>,
    next_error: Mutex<Option<String>>,
    next_rtn: Mutex<Option<(i32, String)>>,
}

impl FakeInvoiceGateway {
    pub fn calls(&self) -> Vec<IssueRequest> {
        self.calls.lock().expect("fake invoice calls").clone()
    }

    /// 下一次 issue 回連線層錯誤（Err）
    pub fn fail_next_with_error(&self, msg: &str) {
        *self.next_error.lock().expect("fake invoice next_error") = Some(msg.to_string());
    }

    /// 下一次 issue 回綠界的錯誤碼（Ok 但 RtnCode ≠ 1）
    pub fn fail_next_with_rtn(&self, code: i32, msg: &str) {
        *self.next_rtn.lock().expect("fake invoice next_rtn") = Some((code, msg.to_string()));
    }
}

/// 可替換的閘道：正式打綠界；測試用 Fake
pub enum InvoiceGateway {
    Ecpay(EcpayInvoiceClient),
    Fake(FakeInvoiceGateway),
}

impl InvoiceGateway {
    pub fn ecpay(cfg: &EcpayConfig) -> anyhow::Result<Self> {
        Ok(Self::Ecpay(EcpayInvoiceClient::new(cfg)?))
    }

    pub fn merchant_id(&self) -> &str {
        match self {
            Self::Ecpay(client) => client.merchant_id(),
            Self::Fake(_) => "2000132",
        }
    }

    pub async fn issue(&self, req: &IssueRequest) -> anyhow::Result<IssueResponse> {
        match self {
            Self::Ecpay(client) => client.issue(req).await,
            Self::Fake(fake) => {
                fake.calls
                    .lock()
                    .expect("fake invoice calls")
                    .push(req.clone());
                let error = fake
                    .next_error
                    .lock()
                    .expect("fake invoice next_error")
                    .take();
                if let Some(msg) = error {
                    anyhow::bail!("{msg}");
                }
                let rtn = fake.next_rtn.lock().expect("fake invoice next_rtn").take();
                if let Some((code, msg)) = rtn {
                    return Ok(IssueResponse {
                        rtn_code: code,
                        rtn_msg: msg.clone(),
                        invoice_no: String::new(),
                        invoice_date: String::new(),
                        random_number: String::new(),
                        raw: json!({ "RtnCode": code, "RtnMsg": msg }),
                    });
                }
                Ok(parse_issue_response(json!({
                    "RtnCode": 1,
                    "RtnMsg": "開立發票成功",
                    "InvoiceNo": "AB12345678",
                    "InvoiceDate": "2026-09-06 15:30:23",
                    "RandomNumber": "1234"
                })))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::*;
    use crate::domain::orders::{OrderItemRow, OrderRow};

    fn sample(invoice_type: &str) -> OrderDetail {
        let (carrier_type, carrier_num, tax_id, title, address, love_code) = match invoice_type {
            "company" => (
                None,
                None,
                Some("04595257"),
                Some("測試公司"),
                Some("台北市信義區市府路 1 號"),
                None,
            ),
            "donation" => (None, None, None, None, None, Some("168")),
            "mobile" => (Some("3"), Some("/ABC+123"), None, None, None, None),
            _ => (Some("1"), None, None, None, None, None),
        };
        let s = |v: Option<&str>| v.map(str::to_string);
        OrderDetail {
            order: OrderRow {
                id: Uuid::now_v7(),
                order_no: "DS260906ABCD".to_string(),
                user_id: None,
                guest_token: "t".repeat(64),
                status: "paid".to_string(),
                email: "a@b.co".to_string(),
                recipient_name: "王小明".to_string(),
                recipient_phone: "0912345678".to_string(),
                shipping_method: "home".to_string(),
                subtotal: 600,
                shipping_fee: 100,
                total: 700,
                note: String::new(),
                invoice_type: if invoice_type == "mobile" {
                    "personal".to_string()
                } else {
                    invoice_type.to_string()
                },
                invoice_carrier_type: s(carrier_type),
                invoice_carrier_num: s(carrier_num),
                invoice_tax_id: s(tax_id),
                invoice_title: s(title),
                invoice_address: s(address),
                invoice_love_code: s(love_code),
                needs_refund: false,
                created_at: Utc::now(),
                paid_at: Some(Utc::now()),
                shipped_at: None,
                completed_at: None,
                cancelled_at: None,
                cancel_reason: None,
            },
            items: vec![
                OrderItemRow {
                    product_name: "雞肉狗糧".to_string(),
                    variant_label: "S".to_string(),
                    unit_price: 200,
                    quantity: 2,
                    line_total: 400,
                    image_path: None,
                },
                OrderItemRow {
                    product_name: "牛肉狗糧".to_string(),
                    variant_label: "預設".to_string(),
                    unit_price: 200,
                    quantity: 1,
                    line_total: 200,
                    image_path: None,
                },
            ],
            shipment: None,
            payment: None,
            invoice: None,
        }
    }

    #[test]
    fn personal_with_ecpay_carrier() {
        let req = build_issue_request("2000132", &sample("personal"));
        assert_eq!(req.merchant_id, "2000132");
        assert_eq!(req.relate_number, "DS260906ABCD");
        assert_eq!(
            (req.carrier_type.as_str(), req.carrier_num.as_str()),
            ("1", "")
        );
        assert_eq!((req.print.as_str(), req.donation.as_str()), ("0", "0"));
        assert_eq!(req.customer_identifier, "");
        assert_eq!(req.customer_id, "");
        assert_eq!(req.customer_name, "王小明");
        assert_eq!(req.customer_email, "a@b.co");
        assert_eq!(req.customer_phone, "0912345678");
        assert_eq!(req.sales_amount, 700);
        assert_eq!(req.items.len(), 3, "兩個品項 + 運費");
        assert_eq!(
            req.items.iter().map(|i| i.item_seq).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert_eq!(req.items.iter().map(|i| i.item_amount).sum::<i32>(), 700);
        assert_eq!(req.items[0].item_name, "雞肉狗糧（S）");
        assert_eq!((req.items[0].item_count, req.items[0].item_price), (2, 200));
        assert_eq!(
            (
                req.items[2].item_name.as_str(),
                req.items[2].item_price,
                req.items[2].item_word.as_str()
            ),
            ("運費", 100, "式")
        );
        assert_eq!(
            (
                req.tax_type.as_str(),
                req.inv_type.as_str(),
                req.vat.as_str()
            ),
            ("1", "07", "1")
        );
    }

    #[test]
    fn mobile_barcode_company_and_donation() {
        let req = build_issue_request("2000132", &sample("mobile"));
        assert_eq!(
            (req.carrier_type.as_str(), req.carrier_num.as_str()),
            ("3", "/ABC+123")
        );

        let req = build_issue_request("2000132", &sample("company"));
        assert_eq!(req.customer_identifier, "04595257");
        assert_eq!(req.customer_name, "測試公司");
        assert_eq!(req.customer_addr, "台北市信義區市府路 1 號");
        assert_eq!((req.print.as_str(), req.donation.as_str()), ("1", "0"));
        assert_eq!(
            (req.carrier_type.as_str(), req.carrier_num.as_str()),
            ("", "")
        );

        let req = build_issue_request("2000132", &sample("donation"));
        assert_eq!(
            (
                req.print.as_str(),
                req.donation.as_str(),
                req.love_code.as_str()
            ),
            ("0", "1", "168")
        );
        assert_eq!(req.carrier_type, "");
        assert_eq!(req.customer_identifier, "");
    }

    #[test]
    fn no_shipping_fee_means_no_fee_line() {
        let mut order = sample("personal");
        order.order.shipping_fee = 0;
        order.order.total = 600;
        let req = build_issue_request("2000132", &order);
        assert_eq!(req.items.len(), 2);
        assert_eq!(req.sales_amount, 600);
    }

    #[test]
    fn serializes_with_ecpay_field_names() {
        let v = serde_json::to_value(build_issue_request("2000132", &sample("company"))).unwrap();
        assert_eq!(v["MerchantID"], "2000132");
        assert_eq!(v["RelateNumber"], "DS260906ABCD");
        assert_eq!(v["CustomerIdentifier"], "04595257");
        assert_eq!(v["Print"], "1");
        assert_eq!(v["SalesAmount"], 700);
        assert_eq!(v["Items"][0]["ItemSeq"], 1);
        assert_eq!(v["Items"][2]["ItemName"], "運費");
        assert_eq!(v["InvType"], "07");
        assert_eq!(v["vat"], "1");
        assert!(v.get("merchant_id").is_none(), "不能出現 snake_case");
    }

    #[test]
    fn parses_issue_response() {
        let resp = parse_issue_response(json!({
            "RtnCode": 1, "RtnMsg": "開立發票成功", "InvoiceNo": "AB12345678",
            "InvoiceDate": "2026-09-06 15:30:23", "RandomNumber": "1234"
        }));
        assert!(resp.is_ok());
        assert_eq!(resp.invoice_no, "AB12345678");
        assert_eq!(resp.random_number, "1234");
        assert_eq!(resp.raw["RtnMsg"], "開立發票成功");
        let bad =
            parse_issue_response(json!({ "RtnCode": 1000007, "RtnMsg": "RelateNumber 重複" }));
        assert!(!bad.is_ok());
        assert_eq!(bad.invoice_no, "");
    }

    #[tokio::test]
    async fn fake_gateway_records_and_fails_on_demand() {
        let gateway = InvoiceGateway::Fake(FakeInvoiceGateway::default());
        let req = build_issue_request("2000132", &sample("personal"));
        let ok = gateway.issue(&req).await.unwrap();
        assert!(ok.is_ok());
        assert_eq!(ok.invoice_no, "AB12345678");
        let InvoiceGateway::Fake(fake) = &gateway else {
            unreachable!()
        };
        fake.fail_next_with_rtn(1000007, "RelateNumber 重複");
        let bad = gateway.issue(&req).await.unwrap();
        assert_eq!(
            (bad.rtn_code, bad.rtn_msg.as_str()),
            (1000007, "RelateNumber 重複")
        );
        fake.fail_next_with_error("connect timeout");
        assert!(
            gateway
                .issue(&req)
                .await
                .unwrap_err()
                .to_string()
                .contains("connect timeout")
        );
        assert!(
            gateway.issue(&req).await.unwrap().is_ok(),
            "錯誤只影響下一次"
        );
        assert_eq!(fake.calls().len(), 4);
    }
}
