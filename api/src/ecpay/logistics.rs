//! 綠界物流 C2C 超商取貨（規格 §8.3）：電子地圖表單、建立物流單、列印託運單、狀態回呼解析、
//! 貨態代碼對照。網路呼叫只在 `LogisticsGateway`；其餘都是純函式，測試不打綠界。
//! 綠界文件的查證結果見計畫 4 的「環境事實」。
use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;

use anyhow::Context;
use chrono::{DateTime, Utc};
use serde_json::{Map, Value};

use crate::config::EcpayConfig;
use crate::domain::orders::OrderDetail;
use crate::domain::shipments::{
    SHIPMENT_ARRIVED, SHIPMENT_CREATED, SHIPMENT_IN_TRANSIT, SHIPMENT_PICKED_UP, SHIPMENT_RETURNED,
};
use crate::ecpay::aio::{CallbackError, CheckoutForm};
use crate::ecpay::{mac, time};

/// 綠界 C2C 超商代碼（規格 §3）
pub const SUB_TYPES: &[&str] = &["UNIMARTC2C", "FAMIC2C", "HILIFEC2C"];
/// GoodsAmount 範圍（規格 §8.3）
pub const GOODS_AMOUNT_MIN: i32 = 1;
pub const GOODS_AMOUNT_MAX: i32 = 20_000;
/// GoodsName 上限：50 個「寬度」，中文等非 ASCII 算 2（綠界文件）
pub const GOODS_NAME_WIDTH_MAX: usize = 50;
/// SenderName 上限：10 個寬度（中文 5 字）
pub const SENDER_NAME_WIDTH_MAX: usize = 10;
/// ReceiverEmail 上限（綠界 String(50)）
pub const RECEIVER_EMAIL_MAX: usize = 50;
/// 綠界 GoodsName／姓名不得含的符號
pub const FORBIDDEN_CHARS: &[char] = &[
    '^', '\'', '`', '!', '@', '#', '%', '&', '*', '+', '\\', '"', '<', '>', '|', '_', '[', ']',
];
/// 建單的 HTTP 逾時（同發票 API）
pub const HTTP_TIMEOUT_SECS: u64 = 20;

pub fn is_sub_type(s: &str) -> bool {
    SUB_TYPES.contains(&s)
}

fn truncate_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

fn field<'a>(params: &'a [(String, String)], name: &str) -> Option<&'a str> {
    params
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

fn required<'a>(
    params: &'a [(String, String)],
    name: &'static str,
) -> Result<&'a str, CallbackError> {
    field(params, name)
        .filter(|v| !v.trim().is_empty())
        .ok_or(CallbackError::Missing(name))
}

fn params_to_json(params: &[(String, String)]) -> Value {
    Value::Object(
        params
            .iter()
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect::<Map<String, Value>>(),
    )
}

fn form_params(fields: &BTreeMap<String, String>) -> Vec<(String, String)> {
    fields.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
}

fn sign(cfg: &EcpayConfig, fields: &mut BTreeMap<String, String>) {
    let mac = mac::check_mac_value_md5(
        &cfg.logistics.hash_key,
        &cfg.logistics.hash_iv,
        &form_params(fields),
    );
    fields.insert("CheckMacValue".to_string(), mac);
}

// ───── 電子地圖 ─────

/// 電子地圖表單（規格 §8.3；不需 CheckMacValue）。token 同時當 MerchantTradeNo 與 ExtraData（都 ≤ 20 字）
pub fn map_form(
    cfg: &EcpayConfig,
    public_base_url: &str,
    token: &str,
    sub_type: &str,
    device_mobile: bool,
) -> CheckoutForm {
    let mut fields = BTreeMap::new();
    let mut put = |k: &str, v: String| {
        fields.insert(k.to_string(), v);
    };
    put("MerchantID", cfg.logistics.merchant_id.clone());
    put("MerchantTradeNo", token.to_string());
    put("LogisticsType", "CVS".to_string());
    put("LogisticsSubType", sub_type.to_string());
    put("IsCollection", "N".to_string());
    put(
        "ServerReplyURL",
        format!("{public_base_url}/api/ecpay/logistics/map-reply"),
    );
    put("ExtraData", token.to_string());
    put("Device", if device_mobile { "1" } else { "0" }.to_string());
    CheckoutForm {
        action: format!("{}/Express/map", cfg.logistics_base_url()),
        fields,
    }
}

/// 綠界地圖用買家瀏覽器 POST 回 map-reply 的欄位（規格 §8.3）；7-11 沒有電話。沒有簽章
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapReply {
    pub token: String,
    pub sub_type: String,
    pub store_id: String,
    pub store_name: String,
    pub store_address: String,
    pub store_phone: String,
    /// CVSOutSide = 1（離島）
    pub outside: bool,
}

pub fn parse_map_reply(params: &[(String, String)]) -> Result<MapReply, CallbackError> {
    let token = truncate_chars(required(params, "ExtraData")?.trim(), 20);
    let sub_type = required(params, "LogisticsSubType")?.trim().to_string();
    if !is_sub_type(&sub_type) {
        return Err(CallbackError::Missing("LogisticsSubType"));
    }
    Ok(MapReply {
        token,
        sub_type,
        store_id: truncate_chars(required(params, "CVSStoreID")?.trim(), 20),
        store_name: truncate_chars(required(params, "CVSStoreName")?.trim(), 40),
        store_address: truncate_chars(required(params, "CVSAddress")?.trim(), 120),
        store_phone: truncate_chars(field(params, "CVSTelephone").unwrap_or("").trim(), 20),
        outside: field(params, "CVSOutSide").is_some_and(|v| v.trim() == "1"),
    })
}

