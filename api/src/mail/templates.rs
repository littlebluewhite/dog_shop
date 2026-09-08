//! askama 模板（`api/templates/mail/*.txt|html`）。每種信一個資料 struct、一對 Txt／Html 包裝（欄位 `m`），
//! `render()` 回 (主旨, 純文字, HTML)。HTML 都套 `layout.html`，需要 `m.shop_name`、`m.contact`
use askama::Template;

pub struct MailItem {
    pub name: String,
    pub label: String,
    pub quantity: i32,
    pub line_total: String,
}

/// `NT$ 1,234`
pub fn twd(n: i32) -> String {
    let digits = n.abs().to_string();
    let mut out = String::with_capacity(digits.len() + 4);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    format!("{}NT$ {out}", if n < 0 { "-" } else { "" })
}

fn pair<T: Template, H: Template>(
    subject: String,
    txt: T,
    html: H,
) -> askama::Result<(String, String, String)> {
    Ok((subject, txt.render()?, html.render()?))
}

// ───── order_created ─────

pub struct OrderCreatedMail {
    pub shop_name: String,
    pub order_no: String,
    pub order_url: String,
    pub items: Vec<MailItem>,
    pub subtotal: String,
    /// 已格式化；免運時是「免運」
    pub shipping_fee: String,
    pub total: String,
    pub payment_label: String,
    /// 「超商取貨：門市 地址」或「宅配：郵遞區號縣市鄉鎮地址」
    pub shipping_desc: String,
    pub contact: String,
}

#[derive(Template)]
#[template(path = "mail/order_created.txt")]
struct OrderCreatedTxt<'a> {
    m: &'a OrderCreatedMail,
}

#[derive(Template)]
#[template(path = "mail/order_created.html")]
struct OrderCreatedHtml<'a> {
    m: &'a OrderCreatedMail,
}

impl OrderCreatedMail {
    pub fn render(&self) -> askama::Result<(String, String, String)> {
        pair(
            format!("【{}】訂單 {} 已成立", self.shop_name, self.order_no),
            OrderCreatedTxt { m: self },
            OrderCreatedHtml { m: self },
        )
    }
}

// ───── payment_instructions ─────

pub struct PaymentInstructionsMail {
    pub shop_name: String,
    pub order_no: String,
    pub order_url: String,
    pub total: String,
    pub method_label: String,
    pub atm_bank_code: Option<String>,
    pub atm_vaccount: Option<String>,
    pub cvs_payment_no: Option<String>,
    /// 台北時間 `yyyy/MM/dd HH:mm:ss`；沒有就空字串
    pub expire_at: String,
    pub contact: String,
}

#[derive(Template)]
#[template(path = "mail/payment_instructions.txt")]
struct PaymentInstructionsTxt<'a> {
    m: &'a PaymentInstructionsMail,
}

#[derive(Template)]
#[template(path = "mail/payment_instructions.html")]
struct PaymentInstructionsHtml<'a> {
    m: &'a PaymentInstructionsMail,
}

impl PaymentInstructionsMail {
    pub fn render(&self) -> askama::Result<(String, String, String)> {
        pair(
            format!("【{}】訂單 {} 繳費資訊", self.shop_name, self.order_no),
            PaymentInstructionsTxt { m: self },
            PaymentInstructionsHtml { m: self },
        )
    }
}

// ───── payment_received ─────

pub struct PaymentReceivedMail {
    pub shop_name: String,
    pub order_no: String,
    pub order_url: String,
    pub total: String,
    pub paid_at: String,
    pub contact: String,
}

#[derive(Template)]
#[template(path = "mail/payment_received.txt")]
struct PaymentReceivedTxt<'a> {
    m: &'a PaymentReceivedMail,
}

#[derive(Template)]
#[template(path = "mail/payment_received.html")]
struct PaymentReceivedHtml<'a> {
    m: &'a PaymentReceivedMail,
}

impl PaymentReceivedMail {
    pub fn render(&self) -> askama::Result<(String, String, String)> {
        pair(
            format!("【{}】訂單 {} 已收到款項", self.shop_name, self.order_no),
            PaymentReceivedTxt { m: self },
            PaymentReceivedHtml { m: self },
        )
    }
}

// ───── order_shipped ─────

pub struct OrderShippedMail {
    pub shop_name: String,
    pub order_no: String,
    pub order_url: String,
    /// true = 超商取貨（顯示門市）；false = 宅配（顯示貨運公司與單號）
    pub cvs: bool,
    pub store_name: String,
    pub store_address: String,
    pub carrier: String,
    pub tracking_no: String,
    pub contact: String,
}

