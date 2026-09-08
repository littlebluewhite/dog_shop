//! 匯入欄位：範本標題（規格 §13）與蝦皮匯出檔的別名（暫定，拿到真檔後補齊）

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Column {
    ExternalRef,
    Name,
    Description,
    Category,
    Option1Name,
    Option1Value,
    Option2Name,
    Option2Value,
    Price,
    Stock,
    Sku,
    /// 「圖片網址」：一格裡逗號分隔多個
    ImageUrls,
    /// 「商品圖片 1」～「商品圖片 9」：一格一個
    Image(u8),
}

impl Column {
    /// 範本欄名（錯誤訊息與前端顯示用）
    pub fn label(self) -> String {
        match self {
            Column::ExternalRef => "商品編號".into(),
            Column::Name => "商品名稱".into(),
            Column::Description => "商品描述".into(),
            Column::Category => "分類".into(),
            Column::Option1Name => "規格名稱1".into(),
            Column::Option1Value => "規格選項1".into(),
            Column::Option2Name => "規格名稱2".into(),
            Column::Option2Value => "規格選項2".into(),
            Column::Price => "價格".into(),
            Column::Stock => "庫存".into(),
            Column::Sku => "SKU".into(),
            Column::ImageUrls => "圖片網址".into(),
            Column::Image(n) => format!("商品圖片{n}"),
        }
    }
}

pub const TEMPLATE_HEADERS: [&str; 12] = [
    "商品編號",
    "商品名稱",
    "商品描述",
    "分類",
    "規格名稱1",
    "規格選項1",
    "規格名稱2",
    "規格選項2",
    "價格",
    "庫存",
    "SKU",
    "圖片網址",
];

/// 標題正規化：去所有空白（含全形空白）、全形英數與括號轉半形、英文小寫
pub fn normalize_header(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_whitespace() && *c != '\u{3000}')
        .map(|c| match c {
            '\u{FF10}'..='\u{FF19}' => char::from_u32(c as u32 - 0xFF10 + '0' as u32).unwrap_or(c),
            '\u{FF21}'..='\u{FF3A}' => char::from_u32(c as u32 - 0xFF21 + 'A' as u32).unwrap_or(c),
            '\u{FF41}'..='\u{FF5A}' => char::from_u32(c as u32 - 0xFF41 + 'a' as u32).unwrap_or(c),
            '（' => '(',
            '）' => ')',
            other => other,
        })
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// 別名表（正規化後比對）。同一個 Column 可有多個別名；「商品圖片N」用前綴＋數字處理
const ALIASES: &[(&str, Column)] = &[
    ("商品編號", Column::ExternalRef),
    ("商品id", Column::ExternalRef),
    ("商品id(父)", Column::ExternalRef),
    ("父sku", Column::ExternalRef),
    ("主商品貨號", Column::ExternalRef),
    ("商品貨號(父)", Column::ExternalRef),
    ("商品名稱", Column::Name),
    ("名稱", Column::Name),
    ("商品描述", Column::Description),
    ("描述", Column::Description),
    ("商品說明", Column::Description),
    ("分類", Column::Category),
    ("類別", Column::Category),
    ("商品分類", Column::Category),
    ("規格名稱1", Column::Option1Name),
    ("規格1名稱", Column::Option1Name),
    ("選項名稱1", Column::Option1Name),
    ("規格選項1", Column::Option1Value),
    ("規格1", Column::Option1Value),
    ("選項1", Column::Option1Value),
    ("規格名稱2", Column::Option2Name),
    ("規格2名稱", Column::Option2Name),
    ("選項名稱2", Column::Option2Name),
    ("規格選項2", Column::Option2Value),
    ("規格2", Column::Option2Value),
    ("選項2", Column::Option2Value),
    ("價格", Column::Price),
    ("售價", Column::Price),
    ("單價", Column::Price),
    ("庫存", Column::Stock),
    ("數量", Column::Stock),
    ("庫存數量", Column::Stock),
    ("sku", Column::Sku),
    ("商品選項貨號", Column::Sku),
    ("規格貨號", Column::Sku),
    ("貨號", Column::Sku),
    ("圖片網址", Column::ImageUrls),
    ("圖片", Column::ImageUrls),
    ("商品圖片", Column::ImageUrls),
];

