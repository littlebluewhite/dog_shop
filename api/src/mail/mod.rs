//! Email 寄送（規格 §12）。Mailer 有三種：Smtp（正式）、Log（沒設 SMTP，與規格不同之處 22）、Capture（測試）
pub mod templates;

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::Context;
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{Mailbox, MultiPart},
    transport::smtp::authentication::Credentials,
};

use crate::config::Config;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Email {
    pub to: String,
    pub subject: String,
    pub text: String,
    pub html: String,
}

/// 連線與寄送的期限：SMTP 接了連線卻不回應時，不能讓 worker 永遠卡在這一筆
pub const SMTP_TIMEOUT: Duration = Duration::from_secs(30);

pub enum Mailer {
    Smtp {
        transport: AsyncSmtpTransport<Tokio1Executor>,
        from: Mailbox,
        send_timeout: Duration,
    },
    /// 只記 to／subject（info）；內文含重設連結與訪客訂單網址（規格 §11），
    /// 只有明確設定 `MAIL_LOG_BODY=1` 才會輸出（只在本機開發用）
    Log { log_body: bool },
    /// 測試用：全部收進 Vec
    Capture(Arc<Mutex<Vec<Email>>>),
}

impl Mailer {
    pub fn from_config(cfg: &Config) -> anyhow::Result<Self> {
        let Some(smtp) = &cfg.smtp else {
            tracing::warn!("SMTP 未設定（SMTP_HOST 空白）：Email 只會記 log，不會真的寄出");
            return Ok(Self::Log {
                log_body: cfg.mail_log_body,
            });
        };
        // 465 = 一開始就是 TLS；其他（587、25）= STARTTLS
        let mut builder = if smtp.port == 465 {
            AsyncSmtpTransport::<Tokio1Executor>::relay(&smtp.host)?
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&smtp.host)?
        };
        builder = builder.port(smtp.port).timeout(Some(SMTP_TIMEOUT));
        if let (Some(user), Some(pass)) = (&smtp.user, &smtp.pass) {
            builder = builder.credentials(Credentials::new(user.clone(), pass.clone()));
        }
        let from: Mailbox = smtp
            .from
            .parse()
            .with_context(|| format!("SMTP_FROM 不是合法的寄件人：{}", smtp.from))?;
        Ok(Self::Smtp {
            transport: builder.build(),
            from,
            send_timeout: SMTP_TIMEOUT,
        })
    }

    pub fn capture() -> (Self, Arc<Mutex<Vec<Email>>>) {
        let sink = Arc::new(Mutex::new(Vec::new()));
        (Self::Capture(sink.clone()), sink)
    }

    pub async fn send(&self, email: Email) -> anyhow::Result<()> {
        match self {
            Self::Smtp {
                transport,
                from,
                send_timeout,
            } => {
                let to: Mailbox = email
                    .to
                    .parse()
                    .with_context(|| format!("收件人格式錯誤：{}", email.to))?;
                let message = Message::builder()
                    .from(from.clone())
                    .to(to)
                    .subject(email.subject)
                    .multipart(MultiPart::alternative_plain_html(email.text, email.html))?;
                // SMTP 伺服器接了連線卻不回應時 send 會永遠等下去，worker 就卡在這一筆（codex P1-1）
                tokio::time::timeout(*send_timeout, transport.send(message))
                    .await
                    .map_err(|_| {
                        anyhow::anyhow!("SMTP 寄送逾時（{} 秒）", send_timeout.as_secs_f64())
                    })?
                    .context("SMTP 寄送失敗")?;
                Ok(())
            }
            Self::Log { log_body } => {
                tracing::info!(to = %email.to, subject = %email.subject, "Email（未設定 SMTP，只記 log）");
                if let Some(body) = body_log_line(&email, *log_body) {
                    tracing::info!(target: "mail_body", to = %email.to, "{body}");
                }
                Ok(())
            }
            Self::Capture(sink) => {
                sink.lock().expect("mail sink").push(email);
                Ok(())
            }
        }
    }
}

/// 要寫進 log 的信件內文。內文含密碼重設連結與訪客訂單網址的 token（規格 §11），
/// 所以由 `MAIL_LOG_BODY` 這個旗標把關，預設回 None
fn body_log_line(email: &Email, log_body: bool) -> Option<String> {
    log_body.then(|| email.text.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn email() -> Email {
        Email {
            to: "buyer@test.local".to_string(),
            subject: "重設密碼".to_string(),
            text: "請點 http://localhost:5173/reset/SECRET-TOKEN 重設".to_string(),
            html: "<p>SECRET-TOKEN</p>".to_string(),
        }
    }

    #[test]
    fn log_mailer_without_opt_in_does_not_format_body() {
        assert_eq!(
            body_log_line(&email(), false),
            None,
            "預設不能把含 token 的內文寫進 log"
        );
        assert_eq!(
            body_log_line(&email(), true).as_deref(),
            Some(email().text.as_str()),
            "明確 opt-in 才輸出內文"
        );
    }
}
