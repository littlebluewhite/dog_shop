//! Email 寄送（規格 §12）。Mailer 有三種：Smtp（正式）、Log（沒設 SMTP，與規格不同之處 22）、Capture（測試）
pub mod templates;

use std::sync::{Arc, Mutex};

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

pub enum Mailer {
    Smtp {
        transport: AsyncSmtpTransport<Tokio1Executor>,
        from: Mailbox,
    },
    /// 只記 to／subject（info）；內文只在 RUST_LOG 開 `mail_body=debug` 時輸出（含重設連結，正式環境不要開）
    Log,
    /// 測試用：全部收進 Vec
    Capture(Arc<Mutex<Vec<Email>>>),
}

impl Mailer {
    pub fn from_config(cfg: &Config) -> anyhow::Result<Self> {
        let Some(smtp) = &cfg.smtp else {
            tracing::warn!("SMTP 未設定（SMTP_HOST 空白）：Email 只會記 log，不會真的寄出");
            return Ok(Self::Log);
        };
        // 465 = 一開始就是 TLS；其他（587、25）= STARTTLS
        let mut builder = if smtp.port == 465 {
            AsyncSmtpTransport::<Tokio1Executor>::relay(&smtp.host)?
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&smtp.host)?
        };
        builder = builder.port(smtp.port);
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
        })
    }

    pub fn capture() -> (Self, Arc<Mutex<Vec<Email>>>) {
        let sink = Arc::new(Mutex::new(Vec::new()));
        (Self::Capture(sink.clone()), sink)
    }

    pub async fn send(&self, email: Email) -> anyhow::Result<()> {
        match self {
            Self::Smtp { transport, from } => {
                let to: Mailbox = email
                    .to
                    .parse()
                    .with_context(|| format!("收件人格式錯誤：{}", email.to))?;
                let message = Message::builder()
                    .from(from.clone())
                    .to(to)
                    .subject(email.subject)
                    .multipart(MultiPart::alternative_plain_html(email.text, email.html))?;
                transport.send(message).await.context("SMTP 寄送失敗")?;
                Ok(())
            }
            Self::Log => {
                tracing::info!(to = %email.to, subject = %email.subject, "Email（未設定 SMTP，只記 log）");
                tracing::debug!(target: "mail_body", to = %email.to, "{}", email.text);
                Ok(())
            }
            Self::Capture(sink) => {
                sink.lock().expect("mail sink").push(email);
                Ok(())
            }
        }
    }
}
