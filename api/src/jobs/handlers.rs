//! job 種類對應的執行函式。Task 9 加 send_email、Task 10 加 issue_invoice
use crate::{
    domain::jobs::{KIND_ISSUE_INVOICE, KIND_SEND_EMAIL},
    jobs::worker::Job,
    state::AppState,
};

pub async fn run(_state: &AppState, job: &Job) -> anyhow::Result<()> {
    match job.kind.as_str() {
        KIND_SEND_EMAIL => anyhow::bail!("send_email handler 尚未實作（Task 9）"),
        KIND_ISSUE_INVOICE => anyhow::bail!("issue_invoice handler 尚未實作（Task 10）"),
        other => anyhow::bail!("未知的 job kind：{other}"),
    }
}