// ───── 建立物流單 ─────

/// 建單要的資料（routes/admin_orders 從訂單與設定湊出來）
#[derive(Debug, Clone)]
pub struct CreateRequest {
    pub merchant_trade_no: String,
    pub sub_type: String,
    /// 商品小計（與規格不同之處 36）
    pub goods_amount: i32,
    pub goods_name: String,
    pub sender_name: String,
    pub sender_phone: String,
    pub receiver_name: String,
    pub receiver_phone: String,
    pub receiver_email: String,
    pub receiver_store_id: String,
    pub return_store_id: Option<String>,
}

pub fn create_url(cfg: &EcpayConfig) -> String {
    format!("{}/Express/Create", cfg.logistics_base_url())
}

/// 組出 POST /Express/Create 的欄位（規格 §8.3 的清單，含 MD5 CheckMacValue）。
/// `now` 由呼叫者傳入，測試才能固定。幕後建單不帶 ClientReplyURL
pub fn create_fields(
    cfg: &EcpayConfig,
    public_base_url: &str,
    req: &CreateRequest,
    now: DateTime<Utc>,
) -> BTreeMap<String, String> {
    let mut fields = BTreeMap::new();
    let mut put = |k: &str, v: String| {
        fields.insert(k.to_string(), v);
    };
    put("MerchantID", cfg.logistics.merchant_id.clone());
    put("MerchantTradeNo", req.merchant_trade_no.clone());
    put("MerchantTradeDate", time::format_datetime(now));
    put("LogisticsType", "CVS".to_string());
    put("LogisticsSubType", req.sub_type.clone());
    put(
        "GoodsAmount",
        req.goods_amount
            .clamp(GOODS_AMOUNT_MIN, GOODS_AMOUNT_MAX)
            .to_string(),
    );
    put("GoodsName", req.goods_name.clone());
    put("SenderName", req.sender_name.clone());
    put("SenderCellPhone", req.sender_phone.clone());
    put("ReceiverName", req.receiver_name.clone());
    put("ReceiverCellPhone", req.receiver_phone.clone());
    if !req.receiver_email.is_empty() && req.receiver_email.chars().count() <= RECEIVER_EMAIL_MAX {
        put("ReceiverEmail", req.receiver_email.clone());
    }
    put("ReceiverStoreID", req.receiver_store_id.clone());
    if let Some(id) = req.return_store_id.as_ref().filter(|s| !s.is_empty()) {
        put("ReturnStoreID", id.clone());
    }
    put(
        "ServerReplyURL",
        format!("{public_base_url}/api/ecpay/logistics/status"),
    );
    // 7-11 C2C 必填；門市關轉店等通知會打這裡（與規格不同之處 35）
    put(
        "LogisticsC2CReplyURL",
        format!("{public_base_url}/api/ecpay/logistics/store-update"),
    );
    put("IsCollection", "N".to_string());
    sign(cfg, &mut fields);
    fields
}

/// `/Express/Create` 成功回應（`1|k=v&…`）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateOk {
    pub logistics_id: String,
    pub rtn_code: i32,
    pub rtn_msg: String,
    pub cvs_payment_no: String,
    /// 7-11 才有
    pub cvs_validation_no: String,
    pub raw: Value,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CreateError {
    /// 綠界回 `0|訊息`
    #[error("綠界拒絕建單：{0}")]
    Rejected(String),
    /// 不是 `1|…`／`0|…`，或成功回應缺欄位
    #[error("綠界回應格式不符：{0}")]
    Malformed(String),
    /// 成功回應的 CheckMacValue 不符（與規格不同之處 40）
    #[error("綠界回應的 CheckMacValue 不符")]
    BadMac,
}

/// 解析建單回應並驗 MD5 MAC。回應本身不含秘密可以存進 raw；不要把 mac::raw_string 印進 log
pub fn parse_create_response(cfg: &EcpayConfig, body: &str) -> Result<CreateOk, CreateError> {
    let body = body.trim();
    let Some((flag, rest)) = body.split_once('|') else {
        return Err(CreateError::Malformed(truncate_chars(body, 200)));
    };
    match flag.trim() {
        "1" => {}
        "0" => return Err(CreateError::Rejected(truncate_chars(rest.trim(), 200))),
        other => {
            return Err(CreateError::Malformed(format!(
                "開頭是 {}",
                truncate_chars(other, 200)
            )));
        }
    }
    let params: Vec<(String, String)> = form_urlencoded::parse(rest.as_bytes())
        .into_owned()
        .collect();
    if !mac::verify_md5(&cfg.logistics.hash_key, &cfg.logistics.hash_iv, &params) {
        return Err(CreateError::BadMac);
    }
    let get = |name: &str| field(&params, name).unwrap_or("").trim().to_string();
    let logistics_id = get("AllPayLogisticsID");
    if logistics_id.is_empty() {
        return Err(CreateError::Malformed("缺 AllPayLogisticsID".to_string()));
    }
    Ok(CreateOk {
        logistics_id,
        rtn_code: get("RtnCode").parse().unwrap_or(0),
        rtn_msg: truncate_chars(&get("RtnMsg"), 200),
        cvs_payment_no: get("CVSPaymentNo"),
        cvs_validation_no: get("CVSValidationNo"),
        raw: params_to_json(&params),
    })
}

