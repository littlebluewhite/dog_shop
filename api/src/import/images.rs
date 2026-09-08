//! 匯入圖片下載（規格 §13：10 秒 timeout、≤ 10 MB、只收圖片 MIME；失敗只當該列警告）。
//! 只擋非 http/https；不擋內網位址（與規格不同之處 60）

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::storage::{self, StorageError, StoredImage};

pub const FETCH_TIMEOUT_SECS: u64 = 10;
pub const MAX_IMAGE_BYTES: usize = storage::MAX_UPLOAD_BYTES;
pub const MAX_REDIRECTS: usize = 3;
pub const FETCH_CONCURRENCY: usize = 6;

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum FetchError {
    #[error("只接受 http／https 網址")]
    Scheme,
    /// 固定文案＋狀態碼或「連線失敗」；不放回應內容
    #[error("下載失敗：{0}")]
    Http(String),
    #[error("不是圖片（{0}）")]
    NotImage(String),
    #[error("圖片超過 10 MB")]
    TooLarge,
    #[error("圖片無法讀取")]
    Decode,
    #[error("存檔失敗")]
    Store,
}

#[derive(Clone)]
pub struct ImageFetcher {
    client: reqwest::Client,
}

impl ImageFetcher {
    pub fn new() -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(FETCH_TIMEOUT_SECS))
            .redirect(reqwest::redirect::Policy::limited(MAX_REDIRECTS))
            .user_agent("dog-shop-import/1.0")
            .build()?;
        Ok(Self { client })
    }

    /// 下載並檢查 MIME 與大小；不解碼
    pub async fn fetch(&self, url: &str) -> Result<Vec<u8>, FetchError> {
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(FetchError::Scheme);
        }
        let resp = self.client.get(url).send().await.map_err(|e| {
            tracing::info!(url, error = %e, "匯入圖片下載連線失敗");
            FetchError::Http("連線失敗".to_string())
        })?;
        let status = resp.status();
        if !status.is_success() {
            return Err(FetchError::Http(format!("HTTP {}", status.as_u16())));
        }
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if !content_type.starts_with("image/") {
            return Err(FetchError::NotImage(
                content_type.chars().take(60).collect(),
            ));
        }
        if resp
            .content_length()
            .is_some_and(|n| n > MAX_IMAGE_BYTES as u64)
        {
            return Err(FetchError::TooLarge);
        }
        let mut resp = resp;
        let mut buf: Vec<u8> = Vec::new();
        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|_| FetchError::Http("連線失敗".to_string()))?
        {
            if buf.len() + chunk.len() > MAX_IMAGE_BYTES {
                return Err(FetchError::TooLarge);
            }
            buf.extend_from_slice(&chunk);
        }
        Ok(buf)
    }
}

/// 下載 → 用既有的 storage::save 重新解碼、縮圖、存 JPEG
pub async fn fetch_and_store(
    fetcher: &ImageFetcher,
    upload_dir: &Path,
    url: &str,
) -> Result<StoredImage, FetchError> {
    let bytes = fetcher.fetch(url).await?;
    storage::save(upload_dir, bytes).await.map_err(|e| match e {
        StorageError::Decode(_) => FetchError::Decode,
        StorageError::Io(err) => {
            tracing::error!(error = %err, "匯入圖片存檔失敗");
            FetchError::Store
        }
    })
}

