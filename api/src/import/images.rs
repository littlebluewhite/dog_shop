//! 匯入圖片下載（規格 §13：10 秒 timeout、≤ 10 MB、只收圖片 MIME；失敗只當該列警告）。
//! 只收 http／https，而且每一次連線（含每一跳轉址）的目的位址都必須是公開位址——內網服務、
//! cloud metadata（169.254.169.254）與私有網段一律擋掉

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
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
    #[error("圖片網址不是公開網址")]
    NotPublic,
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

/// 位址白名單：只放行公開位址。**允許 loopback**（127.0.0.0/8、`::1`）——測試的假圖床就在
/// 127.0.0.1，而容器裡的 loopback 只是 api 自己，沒有別的服務可打。其餘保留範圍一律擋掉。
fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        // IPv4-mapped（`::ffff:10.0.0.1`）換回 v4 再套同一套規則，不然是個現成的繞道
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => is_public_v4(v4),
            None => is_public_v6(v6),
        },
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    if ip.is_loopback() {
        return true;
    }
    let [a, b, _, _] = ip.octets();
    !(ip.is_unspecified()
        || ip.is_multicast()
        || ip.is_broadcast()
        || ip.is_private()      // 10/8、172.16/12、192.168/16
        || ip.is_link_local()   // 169.254/16，含 cloud metadata 的 169.254.169.254
        || (a == 100 && (64..128).contains(&b))) // shared address space 100.64/10
}

fn is_public_v6(ip: Ipv6Addr) -> bool {
    if ip.is_loopback() {
        return true;
    }
    let head = ip.segments()[0];
    !(ip.is_unspecified()
        || ip.is_multicast()
        || (head & 0xfe00) == 0xfc00   // unique local fc00::/7
        || (head & 0xffc0) == 0xfe80) // link-local fe80::/10
}

/// 每一次連線之前都要跑：IP 字面值直接判，網域名先解析、**全部**解析結果都是公開位址才放行
/// （解析到私有位址的網域是最常見的繞法）。解析失敗或沒有結果都當連線失敗。
///
/// 已知殘餘風險：檢查通過之後 reqwest 會自己再解析一次 DNS，兩次之間 IP 被換掉（DNS rebinding）
/// 這裡擋不到；要根治得自己接管連線（自訂 resolver／Connector），代價遠大於這裡要防的威脅。
async fn check_public_host(url: &reqwest::Url) -> Result<(), FetchError> {
    let host = url.host_str().ok_or(FetchError::Scheme)?;
    // IPv6 字面值在 URL 裡是包在中括號裡的
    let literal = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = literal.parse::<IpAddr>() {
        return if is_public_ip(ip) {
            Ok(())
        } else {
            Err(FetchError::NotPublic)
        };
    }
    let port = url.port_or_known_default().unwrap_or(80);
    let resolved = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| FetchError::Http("連線失敗".to_string()))?;
    let mut any = false;
    for addr in resolved {
        any = true;
        if !is_public_ip(addr.ip()) {
            return Err(FetchError::NotPublic);
        }
    }
    if any {
        Ok(())
    } else {
        Err(FetchError::Http("連線失敗".to_string()))
    }
}

fn http_url(url: &str) -> Result<reqwest::Url, FetchError> {
    let parsed = reqwest::Url::parse(url).map_err(|_| FetchError::Scheme)?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(FetchError::Scheme);
    }
    Ok(parsed)
}

#[derive(Clone)]
pub struct ImageFetcher {
    client: reqwest::Client,
}

