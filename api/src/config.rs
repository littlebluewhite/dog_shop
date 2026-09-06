use std::path::PathBuf;

use anyhow::Context;

/// 從環境變數讀進來的設定。測試會直接建構這個 struct，所以欄位都是 pub。
#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    /// 對外網址，例如 http://localhost:5173 或 https://shop.example.com（結尾不帶 /）
    pub public_base_url: String,
    /// cookie 是否加 Secure。開發環境（http）設 COOKIE_SECURE=false
    pub cookie_secure: bool,
    /// 圖片存放目錄
    pub upload_dir: PathBuf,
}

/// 手動實作：database_url 含 DB 密碼，不能被 {:?} 印出來（規格 §11）。
impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("database_url", &"<redacted>")
            .field("public_base_url", &self.public_base_url)
            .field("cookie_secure", &self.cookie_secure)
            .field("upload_dir", &self.upload_dir)
            .finish()
    }
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
        Ok(Self {
            database_url,
            public_base_url,
            cookie_secure,
            upload_dir,
        })
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
            database_url: String::new(),
            public_base_url: base.to_string(),
            cookie_secure: false,
            upload_dir: PathBuf::from("/tmp"),
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
}