// ───── 列印託運單 ─────

/// 列印託運單的表單（規格 §8.3）：前端在新分頁 POST；只有 7-11 帶 CVSValidationNo
pub fn print_form(
    cfg: &EcpayConfig,
    sub_type: &str,
    logistics_id: &str,
    cvs_payment_no: &str,
    cvs_validation_no: &str,
) -> anyhow::Result<CheckoutForm> {
    let path = match sub_type {
        "UNIMARTC2C" => "/Express/PrintUniMartC2COrderInfo",
        "FAMIC2C" => "/Express/PrintFAMIC2COrderInfo",
        "HILIFEC2C" => "/Express/PrintHILIFEC2COrderInfo",
        other => anyhow::bail!("不支援的超商類型 {other}"),
    };
    let mut fields = BTreeMap::new();
    fields.insert("MerchantID".to_string(), cfg.logistics.merchant_id.clone());
    fields.insert("AllPayLogisticsID".to_string(), logistics_id.to_string());
    fields.insert("CVSPaymentNo".to_string(), cvs_payment_no.to_string());
    if sub_type == "UNIMARTC2C" {
        fields.insert("CVSValidationNo".to_string(), cvs_validation_no.to_string());
    }
    sign(cfg, &mut fields);
    Ok(CheckoutForm {
        action: format!("{}{path}", cfg.logistics_base_url()),
        fields,
    })
}

// ───── 狀態通知 ─────

/// 物流狀態通知（ServerReplyURL；規格 §8.3）。其餘欄位原樣留在 raw
#[derive(Debug, Clone)]
pub struct StatusNotification {
    pub merchant_trade_no: String,
    pub logistics_id: String,
    pub rtn_code: i32,
    pub rtn_msg: String,
    pub update_at: Option<DateTime<Utc>>,
    pub raw: Value,
}

pub fn parse_status(
    cfg: &EcpayConfig,
    params: &[(String, String)],
) -> Result<StatusNotification, CallbackError> {
    if !mac::verify_md5(&cfg.logistics.hash_key, &cfg.logistics.hash_iv, params) {
        return Err(CallbackError::BadMac);
    }
    let merchant_trade_no = required(params, "MerchantTradeNo")?.trim().to_string();
    let rtn_code = required(params, "RtnCode")?
        .trim()
        .parse::<i32>()
        .map_err(|_| CallbackError::Missing("RtnCode"))?;
    Ok(StatusNotification {
        merchant_trade_no,
        logistics_id: field(params, "AllPayLogisticsID")
            .unwrap_or("")
            .trim()
            .to_string(),
        rtn_code,
        rtn_msg: truncate_chars(field(params, "RtnMsg").unwrap_or("").trim(), 200),
        update_at: field(params, "UpdateStatusDate").and_then(time::parse_datetime),
        raw: params_to_json(params),
    })
}

/// 更新門市通知（LogisticsC2CReplyURL；7-11 C2C 門市關轉店等）。欄位形狀與狀態通知不同：
/// 沒有 MerchantTradeNo，用 AllPayLogisticsID 找單（與規格不同之處 35）
#[derive(Debug, Clone)]
pub struct StoreUpdate {
    pub logistics_id: String,
    /// 01 取件門市／02 退件門市
    pub store_type: String,
    /// 01 門市關轉店／02 門市舊店號更新／03 退件門市為原寄件門市但無寄件門市資料／04 門市臨時關轉店
    pub status: String,
    pub store_id: String,
    pub raw: Value,
}

pub fn parse_store_update(
    cfg: &EcpayConfig,
    params: &[(String, String)],
) -> Result<StoreUpdate, CallbackError> {
    if !mac::verify_md5(&cfg.logistics.hash_key, &cfg.logistics.hash_iv, params) {
        return Err(CallbackError::BadMac);
    }
    let get = |name: &str| truncate_chars(field(params, name).unwrap_or("").trim(), 20);
    Ok(StoreUpdate {
        logistics_id: required(params, "AllPayLogisticsID")?.trim().to_string(),
        store_type: get("StoreType"),
        status: get("Status"),
        store_id: get("StoreID"),
        raw: params_to_json(params),
    })
}

/// 給老闆看的一句話，存 shipments.last_status_msg
pub fn store_update_message(u: &StoreUpdate) -> String {
    let which = match u.store_type.as_str() {
        "01" => "取件門市",
        "02" => "退件門市",
        other => other,
    };
    let what = match u.status.as_str() {
        "01" => "門市關轉店",
        "02" => "門市舊店號更新",
        "03" => "退件門市無寄件門市資料",
        "04" => "門市臨時關轉店",
        other => other,
    };
    format!("{which}異動：{what}（{}）", u.store_id)
}

