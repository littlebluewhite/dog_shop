use std::path::PathBuf;

use anyhow::Context;

/// 從環境變數讀進來的設定。測試用 `Config::for_tests` 建構；欄位都是 pub。
#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    /// 對外網址，例如 http://localhost:5173 或 https://shop.example.com（結尾不帶 /）
    pub public_base_url: String,
    /// cookie 是否加 Secure。開發環境（http）設 COOKIE_SECURE=false
    pub cookie_secure: bool,
    /// 圖片存放目錄
    pub upload_dir: PathBuf,
    /// 綠界（規格 §8）
    pub ecpay: EcpayConfig,
    /// SMTP；None 表示沒設定，Email 只記 log（與規格不同之處 22）
    pub smtp: Option<SmtpConfig>,
    /// 沒設 SMTP 時要不要把信件內文也寫進 log。內文含重設連結與訪客訂單網址（規格 §11），
    /// 只有 MAIL_LOG_BODY=1 才開，正式環境不要開
    pub mail_log_body: bool,
}

/// 手動實作：database_url 含 DB 密碼，不能被 {:?} 印出來（規格 §11）。
impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("database_url", &"<redacted>")
            .field("public_base_url", &self.public_base_url)
            .field("cookie_secure", &self.cookie_secure)
            .field("upload_dir", &self.upload_dir)
            .field("ecpay", &self.ecpay)
            .field("smtp", &self.smtp)
            .field("mail_log_body", &self.mail_log_body)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EcpayEnv {
    Stage,
    Prod,
}

/// 一組綠界憑證。HashKey / HashIV 不能被 {:?} 印出來（規格 §11）
#[derive(Clone)]
pub struct EcpayCredentials {
    pub merchant_id: String,
    pub hash_key: String,
    pub hash_iv: String,
}

impl std::fmt::Debug for EcpayCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EcpayCredentials")
            .field("merchant_id", &self.merchant_id)
            .field("hash_key", &"<redacted>")
            .field("hash_iv", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Debug)]
pub struct EcpayConfig {
    pub env: EcpayEnv,
    pub aio: EcpayCredentials,
    pub invoice: EcpayCredentials,
}

/// 全方位金流測試特店（規格 §8.5；公開資料，只能用於 stage）：(MerchantID, HashKey, HashIV)
pub const STAGE_AIO: (&str, &str, &str) = ("3002607", "pwFHCqoQZGmho4w6", "EkRm7iFT261dpevs");
/// 電子發票 B2C 測試特店（developers.ecpay.com.tw「測試介接資訊」）
pub const STAGE_INVOICE: (&str, &str, &str) = ("2000132", "ejCk326UnaZWKisg", "q9jcZX8Ib9LM8wYk");

impl EcpayConfig {
    pub fn aio_checkout_url(&self) -> &'static str {
        match self.env {
            EcpayEnv::Stage => "https://payment-stage.ecpay.com.tw/Cashier/AioCheckOut/V5",
            EcpayEnv::Prod => "https://payment.ecpay.com.tw/Cashier/AioCheckOut/V5",
        }
    }

    pub fn invoice_issue_url(&self) -> &'static str {
        match self.env {
            EcpayEnv::Stage => "https://einvoice-stage.ecpay.com.tw/B2CInvoice/Issue",
            EcpayEnv::Prod => "https://einvoice.ecpay.com.tw/B2CInvoice/Issue",
        }
    }
}

#[derive(Clone)]
pub struct SmtpConfig {
    pub host: String,
    /// 465 = 一開始就 TLS；其他（587、25）= STARTTLS
    pub port: u16,
    pub user: Option<String>,
    pub pass: Option<String>,
    /// 寄件人，例如 `狗狗商店 <no-reply@example.com>` 或純地址
    pub from: String,
}

impl std::fmt::Debug for SmtpConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SmtpConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("user", &self.user)
            .field("pass", &self.pass.as_ref().map(|_| "<redacted>"))
            .field("from", &self.from)
            .finish()
    }
}

