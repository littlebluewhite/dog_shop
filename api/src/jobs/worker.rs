//! 認領 → 執行 → 標記。認領用一句 UPDATE … FROM (SELECT … FOR UPDATE SKIP LOCKED LIMIT 10)，
//! 多個 worker 行程也不會搶到同一筆；當機留下的 running 由 requeue_stale 撿回來（與規格不同之處 27）
use std::{future::Future, time::Duration};

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
/// 單一 job 的執行期限：卡住的 handler（例如不回應的 SMTP）不能堵死後面所有 job（codex P1-1）
pub const JOB_TIMEOUT: Duration = Duration::from_secs(120);

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

/// 標記完成；只有還是 running 才會改（可能已被 requeue_stale 重排給別的 worker，計畫 4 交接 2）。
/// 回傳是否真的改到（rows_affected > 0）
pub async fn mark_done(db: &PgPool, id: i64) -> Result<bool, sqlx::Error> {
    // payload 清成 {} 不留個資（計畫 2 交接 2）
    let result = sqlx::query(
        "UPDATE jobs SET status = 'done', payload = '{}'::jsonb, last_error = NULL, updated_at = now() WHERE id = $1 AND status = 'running'",
    )
    .bind(id)
    .execute(db)
    .await?;
    Ok(result.rows_affected() > 0)
}

async fn mark_failed_attempt(db: &PgPool, job: &Job, error: &str) -> Result<(), sqlx::Error> {
    let error: String = error.chars().take(1000).collect();
    if job.is_last_attempt() {
        sqlx::query(
            "UPDATE jobs SET status = 'failed', last_error = $2, updated_at = now() WHERE id = $1 AND status = 'running'",
        )
        .bind(job.id)
        .bind(error)
        .execute(db)
        .await?;
    } else {
        sqlx::query(
            "UPDATE jobs SET status = 'queued', run_at = now() + make_interval(mins => $3), last_error = $2, updated_at = now()
             WHERE id = $1 AND status = 'running'",
        )
        .bind(job.id)
        .bind(error)
        .bind(backoff_minutes(job.attempts))
        .execute(db)
        .await?;
    }
    Ok(())
}

/// 跑一輪：認領、逐筆執行、標記。回處理的筆數。handler 可注入（測試用假的）。
/// 用 tokio::spawn 包住每次呼叫：handler 裡的 panic 不會拖垮整個 worker 迴圈（否則會一路 unwind
/// 到 mod.rs 的 tokio::spawn，那個 task 就悄悄死掉、不會重啟），而是跟 Err 一樣走
/// mark_failed_attempt，讓 max_attempts 照樣生效（Task 8 review）。每筆最多跑 JOB_TIMEOUT
pub async fn run_once_with<F, Fut>(db: &PgPool, handler: F) -> anyhow::Result<usize>
where
    F: Fn(Job) -> Fut,
    Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
{
    run_once_with_timeout(db, JOB_TIMEOUT, handler).await
}

/// 同 run_once_with，但每筆 job 最多跑 `timeout`；超過就 abort 那個 task 並當成失敗
/// （走 mark_failed_attempt，max_attempts 照樣生效）。卡住的 handler 只會拖慢自己這一筆，
/// 不會讓整個 worker 迴圈停住（requeue_stale 救不了卡住的 future）
pub async fn run_once_with_timeout<F, Fut>(
    db: &PgPool,
    timeout: Duration,
    handler: F,
) -> anyhow::Result<usize>
where
    F: Fn(Job) -> Fut,
    Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
{
    let jobs = claim(db, BATCH).await?;
    let n = jobs.len();
    for job in jobs {
        let mut handle = tokio::spawn(handler(job.clone()));
        let outcome = match tokio::time::timeout(timeout, &mut handle).await {
            Ok(Ok(result)) => result,
            Ok(Err(join_err)) => {
                let msg = if join_err.is_panic() {
                    "handler panicked".to_string()
                } else {
                    format!("job task 未完成：{join_err}")
                };
                Err(anyhow::anyhow!(msg))
            }
            Err(_elapsed) => {
                handle.abort();
                Err(anyhow::anyhow!(
                    "job 逾時（{} 秒），已中止",
                    timeout.as_secs_f64()
                ))
            }
        };
        match outcome {
            Ok(()) => {
                if !mark_done(db, job.id).await? {
                    tracing::warn!(
                        job_id = job.id,
                        "job 已不是 running（可能被 requeue_stale 重排），不改狀態"
                    );
                }
            }
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

/// 正式的一輪：用 handlers::run。每個 job 各自 clone 一份 AppState 讓 handler 的 future 不借用
/// run_once 的參數，才能滿足 run_once_with 現在要求的 Send + 'static（tokio::spawn 隔離 panic 需要）
pub async fn run_once(state: &AppState) -> anyhow::Result<usize> {
    run_once_with(&state.db, |job| {
        let state = state.clone();
        async move { handlers::run(&state, &job).await }
    })
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
