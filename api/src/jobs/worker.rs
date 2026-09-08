//! 認領 → 執行 → 標記。認領用一句 UPDATE … FROM (SELECT … FOR UPDATE SKIP LOCKED LIMIT 10)，
//! 多個 worker 行程也不會搶到同一筆；當機留下的 running 由 requeue_stale 撿回來（與規格不同之處 27）
use std::future::Future;

use serde_json::Value;
use sqlx::PgPool;

use crate::{
    jobs::{POLL_INTERVAL, handlers},
    state::AppState,
};

/// 一輪最多認領幾筆（規格 §9 LIMIT 10）
pub const BATCH: i64 = 10;
/// running 超過這麼久當作當機殘留
pub const STALE_RUNNING_MINUTES: i32 = 10;
/// 退避上限：2^attempts 分鐘，最多 2^10（max_attempts 預設 5，實際到不了）
const MAX_BACKOFF_EXP: i32 = 10;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Job {
    pub id: i64,
    pub kind: String,
    pub payload: Value,
    /// 認領時已 +1：第一次執行是 1
    pub attempts: i32,
    pub max_attempts: i32,
}

impl Job {
    /// 這次再失敗就會被標 failed；handler 用它決定要不要做「最後一次」的收尾（例如把發票標 failed）
    pub fn is_last_attempt(&self) -> bool {
        self.attempts >= self.max_attempts
    }
}

/// 認領一批：queued 且 run_at 到了，依 id 排序，跳過被別人鎖住的；認領同時標 running、attempts + 1
pub async fn claim(db: &PgPool, limit: i64) -> Result<Vec<Job>, sqlx::Error> {
    sqlx::query_as::<_, Job>(
        "WITH picked AS (
             SELECT id FROM jobs
             WHERE status = 'queued' AND run_at <= now()
             ORDER BY id
             FOR UPDATE SKIP LOCKED
             LIMIT $1
         )
         UPDATE jobs j SET status = 'running', attempts = j.attempts + 1, updated_at = now()
         FROM picked WHERE j.id = picked.id
         RETURNING j.id, j.kind, j.payload, j.attempts, j.max_attempts",
    )
    .bind(limit)
    .fetch_all(db)
    .await
}

/// 規格 §9：`run_at = now() + 2^attempts 分鐘`
pub fn backoff_minutes(attempts: i32) -> i32 {
    2_i32.pow(attempts.clamp(0, MAX_BACKOFF_EXP) as u32)
}

async fn mark_done(db: &PgPool, id: i64) -> Result<(), sqlx::Error> {
    // payload 清成 {} 不留個資（計畫 2 交接 2）
    sqlx::query(
        "UPDATE jobs SET status = 'done', payload = '{}'::jsonb, last_error = NULL, updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .execute(db)
    .await?;
    Ok(())
}

async fn mark_failed_attempt(db: &PgPool, job: &Job, error: &str) -> Result<(), sqlx::Error> {
    let error: String = error.chars().take(1000).collect();
    if job.is_last_attempt() {
        sqlx::query(
            "UPDATE jobs SET status = 'failed', last_error = $2, updated_at = now() WHERE id = $1",
        )
        .bind(job.id)
        .bind(error)
        .execute(db)
        .await?;
    } else {
        sqlx::query(
            "UPDATE jobs SET status = 'queued', run_at = now() + make_interval(mins => $3), last_error = $2, updated_at = now()
             WHERE id = $1",
        )
        .bind(job.id)
        .bind(error)
        .bind(backoff_minutes(job.attempts))
        .execute(db)
        .await?;
    }
    Ok(())
}

/// 跑一輪：認領、逐筆執行、標記。回處理的筆數。handler 可注入（測試用假的）
pub async fn run_once_with<F, Fut>(db: &PgPool, handler: F) -> anyhow::Result<usize>
where
    F: Fn(Job) -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    let jobs = claim(db, BATCH).await?;
    let n = jobs.len();
    for job in jobs {
        match handler(job.clone()).await {
            Ok(()) => mark_done(db, job.id).await?,
            Err(e) => {
                let msg = format!("{e:#}");
                tracing::warn!(
                    job_id = job.id,
                    kind = %job.kind,
                    attempts = job.attempts,
                    failed_for_good = job.is_last_attempt(),
                    error = %msg,
                    "job 失敗"
                );
                mark_failed_attempt(db, &job, &msg).await?;
            }
        }
    }
    Ok(n)
}

/// 正式的一輪：用 handlers::run
pub async fn run_once(state: &AppState) -> anyhow::Result<usize> {
    run_once_with(
        &state.db,
        |job| async move { handlers::run(state, &job).await },
    )
    .await
}

/// 無限迴圈：每 2 秒跑一輪；一輪認領滿 BATCH 就馬上再跑（佇列長時不用等）
pub async fn run(state: AppState) {
    if let Err(e) = requeue_stale(&state.db).await {
        tracing::error!(error = %format!("{e:#}"), "啟動時重排 stale job 失敗");
    }
    loop {
        match run_once(&state).await {
            Ok(n) if n >= BATCH as usize => continue,
            Ok(_) => {}
            Err(e) => tracing::error!(error = %format!("{e:#}"), "worker 這一輪失敗"),
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// running 超過 STALE_RUNNING_MINUTES 的重新排隊（不改 attempts；認領時會再 +1）
pub async fn requeue_stale(db: &PgPool) -> anyhow::Result<usize> {
    let n = sqlx::query(
        "UPDATE jobs SET status = 'queued', run_at = now(), updated_at = now()
         WHERE status = 'running' AND updated_at < now() - make_interval(mins => $1)",
    )
    .bind(STALE_RUNNING_MINUTES)
    .execute(db)
    .await?
    .rows_affected();
    if n > 0 {
        tracing::warn!(count = n, "重新排隊當機殘留的 running job");
    }
    Ok(n as usize)
}
