//! 排程型工作（規格 §9）：不走 jobs 表，直接定時掃
use std::{future::Future, time::Duration};

use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    domain::{orders, payments},
    state::AppState,
};

/// 每 `every` 跑一次 `task`（第一次立刻跑）；錯誤記 log 不中斷
pub async fn run_every<F, Fut>(every: Duration, name: &'static str, state: AppState, task: F)
where
    F: Fn(AppState) -> Fut + Send + 'static,
    Fut: Future<Output = anyhow::Result<usize>> + Send,
{
    let mut tick = tokio::time::interval(every);
    loop {
        tick.tick().await;
        match task(state.clone()).await {
            Ok(0) => {}
            Ok(n) => tracing::info!(task = name, affected = n, "排程工作完成"),
            Err(e) => tracing::error!(task = name, error = %format!("{e:#}"), "排程工作失敗"),
        }
    }
}

/// 過期未付款（規格 §5）：到期時間 = 該訂單所有 payments.expire_at 的最大值 + 2 小時；
/// 沒有任何繳費期限就 created_at + 3 天。到期 → cancelled(expired)、歸還庫存、pending 的 payments 標 expired。
/// 每筆一個交易（一筆失敗不影響其他）；一輪最多 500 筆，下一輪再繼續。
/// 交易內先 UPDATE payments 再 cancel_in_tx（先鎖 payments 再鎖 orders），
/// 和 payments::apply_return 的鎖定順序一致，避免兩者互相死鎖（Task 8 controller ruling）；
/// 訂單狀態已不是 pending_payment（cancel_in_tx 回 false）就整筆 rollback，payments 的更新也一併撤銷。
pub async fn expire_unpaid_orders(db: &PgPool) -> anyhow::Result<usize> {
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT o.id FROM orders o
         WHERE o.status = 'pending_payment'
           AND COALESCE(
                 (SELECT max(p.expire_at) FROM payments p WHERE p.order_id = o.id) + interval '2 hours',
                 o.created_at + interval '3 days'
               ) <= now()
         ORDER BY o.created_at
         LIMIT 500",
    )
    .fetch_all(db)
    .await?;
    let mut n = 0;
    for id in ids {
        let mut tx = db.begin().await?;
        sqlx::query("UPDATE payments SET status = $2, updated_at = now() WHERE order_id = $1 AND status = $3")
            .bind(id)
            .bind(payments::PAYMENT_EXPIRED)
            .bind(payments::PAYMENT_PENDING)
            .execute(&mut *tx)
            .await?;
        if orders::cancel_in_tx(&mut tx, id, "expired").await? {
            tx.commit().await?;
            n += 1;
        } else {
            tx.rollback().await?;
        }
    }
    Ok(n)
}

/// 出貨超過 14 天且沒被退回 → completed（規格 §9）。shipped_at 由計畫 4 的出貨寫入
pub async fn auto_complete_shipped(db: &PgPool) -> anyhow::Result<usize> {
    let n = sqlx::query(
        "UPDATE orders o SET status = 'completed', completed_at = now()
         FROM shipments s
         WHERE s.order_id = o.id
           AND o.status = 'shipped'
           AND o.shipped_at <= now() - interval '14 days'
           AND s.status <> 'returned'",
    )
    .execute(db)
    .await?
    .rows_affected();
    Ok(n as usize)
}

/// 每天清理：過期 session、過期或用過的重設 token、過期門市選擇、30 天前做完的 job
pub async fn purge_expired(db: &PgPool) -> anyhow::Result<usize> {
    let mut n = 0u64;
    n += sqlx::query("DELETE FROM sessions WHERE expires_at <= now()")
        .execute(db)
        .await?
        .rows_affected();
    n +=
        sqlx::query("DELETE FROM password_resets WHERE expires_at <= now() OR used_at IS NOT NULL")
            .execute(db)
            .await?
            .rows_affected();
    n += sqlx::query("DELETE FROM cvs_store_selections WHERE expires_at <= now()")
        .execute(db)
        .await?
        .rows_affected();
    n += sqlx::query(
        "DELETE FROM jobs WHERE status = 'done' AND updated_at <= now() - interval '30 days'",
    )
    .execute(db)
    .await?
    .rows_affected();
    Ok(n as usize)
}
