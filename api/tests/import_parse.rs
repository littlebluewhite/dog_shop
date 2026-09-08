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