/// 依 urls 順序回傳；最多 FETCH_CONCURRENCY 個同時下載。
/// 最壞情況：每張 10 秒 timeout，N 張要 N/6 × 10 秒 —— 500 個商品各 9 張約 2 小時，所以 commit 的
/// 回應時間由圖片數決定（手冊要提醒老闆分批匯入）
pub async fn fetch_all(
    fetcher: &ImageFetcher,
    upload_dir: &Path,
    urls: &[String],
) -> Vec<(String, Result<StoredImage, FetchError>)> {
    let sem = Arc::new(Semaphore::new(FETCH_CONCURRENCY));
    let mut set = JoinSet::new();
    for (i, url) in urls.iter().cloned().enumerate() {
        let sem = sem.clone();
        let fetcher = fetcher.clone();
        let dir = upload_dir.to_path_buf();
        set.spawn(async move {
            let _permit = sem.acquire_owned().await.expect("semaphore 不會關閉");
            let r = fetch_and_store(&fetcher, &dir, &url).await;
            (i, url, r)
        });
    }
    let mut out: Vec<Option<(String, Result<StoredImage, FetchError>)>> =
        (0..urls.len()).map(|_| None).collect();
    while let Some(joined) = set.join_next().await {
        match joined {
            Ok((i, url, r)) => out[i] = Some((url, r)),
            Err(e) => tracing::error!(error = %e, "匯入圖片工作 panic"),
        }
    }
    out.into_iter()
        .enumerate()
        .map(|(i, slot)| slot.unwrap_or_else(|| (urls[i].clone(), Err(FetchError::Store))))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        body::Bytes,
        http::{HeaderValue, StatusCode, header},
        response::IntoResponse,
        routing::get,
    };
    use std::net::SocketAddr;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let img = image::RgbImage::from_pixel(width, height, image::Rgb([200, 120, 40]));
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    /// 假圖床：/ok.png 真圖、/html 文字、/big 與 /big-no-length 都是超過 10 MB 的假圖、
    /// /500 伺服器錯、/redirect → /ok.png
    async fn serve() -> SocketAddr {
        async fn ok() -> impl IntoResponse {
            (
                [(header::CONTENT_TYPE, HeaderValue::from_static("image/png"))],
                Bytes::from(png(64, 48)),
            )
        }
        async fn html() -> impl IntoResponse {
            (
                [(header::CONTENT_TYPE, HeaderValue::from_static("text/html"))],
                "<html>",
            )
        }
        async fn big() -> impl IntoResponse {
            // 簡報原本是「宣告 Content-Length: 20 MB、實際只送 1 KB」，靠假的 header 讓
            // fetch() 不用真的下載就擋下。但這種宣告與實際不符的 body 沒辦法在真的
            // hyper／reqwest 之間重現：body 長度對 hyper 已知時，debug build 會斷言宣告值要
            // 跟已知長度一致（不一致就在 per-connection task 裡 panic、連線直接斷掉）；改成
            // 長度對 hyper「未知」的 body（例如用 http-body-util 的 Channel）雖然不會踩到
            // assert，但 hyper／reqwest 會把「body 送到一半、實際位元組數遠少於宣告值就斷線」
            // 判定成連線不完整（hyper::Error(IncompleteMessage)），一樣連 Response 都拿不到
            // ——兩條路 fetch() 最後都只會看到連線失敗，測不到 TooLarge。這裡改成 body 真的
            // 超過 10 MB（跟 /big-no-length 一樣的大小，Content-Length 由 axum 依實際大小
            // 自動填，不是宣告不實的假數字），一樣能驗證 fetch() 是靠 header 就提早擋下、
            // 不用等把整包 body 讀完。與簡報的差異只在這個測試假伺服器的 fixture 寫法，
            // 不影響 images.rs 的正式邏輯。
            (
                [(header::CONTENT_TYPE, HeaderValue::from_static("image/png"))],
                Bytes::from(vec![0u8; MAX_IMAGE_BYTES + 1]),
            )
        }
        async fn big_no_length() -> impl IntoResponse {
            (
                [(header::CONTENT_TYPE, HeaderValue::from_static("image/png"))],
                Bytes::from(vec![0u8; MAX_IMAGE_BYTES + 1]),
            )
        }
        async fn fail() -> impl IntoResponse {
            (StatusCode::INTERNAL_SERVER_ERROR, "boom")
        }
        async fn redirect() -> impl IntoResponse {
            (
                StatusCode::FOUND,
                [(header::LOCATION, HeaderValue::from_static("/ok.png"))],
            )
        }
        async fn corrupt() -> impl IntoResponse {
            (
                [(header::CONTENT_TYPE, HeaderValue::from_static("image/png"))],
                "not really a png",
            )
        }
        let app = Router::new()
            .route("/ok.png", get(ok))
            .route("/html", get(html))
            .route("/big", get(big))
            .route("/big-no-length", get(big_no_length))
            .route("/500", get(fail))
            .route("/redirect", get(redirect))
            .route("/corrupt", get(corrupt));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        addr
    }

    #[tokio::test]
    async fn fetches_stores_and_classifies_failures() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let addr = serve().await;
        let base = format!("http://{addr}");
        let dir = tempfile::tempdir().unwrap();
        let fetcher = ImageFetcher::new().unwrap();

        let stored = fetch_and_store(&fetcher, dir.path(), &format!("{base}/ok.png"))
            .await
            .unwrap();
        assert!(stored.path.starts_with("/uploads/"));
        assert!(
            dir.path()
                .join(stored.path.trim_start_matches("/uploads/"))
                .exists()
        );

        let via_redirect = fetch_and_store(&fetcher, dir.path(), &format!("{base}/redirect"))
            .await
            .unwrap();
        assert_ne!(via_redirect.path, stored.path);

        assert_eq!(
            fetch_and_store(&fetcher, dir.path(), "ftp://x/y.png")
                .await
                .unwrap_err(),
            FetchError::Scheme
        );
        assert_eq!(
            fetch_and_store(&fetcher, dir.path(), "/relative.png")
                .await
                .unwrap_err(),
            FetchError::Scheme
        );
        assert_eq!(
            fetch_and_store(&fetcher, dir.path(), &format!("{base}/html"))
                .await
                .unwrap_err(),
            FetchError::NotImage("text/html".into())
        );
        assert_eq!(
            fetch_and_store(&fetcher, dir.path(), &format!("{base}/big"))
                .await
                .unwrap_err(),
            FetchError::TooLarge
        );
        assert_eq!(
            fetch_and_store(&fetcher, dir.path(), &format!("{base}/big-no-length"))
                .await
                .unwrap_err(),
            FetchError::TooLarge
        );
        assert_eq!(
            fetch_and_store(&fetcher, dir.path(), &format!("{base}/500"))
                .await
                .unwrap_err(),
            FetchError::Http("HTTP 500".into())
        );
        assert_eq!(
            fetch_and_store(&fetcher, dir.path(), &format!("{base}/corrupt"))
                .await
                .unwrap_err(),
            FetchError::Decode
        );
        let unreachable = fetch_and_store(&fetcher, dir.path(), "http://127.0.0.1:1/x.png")
            .await
            .unwrap_err();
        assert_eq!(unreachable, FetchError::Http("連線失敗".to_string()));
    }

    #[tokio::test]
    async fn fetch_all_keeps_order_and_isolates_failures() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let addr = serve().await;
        let base = format!("http://{addr}");
        let dir = tempfile::tempdir().unwrap();
        let fetcher = ImageFetcher::new().unwrap();
        let urls: Vec<String> = (0..8)
            .map(|i| {
                if i == 3 {
                    format!("{base}/500")
                } else {
                    format!("{base}/ok.png")
                }
            })
            .collect();
        let results = fetch_all(&fetcher, dir.path(), &urls).await;
        assert_eq!(results.len(), 8);
        for (i, (url, r)) in results.iter().enumerate() {
            assert_eq!(url, &urls[i]);
            if i == 3 {
                assert!(r.is_err());
            } else {
                assert!(r.is_ok());
            }
        }
    }
}
