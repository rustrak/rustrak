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
use chrono::{DateTime, Duration, DurationRound, Utc};
use chrono_tz::Tz;
use serde_json::Value;
use sqlx::types::Json;
use tokio::task::JoinHandle;

use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::schedule::{truncate_to_minute, Schedule};

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

    /// Busy with work started outside the schedule. A due run waits for it
    /// like it waits for its own last run, leaving that work's history alone.
    fn busy(&self) -> bool {
        false
    }

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
            let settings = &state.settings;
            let Some((schedule, tz)) = job.schedule(settings) else {
                if state.next_run_at.is_some() {
                    set_next_run(pool, key, None, settings).await?;
                }
                continue;
            };
            let Some(due) = state.next_run_at else {
                set_next_run(pool, key, Some(schedule.next_after(now, tz)), settings).await?;
                continue;
            };
            if due > now || self.is_running(key) || job.busy() {
                continue;
            }

            let next = schedule.next_after(now, tz);
            let late = now - due > ON_TIME;
            if late && job.misfire() == Misfire::Skip {
                set_next_run(pool, key, Some(next), settings).await?;
                continue;
            }

            let claimed = sqlx::query(
                "UPDATE scheduled_jobs SET next_run_at = $2, last_started_at = $3 WHERE key = $1 AND settings = $4",
            )
            .bind(key)
            .bind(next)
            .bind(now)
            .bind(Json(settings))
            .execute(pool)
            .await?
            .rows_affected()
                > 0;
            if !claimed {
                // Settings saved since the read: the next tick reschedules.
                continue;
            }
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
        let running = Running(Arc::clone(&self.running), job.key());
        tokio::spawn(async move {
            let _running = running;
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
            Self::record_finish(&pool, key, now, outcome).await;
        })
    }

    /// Records a run of `key` asked for now, outside the schedule, so it is
    /// the job's last run like any other: its start at `started_at`, then the
    /// outcome of `run` in the background. The run has already started, so a
    /// failure to record it is logged, not returned.
    pub async fn record_run_now(
        pool: DbPool,
        key: &'static str,
        started_at: DateTime<Utc>,
        run: impl std::future::Future<Output = AppResult<Value>> + Send + 'static,
    ) {
        // Milliseconds, so the finish matches the start on every backend
        // (Postgres keeps microseconds).
        let started_at = started_at
            .duration_trunc(Duration::milliseconds(1))
            .unwrap_or(started_at);
        if let Err(e) = Self::record_start(&pool, key, started_at).await {
            log::error!("Could not record the start of scheduled job {key}: {e}");
        }
        tokio::spawn(async move {
            let outcome = run.await;
            Self::record_finish(&pool, key, started_at, outcome).await;
        });
    }

    async fn record_start(pool: &DbPool, key: &str, at: DateTime<Utc>) -> AppResult<()> {
        Self::ensure(pool, key).await?;
        sqlx::query("UPDATE scheduled_jobs SET last_started_at = $2 WHERE key = $1")
            .bind(key)
            .bind(at)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Records how the run of `key` started at `started_at` ended, unless a
    /// later run has started since (its own finish will be recorded). A
    /// failure to record is logged: the run itself already happened.
    async fn record_finish(
        pool: &DbPool,
        key: &str,
        started_at: DateTime<Utc>,
        outcome: AppResult<Value>,
    ) {
        let (status, result, error) = match outcome {
            Ok(result) => ("ok", Some(Json(result)), None),
            Err(e) => ("error", None, Some(e.to_string())),
        };
        if let Err(e) = sqlx::query(
            r#"
            UPDATE scheduled_jobs
            SET last_finished_at = $2, last_status = $3, last_result = $4, last_error = $5
            WHERE key = $1 AND last_started_at = $6
            "#,
        )
        .bind(key)
        .bind(Utc::now())
        .bind(status)
        .bind(result)
        .bind(error)
        .bind(started_at)
        .execute(pool)
        .await
        {
            log::error!("Could not record the run of scheduled job {key}: {e}");
        }
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

    /// [`Self::update_settings`], with the next run computed now, as the next
    /// tick would, so the caller can show it at once.
    /// `schedule` is what the job's [`ScheduledJob::schedule`] reads from
    /// `settings`.
    pub async fn save_settings(
        pool: &DbPool,
        key: &str,
        settings: Value,
        schedule: Option<(Schedule, Tz)>,
        now: DateTime<Utc>,
    ) -> AppResult<JobState> {
        Self::update_settings(pool, key, settings.clone()).await?;
        if let Some((schedule, tz)) = schedule {
            let next = schedule.next_after(truncate_to_minute(now), tz);
            set_next_run(pool, key, Some(next), &settings).await?;
        }
        Self::job(pool, key).await
    }
}

/// Only while the settings are still the ones `next` was computed from, so a
/// save racing a tick is not overwritten by the old schedule.
async fn set_next_run(
    pool: &DbPool,
    key: &str,
    next: Option<DateTime<Utc>>,
    settings: &Value,
) -> AppResult<()> {
    sqlx::query("UPDATE scheduled_jobs SET next_run_at = $2 WHERE key = $1 AND settings = $3")
        .bind(key)
        .bind(next)
        .bind(Json(settings))
        .execute(pool)
        .await?;
    Ok(())
}

/// Takes a job out of the running set however its task ends: done, panicked
/// or aborted.
struct Running(Arc<Mutex<HashSet<&'static str>>>, &'static str);

impl Drop for Running {
    fn drop(&mut self) {
        lock(&self.0).remove(self.1);
    }
}

/// The set is plain data; a panic while it was held leaves nothing torn.
fn lock<'a>(
    set: &'a Mutex<HashSet<&'static str>>,
) -> std::sync::MutexGuard<'a, HashSet<&'static str>> {
    set.lock().unwrap_or_else(PoisonError::into_inner)
}
