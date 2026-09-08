//! job 種類對應的執行函式：send_email（本任務）、issue_invoice（Task 10）
use anyhow::Context;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    auth::tokens::sha256_hex,
    domain::{
        invoices,
        jobs::{self, KIND_ISSUE_INVOICE, KIND_SEND_EMAIL},
        orders::{
            self, OrderDetail, PAYMENT_ATM, PAYMENT_CREDIT, PAYMENT_CVS_CODE, SHIPPING_CVS,
            STATUS_COMPLETED, STATUS_PAID, STATUS_SHIPPED,
        },
        password_resets, payments,
        settings::{self, ShopSettings},
        users,
    },
    ecpay::{invoice, time},
    jobs::worker::Job,
    mail::{
        Email,
        templates::{self, MailItem},
    },
    state::AppState,
};

/// 同一使用者 10 分鐘內只寄一封重設信（與規格不同之處 25）
pub const RESET_THROTTLE_MINUTES: i32 = 10;

pub async fn run(state: &AppState, job: &Job) -> anyhow::Result<()> {
    match job.kind.as_str() {
        KIND_SEND_EMAIL => send_email(state, &job.payload).await,
        KIND_ISSUE_INVOICE => issue_invoice(state, job).await,
        other => anyhow::bail!("未知的 job kind：{other}"),
    }
}

fn payload_uuid(payload: &Value, key: &str) -> anyhow::Result<Uuid> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .and_then(|s| Uuid::parse_str(s).ok())
        .with_context(|| format!("payload 缺少 {key}"))
}

/// 信尾的聯絡方式：商店名稱（＋聯絡 Email）
fn contact_line(shop: &ShopSettings) -> String {
    if shop.contact_email.is_empty() {
        shop.name.clone()
    } else {
        format!("{}（{}）", shop.name, shop.contact_email)
    }
}

/// 訂單頁網址；訪客訂單帶 guest_token（規格 §11：只出現在訂單頁網址與 Email）
fn order_url(base: &str, detail: &OrderDetail) -> String {
    match detail.order.user_id {
        Some(_) => format!("{base}/orders/{}", detail.order.id),
        None => format!(
            "{base}/orders/{}?t={}",
            detail.order.id, detail.order.guest_token
        ),
    }
}

fn payment_label(method: &str) -> &'static str {
    match method {
        PAYMENT_CREDIT => "信用卡",
        PAYMENT_ATM => "ATM 轉帳",
        PAYMENT_CVS_CODE => "超商代碼繳費",
        _ => "－",
    }
}

fn mail_items(detail: &OrderDetail) -> Vec<MailItem> {
    detail
        .items
        .iter()
        .map(|i| MailItem {
            name: i.product_name.clone(),
            label: i.variant_label.clone(),
            quantity: i.quantity,
            line_total: templates::twd(i.line_total),
        })
        .collect()
}

fn shipping_desc(detail: &OrderDetail) -> String {
    let Some(s) = detail.shipment.as_ref() else {
        return String::new();
    };
    let or_empty = |v: &Option<String>| v.clone().unwrap_or_default();
    if s.method == SHIPPING_CVS {
        format!(
            "超商取貨：{} {}",
            or_empty(&s.cvs_store_name),
            or_empty(&s.cvs_store_address)
        )
    } else {
        format!(
            "宅配：{}{}{}{}",
            or_empty(&s.home_postal_code),
            or_empty(&s.home_city),
            or_empty(&s.home_district),
            or_empty(&s.home_street)
        )
    }
}