pub fn match_header(raw: &str) -> Option<Column> {
    let key = normalize_header(raw);
    if key.is_empty() {
        return None;
    }
    if let Some((_, c)) = ALIASES.iter().find(|(alias, _)| *alias == key) {
        return Some(*c);
    }
    for prefix in ["商品圖片", "圖片", "主圖"] {
        if let Some(rest) = key.strip_prefix(prefix)
            && let Ok(n) = rest.parse::<u8>()
            && (1..=9).contains(&n)
        {
            return Some(Column::Image(n));
        }
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderMap {
    /// index = 工作表欄位；None = 對不上或重複
    pub columns: Vec<Option<Column>>,
    /// 對不上的原始標題（給預覽頁列出）
    pub unmatched: Vec<String>,
}

pub fn map_headers(row: &[String]) -> HeaderMap {
    let mut seen: std::collections::HashSet<Column> = std::collections::HashSet::new();
    let mut columns = Vec::with_capacity(row.len());
    let mut unmatched = Vec::new();
    for raw in row {
        let trimmed = raw.trim();
        match match_header(trimmed) {
            Some(c) if seen.insert(c) => columns.push(Some(c)),
            Some(_) => {
                unmatched.push(trimmed.to_string());
                columns.push(None);
            }
            None => {
                if !trimmed.is_empty() {
                    unmatched.push(trimmed.to_string());
                }
                columns.push(None);
            }
        }
    }
    HeaderMap { columns, unmatched }
}

/// 標題列：對到「商品名稱」且對到至少 3 個欄位
pub fn is_header_row(row: &[String]) -> bool {
    let m = map_headers(row);
    let matched = m.columns.iter().flatten().count();
    matched >= 3 && m.columns.contains(&Some(Column::Name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_fullwidth_and_spaces() {
        assert_eq!(normalize_header(" 規格名稱 １ "), "規格名稱1");
        assert_eq!(normalize_header("商品圖片　２"), "商品圖片2");
        assert_eq!(normalize_header("SKU"), "sku");
        assert_eq!(normalize_header("商品ID（父）"), "商品id(父)");
    }

    #[test]
    fn matches_template_and_shopee_aliases() {
        for (h, c) in [
            ("商品編號", Column::ExternalRef),
            ("商品ID", Column::ExternalRef),
            ("父SKU", Column::ExternalRef),
            ("主商品貨號", Column::ExternalRef),
            ("商品名稱", Column::Name),
            ("商品描述", Column::Description),
            ("分類", Column::Category),
            ("類別", Column::Category),
            ("規格名稱1", Column::Option1Name),
            ("規格名稱 1", Column::Option1Name),
            ("規格選項1", Column::Option1Value),
            ("規格選項 2", Column::Option2Value),
            ("價格", Column::Price),
            ("售價", Column::Price),
            ("庫存", Column::Stock),
            ("數量", Column::Stock),
            ("SKU", Column::Sku),
            ("商品選項貨號", Column::Sku),
            ("圖片網址", Column::ImageUrls),
            ("商品圖片 1", Column::Image(1)),
            ("商品圖片9", Column::Image(9)),
            ("圖片3", Column::Image(3)),
        ] {
            assert_eq!(match_header(h), Some(c), "{h}");
        }
        assert_eq!(match_header("商品圖片 10"), None);
        assert_eq!(match_header("品牌"), None);
        assert_eq!(match_header(""), None);
    }

    #[test]
    fn maps_headers_and_reports_unmatched_and_duplicates() {
        let row: Vec<String> = [
            "商品編號",
            "商品名稱",
            "品牌",
            "價格",
            "價格",
            "商品圖片 1",
            "商品圖片 2",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let m = map_headers(&row);
        assert_eq!(m.columns[0], Some(Column::ExternalRef));
        assert_eq!(m.columns[1], Some(Column::Name));
        assert_eq!(m.columns[2], None);
        assert_eq!(m.columns[3], Some(Column::Price));
        assert_eq!(m.columns[4], None, "重複的欄位第二個不對應");
        assert_eq!(m.columns[5], Some(Column::Image(1)));
        assert_eq!(m.unmatched, vec!["品牌".to_string(), "價格".to_string()]);
        assert!(is_header_row(&row));
        let not: Vec<String> = ["請依範本填寫", "", "商品名稱"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!(!is_header_row(&not));
    }
}
