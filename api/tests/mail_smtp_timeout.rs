//! SMTP 寄送要有期限：接了連線卻不回應的伺服器不能讓 worker 永遠卡在這一筆（codex P1-1）
use std::time::{Duration, Instant};

use dog_shop_api::mail::{Email, Mailer};
use lettre::{AsyncSmtpTransport, Tokio1Executor};

/// 假 SMTP：accept 之後什麼都不回（連 220 greeting 都沒有），連線也不關
async fn hanging_smtp_port() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let _keep_open = stream;
                std::future::pending::<()>().await;
            });
        }
    });
    port
}

fn mail() -> Email {
    Email {
        to: "buyer@test.local".to_string(),
        subject: "測試".to_string(),
        text: "內文".to_string(),
        html: "<p>內文</p>".to_string(),
    }
}

#[tokio::test]
async fn smtp_send_times_out_instead_of_hanging() {
    let port = hanging_smtp_port().await;
    let mailer = Mailer::Smtp {
        transport: AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous("127.0.0.1")
            .port(port)
            .build(),
        from: "商店 <shop@test.local>".parse().unwrap(),
        send_timeout: Duration::from_secs(1),
    };
    let started = Instant::now();
    // 整個測試體包一層 10 秒：修正前 send 沒有期限，這裡會是「卡住」而不是明確失敗
    let result = tokio::time::timeout(Duration::from_secs(10), mailer.send(mail()))
        .await
        .expect("send 沒有在 10 秒內回來：SMTP 卡住會堵死整個 worker");
    let err = result.expect_err("不回應的 SMTP 應該回 Err");
    let msg = format!("{err:#}");
    assert!(msg.contains("逾時"), "錯誤訊息要說是逾時：{msg}");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "應該在 send_timeout 後就放棄，實際花了 {:?}",
        started.elapsed()
    );
}
