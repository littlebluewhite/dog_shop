//! CheckMacValue（規格 §8.1）：參數依 key 排序（不分大小寫）→ `HashKey=…&k=v&…&HashIV=…`
//! → .NET 風格 URL encode → 轉小寫 → SHA256 → 轉大寫。物流用的 MD5 版本在計畫 4。
use sha2::{Digest, Sha256};

/// .NET `HttpUtility.UrlEncode` 的規則：`A-Z a-z 0-9 - _ . ! * ( )` 原樣、空白變 `+`、
/// 其他每個 byte 變 `%xx`（小寫 hex）。綠界的替換表（`%2d→-`、`%5f→_`、`%2e→.`、`%21→!`、
/// `%2a→*`、`%28→(`、`%29→)`）是給會把這些字元編碼的語言用的，這裡本來就不編碼它們。
pub fn dotnet_url_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 3);
    for b in input.bytes() {
        match b {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'*'
            | b'('
            | b')' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02x}")),
        }
    }
    out
}

/// 排序、串接、編碼、轉小寫後（還沒雜湊）的字串；拆出來方便對照文件範例
pub fn raw_string(hash_key: &str, hash_iv: &str, params: &[(String, String)]) -> String {
    let mut sorted: Vec<&(String, String)> = params.iter().collect();
    sorted.sort_by_key(|(k, _)| k.to_ascii_lowercase());
    let mut joined = format!("HashKey={hash_key}");
    for (k, v) in sorted {
        joined.push('&');
        joined.push_str(k);
        joined.push('=');
        joined.push_str(v);
    }
    joined.push_str("&HashIV=");
    joined.push_str(hash_iv);
    dotnet_url_encode(&joined).to_lowercase()
}

/// SHA256（EncryptType=1）的 CheckMacValue，大寫 hex
pub fn check_mac_value(hash_key: &str, hash_iv: &str, params: &[(String, String)]) -> String {
    let digest = Sha256::digest(raw_string(hash_key, hash_iv, params).as_bytes());
    hex::encode_upper(digest)
}

/// 驗回呼：取出 `CheckMacValue`（key 不分大小寫），用其餘欄位重算再比對（值不分大小寫）
pub fn verify(hash_key: &str, hash_iv: &str, params: &[(String, String)]) -> bool {
    let Some((_, given)) = params
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("CheckMacValue"))
    else {
        return false;
    };
    let rest: Vec<(String, String)> = params
        .iter()
        .filter(|(k, _)| !k.eq_ignore_ascii_case("CheckMacValue"))
        .cloned()
        .collect();
    check_mac_value(hash_key, hash_iv, &rest).eq_ignore_ascii_case(given)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "pwFHCqoQZGmho4w6";
    const IV: &str = "EkRm7iFT261dpevs";

    /// developers.ecpay.com.tw 全方位金流「檢查碼機制」的範例參數
    fn doc_params() -> Vec<(String, String)> {
        [
            ("TradeDesc", "促銷方案"),
            ("PaymentType", "aio"),
            ("MerchantTradeDate", "2023/03/12 15:30:23"),
            ("MerchantTradeNo", "ecpay20230312153023"),
            ("MerchantID", "3002607"),
            ("ReturnURL", "https://www.ecpay.com.tw/receive.php"),
            ("ItemName", "Apple iphone 15"),
            ("TotalAmount", "30000"),
            ("ChoosePayment", "ALL"),
            ("EncryptType", "1"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
    }

    #[test]
    fn url_encode_is_dotnet_style() {
        assert_eq!(dotnet_url_encode("Apple iphone 15"), "Apple+iphone+15");
        assert_eq!(
            dotnet_url_encode("2023/03/12 15:30:23"),
            "2023%2f03%2f12+15%3a30%3a23"
        );
        assert_eq!(dotnet_url_encode("a-b_c.d!e*f(g)"), "a-b_c.d!e*f(g)");
        assert_eq!(
            dotnet_url_encode("促銷方案"),
            "%e4%bf%83%e9%8a%b7%e6%96%b9%e6%a1%88"
        );
        assert_eq!(dotnet_url_encode("a&b=c#d"), "a%26b%3dc%23d");
    }

    #[test]
    fn raw_string_matches_doc() {
        assert_eq!(
            raw_string(KEY, IV, &doc_params()),
            "hashkey%3dpwfhcqoqzgmho4w6%26choosepayment%3dall%26encrypttype%3d1%26itemname%3dapple+iphone+15%26merchantid%3d3002607%26merchanttradedate%3d2023%2f03%2f12+15%3a30%3a23%26merchanttradeno%3decpay20230312153023%26paymenttype%3daio%26returnurl%3dhttps%3a%2f%2fwww.ecpay.com.tw%2freceive.php%26totalamount%3d30000%26tradedesc%3d%e4%bf%83%e9%8a%b7%e6%96%b9%e6%a1%88%26hashiv%3dekrm7ift261dpevs"
        );
    }

    #[test]
    fn check_mac_value_matches_doc() {
        assert_eq!(
            check_mac_value(KEY, IV, &doc_params()),
            "6C51C9E6888DE861FD62FB1DD17029FC742634498FD813DC43D4243B5685B840"
        );
    }

    #[test]
    fn verify_accepts_correct_and_rejects_tampered() {
        let mut params = doc_params();
        // 綠界回傳的值是大寫，但比對不分大小寫
        params.push((
            "CheckMacValue".to_string(),
            "6c51c9e6888de861fd62fb1dd17029fc742634498fd813dc43d4243b5685b840".to_string(),
        ));
        assert!(verify(KEY, IV, &params));

        // TotalAmount 是 doc_params 的第 8 個
        params[7].1 = "30001".to_string();
        assert!(!verify(KEY, IV, &params), "改了金額就不能過");

        assert!(!verify(KEY, IV, &doc_params()), "沒有 CheckMacValue 不能過");

        let mut params2 = doc_params();
        params2.push((
            "CheckMacValue".to_string(),
            check_mac_value(KEY, IV, &doc_params()),
        ));
        assert!(!verify("wrongkey", IV, &params2), "key 不對不能過");
    }
}
