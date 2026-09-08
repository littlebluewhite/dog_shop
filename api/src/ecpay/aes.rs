//! 電子發票的 Data 加解密（規格 §8.4）：JSON → URL encode（.NET 風格）→ AES-128-CBC/PKCS7（HashKey 為 key、HashIV 為 iv）→ Base64
use aes::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit, block_padding::Pkcs7};
use anyhow::Context;
use base64::{Engine, engine::general_purpose::STANDARD};
use percent_encoding::percent_decode_str;
use serde_json::Value;

use crate::ecpay::mac::dotnet_url_encode;

type Enc = cbc::Encryptor<aes::Aes128>;
type Dec = cbc::Decryptor<aes::Aes128>;

/// HashKey／HashIV 都必須剛好 16 bytes
pub fn key16(s: &str) -> anyhow::Result<[u8; 16]> {
    s.as_bytes().try_into().map_err(|_| {
        anyhow::anyhow!(
            "綠界發票 HashKey/HashIV 長度必須是 16 bytes（收到 {} bytes）",
            s.len()
        )
    })
}

pub fn encrypt(key: &[u8; 16], iv: &[u8; 16], plain: &[u8]) -> Vec<u8> {
    Enc::new(key.into(), iv.into()).encrypt_padded_vec_mut::<Pkcs7>(plain)
}

pub fn decrypt(key: &[u8; 16], iv: &[u8; 16], cipher: &[u8]) -> anyhow::Result<Vec<u8>> {
    Dec::new(key.into(), iv.into())
        .decrypt_padded_vec_mut::<Pkcs7>(cipher)
        .map_err(|e| anyhow::anyhow!("AES 解密失敗：{e}"))
}

/// .NET UrlEncode 的反向：`+` 是空白，`%xx` 解碼
pub fn url_decode(s: &str) -> anyhow::Result<String> {
    let plus_fixed = s.replace('+', "%20");
    Ok(percent_decode_str(&plus_fixed)
        .decode_utf8()
        .context("URL decode 的結果不是 UTF-8")?
        .into_owned())
}

/// 送出去的 Data
pub fn encode_data(key: &[u8; 16], iv: &[u8; 16], data: &Value) -> String {
    let encoded = dotnet_url_encode(&data.to_string());
    STANDARD.encode(encrypt(key, iv, encoded.as_bytes()))
}

/// 收回來的 Data
pub fn decode_data(key: &[u8; 16], iv: &[u8; 16], b64: &str) -> anyhow::Result<Value> {
    let cipher = STANDARD.decode(b64.trim()).context("Data 不是 Base64")?;
    let plain = decrypt(key, iv, &cipher)?;
    let text = url_decode(std::str::from_utf8(&plain).context("解密結果不是 UTF-8")?)?;
    serde_json::from_str(&text).context("解密結果不是 JSON")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const KEY: &[u8; 16] = b"ejCk326UnaZWKisg";
    const IV: &[u8; 16] = b"q9jcZX8Ib9LM8wYk";

    #[test]
    fn known_vector_from_openssl() {
        // printf '%s' '%7B%22Name%22%3A%22Test%22%2C%22ID%22%3A%2211%22%7D' \
        //   | openssl enc -aes-128-cbc -K 656a436b333236556e615a574b697367 -iv 71396a635a58384962394c4d3877596b -nosalt -base64 -A
        let plain = b"%7B%22Name%22%3A%22Test%22%2C%22ID%22%3A%2211%22%7D";
        let cipher = encrypt(KEY, IV, plain);
        assert_eq!(cipher.len(), 64, "51 bytes 補到 64");
        assert_eq!(
            STANDARD.encode(&cipher),
            "uvI4yrErM37XNQkXGAgRgJAgHn2t72jahaMZzYhWL1EKCKsTJAo/hk3KsD1/DLNXvLojRdhbR7dzkFTVZwgUwQ=="
        );
        assert_eq!(decrypt(KEY, IV, &cipher).unwrap(), plain.to_vec());
    }

    #[test]
    fn round_trip_json_with_chinese() {
        let v = json!({ "Name": "Test 測試", "ID": "11", "Items": [{ "a": 1 }] });
        let b64 = encode_data(KEY, IV, &v);
        assert_eq!(decode_data(KEY, IV, &b64).unwrap(), v);
    }

    #[test]
    fn url_decode_handles_plus_and_percent() {
        assert_eq!(url_decode("a+b%2c%e6%b8%ac").unwrap(), "a b,測");
        assert_eq!(url_decode("%7B%22x%22%3A1%7D").unwrap(), "{\"x\":1}");
    }

    #[test]
    fn key_must_be_16_bytes() {
        assert!(key16("short").is_err());
        assert!(key16("ejCk326UnaZWKisg!").is_err());
        assert_eq!(key16("ejCk326UnaZWKisg").unwrap(), *KEY);
    }

    #[test]
    fn wrong_key_does_not_decrypt() {
        let cipher = encrypt(KEY, IV, b"hello");
        let other = b"0000000000000000";
        let result = decrypt(other, IV, &cipher);
        assert!(result.map(|p| p != b"hello").unwrap_or(true));
        assert!(decode_data(KEY, IV, "not base64!!").is_err());
    }
}