/// 貨態代碼 → shipments.status（官方貨態代碼表；全家與萊爾富 B2C/C2C 共用一組；與規格不同之處 42）。
/// 不在表上的代碼回 None：只記 last_status_code／last_status_msg，不改狀態
pub fn shipment_status_for(rtn_code: i32) -> Option<&'static str> {
    Some(match rtn_code {
        // 已建檔／上傳處理中／檔案傳送成功／超商接受資料中
        300 | 310 | 2001 | 2024 => SHIPMENT_CREATED,
        // 賣家已交寄（2068 交貨便收件、3032 賣家已到門市寄件）、物流中心驗收（2030、3024）、轉運／配送中
        2068 | 3032 | 2030 | 3024 | 3001 | 3006 => SHIPMENT_IN_TRANSIT,
        // 到店（2073 配達買家門市、2063 門市配達、3018 到店尚未取貨）、重新配達取件門市、轉換店送達
        2073 | 2063 | 3018 | 2098 | 3029 => SHIPMENT_ARRIVED,
        // 消費者成功取件
        2067 | 3022 => SHIPMENT_PICKED_UP,
        // 七天未取離開門市、退回物流中心／寄件門市的各種原因（2078～2093 是 7-11 的「買家未取貨退回」細分碼）
        2074
        | 2076
        | 2077
        | 2078..=2093
        | 2069
        | 2070
        | 2072
        | 2075
        | 2099
        | 3019
        | 3020
        | 3021
        | 3023
        | 3025
        | 7011 => SHIPMENT_RETURNED,
        _ => return None,
    })
}

// ───── 名稱清理 ─────

/// 中文等非 ASCII 算 2、其餘算 1（綠界的算法）
fn width(c: char) -> usize {
    if c.is_ascii() { 1 } else { 2 }
}

fn strip_and_fit(raw: &str, width_max: usize, drop_spaces: bool) -> String {
    let mut out = String::new();
    let mut used = 0;
    for c in raw.chars() {
        if FORBIDDEN_CHARS.contains(&c) || c.is_control() || (drop_spaces && c.is_whitespace()) {
            continue;
        }
        let w = width(c);
        if used + w > width_max {
            break;
        }
        used += w;
        out.push(c);
    }
    out.trim().to_string()
}

/// 去掉綠界禁用符號、依寬度截到 50；空的話回「商品」
pub fn sanitize_goods_name(raw: &str) -> String {
    let out = strip_and_fit(raw, GOODS_NAME_WIDTH_MAX, false);
    if out.is_empty() {
        "商品".to_string()
    } else {
        out
    }
}

/// 寄件人／收件人姓名：去掉空白與禁用符號、寬度 ≤ 10（綠界會自己去空白，先做免得長度算錯）
pub fn sanitize_name(raw: &str) -> String {
    strip_and_fit(raw, SENDER_NAME_WIDTH_MAX, true)
}

/// GoodsName：第一個品項名稱，多品項加「等 N 件」（N = 總數量）
pub fn goods_name(order: &OrderDetail) -> String {
    let first = order
        .items
        .first()
        .map(|i| i.product_name.as_str())
        .unwrap_or("商品");
    let raw = if order.items.len() > 1 {
        let count: i32 = order.items.iter().map(|i| i.quantity).sum();
        format!("{first} 等{count}件")
    } else {
        first.to_string()
    };
    sanitize_goods_name(&raw)
}

// ───── 閘道 ─────

/// 真的打綠界（表單 POST，回純文字）
pub struct EcpayLogisticsClient {
    client: reqwest::Client,
}

impl EcpayLogisticsClient {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(HTTP_TIMEOUT_SECS))
                .build()
                .context("建立 HTTP client")?,
        })
    }

    pub async fn post_form(
        &self,
        url: &str,
        fields: &BTreeMap<String, String>,
    ) -> anyhow::Result<String> {
        let body = form_urlencoded::Serializer::new(String::new())
            .extend_pairs(fields.iter())
            .finish();
        self.client
            .post(url)
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(body)
            .send()
            .await
            .context("連線綠界物流 API")?
            .error_for_status()
            .context("綠界物流 API HTTP 錯誤")?
            .text()
            .await
            .context("讀取綠界物流回應")
    }
}

