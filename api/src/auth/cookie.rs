use axum::http::{HeaderMap, header::COOKIE};

pub const SESSION_COOKIE: &str = "sid";
const SESSION_MAX_AGE_SECS: i64 = 30 * 24 * 60 * 60;

/// 從 Cookie header 取一個值（可能有多個 Cookie header、每個裡面用 ; 分隔）
pub fn get_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|line| line.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.trim().to_string())
}

/// Set-Cookie 的值：HttpOnly、SameSite=Lax、Path=/、30 天；Secure 依設定（規格 §11）
pub fn session_cookie(sid: &str, secure: bool) -> String {
    format!(
        "{SESSION_COOKIE}={sid}; Path=/; HttpOnly; SameSite=Lax; Max-Age={SESSION_MAX_AGE_SECS}{}",
        if secure { "; Secure" } else { "" }
    )
}

/// 清掉 cookie（Max-Age=0）
pub fn clear_session_cookie(secure: bool) -> String {
    format!(
        "{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{}",
        if secure { "; Secure" } else { "" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn reads_named_cookie() {
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, HeaderValue::from_static("a=1; sid=abc-123; b=2"));
        assert_eq!(get_cookie(&headers, "sid").as_deref(), Some("abc-123"));
        assert_eq!(get_cookie(&headers, "zzz"), None);
    }

    #[test]
    fn cookie_attributes() {
        let cookie = session_cookie("x", true);
        for part in [
            "sid=x",
            "Path=/",
            "HttpOnly",
            "SameSite=Lax",
            "Max-Age=2592000",
            "Secure",
        ] {
            assert!(cookie.contains(part), "{cookie} 缺 {part}");
        }
        assert!(!session_cookie("x", false).contains("Secure"));
        assert!(clear_session_cookie(false).contains("Max-Age=0"));
    }
}
