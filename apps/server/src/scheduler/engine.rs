//! Runs [`ScheduledJob`]s on their [`Schedule`]s.
//!
//! One engine, many jobs: what varies is the work, so that is the trait;
//! when to run, what to record and how to recover from downtime is the
//! engine's, written once. State lives in `scheduled_jobs` so a restart
//! neither skips nor repeats a run. Rustrak is a single process, so an
//! in-memory set is enough to keep a job from overlapping itself.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;
use serde_json::Value;
use sqlx::types::Json;
use tokio::task::JoinHandle;

use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::scheduler::schedule::{truncate_to_minute, Schedule};

/// What to do when the server was down at a job's run time (Quartz's
/// misfire instruction).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Misfire {
    /// Drop the missed run and wait for the next slot.
    Skip,
    /// Run once now, however many runs were missed, then carry on.
    RunOnce,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobRunStatus {
    Ok,
    Error,
}

/// A job's row: its settings, its clock and its last run.
#[derive(Debug, Clone)]
pub struct JobState {
    pub settings: Value,
    pub next_run_at: Option<DateTime<Utc>>,
    pub last_started_at: Option<DateTime<Utc>>,
    pub last_finished_at: Option<DateTime<Utc>>,
    pub last_status: Option<JobRunStatus>,
    pub last_result: Option<Value>,
    pub last_error: Option<String>,
}

/// A piece of recurring work.
#[async_trait]
pub trait ScheduledJob: Send + Sync {
    /// Its row in `scheduled_jobs`. Never change it once shipped.
    fn key(&self) -> &'static str;

    /// When it runs, read from its stored settings. `None` disables it.
    fn schedule(&self, settings: &Value) -> Option<(Schedule, Tz)>;

    fn misfire(&self) -> Misfire;

    /// Does the work. The returned value is stored as the run's result.
    async fn run(&self, ctx: JobContext<'_>) -> AppResult<Value>;
}

/// What a run gets to work with.
pub struct JobContext<'a> {
    pub pool: &'a DbPool,
    pub settings: &'a Value,
    /// The slot being run. After downtime it is in the past, so a job can
    /// stay idempotent per slot.
    pub scheduled_for: DateTime<Utc>,
    /// The tick that started the run: the clock a job reads instead of the
    /// wall clock, so tests can drive it.
    pub now: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct JobRow {
    settings: Json<Value>,
    next_run_at: Option<DateTime<Utc>>,
    last_started_at: Option<DateTime<Utc>>,
    last_finished_at: Option<DateTime<Utc>>,
    last_status: Option<String>,
    last_result: Option<Json<Value>>,
    last_error: Option<String>,
}

impl From<JobRow> for JobState {
    fn from(row: JobRow) -> Self {
        Self {
            settings: row.settings.0,
            next_run_at: row.next_run_at,
            last_started_at: row.last_started_at,
            last_finished_at: row.last_finished_at,
            last_status: row.last_status.map(|s| match s.as_str() {
                "ok" => JobRunStatus::Ok,
                _ => JobRunStatus::Error,
            }),
            last_result: row.last_result.map(|r| r.0),
            last_error: row.last_error,
        }
    }
}

/// How late a run may start and still count as on time: the engine ticks
/// once a minute, and a tick can land just after the minute it is for.
const ON_TIME: Duration = Duration::minutes(1);

pub struct Scheduler {
    jobs: Vec<Arc<dyn ScheduledJob>>,
    running: Arc<Mutex<HashSet<&'static str>>>,
}

impl Scheduler {
    pub fn new(jobs: Vec<Arc<dyn ScheduledJob>>) -> Self {
        Self {
            jobs,
            running: Arc::default(),
        }
    }