/// 測試用：依請求欄位組一個像綠界的成功回應（`1|…`，帶正確 MD5 MAC；用 stage 憑證）
pub fn fake_success_body(fields: &BTreeMap<String, String>) -> String {
    let (merchant_id, key, iv) = crate::config::STAGE_LOGISTICS;
    let get = |name: &str| fields.get(name).cloned().unwrap_or_default();
    let trade_no = get("MerchantTradeNo");
    let sub_type = get("LogisticsSubType");
    let mut params: Vec<(String, String)> = [
        ("MerchantID", merchant_id.to_string()),
        ("MerchantTradeNo", trade_no.clone()),
        ("RtnCode", "300".to_string()),
        ("RtnMsg", "訂單處理中(已收到訂單資料)".to_string()),
        ("AllPayLogisticsID", format!("FAKE{trade_no}")),
        ("LogisticsType", "CVS".to_string()),
        ("LogisticsSubType", sub_type.clone()),
        ("GoodsAmount", get("GoodsAmount")),
        ("UpdateStatusDate", "2026/09/08 12:00:00".to_string()),
        ("ReceiverName", get("ReceiverName")),
        ("ReceiverPhone", String::new()),
        ("ReceiverCellPhone", get("ReceiverCellPhone")),
        ("ReceiverEmail", get("ReceiverEmail")),
        ("ReceiverAddress", String::new()),
        ("CVSPaymentNo", "F0001234".to_string()),
        (
            "CVSValidationNo",
            if sub_type == "UNIMARTC2C" { "1234" } else { "" }.to_string(),
        ),
        ("BookingNote", String::new()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect();
    let mac = mac::check_mac_value_md5(key, iv, &params);
    params.push(("CheckMacValue".to_string(), mac));
    let encoded = form_urlencoded::Serializer::new(String::new())
        .extend_pairs(params.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .finish();
    format!("1|{encoded}")
}

/// 測試用：記錄請求、回排好的回應；沒排就回 fake_success_body
#[derive(Default)]
pub struct FakeLogisticsGateway {
    calls: Mutex<Vec<(String, BTreeMap<String, String>)>>,
    queued: Mutex<VecDeque<Result<String, String>>>,
}

impl FakeLogisticsGateway {
    pub fn calls(&self) -> Vec<(String, BTreeMap<String, String>)> {
        self.calls.lock().expect("fake logistics calls").clone()
    }

    /// 下一次呼叫回這段純文字（例如 `0|收件人姓名格式錯誤`）
    pub fn respond_with(&self, body: &str) {
        self.queued
            .lock()
            .expect("fake logistics queue")
            .push_back(Ok(body.to_string()));
    }

    /// 下一次呼叫回連線層錯誤
    pub fn fail_next(&self, msg: &str) {
        self.queued
            .lock()
            .expect("fake logistics queue")
            .push_back(Err(msg.to_string()));
    }
}

/// 可替換的閘道：正式打綠界；測試用 Fake（同 InvoiceGateway 的形狀）
pub enum LogisticsGateway {
    Ecpay(EcpayLogisticsClient),
    Fake(FakeLogisticsGateway),
}

impl LogisticsGateway {
    pub fn ecpay() -> anyhow::Result<Self> {
        Ok(Self::Ecpay(EcpayLogisticsClient::new()?))
    }

    pub async fn post_form(
        &self,
        url: &str,
        fields: &BTreeMap<String, String>,
    ) -> anyhow::Result<String> {
        match self {
            Self::Ecpay(client) => client.post_form(url, fields).await,
            Self::Fake(fake) => {
                fake.calls
                    .lock()
                    .expect("fake logistics calls")
                    .push((url.to_string(), fields.clone()));
                let next = fake
                    .queued
                    .lock()
                    .expect("fake logistics queue")
                    .pop_front();
                match next {
                    Some(Ok(body)) => Ok(body),
                    Some(Err(msg)) => anyhow::bail!("{msg}"),
                    None => Ok(fake_success_body(fields)),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, STAGE_LOGISTICS};
    use crate::domain::orders::{OrderDetail, OrderItemRow, OrderRow};
    use chrono::TimeZone;
    use uuid::Uuid;

    fn cfg() -> Config {
        Config::for_tests(std::path::PathBuf::from("/tmp/dog_shop_logistics_test"))
    }

    fn params(fields: &BTreeMap<String, String>) -> Vec<(String, String)> {
        fields.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }

    fn order(items: &[(&str, i32)], subtotal: i32) -> OrderDetail {
        OrderDetail {
            order: OrderRow {
                id: Uuid::now_v7(),
                order_no: "DS260908ABCD".to_string(),
                user_id: None,
                guest_token: "t".to_string(),
                status: "paid".to_string(),
                email: "buyer@test.local".to_string(),
                recipient_name: "王小明".to_string(),
                recipient_phone: "0912345678".to_string(),
                shipping_method: "cvs".to_string(),
                subtotal,
                shipping_fee: 60,
                total: subtotal + 60,
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
                paid_at: Some(Utc::now()),
                shipped_at: None,
                completed_at: None,
                cancelled_at: None,
                cancel_reason: None,
            },
            items: items
                .iter()
                .map(|(name, qty)| OrderItemRow {
                    product_name: name.to_string(),
                    variant_label: "預設".to_string(),
                    unit_price: 100,
                    quantity: *qty,
                    line_total: 100 * qty,
                    image_path: None,
                })
                .collect(),
            shipment: None,
            payment: None,
            invoice: None,
        }
    }

    fn request() -> CreateRequest {
        CreateRequest {
            merchant_trade_no: "DS260908ABCDL01".to_string(),
            sub_type: "UNIMARTC2C".to_string(),
            goods_amount: 700,
            goods_name: "雞肉狗糧".to_string(),
            sender_name: "狗狗商店".to_string(),
            sender_phone: "0987654321".to_string(),
            receiver_name: "王小明".to_string(),
            receiver_phone: "0912345678".to_string(),
            receiver_email: "buyer@test.local".to_string(),
            receiver_store_id: "131386".to_string(),
            return_store_id: None,
        }
    }

    #[test]
    fn map_form_has_spec_fields_and_no_mac() {
        let cfg = cfg();
        let form = map_form(
            &cfg.ecpay,
            "http://localhost:5173",
            "AbCdEfGhIjKlMnOpQrSt",
            "FAMIC2C",
            true,
        );
        assert_eq!(
            form.action,
            "https://logistics-stage.ecpay.com.tw/Express/map"
        );
        let f = &form.fields;
        assert_eq!(f["MerchantID"], "2000933");
        assert_eq!(f["MerchantTradeNo"], "AbCdEfGhIjKlMnOpQrSt");
        assert_eq!(f["ExtraData"], "AbCdEfGhIjKlMnOpQrSt");
        assert_eq!(f["LogisticsType"], "CVS");
        assert_eq!(f["LogisticsSubType"], "FAMIC2C");
        assert_eq!(f["IsCollection"], "N");
        assert_eq!(
            f["ServerReplyURL"],
            "http://localhost:5173/api/ecpay/logistics/map-reply"
        );
        assert_eq!(f["Device"], "1");
        assert!(!f.contains_key("CheckMacValue"), "電子地圖不需簽章");
        assert_eq!(
            map_form(&cfg.ecpay, "http://x", "t", "UNIMARTC2C", false).fields["Device"],
            "0"
        );
    }

    #[test]
    fn parse_map_reply_reads_store_and_tolerates_missing_phone() {
        let p = vec![
            ("MerchantID".to_string(), "2000933".to_string()),
            ("MerchantTradeNo".to_string(), "tok".to_string()),
            ("LogisticsSubType".to_string(), "UNIMARTC2C".to_string()),
            ("CVSStoreID".to_string(), " 991182 ".to_string()),
            ("CVSStoreName".to_string(), "測試門市".to_string()),
            (
                "CVSAddress".to_string(),
                "台北市中正區重慶南路一段 122 號".to_string(),
            ),
            ("CVSOutSide".to_string(), "0".to_string()),
            ("ExtraData".to_string(), "tok".to_string()),
        ];
        let r = parse_map_reply(&p).unwrap();
        assert_eq!(r.token, "tok");
        assert_eq!(r.sub_type, "UNIMARTC2C");
        assert_eq!(r.store_id, "991182");
        assert_eq!(r.store_name, "測試門市");
        assert_eq!(r.store_phone, "", "7-11 不回電話");
        assert!(!r.outside);

        let mut missing = p.clone();
        missing.retain(|(k, _)| k != "CVSStoreID");
        assert_eq!(
            parse_map_reply(&missing).unwrap_err(),
            CallbackError::Missing("CVSStoreID")
        );
        let mut bad = p.clone();
        bad[2].1 = "TCAT".to_string();
        assert_eq!(
            parse_map_reply(&bad).unwrap_err(),
            CallbackError::Missing("LogisticsSubType")
        );
        let mut no_token = p;
        no_token.retain(|(k, _)| k != "ExtraData");
        assert_eq!(
            parse_map_reply(&no_token).unwrap_err(),
            CallbackError::Missing("ExtraData")
        );
    }

    #[test]
    fn create_fields_match_spec_and_mac_verifies() {
        let cfg = cfg();
        let now = Utc.with_ymd_and_hms(2026, 9, 8, 4, 5, 6).unwrap(); // 台北 12:05:06
        let f = create_fields(&cfg.ecpay, "https://shop.example", &request(), now);
        assert_eq!(f["MerchantID"], "2000933");
        assert_eq!(f["MerchantTradeNo"], "DS260908ABCDL01");
        assert_eq!(f["MerchantTradeDate"], "2026/09/08 12:05:06");
        assert_eq!(f["LogisticsType"], "CVS");
        assert_eq!(f["LogisticsSubType"], "UNIMARTC2C");
        assert_eq!(f["GoodsAmount"], "700");
        assert_eq!(f["GoodsName"], "雞肉狗糧");
        assert_eq!(f["SenderName"], "狗狗商店");
        assert_eq!(f["SenderCellPhone"], "0987654321");
        assert_eq!(f["ReceiverName"], "王小明");
        assert_eq!(f["ReceiverCellPhone"], "0912345678");
        assert_eq!(f["ReceiverEmail"], "buyer@test.local");
        assert_eq!(f["ReceiverStoreID"], "131386");
        assert_eq!(
            f["ServerReplyURL"],
            "https://shop.example/api/ecpay/logistics/status"
        );
        assert_eq!(
            f["LogisticsC2CReplyURL"],
            "https://shop.example/api/ecpay/logistics/store-update"
        );
        assert_eq!(f["IsCollection"], "N");
        assert!(!f.contains_key("ReturnStoreID"), "沒設退貨門市就不帶");
        assert!(!f.contains_key("ClientReplyURL"), "幕後建單不帶");
        assert_eq!(f["CheckMacValue"].len(), 32);
        assert!(mac::verify_md5(
            STAGE_LOGISTICS.1,
            STAGE_LOGISTICS.2,
            &params(&f)
        ));

        let mut req = request();
        req.return_store_id = Some("991182".to_string());
        req.goods_amount = 25_000;
        req.receiver_email = "a".repeat(46) + "@x.tw"; // 51 字
        let f = create_fields(&cfg.ecpay, "https://shop.example", &req, now);
        assert_eq!(f["ReturnStoreID"], "991182");
        assert_eq!(f["GoodsAmount"], "20000", "夾在 1～20000");
        assert!(!f.contains_key("ReceiverEmail"), "Email 超過 50 字不帶");
        assert_eq!(
            create_url(&cfg.ecpay),
            "https://logistics-stage.ecpay.com.tw/Express/Create"
        );
    }

    #[test]
    fn parse_create_response_success_rejected_bad_mac_malformed() {
        let cfg = cfg();
        let f = create_fields(&cfg.ecpay, "https://shop.example", &request(), Utc::now());
        let body = fake_success_body(&f);
        assert!(body.starts_with("1|"));
        let ok = parse_create_response(&cfg.ecpay, &body).unwrap();
        assert_eq!(ok.logistics_id, "FAKEDS260908ABCDL01");
        assert_eq!(ok.rtn_code, 300);
        assert_eq!(ok.rtn_msg, "訂單處理中(已收到訂單資料)");
        assert_eq!(ok.cvs_payment_no, "F0001234");
        assert_eq!(ok.cvs_validation_no, "1234", "7-11 才有驗證碼");
        assert_eq!(ok.raw["MerchantTradeNo"], "DS260908ABCDL01");

        assert_eq!(
            parse_create_response(&cfg.ecpay, "0|收件人姓名格式錯誤").unwrap_err(),
            CreateError::Rejected("收件人姓名格式錯誤".to_string())
        );
        let tampered = body.replace("RtnCode=300", "RtnCode=301");
        assert_eq!(
            parse_create_response(&cfg.ecpay, &tampered).unwrap_err(),
            CreateError::BadMac
        );
        assert!(matches!(
            parse_create_response(&cfg.ecpay, "<html>500</html>").unwrap_err(),
            CreateError::Malformed(_)
        ));
        assert!(matches!(
            parse_create_response(&cfg.ecpay, "1|MerchantID=2000933").unwrap_err(),
            CreateError::BadMac
        ));

        // 反向代理的錯誤頁：`|` 前面那一大段不能無界進 log（審查 Minor 2）
        let long = "x".repeat(5000) + "|y";
        let CreateError::Malformed(m) = parse_create_response(&cfg.ecpay, &long).unwrap_err()
        else {
            panic!("應該是 Malformed");
        };
        assert!(m.chars().count() <= 210, "{}", m.chars().count());

        let mut fam = request();
        fam.sub_type = "FAMIC2C".to_string();
        let f = create_fields(&cfg.ecpay, "https://shop.example", &fam, Utc::now());
        let ok = parse_create_response(&cfg.ecpay, &fake_success_body(&f)).unwrap();
        assert_eq!(ok.cvs_validation_no, "", "全家沒有驗證碼");
    }

    #[test]
    fn print_form_per_sub_type() {
        let cfg = cfg();
        let f = print_form(&cfg.ecpay, "UNIMARTC2C", "10035", "F0001234", "1234").unwrap();
        assert_eq!(
            f.action,
            "https://logistics-stage.ecpay.com.tw/Express/PrintUniMartC2COrderInfo"
        );
        assert_eq!(f.fields["AllPayLogisticsID"], "10035");
        assert_eq!(f.fields["CVSPaymentNo"], "F0001234");
        assert_eq!(f.fields["CVSValidationNo"], "1234");
        assert!(mac::verify_md5(
            STAGE_LOGISTICS.1,
            STAGE_LOGISTICS.2,
            &params(&f.fields)
        ));

        let f = print_form(&cfg.ecpay, "FAMIC2C", "10035", "F0001234", "").unwrap();
        assert_eq!(
            f.action,
            "https://logistics-stage.ecpay.com.tw/Express/PrintFAMIC2COrderInfo"
        );
        assert!(!f.fields.contains_key("CVSValidationNo"));
        let f = print_form(&cfg.ecpay, "HILIFEC2C", "10035", "F0001234", "").unwrap();
        assert_eq!(
            f.action,
            "https://logistics-stage.ecpay.com.tw/Express/PrintHILIFEC2COrderInfo"
        );
        assert!(print_form(&cfg.ecpay, "TCAT", "1", "2", "3").is_err());
    }

    #[test]
    fn parse_status_requires_valid_mac_and_fields() {
        let cfg = cfg();
        let mut p = vec![
            ("MerchantID".to_string(), "2000933".to_string()),
            ("MerchantTradeNo".to_string(), "DS260908ABCDL01".to_string()),
            ("RtnCode".to_string(), "2067".to_string()),
            ("RtnMsg".to_string(), "消費者成功取件".to_string()),
            ("AllPayLogisticsID".to_string(), "10035".to_string()),
            ("LogisticsType".to_string(), "CVS".to_string()),
            ("LogisticsSubType".to_string(), "UNIMARTC2C".to_string()),
            ("GoodsAmount".to_string(), "700".to_string()),
            (
                "UpdateStatusDate".to_string(),
                "2026/09/10 18:30:00".to_string(),
            ),
        ];
        let macv = mac::check_mac_value_md5(STAGE_LOGISTICS.1, STAGE_LOGISTICS.2, &p);
        p.push(("CheckMacValue".to_string(), macv));
        let n = parse_status(&cfg.ecpay, &p).unwrap();
        assert_eq!(n.merchant_trade_no, "DS260908ABCDL01");
        assert_eq!(n.logistics_id, "10035");
        assert_eq!(n.rtn_code, 2067);
        assert_eq!(n.rtn_msg, "消費者成功取件");
        assert_eq!(
            n.update_at,
            Some(Utc.with_ymd_and_hms(2026, 9, 10, 10, 30, 0).unwrap())
        );
        assert_eq!(n.raw["RtnCode"], "2067");

        let mut bad = p.clone();
        bad[2].1 = "2074".to_string();
        assert_eq!(
            parse_status(&cfg.ecpay, &bad).unwrap_err(),
            CallbackError::BadMac
        );
        let mut no_code: Vec<_> = p.iter().filter(|(k, _)| k != "RtnCode").cloned().collect();
        let macv = mac::check_mac_value_md5(
            STAGE_LOGISTICS.1,
            STAGE_LOGISTICS.2,
            &no_code[..no_code.len() - 1],
        );
        no_code.last_mut().unwrap().1 = macv;
        assert_eq!(
            parse_status(&cfg.ecpay, &no_code).unwrap_err(),
            CallbackError::Missing("RtnCode")
        );
    }

    #[test]
    fn parse_store_update_and_message() {
        let cfg = cfg();
        let mut p = vec![
            ("MerchantID".to_string(), "2000933".to_string()),
            ("AllPayLogisticsID".to_string(), "10035".to_string()),
            ("GoodsName".to_string(), "雞肉狗糧".to_string()),
            ("GoodsAmount".to_string(), "700".to_string()),
            ("StoreType".to_string(), "01".to_string()),
            ("Status".to_string(), "01".to_string()),
            ("StoreID".to_string(), "991182".to_string()),
        ];
        let macv = mac::check_mac_value_md5(STAGE_LOGISTICS.1, STAGE_LOGISTICS.2, &p);
        p.push(("CheckMacValue".to_string(), macv));
        let u = parse_store_update(&cfg.ecpay, &p).unwrap();
        assert_eq!(u.logistics_id, "10035");
        assert_eq!(
            store_update_message(&u),
            "取件門市異動：門市關轉店（991182）"
        );
        p[6].1 = "000001".to_string();
        assert_eq!(
            parse_store_update(&cfg.ecpay, &p).unwrap_err(),
            CallbackError::BadMac
        );
    }

    #[test]
    fn status_code_table() {
        assert_eq!(shipment_status_for(300), Some(SHIPMENT_CREATED));
        assert_eq!(shipment_status_for(2030), Some(SHIPMENT_IN_TRANSIT));
        assert_eq!(shipment_status_for(3024), Some(SHIPMENT_IN_TRANSIT));
        assert_eq!(shipment_status_for(2068), Some(SHIPMENT_IN_TRANSIT));
        assert_eq!(shipment_status_for(2073), Some(SHIPMENT_ARRIVED));
        assert_eq!(shipment_status_for(2063), Some(SHIPMENT_ARRIVED));
        assert_eq!(shipment_status_for(3018), Some(SHIPMENT_ARRIVED));
        assert_eq!(shipment_status_for(2098), Some(SHIPMENT_ARRIVED));
        assert_eq!(shipment_status_for(2067), Some(SHIPMENT_PICKED_UP));
        assert_eq!(shipment_status_for(3022), Some(SHIPMENT_PICKED_UP));
        assert_eq!(shipment_status_for(2074), Some(SHIPMENT_RETURNED));
        assert_eq!(shipment_status_for(3020), Some(SHIPMENT_RETURNED));
        assert_eq!(shipment_status_for(2088), Some(SHIPMENT_RETURNED));
        assert_eq!(shipment_status_for(2101), None, "門市關轉店只記錄");
        assert_eq!(shipment_status_for(9999), None);
    }

    #[test]
    fn goods_name_and_names_are_sanitized() {
        assert_eq!(
            sanitize_goods_name("雞肉狗糧 #1 [大包] <特價>"),
            "雞肉狗糧 1 大包 特價"
        );
        assert_eq!(sanitize_goods_name(""), "商品");
        assert_eq!(sanitize_goods_name("^'`!@#%&*+\\\"<>|_[]"), "商品");
        let long = "狗".repeat(40);
        assert_eq!(
            sanitize_goods_name(&long).chars().count(),
            25,
            "中文算 2，寬度上限 50"
        );
        let mixed = "abc".to_string() + &"狗".repeat(30);
        assert_eq!(sanitize_goods_name(&mixed).chars().count(), 3 + 23);
        assert_eq!(goods_name(&order(&[("雞肉狗糧", 1)], 300)), "雞肉狗糧");
        assert_eq!(
            goods_name(&order(&[("雞肉狗糧", 2), ("潔牙骨", 1)], 700)),
            "雞肉狗糧 等3件"
        );
        assert_eq!(sanitize_name(" 狗狗 商店! "), "狗狗商店");
        assert_eq!(sanitize_name("王小明"), "王小明");
        assert_eq!(
            sanitize_name(&"商".repeat(8)),
            "商".repeat(5),
            "寄件人寬度上限 10"
        );
        assert!(is_sub_type("FAMIC2C") && !is_sub_type("TCAT"));
    }
}
