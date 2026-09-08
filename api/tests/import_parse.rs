use dog_shop_api::import::{ImportError, parse_xlsx};
use rust_xlsxwriter::Workbook;

fn sheet(rows: &[&[&str]]) -> Vec<u8> {
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    for (r, row) in rows.iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            // 純數字寫成數字格，測 Float → 整數字串
            if let Ok(n) = cell.parse::<f64>() {
                ws.write_number(r as u32, c as u16, n).unwrap();
            } else {
                ws.write_string(r as u32, c as u16, *cell).unwrap();
            }
        }
    }
    wb.save_to_buffer().unwrap()
}

#[test]
fn parses_a_real_xlsx_with_numeric_cells() {
    let bytes = sheet(&[
        &["商品編號", "商品名稱", "價格", "庫存", "圖片網址"],
        &["A1", "狗糧", "1200", "10", "https://img.example/a.jpg"],
        &["B2", "玩具", "99.0", "", ""],
    ]);
    let p = parse_xlsx(&bytes).unwrap();
    assert_eq!(p.products.len(), 2);
    assert_eq!(p.products[0].variants[0].price, 1200);
    assert_eq!(p.products[0].variants[0].stock, Some(10));
    assert_eq!(p.products[1].variants[0].price, 99);
    assert_eq!(p.products[1].variants[0].stock, None);
    assert!(p.errors.is_empty(), "{:?}", p.errors);
}

#[test]
fn garbage_is_unreadable() {
    assert!(matches!(
        parse_xlsx(b"not an xlsx"),
        Err(ImportError::Unreadable)
    ));
    assert!(matches!(parse_xlsx(b""), Err(ImportError::Unreadable)));
}

#[test]
fn formatted_empty_tail_rows_do_not_count() {
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    let rows: &[&[&str]] = &[
        &["商品編號", "商品名稱", "價格", "庫存", "圖片網址"],
        &["A1", "狗糧", "1200", "10", ""],
        &["B2", "玩具", "99", "", ""],
    ];
    for (r, row) in rows.iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            if let Ok(n) = cell.parse::<f64>() {
                ws.write_number(r as u32, c as u16, n).unwrap();
            } else {
                ws.write_string(r as u32, c as u16, *cell).unwrap();
            }
        }
    }
    ws.write_string(6000, 0, " ").unwrap();
    let bytes = wb.save_to_buffer().unwrap();
    let p = parse_xlsx(&bytes).unwrap();
    assert_eq!(p.row_count, 2);
    assert_eq!(p.products.len(), 2);
}

/// 宣告的工作表範圍超過格數預算就要直接拒絕，不能讓 calamine 去配置整個稠密矩陣。
/// 這裡用「略超預算」的形狀（25_001 × 101 ≈ 2.5M 格）：舊碼只配約 80 MB，回 TooManyRows，
/// 所以斷言乾淨地紅。真正極端的遠格檔（XFD1048576）只在新碼上測，見下一個測試。
#[test]
fn a_sheet_range_over_the_cell_budget_is_rejected() {
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    for (c, h) in ["商品編號", "商品名稱", "價格", "庫存", "圖片網址"]
        .iter()
        .enumerate()
    {
        ws.write_string(0, c as u16, *h).unwrap();
    }
    ws.write_string(1, 0, "A1").unwrap();
    ws.write_string(1, 1, "狗糧").unwrap();
    ws.write_number(1, 2, 1200.0).unwrap();
    // 一格遠在右下角：25_001 列 × 101 欄 ≈ 2.5M 格，超過 200 萬格的上限
    ws.write_string(25_000, 100, " ").unwrap();
    let bytes = wb.save_to_buffer().unwrap();
    let r = parse_xlsx(&bytes);
    assert!(matches!(&r, Err(ImportError::TooBig)), "{r:?}");
}

/// 極端的遠格檔：只有標題列與 `XFD1048576` 一格，宣告矩形是 170 億格。舊碼會拿它去配置
/// 整個稠密矩陣（行程直接被作業系統收掉），新碼在任何配置之前就靠宣告矩形擋下來，所以這個
/// 測試同時釘住「會拒絕」與「拒絕得夠快」。
#[test]
fn a_far_away_cell_is_rejected_without_allocating() {
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    for (c, h) in ["商品編號", "商品名稱", "價格", "庫存", "圖片網址"]
        .iter()
        .enumerate()
    {
        ws.write_string(0, c as u16, *h).unwrap();
    }
    ws.write_string(1, 0, "A1").unwrap();
    ws.write_string(1, 1, "狗糧").unwrap();
    ws.write_number(1, 2, 1200.0).unwrap();
    // xlsx 的最後一格（XFD1048576，0-based）
    ws.write_string(1_048_575, 16_383, " ").unwrap();
    let bytes = wb.save_to_buffer().unwrap();

    let started = std::time::Instant::now();
    let r = parse_xlsx(&bytes);
    let elapsed = started.elapsed();
    assert!(matches!(&r, Err(ImportError::TooBig)), "{r:?}");
    assert!(elapsed < std::time::Duration::from_secs(5), "{elapsed:?}");
}