    /// Starts every job due at `now`, each in its own task so a long cleanup
    /// never delays the monitor sweep. Returns their handles; the server's
    /// loop drops them, tests await them.
    pub async fn tick(&self, pool: &DbPool, now: DateTime<Utc>) -> AppResult<Vec<JoinHandle<()>>> {
        let now = truncate_to_minute(now);
        let mut started = Vec::new();
        for job in &self.jobs {
            let key = job.key();
            let state = Self::ensure(pool, key).await?;
            let Some((schedule, tz)) = job.schedule(&state.settings) else {
                if state.next_run_at.is_some() {
                    set_next_run(pool, key, None).await?;
                }
                continue;
            };
            let Some(due) = state.next_run_at else {
                set_next_run(pool, key, Some(schedule.next_after(now, tz))).await?;
                continue;
            };
            if due > now || self.is_running(key) {
                continue;
            }

            let next = schedule.next_after(now, tz);
            let late = now - due > ON_TIME;
            if late && job.misfire() == Misfire::Skip {
                set_next_run(pool, key, Some(next)).await?;
                continue;
            }

            sqlx::query(
                "UPDATE scheduled_jobs SET next_run_at = $2, last_started_at = $3 WHERE key = $1",
            )
            .bind(key)
            .bind(next)
            .bind(now)
            .execute(pool)
            .await?;
            lock(&self.running).insert(key);
            started.push(self.spawn(pool.clone(), Arc::clone(job), state.settings, due, now));
        }
        Ok(started)
    }

    fn is_running(&self, key: &str) -> bool {
        lock(&self.running).contains(key)
    }

    fn spawn(
        &self,
        pool: DbPool,
        job: Arc<dyn ScheduledJob>,
        settings: Value,
        scheduled_for: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> JoinHandle<()> {
        let running = Arc::clone(&self.running);
        tokio::spawn(async move {
            let key = job.key();
            let outcome = job
                .run(JobContext {
                    pool: &pool,
                    settings: &settings,
                    scheduled_for,
                    now,
                })
                .await;
            if let Err(e) = &outcome {
                log::error!("Scheduled job {key} failed: {e}");
            }
            let (status, result, error) = match outcome {
                Ok(result) => ("ok", Some(Json(result)), None),
                Err(e) => ("error", None, Some(e.to_string())),
            };
            if let Err(e) = sqlx::query(
                r#"
                UPDATE scheduled_jobs
                SET last_finished_at = $2, last_status = $3, last_result = $4, last_error = $5
                WHERE key = $1
                "#,
            )
            .bind(key)
            .bind(Utc::now())
            .bind(status)
            .bind(result)
            .bind(error)
            .execute(&pool)
            .await
            {
                log::error!("Could not record the run of scheduled job {key}: {e}");
            }
            lock(&running).remove(key);
        })
    }

    /// A job's row, created with empty settings the first time it is asked
    /// for.
    async fn ensure(pool: &DbPool, key: &str) -> AppResult<JobState> {
        sqlx::query("INSERT INTO scheduled_jobs (key, settings) VALUES ($1, $2) ON CONFLICT (key) DO NOTHING")
            .bind(key)
            .bind(Json(Value::Object(Default::default())))
            .execute(pool)
            .await?;
        Self::job(pool, key).await
    }

    pub async fn job(pool: &DbPool, key: &str) -> AppResult<JobState> {
        sqlx::query_as::<_, JobRow>(
            r#"
            SELECT settings, next_run_at, last_started_at, last_finished_at,
                   last_status, last_result, last_error
            FROM scheduled_jobs WHERE key = $1
            "#,
        )
        .bind(key)
        .fetch_optional(pool)
        .await?
        .map(JobState::from)
        .ok_or_else(|| AppError::NotFound(format!("Scheduled job {key} not found")))
    }

    /// Replaces a job's settings. Its next run is recomputed on the next
    /// tick, so a new schedule applies within a minute.
    pub async fn update_settings(pool: &DbPool, key: &str, settings: Value) -> AppResult<JobState> {
        Self::ensure(pool, key).await?;
        sqlx::query("UPDATE scheduled_jobs SET settings = $2, next_run_at = NULL WHERE key = $1")
            .bind(key)
            .bind(Json(settings))
            .execute(pool)
            .await?;
        Self::job(pool, key).await
    }
}

async fn set_next_run(pool: &DbPool, key: &str, next: Option<DateTime<Utc>>) -> AppResult<()> {
    sqlx::query("UPDATE scheduled_jobs SET next_run_at = $2 WHERE key = $1")
        .bind(key)
        .bind(next)
        .execute(pool)
        .await?;
    Ok(())
}

/// The set is plain data; a panic while it was held leaves nothing torn.
fn lock<'a>(
    set: &'a Mutex<HashSet<&'static str>>,
) -> std::sync::MutexGuard<'a, HashSet<&'static str>> {
    set.lock().unwrap_or_else(PoisonError::into_inner)
}