async fn send_email(state: &AppState, payload: &Value) -> anyhow::Result<()> {
    let template = payload
        .get("template")
        .and_then(Value::as_str)
        .context("payload 缺少 template")?
        .to_string();
    let shop = settings::get_all(&state.db).await?.shop;
    let base = state.config.public_base_url.as_str();
    let contact = contact_line(&shop);

    if template == "password_reset" {
        return send_password_reset(
            state,
            payload_uuid(payload, "user_id")?,
            &shop.name,
            base,
            &contact,
        )
        .await;
    }

    let order_id = payload_uuid(payload, "order_id")?;
    let Some(detail) = orders::get_detail(&state.db, order_id).await? else {
        tracing::warn!(%order_id, template = %template, "訂單不存在，略過寄信");
        return Ok(());
    };
    let order_url = order_url(base, &detail);
    let order_no = detail.order.order_no.clone();
    let total = templates::twd(detail.order.total);
    let (subject, text, html) = match template.as_str() {
        "order_created" => templates::OrderCreatedMail {
            shop_name: shop.name.clone(),
            order_no,
            order_url,
            items: mail_items(&detail),
            subtotal: templates::twd(detail.order.subtotal),
            shipping_fee: if detail.order.shipping_fee == 0 {
                "免運".to_string()
            } else {
                templates::twd(detail.order.shipping_fee)
            },
            total,
            payment_label: payment_label(
                detail
                    .payment
                    .as_ref()
                    .map(|p| p.method.as_str())
                    .unwrap_or(""),
            )
            .to_string(),
            shipping_desc: shipping_desc(&detail),
            contact,
        }
        .render()?,
        "payment_instructions" => {
            let payment = payments::get(&state.db, payload_uuid(payload, "payment_id")?)
                .await?
                .context("payment_instructions 找不到 payment")?;
            templates::PaymentInstructionsMail {
                shop_name: shop.name.clone(),
                order_no,
                order_url,
                total: templates::twd(payment.amount),
                method_label: payment_label(&payment.method).to_string(),
                atm_bank_code: payment.atm_bank_code.clone(),
                atm_vaccount: payment.atm_vaccount.clone(),
                cvs_payment_no: payment.cvs_payment_no.clone(),
                expire_at: payment
                    .expire_at
                    .map(time::format_datetime)
                    .unwrap_or_default(),
                contact,
            }
            .render()?
        }
        "payment_received" => templates::PaymentReceivedMail {
            shop_name: shop.name.clone(),
            order_no,
            order_url,
            total,
            paid_at: detail
                .order
                .paid_at
                .map(time::format_datetime)
                .unwrap_or_default(),
            contact,
        }
        .render()?,
        "order_shipped" => {
            let s = detail.shipment.as_ref();
            let or_empty = |v: Option<&String>| v.cloned().unwrap_or_default();
            templates::OrderShippedMail {
                shop_name: shop.name.clone(),
                order_no,
                order_url,
                cvs: detail.order.shipping_method == SHIPPING_CVS,
                store_name: or_empty(s.and_then(|s| s.cvs_store_name.as_ref())),
                store_address: or_empty(s.and_then(|s| s.cvs_store_address.as_ref())),
                carrier: or_empty(s.and_then(|s| s.carrier.as_ref())),
                tracking_no: or_empty(s.and_then(|s| s.tracking_no.as_ref())),
                contact,
            }
            .render()?
        }
        "invoice_issued" => {
            let Some(inv) = detail.invoice.as_ref().filter(|i| i.invoice_no.is_some()) else {
                anyhow::bail!("invoice_issued 但發票還沒開立");
            };
            templates::InvoiceIssuedMail {
                shop_name: shop.name.clone(),
                order_no,
                order_url,
                invoice_no: inv.invoice_no.clone().unwrap_or_default(),
                invoice_date: inv
                    .invoice_date
                    .map(time::format_datetime)
                    .unwrap_or_default(),
                random_number: inv.random_number.clone().unwrap_or_default(),
                total,
                contact,
            }
            .render()?
        }
        other => anyhow::bail!("未知的 email 模板：{other}"),
    };
    state
        .mailer
        .send(Email {
            to: detail.order.email.clone(),
            subject,
            text,
            html,
        })
        .await
}

