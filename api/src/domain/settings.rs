use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use sqlx::PgPool;

use crate::domain::users::{is_tw_mobile, is_valid_email};
use crate::error::{ApiError, FieldErrors};

pub const SHOP_KEY: &str = "shop";
pub const SHIPPING_KEY: &str = "shipping";
pub const PAYMENT_METHODS_KEY: &str = "payment_methods";
pub const SENDER_KEY: &str = "sender";
pub const RETURN_STORE_KEY: &str = "return_store";
/// 綠界 C2C 超商代碼（規格 §3）
pub const CVS_SUB_TYPES: &[&str] = &["UNIMARTC2C", "FAMIC2C", "HILIFEC2C"];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ShopSettings {
    pub name: String,
    pub description: String,
    pub contact_email: String,
    pub contact_phone: String,
}

/// 運費（元）。free_threshold = 0 表示不免運；> 0 時商品小計 >= 門檻就免運
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ShippingSettings {
    pub cvs_fee: i32,
    pub home_fee: i32,
    pub free_threshold: i32,
}

impl Default for ShippingSettings {
    fn default() -> Self {
        Self {
            cvs_fee: 60,
            home_fee: 100,
            free_threshold: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PaymentMethods {
    pub credit: bool,
    pub atm: bool,
    pub cvs_code: bool,
}

impl Default for PaymentMethods {
    fn default() -> Self {
        Self {
            credit: true,
            atm: true,
            cvs_code: true,
        }
    }
}

/// 寄件人（計畫 4 建物流單用）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SenderSettings {
    pub name: String,
    pub phone: String,
}

/// 退貨門市（計畫 4 建物流單用）。sub_type 空字串 = 還沒設定
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ReturnStore {
    pub sub_type: String,
    pub store_id: String,
    pub store_name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AllSettings {
    pub shop: ShopSettings,
    pub shipping: ShippingSettings,
    pub payment_methods: PaymentMethods,
    pub sender: SenderSettings,
    pub return_store: ReturnStore,
}

/// 給前台看的部分：不含寄件人與退貨門市
#[derive(Debug, Clone, Serialize)]
pub struct PublicSettings {
    pub shop: ShopSettings,
    pub shipping: ShippingSettings,
    pub payment_methods: PaymentMethods,
}

impl AllSettings {
    pub fn public(&self) -> PublicSettings {
        PublicSettings {
            shop: self.shop.clone(),
            shipping: self.shipping.clone(),
            payment_methods: self.payment_methods.clone(),
        }
    }

    /// 去頭尾空白（後台 PUT 進來時用）
    pub fn trimmed(mut self) -> Self {
        for s in [
            &mut self.shop.name,
            &mut self.shop.description,
            &mut self.shop.contact_email,
            &mut self.shop.contact_phone,
            &mut self.sender.name,
            &mut self.sender.phone,
            &mut self.return_store.sub_type,
            &mut self.return_store.store_id,
            &mut self.return_store.store_name,
        ] {
            *s = s.trim().to_string();
        }
        self
    }
}

pub async fn get(db: &PgPool, key: &str) -> Result<Option<Value>, sqlx::Error> {
    sqlx::query_scalar::<_, Value>("SELECT value FROM settings WHERE key = $1")
        .bind(key)
        .fetch_optional(db)
        .await
}

/// 壞掉或缺欄位的 JSON 一律退回預設值，設定頁永遠打得開
fn parse<T: DeserializeOwned + Default>(value: Value) -> T {
    serde_json::from_value(value).unwrap_or_default()
}

pub async fn get_all(db: &PgPool) -> Result<AllSettings, sqlx::Error> {
    let rows: Vec<(String, Value)> = sqlx::query_as("SELECT key, value FROM settings")
        .fetch_all(db)
        .await?;
    let mut all = AllSettings::default();
    for (key, value) in rows {
        match key.as_str() {
            SHOP_KEY => all.shop = parse(value),
            SHIPPING_KEY => all.shipping = parse(value),
            PAYMENT_METHODS_KEY => all.payment_methods = parse(value),
            SENDER_KEY => all.sender = parse(value),
            RETURN_STORE_KEY => all.return_store = parse(value),
            _ => {}
        }
    }
    Ok(all)
}

/// 五把 key 整組 upsert（一個交易）
pub async fn put_all(db: &PgPool, all: &AllSettings) -> Result<(), ApiError> {
    let entries = [
        (SHOP_KEY, serde_json::to_value(&all.shop)),
        (SHIPPING_KEY, serde_json::to_value(&all.shipping)),
        (
            PAYMENT_METHODS_KEY,
            serde_json::to_value(&all.payment_methods),
        ),
        (SENDER_KEY, serde_json::to_value(&all.sender)),
        (RETURN_STORE_KEY, serde_json::to_value(&all.return_store)),
    ];
    let mut tx = db.begin().await?;
    for (key, value) in entries {
        let value = value.map_err(|e| ApiError::Internal(e.into()))?;
        sqlx::query(
            "INSERT INTO settings (key, value, updated_at) VALUES ($1, $2, now())
             ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()",
        )
        .bind(key)
        .bind(value)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub fn validate(all: &AllSettings) -> Result<(), ApiError> {
    let mut errors = FieldErrors::new();
    let name_len = all.shop.name.chars().count();
    if name_len == 0 || name_len > 60 {
        errors.add("shop.name", "必填，最多 60 字");
    }
    if all.shop.description.chars().count() > 500 {
        errors.add("shop.description", "最多 500 字");
    }
    if !all.shop.contact_email.is_empty() && !is_valid_email(&all.shop.contact_email) {
        errors.add("shop.contact_email", "Email 格式不正確");
    }
    if all.shop.contact_phone.chars().count() > 20 {
        errors.add("shop.contact_phone", "最多 20 字");
    }
    for (field, value) in [
        ("shipping.cvs_fee", all.shipping.cvs_fee),
        ("shipping.home_fee", all.shipping.home_fee),
    ] {
        if !(0..=10_000).contains(&value) {
            errors.add(field, "0 到 10000 之間的整數");
        }
    }
    if !(0..=1_000_000).contains(&all.shipping.free_threshold) {
        errors.add(
            "shipping.free_threshold",
            "0 到 1000000 之間的整數（0 表示不免運）",
        );
    }
    let pm = &all.payment_methods;
    if !(pm.credit || pm.atm || pm.cvs_code) {
        errors.add("payment_methods", "至少要開一種付款方式");
    }
    if all.sender.name.chars().count() > 10 {
        errors.add("sender.name", "最多 10 字");
    }
    if !all.sender.phone.is_empty() && !is_tw_mobile(&all.sender.phone) {
        errors.add("sender.phone", "手機格式：09 開頭共 10 碼");
    }
    if !all.return_store.sub_type.is_empty()
        && !CVS_SUB_TYPES.contains(&all.return_store.sub_type.as_str())
    {
        errors.add(
            "return_store.sub_type",
            "只能是 UNIMARTC2C、FAMIC2C 或 HILIFEC2C",
        );
    }
    if all.return_store.store_id.chars().count() > 10 {
        errors.add("return_store.store_id", "最多 10 字");
    }
    if all.return_store.store_name.chars().count() > 30 {
        errors.add("return_store.store_name", "最多 30 字");
    }
    errors.into_result()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_are_valid() {
        let mut all = AllSettings::default();
        all.shop.name = "店".to_string();
        assert!(validate(&all).is_ok());
    }

    #[test]
    fn bad_values_are_field_errors() {
        let all = AllSettings {
            shop: ShopSettings {
                name: String::new(),
                contact_email: "nope".to_string(),
                ..Default::default()
            },
            shipping: ShippingSettings {
                cvs_fee: -1,
                home_fee: 20_000,
                free_threshold: 5,
            },
            payment_methods: PaymentMethods {
                credit: false,
                atm: false,
                cvs_code: false,
            },
            sender: SenderSettings {
                name: String::new(),
                phone: "123".to_string(),
            },
            return_store: ReturnStore {
                sub_type: "SEVEN".to_string(),
                ..Default::default()
            },
        };
        let err = validate(&all).unwrap_err();
        let ApiError::Validation { details, .. } = err else {
            panic!("expected validation");
        };
        let fields = &details["fields"];
        for key in [
            "shop.name",
            "shop.contact_email",
            "shipping.cvs_fee",
            "shipping.home_fee",
            "payment_methods",
            "sender.phone",
            "return_store.sub_type",
        ] {
            assert!(fields.get(key).is_some(), "缺 {key}: {fields}");
        }
        assert!(fields.get("shipping.free_threshold").is_none());
    }

    #[test]
    fn parse_falls_back_to_default() {
        let s: ShippingSettings = parse(json!({ "cvs_fee": "not a number" }));
        assert_eq!(s.cvs_fee, 60);
        let s: ShippingSettings = parse(json!({ "cvs_fee": 80 }));
        assert_eq!((s.cvs_fee, s.home_fee), (80, 100));
    }
}