impl ImageFetcher {
    pub fn new() -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(FETCH_TIMEOUT_SECS))
            // 轉址自己跟：每一跳都要重跑公開位址檢查，reqwest 自動跟就跳過檢查了
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("dog-shop-import/1.0")
            .build()?;
        Ok(Self { client })
    }

    /// 下載並檢查 MIME 與大小；不解碼。
    /// 整個過程（所有跳數＋讀 body）包在**一個** 10 秒的 timeout 裡：自己跟轉址之後，光靠
    /// client 的 timeout 會變成每一跳各 10 秒（最壞 40 秒一張圖），那就不是規格 §13 的 10 秒了。
    pub async fn fetch(&self, url: &str) -> Result<Vec<u8>, FetchError> {
        tokio::time::timeout(
            Duration::from_secs(FETCH_TIMEOUT_SECS),
            self.fetch_following_redirects(url),
        )
        .await
        .unwrap_or_else(|_| Err(FetchError::Http("下載超過 10 秒".to_string())))
    }

    /// 最多跟 MAX_REDIRECTS 跳，每一跳（含第一次請求）都先跑公開位址檢查。
    /// 連線錯誤只記 host 與去掉網址的錯誤——網址可能帶簽名 token，不能進 log。
    async fn fetch_following_redirects(&self, url: &str) -> Result<Vec<u8>, FetchError> {
        let mut current = http_url(url)?;
        let mut hops = 0usize;
        let resp = loop {
            check_public_host(&current).await?;
            let host = current.host_str().unwrap_or("?").to_string();
            let resp = self.client.get(current.clone()).send().await.map_err(|e| {
                tracing::info!(host = %host, error = %e.without_url(), "匯入圖片下載連線失敗");
                FetchError::Http("連線失敗".to_string())
            })?;
            if !matches!(resp.status().as_u16(), 301 | 302 | 303 | 307 | 308) {
                break resp;
            }
            if hops >= MAX_REDIRECTS {
                return Err(FetchError::Http("轉址超過 3 次".to_string()));
            }
            let location = resp
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| FetchError::Http("連線失敗".to_string()))?;
            // 相對網址（Location: /ok.png）要用這一跳的網址當基底解析
            current = current
                .join(location)
                .map_err(|_| FetchError::Http("連線失敗".to_string()))?;
            if !matches!(current.scheme(), "http" | "https") {
                return Err(FetchError::Scheme);
            }
            hops += 1;
        };
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
            let label = if content_type.is_empty() {
                "未標示".to_string()
            } else {
                content_type.chars().take(60).collect()
            };
            return Err(FetchError::NotImage(label));
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
        body::{Body, Bytes},
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

    /// 假圖床：/ok.png 真圖、/html 文字、/big 是超過 10 MB 的假圖（有 Content-Length）、
    /// /big-no-length 是真的沒有 Content-Length 的 chunked 回應（超過 10 MB）、/no-type 完全
    /// 沒有 Content-Type header、/500 伺服器錯、/redirect → /ok.png、
    /// /redirect-private → 私有位址、/redirect-loop → 自己、/redirect-relative → 相對的 ok.png
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
            // fetch() 不用真的下載就擋下。但宣告值跟實際位元組數不符的 body，不管長度對
            // hyper 是已知還未知，都沒辦法在真的 hyper／reqwest 之間重現：已知長度時，
            // debug build 會斷言宣告值要跟已知長度一致（不一致就在 per-connection task 裡
            // panic、連線直接斷掉）；改成未知長度（例如下面 /big-no-length 用的
            // http-body-util Channel）雖然不會踩到那個 assert，但 hyper 端會照樣信任
            // 手動設的 Content-Length header 去寫、body 卻只送 1 KB 就結束，等於「宣告
            // 20 MB、實際只給 1 KB 就斷線」，一樣會被判定連線不完整
            // （hyper::Error(IncompleteMessage)），連 Response 都拿不到。真正的關鍵是
            // 「宣告值遠大於實際送出量」這件事本身，不是 Channel 能不能用——Channel 沒問題，
            // /big-no-length 就是用它、而且是真的沒有 Content-Length 的 chunked 回應。這裡
            // 改成 body 真的超過 10 MB（跟 /big-no-length 一樣的大小，Content-Length 由
            // axum 依實際大小自動填，不是宣告不實的假數字），一樣能驗證 fetch() 是靠 header
            // 就提早擋下、不用等把整包 body 讀完。與簡報的差異只在這個測試假伺服器的
            // fixture 寫法，不影響 images.rs 的正式邏輯。
            (
                [(header::CONTENT_TYPE, HeaderValue::from_static("image/png"))],
                Bytes::from(vec![0u8; MAX_IMAGE_BYTES + 1]),
            )
        }
        async fn big_no_length() -> impl IntoResponse {
            // 跟 /big 不同：這裡完全不設 Content-Length，body 用 http-body-util 的 Channel
            // 邊生邊送（64 KB 一個 chunk，最多 200 個，合計 12.5 MB，超過 10 MB 上限）。
            // Channel 沒有覆寫 size_hint，對 hyper 來說 body 長度是「未知」，所以會用
            // Transfer-Encoding: chunked，不帶 Content-Length；fetch() 的預檢
            // （resp.content_length()）過不了這關就會放行，一路進迴圈邊讀邊累加，才會真的
            // 測到「累加超過 MAX_IMAGE_BYTES 就回 TooLarge」那個分支。在 handler 裡直接
            // await 把 200 個 chunk 都送完會卡住（buffer 只有 16、沒有人先消費），所以用
            // tokio::spawn 讓 body 用背景工作邊送，handler 先把 response 回傳給 hyper 讓它
            // 開始邊收邊寫給客戶端；client（fetch()）超過 10 MB 提早斷線後，背景工作的
            // send_data 會失敗，用 break 收掉、不 unwrap／panic。
            let (mut tx, body) =
                http_body_util::channel::Channel::<Bytes, std::convert::Infallible>::new(16);
            tokio::spawn(async move {
                let chunk = Bytes::from(vec![0u8; 64 * 1024]);
                for _ in 0..200 {
                    if tx.send_data(chunk.clone()).await.is_err() {
                        break;
                    }
                }
            });
            (
                [(header::CONTENT_TYPE, HeaderValue::from_static("image/png"))],
                Body::new(body),
            )
        }
        async fn no_type() -> impl IntoResponse {
            // 完全不設 Content-Type：用 http::Response::builder() 直接組，不透過
            // `impl IntoResponse for Bytes`（那個會自動補 application/octet-stream）。
            axum::response::Response::builder()
                .status(StatusCode::OK)
                .body(Body::from(Bytes::from_static(b"\x01\x02\x03")))
                .unwrap()
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
        async fn redirect_private() -> impl IntoResponse {
            (
                StatusCode::FOUND,
                [(
                    header::LOCATION,
                    HeaderValue::from_static("http://192.168.0.1/x.png"),
                )],
            )
        }
        async fn redirect_loop() -> impl IntoResponse {
            (
                StatusCode::FOUND,
                [(header::LOCATION, HeaderValue::from_static("/redirect-loop"))],
            )
        }
        async fn redirect_relative() -> impl IntoResponse {
            (
                StatusCode::FOUND,
                [(header::LOCATION, HeaderValue::from_static("ok.png"))],
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
            .route("/no-type", get(no_type))
            .route("/500", get(fail))
            .route("/redirect", get(redirect))
            .route("/redirect-private", get(redirect_private))
            .route("/redirect-loop", get(redirect_loop))
            .route("/redirect-relative", get(redirect_relative))
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
            fetch_and_store(&fetcher, dir.path(), &format!("{base}/no-type"))
                .await
                .unwrap_err(),
            FetchError::NotImage("未標示".into())
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

    #[test]
    fn is_public_ip_allows_loopback_and_rejects_reserved_ranges() {
        let cases: &[(&str, bool)] = &[
            ("127.0.0.1", true),
            ("127.4.5.6", true),
            ("::1", true),
            ("8.8.8.8", true),
            ("2001:4860:4860::8888", true),
            ("0.0.0.0", false),
            ("10.0.0.1", false),
            ("172.16.0.1", false),
            ("192.168.0.1", false),
            ("169.254.169.254", false),
            ("100.64.0.1", false),
            ("224.0.0.1", false),
            ("255.255.255.255", false),
            ("::", false),
            ("fe80::1", false),
            ("fd00::1", false),
            ("ff02::1", false),
            ("::ffff:10.0.0.1", false),
            ("::ffff:8.8.8.8", true),
        ];
        for (raw, expected) in cases {
            let ip: IpAddr = raw.parse().unwrap();
            assert_eq!(is_public_ip(ip), *expected, "{raw}");
        }
    }

    /// 私有／保留位址在**連線之前**就被擋下來——這些位址上沒有任何測試伺服器在聽，
    /// 拿到 NotPublic（而不是「連線失敗」或逾時）就證明根本沒有送出封包。
    #[tokio::test]
    async fn private_addresses_are_rejected_before_connecting() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let dir = tempfile::tempdir().unwrap();
        let fetcher = ImageFetcher::new().unwrap();
        for url in [
            "http://10.0.0.1/a.png",
            "http://169.254.169.254/latest/meta-data",
            "http://[fe80::1]/x.png",
            "http://100.64.0.1/x.png",
        ] {
            assert_eq!(
                fetch_and_store(&fetcher, dir.path(), url)
                    .await
                    .unwrap_err(),
                FetchError::NotPublic,
                "{url}"
            );
        }
    }

    /// 轉址的每一跳都要重驗：第一跳是公開的（127.0.0.1 的測試伺服器），Location 指向私有位址。
    #[tokio::test]
    async fn a_redirect_into_a_private_address_is_rejected() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let addr = serve().await;
        let dir = tempfile::tempdir().unwrap();
        let fetcher = ImageFetcher::new().unwrap();
        assert_eq!(
            fetch_and_store(
                &fetcher,
                dir.path(),
                &format!("http://{addr}/redirect-private")
            )
            .await
            .unwrap_err(),
            FetchError::NotPublic
        );
    }

    /// 轉址迴圈要在 3 跳之後停下來，而且不能卡住（外層還有 10 秒 timeout 兜底）。
    #[tokio::test]
    async fn a_redirect_loop_stops_after_three_hops() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let addr = serve().await;
        let dir = tempfile::tempdir().unwrap();
        let fetcher = ImageFetcher::new().unwrap();
        let started = std::time::Instant::now();
        let err = fetch_and_store(
            &fetcher,
            dir.path(),
            &format!("http://{addr}/redirect-loop"),
        )
        .await
        .unwrap_err();
        assert_eq!(err, FetchError::Http("轉址超過 3 次".to_string()));
        assert!(started.elapsed() < Duration::from_secs(5), "不能卡住");
    }

    /// 相對的 Location（`ok.png`，沒有開頭斜線）要用這一跳的網址當基底解析。
    #[tokio::test]
    async fn a_relative_redirect_resolves_against_the_current_url() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let addr = serve().await;
        let dir = tempfile::tempdir().unwrap();
        let fetcher = ImageFetcher::new().unwrap();
        let stored = fetch_and_store(
            &fetcher,
            dir.path(),
            &format!("http://{addr}/redirect-relative"),
        )
        .await
        .unwrap();
        assert!(stored.path.starts_with("/uploads/"));
    }
}
