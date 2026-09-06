//! 圖片：重新解碼 → 縮圖 → JPEG → 存檔。不保留原檔（規格 §11）。
//! 輸出用 JPEG 而不是 WebP：image crate 的 WebP 只支援無損，商品照會太大（見計畫「與規格不同之處」1）。
use std::{
    io::Cursor,
    path::{Path, PathBuf},
};

use chrono::{Datelike, Utc};
use image::{
    DynamicImage, ImageDecoder, ImageError, ImageReader, Limits, codecs::jpeg::JpegEncoder,
    imageops::FilterType,
};
use serde::Serialize;
use uuid::Uuid;

pub const MAX_UPLOAD_BYTES: usize = 10 * 1024 * 1024;
pub const MAIN_MAX_EDGE: u32 = 1600;
pub const THUMB_MAX_EDGE: u32 = 400;
pub const ALLOWED_MIME: &[&str] = &["image/jpeg", "image/png", "image/webp", "image/gif"];

#[derive(Debug, Serialize)]
pub struct StoredImage {
    /// 網址路徑，例如 /uploads/2026/09/0192a1b2-....jpg
    pub path: String,
    pub thumb_path: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("圖片無法讀取")]
    Decode(#[from] ImageError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub struct Encoded {
    pub main: Vec<u8>,
    pub thumb: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// 解碼（限制尺寸／記憶體、套用 EXIF 方向）→ 主圖最長邊 1600（小圖不放大）→ 縮圖最長邊 400
/// → 兩張都轉 JPEG（去透明層）
pub fn encode(bytes: &[u8]) -> Result<Encoded, ImageError> {
    let mut limits = Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    limits.max_alloc = Some(256 << 20);
    let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    reader.limits(limits.clone());
    let mut decoder = reader.into_decoder()?;
    // into_decoder() 只檢查寬高，不像 decode() 會先 reserve() 檢查 max_alloc；補上這一步
    // 避免一張寬高剛好在上限內、但像素資料仍超過 256 MiB 的圖片繞過記憶體上限。
    limits.reserve(decoder.total_bytes())?;
    let orientation = decoder.orientation()?;
    let mut decoded = DynamicImage::from_decoder(decoder)?;
    decoded.apply_orientation(orientation);
    let main = if decoded.width().max(decoded.height()) > MAIN_MAX_EDGE {
        decoded.resize(MAIN_MAX_EDGE, MAIN_MAX_EDGE, FilterType::Lanczos3)
    } else {
        decoded
    };
    let thumb = main.resize(THUMB_MAX_EDGE, THUMB_MAX_EDGE, FilterType::Triangle);
    Ok(Encoded {
        width: main.width(),
        height: main.height(),
        main: to_jpeg(&main, 82)?,
        thumb: to_jpeg(&thumb, 80)?,
    })
}

fn to_jpeg(img: &DynamicImage, quality: u8) -> Result<Vec<u8>, ImageError> {
    let mut buffer = Cursor::new(Vec::new());
    let encoder = JpegEncoder::new_with_quality(&mut buffer, quality);
    DynamicImage::ImageRgb8(img.to_rgb8()).write_with_encoder(encoder)?;
    Ok(buffer.into_inner())
}

/// 存到 {upload_dir}/yyyy/mm/{uuid}.jpg 與 {uuid}_thumb.jpg；回網址路徑
pub async fn save(upload_dir: &Path, bytes: Vec<u8>) -> Result<StoredImage, StorageError> {
    // 解碼與縮圖很吃 CPU，丟到 blocking thread
    let encoded = tokio::task::spawn_blocking(move || encode(&bytes))
        .await
        .map_err(std::io::Error::other)??;
    let now = Utc::now();
    let rel_dir = format!("{:04}/{:02}", now.year(), now.month());
    let id = Uuid::now_v7();
    let dir: PathBuf = upload_dir.join(&rel_dir);
    tokio::fs::create_dir_all(&dir).await?;
    tokio::fs::write(dir.join(format!("{id}.jpg")), &encoded.main).await?;
    tokio::fs::write(dir.join(format!("{id}_thumb.jpg")), &encoded.thumb).await?;
    Ok(StoredImage {
        path: format!("/uploads/{rel_dir}/{id}.jpg"),
        thumb_path: format!("/uploads/{rel_dir}/{id}_thumb.jpg"),
        width: encoded.width,
        height: encoded.height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(width, height, image::Rgba([200, 30, 30, 128]));
        let mut buffer = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(img)
            .write_to(&mut buffer, image::ImageFormat::Png)
            .unwrap();
        buffer.into_inner()
    }

    #[test]
    fn big_image_is_resized_and_becomes_jpeg() {
        let encoded = encode(&png(3200, 1600)).unwrap();
        assert_eq!((encoded.width, encoded.height), (1600, 800));
        assert_eq!(&encoded.main[..2], &[0xFF, 0xD8], "JPEG 開頭是 FF D8");
        let thumb = image::load_from_memory(&encoded.thumb).unwrap();
        assert_eq!((thumb.width(), thumb.height()), (400, 200));
    }

    #[test]
    fn small_image_is_not_upscaled() {
        let encoded = encode(&png(300, 200)).unwrap();
        assert_eq!((encoded.width, encoded.height), (300, 200));
    }

    #[test]
    fn garbage_is_decode_error() {
        assert!(encode(b"not an image").is_err());
    }

    #[tokio::test]
    async fn save_writes_two_files_under_year_month() {
        let dir = std::env::temp_dir().join(format!("dog_shop_storage_{}", Uuid::new_v4()));
        let stored = save(&dir, png(50, 40)).await.unwrap();
        assert!(stored.path.starts_with("/uploads/"));
        assert!(stored.thumb_path.ends_with("_thumb.jpg"));
        let rel = stored.path.trim_start_matches("/uploads/");
        assert!(dir.join(rel).is_file());
        assert!(
            dir.join(stored.thumb_path.trim_start_matches("/uploads/"))
                .is_file()
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
