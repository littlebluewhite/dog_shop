//! xlsx → 字串格 → ParsedImport（規格 §13；與規格不同之處 52、53）

use std::collections::HashMap;
use std::io::Cursor;

use calamine::{Data, Reader, Xlsx, open_workbook_from_rs};
use serde::Serialize;

use crate::domain::products::{MAX_IMAGES, MAX_PRICE, MAX_VARIANTS};
use crate::import::MAX_ROWS;
use crate::import::columns::{Column, HeaderMap, is_header_row, map_headers};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RowError {
    /// 工作表 1-based 列號
    pub row: u32,
    /// 範本欄名；整列的錯誤是 None
    pub column: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ImportVariant {
    pub row: u32,
    pub option1_value: Option<String>,
    pub option2_value: Option<String>,
    pub sku: Option<String>,
    pub price: i32,
    /// 空白 = 保留現值（新商品為 0）
    pub stock: Option<i32>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ImportProduct {
    pub external_ref: String,
    pub first_row: u32,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub option1_name: Option<String>,
    pub option2_name: Option<String>,
    pub image_urls: Vec<String>,
    pub variants: Vec<ImportVariant>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ParsedImport {
    pub sheet: String,
    pub header_row: u32,
    /// 標題列之後、非整列空白的列數
    pub row_count: u32,
    pub products: Vec<ImportProduct>,
    pub errors: Vec<RowError>,
    pub unmatched_columns: Vec<String>,
}

impl ParsedImport {
    pub fn variant_count(&self) -> usize {
        self.products.iter().map(|p| p.variants.len()).sum()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("檔案不是 xlsx 或已損壞")]
    Unreadable,
    #[error("找不到標題列（需要「商品名稱」以及至少兩個其他範本欄位）")]
    NoHeader,
    #[error("缺少必要欄位：{0}")]
    MissingColumns(String),
    #[error("資料列超過 {0} 列")]
    TooManyRows(usize),
}

/// 儲存格 → 字串：數字去掉 .0；日期／錯誤當空白；去頭尾空白
fn cell_to_string(d: &Data) -> String {
    match d {
        Data::String(s) => s.trim().to_string(),
        Data::Int(i) => i.to_string(),
        Data::Float(f) if f.fract() == 0.0 && f.abs() < 1e15 => format!("{}", *f as i64),
        Data::Float(f) => f.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::DateTimeIso(s) | Data::DurationIso(s) => s.trim().to_string(),
        Data::DateTime(_) | Data::Error(_) | Data::Empty => String::new(),
    }
}

/// 第一個工作表 → 字串格 → parse_grid
pub fn parse_xlsx(bytes: &[u8]) -> Result<ParsedImport, ImportError> {
    let mut wb: Xlsx<_> =
        open_workbook_from_rs(Cursor::new(bytes.to_vec())).map_err(|_| ImportError::Unreadable)?;
    let name = wb
        .sheet_names()
        .first()
        .cloned()
        .ok_or(ImportError::Unreadable)?;
    let range = wb
        .worksheet_range(&name)
        .map_err(|_| ImportError::Unreadable)?;
    let grid: Vec<Vec<String>> = range
        .rows()
        .map(|r| r.iter().map(cell_to_string).collect())
        .collect();
    parse_grid(&name, &grid)
}

fn blank(s: &str) -> bool {
    s.trim().is_empty()
}

fn opt(s: &str) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// 「1,200」「NT$1,200」「1200元」「３５０」「99.0」「1,200.00」「ＮＴ＄1200」→ 1200／1200／1200／350／99／1200／1200；
/// 「1.5」「-1」「abc」「12a3」「1t2」→ None
fn parse_amount(raw: &str) -> Option<i64> {
    let t = raw.trim();
    let t = ["NT$", "nt$", "Nt$", "NT＄", "ＮＴ＄", "$", "＄"]
        .iter()
        .find_map(|p| t.strip_prefix(p))
        .unwrap_or(t);
    let t = t.strip_suffix('元').unwrap_or(t);
    let mut s = String::new();
    for c in t.chars() {
        match c {
            '0'..='9' | '.' => s.push(c),
            '\u{FF10}'..='\u{FF19}' => s.push(char::from_u32(c as u32 - 0xFF10 + '0' as u32)?),
            '．' => s.push('.'),
            '-' | '－' => return None,
            c if c.is_whitespace() => {}
            ',' | '，' => {}
            _ => return None,
        }
    }
    let (int_part, frac_part) = match s.split_once('.') {
        Some((i, f)) => (i, f),
        None => (s.as_str(), ""),
    };
    if int_part.is_empty() || !frac_part.chars().all(|c| c == '0') {
        return None;
    }
    int_part.parse::<i64>().ok()
}

fn split_urls(raw: &str) -> Vec<String> {
    raw.split([',', '，', '\n', ';', '；'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

struct Row<'a> {
    map: &'a HeaderMap,
    cells: &'a [String],
}

impl Row<'_> {
    fn get(&self, col: Column) -> &str {
        self.map
            .columns
            .iter()
            .position(|c| *c == Some(col))
            .and_then(|i| self.cells.get(i))
            .map(String::as_str)
            .unwrap_or("")
    }
    fn is_blank(&self) -> bool {
        self.cells.iter().all(|c| blank(c))
    }
    /// 每個網址標記來源欄（「圖片網址」或「商品圖片N」），壞網址的錯誤才能指到正確欄位
    fn image_urls(&self) -> Vec<(Column, String)> {
        let mut urls: Vec<(Column, String)> = split_urls(self.get(Column::ImageUrls))
            .into_iter()
            .map(|u| (Column::ImageUrls, u))
            .collect();
        for n in 1..=9u8 {
            if let Some(u) = opt(self.get(Column::Image(n))) {
                urls.push((Column::Image(n), u));
            }
        }
        urls
    }
}

pub fn parse_grid(sheet: &str, grid: &[Vec<String>]) -> Result<ParsedImport, ImportError> {
    let header_idx = grid
        .iter()
        .position(|r| is_header_row(r))
        .ok_or(ImportError::NoHeader)?;
    let map = map_headers(&grid[header_idx]);
    let missing: Vec<String> = [Column::ExternalRef, Column::Name, Column::Price]
        .into_iter()
        .filter(|c| !map.columns.contains(&Some(*c)))
        .map(Column::label)
        .collect();
    if !missing.is_empty() {
        return Err(ImportError::MissingColumns(missing.join("、")));
    }
    let data_rows = &grid[header_idx + 1..];
    let non_blank = data_rows
        .iter()
        .filter(|r| !r.iter().all(|c| blank(c)))
        .count();
    if non_blank > MAX_ROWS {
        return Err(ImportError::TooManyRows(MAX_ROWS));
    }

    let mut out = ParsedImport {
        sheet: sheet.to_string(),
        header_row: header_idx as u32 + 1,
        unmatched_columns: map.unmatched.clone(),
        ..Default::default()
    };
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut last: Option<usize> = None;

    for (offset, cells) in data_rows.iter().enumerate() {
        let row_no = (header_idx + 1 + offset) as u32 + 1;
        let row = Row { map: &map, cells };
        if row.is_blank() {
            continue;
        }
        let external_ref = row.get(Column::ExternalRef).trim().to_string();
        let target: Option<usize> = if !external_ref.is_empty() {
            match index.get(&external_ref) {
                Some(&i) => Some(i),
                None => {
                    if external_ref.chars().count() > 100 {
                        out.errors.push(RowError {
                            row: row_no,
                            column: Some(Column::ExternalRef.label()),
                            message: "商品編號最多 100 字".into(),
                        });
                    }
                    let name = row.get(Column::Name).trim().to_string();
                    if name.is_empty() {
                        out.errors.push(RowError {
                            row: row_no,
                            column: Some(Column::Name.label()),
                            message: "商品名稱必填".into(),
                        });
                    } else if name.chars().count() > 120 {
                        out.errors.push(RowError {
                            row: row_no,
                            column: Some(Column::Name.label()),
                            message: "商品名稱最多 120 字".into(),
                        });
                    }
                    let image_urls = row.image_urls();
                    for (col, u) in &image_urls {
                        if !(u.starts_with("http://") || u.starts_with("https://")) {
                            out.errors.push(RowError {
                                row: row_no,
                                column: Some(col.label()),
                                message: format!("不是 http(s) 網址：{u}"),
                            });
                        }
                    }
                    if image_urls.len() > MAX_IMAGES {
                        out.errors.push(RowError {
                            row: row_no,
                            column: Some(Column::ImageUrls.label()),
                            message: format!("圖片最多 {MAX_IMAGES} 張"),
                        });
                    }
                    out.products.push(ImportProduct {
                        external_ref: external_ref.clone(),
                        first_row: row_no,
                        name,
                        description: opt(row.get(Column::Description)),
                        category: opt(row.get(Column::Category)),
                        option1_name: opt(row.get(Column::Option1Name)),
                        option2_name: opt(row.get(Column::Option2Name)),
                        image_urls: image_urls.into_iter().map(|(_, u)| u).collect(),
                        variants: Vec::new(),
                    });
                    let i = out.products.len() - 1;
                    index.insert(external_ref.clone(), i);
                    Some(i)
                }
            }
        } else if !blank(row.get(Column::Name)) {
            out.errors.push(RowError {
                row: row_no,
                column: Some(Column::ExternalRef.label()),
                message: "這一列有商品名稱但沒有商品編號".into(),
            });
            out.row_count += 1;
            continue;
        } else {
            let has_variant_data = !blank(row.get(Column::Option1Value))
                || !blank(row.get(Column::Option2Value))
                || !blank(row.get(Column::Price));
            if !has_variant_data {
                continue;
            }
            match last {
                Some(i) => Some(i),
                None => {
                    out.errors.push(RowError {
                        row: row_no,
                        column: Some(Column::ExternalRef.label()),
                        message: "這一列沒有商品編號，也接不到上一個商品".into(),
                    });
                    out.row_count += 1;
                    None
                }
            }
        };
        let Some(i) = target else { continue };
        out.row_count += 1;
        last = Some(i);

        // 規格
        let price = match parse_amount(row.get(Column::Price)) {
            Some(p) if p <= MAX_PRICE as i64 => p as i32,
            Some(_) => {
                out.errors.push(RowError {
                    row: row_no,
                    column: Some(Column::Price.label()),
                    message: "價格最多 9,999,999".into(),
                });
                0
            }
            None => {
                let msg = if blank(row.get(Column::Price)) {
                    "價格必填"
                } else {
                    "價格要是 0 以上的整數"
                };
                out.errors.push(RowError {
                    row: row_no,
                    column: Some(Column::Price.label()),
                    message: msg.into(),
                });
                0
            }
        };
        let stock = if blank(row.get(Column::Stock)) {
            None
        } else {
            match parse_amount(row.get(Column::Stock)) {
                Some(s) if s <= i32::MAX as i64 => Some(s as i32),
                _ => {
                    out.errors.push(RowError {
                        row: row_no,
                        column: Some(Column::Stock.label()),
                        message: "庫存要是 0 以上的整數".into(),
                    });
                    None
                }
            }
        };
        let sku = opt(row.get(Column::Sku));
        if sku.as_ref().is_some_and(|s| s.chars().count() > 60) {
            out.errors.push(RowError {
                row: row_no,
                column: Some(Column::Sku.label()),
                message: "SKU 最多 60 字".into(),
            });
        }
        let variant = ImportVariant {
            row: row_no,
            option1_value: opt(row.get(Column::Option1Value)),
            option2_value: opt(row.get(Column::Option2Value)),
            sku,
            price,
            stock,
        };
        let product = &mut out.products[i];
        if product.variants.iter().any(|v| {
            v.option1_value == variant.option1_value && v.option2_value == variant.option2_value
        }) {
            out.errors.push(RowError {
                row: row_no,
                column: Some(Column::Option1Value.label()),
                message: "規格組合重複".into(),
            });
        }
        product.variants.push(variant);
    }

    // 商品層級的一致性檢查
    for p in &out.products {
        if p.option1_name.is_none() && p.variants.len() > 1 {
            out.errors.push(RowError {
                row: p.variants[1].row,
                column: Some(Column::Option1Name.label()),
                message: "沒有規格名稱時只能有一列".into(),
            });
        }
        if p.option2_name.is_some() && p.option1_name.is_none() {
            out.errors.push(RowError {
                row: p.first_row,
                column: Some(Column::Option2Name.label()),
                message: "要先有規格名稱1 才能有規格名稱2".into(),
            });
        }
        if p.option1_name.is_some() && p.variants.iter().any(|v| v.option1_value.is_none()) {
            let r = p
                .variants
                .iter()
                .find(|v| v.option1_value.is_none())
                .map(|v| v.row)
                .unwrap_or(p.first_row);
            out.errors.push(RowError {
                row: r,
                column: Some(Column::Option1Value.label()),
                message: "有規格名稱時每一列都要填規格選項1".into(),
            });
        }
        if p.option1_name.is_none() && p.variants.iter().any(|v| v.option1_value.is_some()) {
            let r = p
                .variants
                .iter()
                .find(|v| v.option1_value.is_some())
                .map(|v| v.row)
                .unwrap_or(p.first_row);
            out.errors.push(RowError {
                row: r,
                column: Some(Column::Option1Value.label()),
                message: "有規格選項1 就要填規格名稱1".into(),
            });
        }
        if p.option2_name.is_none() && p.variants.iter().any(|v| v.option2_value.is_some()) {
            let r = p
                .variants
                .iter()
                .find(|v| v.option2_value.is_some())
                .map(|v| v.row)
                .unwrap_or(p.first_row);
            out.errors.push(RowError {
                row: r,
                column: Some(Column::Option2Value.label()),
                message: "有規格選項2 就要填規格名稱2".into(),
            });
        }
        if p.variants.len() > MAX_VARIANTS {
            out.errors.push(RowError {
                row: p.first_row,
                column: None,
                message: format!("規格最多 {MAX_VARIANTS} 個"),
            });
        }
    }
    out.errors.sort_by_key(|e| (e.row, e.column.clone()));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(rows: &[&[&str]]) -> Vec<Vec<String>> {
        rows.iter()
            .map(|r| r.iter().map(|s| s.to_string()).collect())
            .collect()
    }

    const HEADER: &[&str] = &[
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

    #[test]
    fn groups_rows_by_external_ref_and_skips_instruction_rows() {
        let g = grid(&[
            &["請依範本填寫，第一列是說明"],
            &[],
            HEADER,
            &[
                "A1",
                "狗糧 5kg",
                "很好吃\n第二行",
                "狗糧",
                "口味",
                "雞肉",
                "",
                "",
                "1,200",
                "10",
                "DOG-C",
                "https://img.example/a.jpg, https://img.example/b.jpg",
            ],
            &["", "", "", "", "", "牛肉", "", "", "1300", "", "DOG-B", ""],
            &["A1", "", "", "", "", "魚肉", "", "", "1400", "3", "", ""],
            &["", "", "", "", "", "", "", "", "", "", "", ""],
            &["B2", "玩具球", "", "", "", "", "", "", "99", "0", "", ""],
        ]);
        let p = parse_grid("工作表1", &g).unwrap();
        assert_eq!(p.header_row, 3);
        assert_eq!(p.row_count, 4);
        assert!(p.errors.is_empty(), "{:?}", p.errors);
        assert_eq!(p.products.len(), 2);
        let a = &p.products[0];
        assert_eq!(a.external_ref, "A1");
        assert_eq!(a.first_row, 4);
        assert_eq!(a.name, "狗糧 5kg");
        assert_eq!(a.description.as_deref(), Some("很好吃\n第二行"));
        assert_eq!(a.category.as_deref(), Some("狗糧"));
        assert_eq!(a.option1_name.as_deref(), Some("口味"));
        assert_eq!(a.option2_name, None);
        assert_eq!(
            a.image_urls,
            vec!["https://img.example/a.jpg", "https://img.example/b.jpg"]
        );
        assert_eq!(a.variants.len(), 3);
        assert_eq!(
            a.variants[0],
            ImportVariant {
                row: 4,
                option1_value: Some("雞肉".into()),
                option2_value: None,
                sku: Some("DOG-C".into()),
                price: 1200,
                stock: Some(10)
            }
        );
        assert_eq!(a.variants[1].price, 1300);
        assert_eq!(a.variants[1].stock, None, "庫存空白 = None");
        assert_eq!(a.variants[2].row, 6);
        let b = &p.products[1];
        assert_eq!(b.name, "玩具球");
        assert_eq!(b.option1_name, None);
        assert_eq!(b.variants.len(), 1);
        assert_eq!(p.variant_count(), 4);
        assert!(p.unmatched_columns.is_empty());
    }

    #[test]
    fn reports_row_errors_with_column_names() {
        // 孤兒列要放在任何商品之前，否則會被接到前一個商品（那是刻意的行為，見第一個測試）
        let g = grid(&[
            HEADER,
            &["", "", "", "", "", "紅", "", "", "50", "", "", ""],
            &["A1", "", "", "", "", "", "", "", "100", "", "", ""],
            &[
                "C3",
                "壞價格",
                "",
                "",
                "顏色",
                "紅",
                "",
                "",
                "abc",
                "-1",
                "",
                "not-a-url",
            ],
            &["C3", "", "", "", "", "紅", "", "", "60", "", "", ""],
            &[
                "D4",
                "沒規格名卻多列",
                "",
                "",
                "",
                "紅",
                "",
                "",
                "60",
                "",
                "",
                "",
            ],
            &["D4", "", "", "", "", "藍", "", "", "60", "", "", ""],
            &["E5", "太貴", "", "", "", "", "", "", "10000000", "", "", ""],
            &[
                "",
                "漏編號的新商品",
                "",
                "",
                "",
                "",
                "",
                "",
                "70",
                "",
                "",
                "",
            ],
        ]);
        let p = parse_grid("s", &g).unwrap();
        let msgs: Vec<(u32, Option<String>, String)> = p
            .errors
            .iter()
            .map(|e| (e.row, e.column.clone(), e.message.clone()))
            .collect();
        assert!(
            msgs.contains(&(
                2,
                Some("商品編號".into()),
                "這一列沒有商品編號，也接不到上一個商品".into()
            )),
            "{msgs:?}"
        );
        assert!(
            msgs.contains(&(3, Some("商品名稱".into()), "商品名稱必填".into())),
            "{msgs:?}"
        );
        assert!(
            msgs.contains(&(4, Some("價格".into()), "價格要是 0 以上的整數".into())),
            "{msgs:?}"
        );
        assert!(
            msgs.contains(&(4, Some("庫存".into()), "庫存要是 0 以上的整數".into())),
            "{msgs:?}"
        );
        assert!(
            msgs.contains(&(
                4,
                Some("圖片網址".into()),
                "不是 http(s) 網址：not-a-url".into()
            )),
            "{msgs:?}"
        );
        assert!(
            msgs.contains(&(5, Some("規格選項1".into()), "規格組合重複".into())),
            "{msgs:?}"
        );
        assert!(
            msgs.contains(&(
                6,
                Some("規格選項1".into()),
                "有規格選項1 就要填規格名稱1".into()
            )),
            "{msgs:?}"
        );
        assert!(
            msgs.contains(&(
                7,
                Some("規格名稱1".into()),
                "沒有規格名稱時只能有一列".into()
            )),
            "{msgs:?}"
        );
        assert!(
            msgs.contains(&(8, Some("價格".into()), "價格最多 9,999,999".into())),
            "{msgs:?}"
        );
        assert!(
            msgs.contains(&(
                9,
                Some("商品編號".into()),
                "這一列有商品名稱但沒有商品編號".into()
            )),
            "{msgs:?}"
        );
        // 有錯的商品仍會出現在 products（預覽要列出來），但錯誤數 > 0 就不能 commit；孤兒列不是商品
        assert_eq!(p.products.len(), 4);
        assert_eq!(
            p.products.last().unwrap().variants.len(),
            1,
            "第 9 列沒被掛到 E5"
        );
    }

    #[test]
    fn missing_required_columns_and_no_header() {
        let g = grid(&[&["商品名稱", "價格", "庫存"], &["x", "1", "2"]]);
        match parse_grid("s", &g) {
            Err(ImportError::MissingColumns(cols)) => assert_eq!(cols, "商品編號"),
            other => panic!("{other:?}"),
        }
        let g = grid(&[&["a", "b"], &["c", "d"]]);
        assert!(matches!(parse_grid("s", &g), Err(ImportError::NoHeader)));
    }

    #[test]
    fn shopee_style_image_columns_and_fullwidth_headers() {
        let g = grid(&[
            &[
                "商品ID",
                "商品名稱",
                "規格名稱 1",
                "規格選項 1",
                "售價",
                "數量",
                "商品圖片 1",
                "商品圖片 2",
                "品牌",
            ],
            &[
                "S1",
                "蝦皮商品",
                "尺寸",
                "S",
                "３５０",
                "5",
                "https://a/1.jpg",
                "https://a/2.jpg",
                "無",
            ],
            &["S1", "", "", "M", "350", "5", "", "", ""],
            &["S2", "壞圖", "", "", "350", "1", "", "not-a-url", ""],
        ]);
        let p = parse_grid("s", &g).unwrap();
        assert_eq!(
            p.errors,
            vec![RowError {
                row: 4,
                column: Some("商品圖片2".into()),
                message: "不是 http(s) 網址：not-a-url".into(),
            }]
        );
        assert_eq!(p.unmatched_columns, vec!["品牌".to_string()]);
        assert_eq!(p.products.len(), 2);
        let s = &p.products[0];
        assert_eq!(s.image_urls, vec!["https://a/1.jpg", "https://a/2.jpg"]);
        assert_eq!(s.variants[0].price, 350, "全形數字也要能讀");
        assert_eq!(s.variants.len(), 2);
    }

    #[test]
    fn too_many_rows_and_too_many_images() {
        let mut rows: Vec<Vec<String>> = vec![HEADER.iter().map(|s| s.to_string()).collect()];
        for i in 0..(crate::import::MAX_ROWS + 1) {
            rows.push(vec![
                format!("R{i}"),
                "x".into(),
                "".into(),
                "".into(),
                "".into(),
                "".into(),
                "".into(),
                "".into(),
                "1".into(),
                "".into(),
                "".into(),
                "".into(),
            ]);
        }
        assert!(matches!(
            parse_grid("s", &rows),
            Err(ImportError::TooManyRows(_))
        ));
        let urls = (0..10)
            .map(|i| format!("https://a/{i}.jpg"))
            .collect::<Vec<_>>()
            .join(",");
        let g = grid(&[
            HEADER,
            &["A", "x", "", "", "", "", "", "", "1", "", "", &urls],
        ]);
        let p = parse_grid("s", &g).unwrap();
        assert_eq!(p.errors[0].message, "圖片最多 9 張");
    }

    #[test]
    fn parse_amount_handles_decimals_currency_and_fullwidth() {
        assert_eq!(parse_amount("1,200"), Some(1200));
        assert_eq!(parse_amount("NT$1,200"), Some(1200));
        assert_eq!(parse_amount("1200元"), Some(1200));
        assert_eq!(parse_amount("３５０"), Some(350));
        assert_eq!(parse_amount("99.0"), Some(99));
        assert_eq!(parse_amount("1,200.00"), Some(1200));
        assert_eq!(parse_amount("1.5"), None);
        assert_eq!(parse_amount("-1"), None);
        assert_eq!(parse_amount("abc"), None);
        assert_eq!(parse_amount(""), None);
        assert_eq!(parse_amount("12a3"), None);
        assert_eq!(parse_amount("ＮＴ＄1200"), Some(1200));
        assert_eq!(parse_amount("1t2"), None);
    }
}