#[derive(Template)]
#[template(path = "mail/order_shipped.txt")]
struct OrderShippedTxt<'a> {
    m: &'a OrderShippedMail,
}

#[derive(Template)]
#[template(path = "mail/order_shipped.html")]
struct OrderShippedHtml<'a> {
    m: &'a OrderShippedMail,
}

impl OrderShippedMail {
    pub fn render(&self) -> askama::Result<(String, String, String)> {
        pair(
            format!("【{}】訂單 {} 已出貨", self.shop_name, self.order_no),
            OrderShippedTxt { m: self },
            OrderShippedHtml { m: self },
        )
    }
}

// ───── invoice_issued ─────

pub struct InvoiceIssuedMail {
    pub shop_name: String,
    pub order_no: String,
    pub order_url: String,
    pub invoice_no: String,
    pub invoice_date: String,
    pub random_number: String,
    pub total: String,
    pub contact: String,
}

#[derive(Template)]
#[template(path = "mail/invoice_issued.txt")]
struct InvoiceIssuedTxt<'a> {
    m: &'a InvoiceIssuedMail,
}

#[derive(Template)]
#[template(path = "mail/invoice_issued.html")]
struct InvoiceIssuedHtml<'a> {
    m: &'a InvoiceIssuedMail,
}

impl InvoiceIssuedMail {
    pub fn render(&self) -> askama::Result<(String, String, String)> {
        pair(
            format!(
                "【{}】訂單 {} 電子發票已開立",
                self.shop_name, self.order_no
            ),
            InvoiceIssuedTxt { m: self },
            InvoiceIssuedHtml { m: self },
        )
    }
}

// ───── password_reset ─────

pub struct PasswordResetMail {
    pub shop_name: String,
    pub user_name: String,
    pub reset_url: String,
    pub ttl_minutes: i64,
    pub contact: String,
}

#[derive(Template)]
#[template(path = "mail/password_reset.txt")]
struct PasswordResetTxt<'a> {
    m: &'a PasswordResetMail,
}

#[derive(Template)]
#[template(path = "mail/password_reset.html")]
struct PasswordResetHtml<'a> {
    m: &'a PasswordResetMail,
}

impl PasswordResetMail {
    pub fn render(&self) -> askama::Result<(String, String, String)> {
        pair(
            format!("【{}】重設密碼", self.shop_name),
            PasswordResetTxt { m: self },
            PasswordResetHtml { m: self },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twd_groups_thousands() {
        assert_eq!(twd(0), "NT$ 0");
        assert_eq!(twd(700), "NT$ 700");
        assert_eq!(twd(1234), "NT$ 1,234");
        assert_eq!(twd(1_234_567), "NT$ 1,234,567");
        assert_eq!(twd(-50), "-NT$ 50");
    }

    #[test]
    fn password_reset_renders_link_in_both_parts() {
        let mail = PasswordResetMail {
            shop_name: "狗狗商店".to_string(),
            user_name: "小美".to_string(),
            reset_url: "https://shop.example.com/reset/abc".to_string(),
            ttl_minutes: 60,
            contact: "狗狗商店（hi@example.com）".to_string(),
        };
        let (subject, text, html) = mail.render().unwrap();
        assert_eq!(subject, "【狗狗商店】重設密碼");
        assert!(text.contains("小美") && text.contains("60 分鐘"));
        assert!(text.contains("https://shop.example.com/reset/abc"));
        assert!(html.contains("href=\"https://shop.example.com/reset/abc\""));
        assert!(html.contains("hi@example.com"));
    }

    #[test]
    fn html_escapes_user_text_but_txt_does_not() {
        let mail = PaymentReceivedMail {
            shop_name: "A&B <shop>".to_string(),
            order_no: "DS260906ABCD".to_string(),
            order_url: "https://shop.example.com/orders/x?t=y".to_string(),
            total: twd(700),
            paid_at: "2026/09/06 15:30:23".to_string(),
            contact: "A&B".to_string(),
        };
        let (_, text, html) = mail.render().unwrap();
        assert!(text.contains("A&B <shop>"));
        // askama 的內建 Html escaper 用數字字元參照，不是命名實體（askama-0.16.1 src/filters/escape.rs）
        assert!(html.contains("A&#38;B &#60;shop&#62;"));
        assert!(!html.contains("<shop>"));
    }
}