/// 讀環境變數，去頭尾空白，空字串當沒設
fn env_trimmed(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// 布林開關：只有 "1"／"true"（不分大小寫）算開；沒設或其他值都是關
fn flag_enabled(value: Option<&str>) -> bool {
    matches!(
        value.map(str::to_ascii_lowercase).as_deref(),
        Some("1" | "true")
    )
}

/// 讀一組憑證：stage 時空值退回公開測試憑證；prod 時三個都必填（與規格不同之處 24）
fn credentials(
    prefix: &str,
    env: EcpayEnv,
    stage: (&str, &str, &str),
) -> anyhow::Result<EcpayCredentials> {
    let read = |suffix: &str, fallback: &str| -> anyhow::Result<String> {
        let name = format!("{prefix}_{suffix}");
        match (env_trimmed(&name), env) {
            (Some(value), _) => Ok(value),
            (None, EcpayEnv::Stage) => Ok(fallback.to_string()),
            (None, EcpayEnv::Prod) => anyhow::bail!("ECPAY_ENV=prod 時必須設定 {name}"),
        }
    };
    Ok(EcpayCredentials {
        merchant_id: read("MERCHANT_ID", stage.0)?,
        hash_key: read("HASH_KEY", stage.1)?,
        hash_iv: read("HASH_IV", stage.2)?,
    })
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let database_url = std::env::var("DATABASE_URL").context("缺少環境變數 DATABASE_URL")?;
        let public_base_url = std::env::var("PUBLIC_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:5173".to_string())
            .trim_end_matches('/')
            .to_string();
        let cookie_secure = match std::env::var("COOKIE_SECURE") {
            Ok(value) => !matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "false" | "0" | "no"
            ),
            Err(_) => true,
        };
        let upload_dir =
            PathBuf::from(std::env::var("UPLOAD_DIR").unwrap_or_else(|_| "./uploads".to_string()));

        let ecpay_env = match env_trimmed("ECPAY_ENV").as_deref() {
            None | Some("stage") => EcpayEnv::Stage,
            Some("prod") => EcpayEnv::Prod,
            Some(other) => anyhow::bail!("ECPAY_ENV 只能是 stage 或 prod，收到 {other}"),
        };
        let ecpay = EcpayConfig {
            env: ecpay_env,
            aio: credentials("ECPAY_AIO", ecpay_env, STAGE_AIO)?,
            invoice: credentials("ECPAY_INVOICE", ecpay_env, STAGE_INVOICE)?,
        };

        let smtp = match env_trimmed("SMTP_HOST") {
            None => None,
            Some(host) => Some(SmtpConfig {
                host,
                port: env_trimmed("SMTP_PORT")
                    .map(|p| p.parse::<u16>().context("SMTP_PORT 要是 1～65535 的數字"))
                    .transpose()?
                    .unwrap_or(587),
                user: env_trimmed("SMTP_USER"),
                pass: env_trimmed("SMTP_PASS"),
                from: env_trimmed("SMTP_FROM").context("有 SMTP_HOST 就必須設定 SMTP_FROM")?,
            }),
        };

        Ok(Self {
            database_url,
            public_base_url,
            cookie_secure,
            upload_dir,
            ecpay,
            smtp,
            mail_log_body: flag_enabled(env_trimmed("MAIL_LOG_BODY").as_deref()),
        })
    }

    /// 測試用：stage 憑證、沒有 SMTP、對外網址 http://localhost:5173、cookie 不加 Secure
    pub fn for_tests(upload_dir: PathBuf) -> Self {
        Self {
            database_url: String::new(),
            public_base_url: "http://localhost:5173".to_string(),
            cookie_secure: false,
            upload_dir,
            ecpay: EcpayConfig {
                env: EcpayEnv::Stage,
                aio: EcpayCredentials {
                    merchant_id: STAGE_AIO.0.to_string(),
                    hash_key: STAGE_AIO.1.to_string(),
                    hash_iv: STAGE_AIO.2.to_string(),
                },
                invoice: EcpayCredentials {
                    merchant_id: STAGE_INVOICE.0.to_string(),
                    hash_key: STAGE_INVOICE.1.to_string(),
                    hash_iv: STAGE_INVOICE.2.to_string(),
                },
            },
            smtp: None,
            mail_log_body: false,
        }
    }

    /// 只留 scheme://host[:port]，用來和瀏覽器送來的 Origin header 比對
    pub fn public_origin(&self) -> &str {
        let url = &self.public_base_url;
        let Some(scheme_end) = url.find("://") else {
            return url;
        };
        let rest = &url[scheme_end + 3..];
        match rest.find('/') {
            Some(i) => &url[..scheme_end + 3 + i],
            None => url,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(base: &str) -> Config {
        Config {
            public_base_url: base.to_string(),
            ..Config::for_tests(PathBuf::from("/tmp"))
        }
    }

    #[test]
    fn public_origin_strips_path() {
        assert_eq!(
            cfg("https://shop.example.com/some/path").public_origin(),
            "https://shop.example.com"
        );
        assert_eq!(
            cfg("http://localhost:5173").public_origin(),
            "http://localhost:5173"
        );
    }

    #[test]
    fn debug_redacts_secrets() {
        let mut cfg = Config::for_tests(PathBuf::from("/tmp"));
        cfg.database_url = "postgres://u:dbpass@h/db".to_string();
        cfg.smtp = Some(SmtpConfig {
            host: "smtp.example.com".to_string(),
            port: 587,
            user: Some("mailer".to_string()),
            pass: Some("mailpass".to_string()),
            from: "shop@example.com".to_string(),
        });
        let text = format!("{cfg:?}");
        for secret in [
            "dbpass",
            "mailpass",
            STAGE_AIO.1,
            STAGE_AIO.2,
            STAGE_INVOICE.1,
            STAGE_INVOICE.2,
        ] {
            assert!(!text.contains(secret), "{secret} 出現在 Debug 輸出：{text}");
        }
        assert!(text.contains("3002607"));
        assert!(text.contains("2000132"));
        assert!(text.contains("smtp.example.com"));
        assert!(text.contains("mailer"));
    }

    /// 內文含重設 token 與 guest_token（規格 §11）：預設不印，要明確 opt-in。
    /// 直接測解讀規則而不是設環境變數：std::env::set_var 在 edition 2024 是 unsafe，
    /// 而且會影響同時跑的其他測試
    #[test]
    fn mail_log_body_defaults_off_and_needs_explicit_opt_in() {
        assert!(!flag_enabled(None), "沒設就是關");
        assert!(flag_enabled(Some("1")));
        assert!(flag_enabled(Some("true")));
        assert!(flag_enabled(Some("TRUE")));
        assert!(!flag_enabled(Some("false")));
        assert!(!flag_enabled(Some("0")));
        assert!(!Config::for_tests(PathBuf::from("/tmp")).mail_log_body);
    }

    #[test]
    fn urls_follow_env() {
        let mut cfg = Config::for_tests(PathBuf::from("/tmp"));
        assert_eq!(
            cfg.ecpay.aio_checkout_url(),
            "https://payment-stage.ecpay.com.tw/Cashier/AioCheckOut/V5"
        );
        assert_eq!(
            cfg.ecpay.invoice_issue_url(),
            "https://einvoice-stage.ecpay.com.tw/B2CInvoice/Issue"
        );
        cfg.ecpay.env = EcpayEnv::Prod;
        assert_eq!(
            cfg.ecpay.aio_checkout_url(),
            "https://payment.ecpay.com.tw/Cashier/AioCheckOut/V5"
        );
        assert_eq!(
            cfg.ecpay.invoice_issue_url(),
            "https://einvoice.ecpay.com.tw/B2CInvoice/Issue"
        );
    }
}