async fn send_password_reset(
    state: &AppState,
    user_id: Uuid,
    shop_name: &str,
    base: &str,
    contact: &str,
) -> anyhow::Result<()> {
    let Some(user) = users::find_by_id(&state.db, user_id).await? else {
        tracing::warn!(%user_id, "使用者不存在，略過重設信");
        return Ok(());
    };
    let recent: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM password_resets
                        WHERE user_id = $1 AND created_at > now() - make_interval(mins => $2))",
    )
    .bind(user_id)
    .bind(RESET_THROTTLE_MINUTES)
    .fetch_one(&state.db)
    .await?;
    if recent {
        tracing::info!(%user_id, "{RESET_THROTTLE_MINUTES} 分鐘內已寄過重設信，略過");
        return Ok(());
    }
    // token 在寄信當下才產生（與規格不同之處 11）；DB 只有 SHA-256
    let raw = password_resets::create(&state.db, user_id).await?;
    let (subject, text, html) = templates::PasswordResetMail {
        shop_name: shop_name.to_string(),
        user_name: user.name.clone(),
        reset_url: format!("{base}/reset/{raw}"),
        ttl_minutes: password_resets::RESET_TTL_MINUTES,
        contact: contact.to_string(),
    }
    .render()?;
    if let Err(e) = state
        .mailer
        .send(Email {
            to: user.email.clone(),
            subject,
            text,
            html,
        })
        .await
    {
        // 寄失敗就把剛建的 token 作廢，重試時才能再產生（不然會被 10 分鐘節流擋住）
        sqlx::query("DELETE FROM password_resets WHERE token_hash = $1")
            .bind(sha256_hex(&raw))
            .execute(&state.db)
            .await?;
        return Err(e);
    }
    Ok(())
}

/// 開立電子發票（規格 §8.4、與規格不同之處 29）。已開立就略過；訂單不是已付款狀態也略過（done）。
/// 綠界回錯或連不上 → 記在 invoices.error 並回 Err 讓 worker 重試；最後一次失敗把 invoices 標 failed
async fn issue_invoice(state: &AppState, job: &Job) -> anyhow::Result<()> {
    let order_id = payload_uuid(&job.payload, "order_id")?;
    let Some(detail) = orders::get_detail(&state.db, order_id).await? else {
        tracing::warn!(%order_id, "訂單不存在，略過開發票");
        return Ok(());
    };
    let Some(invoice) = detail.invoice.as_ref() else {
        anyhow::bail!("訂單 {order_id} 沒有 invoices 列");
    };
    if invoice.status == invoices::STATUS_ISSUED {
        tracing::info!(%order_id, "發票已開立，略過");
        return Ok(());
    }
    if !matches!(
        detail.order.status.as_str(),
        STATUS_PAID | STATUS_SHIPPED | STATUS_COMPLETED
    ) {
        tracing::warn!(%order_id, status = %detail.order.status, "訂單不是已付款狀態，不開發票");
        return Ok(());
    }

    let request = invoice::build_issue_request(state.invoices.merchant_id(), &detail);
    invoices::record_request(&state.db, order_id, &serde_json::to_value(&request)?).await?;
    match state.invoices.issue(&request).await {
        Ok(resp) if resp.is_ok() => {
            let invoice_date = time::parse_taipei(&resp.invoice_date, "%Y-%m-%d %H:%M:%S");
            invoices::mark_issued(
                &state.db,
                order_id,
                &resp.invoice_no,
                invoice_date,
                &resp.random_number,
                &resp.raw,
            )
            .await?;
            let mut tx = state.db.begin().await?;
            jobs::enqueue(
                &mut tx,
                KIND_SEND_EMAIL,
                json!({ "template": "invoice_issued", "order_id": order_id }),
                Some(&format!("email:invoice_issued:{order_id}")),
            )
            .await?;
            tx.commit().await?;
            tracing::info!(%order_id, invoice_no = %resp.invoice_no, "發票開立成功");
            Ok(())
        }
        Ok(resp) => {
            let msg = format!("綠界 RtnCode {}：{}", resp.rtn_code, resp.rtn_msg);
            invoices::record_failure(
                &state.db,
                order_id,
                Some(&resp.raw),
                &msg,
                job.is_last_attempt(),
            )
            .await?;
            anyhow::bail!("{msg}")
        }
        Err(e) => {
            let msg = format!("{e:#}");
            invoices::record_failure(&state.db, order_id, None, &msg, job.is_last_attempt())
                .await?;
            Err(e)
        }
    }
}
