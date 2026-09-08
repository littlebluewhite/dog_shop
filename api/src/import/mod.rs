//! 商品匯入（規格 §13）：xlsx → 欄位對應 → 解析 → 下載圖片 → 寫入商品

pub mod apply;
pub mod columns;
pub mod images;
pub mod parse;

/// xlsx 上限 5 MB（規格沒定；蝦皮匯出通常 < 1 MB）
pub const MAX_XLSX_BYTES: usize = 5 * 1024 * 1024;
/// 資料列上限（標題列之後）
pub const MAX_ROWS: usize = 5000;

// Task 4 再打開
// pub use apply::{ImportResult, ImportWarning, ImportedProduct, apply};
pub use images::{FetchError, ImageFetcher, fetch_all, fetch_and_store};
pub use parse::{
    ImportError, ImportProduct, ImportVariant, ParsedImport, RowError, parse_grid, parse_xlsx,
};
