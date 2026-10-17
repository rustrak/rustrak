//! The jobs the server schedules.

use actix_web::web;
use async_trait::async_trait;
use chrono_tz::Tz;
use serde_json::{json, Value};

use crate::digest::processors::Processors;
use crate::error::AppResult;
use crate::models::AutoCleanupSettings;
use crate::scheduler::engine::{JobContext, Misfire, ScheduledJob};
use crate::scheduler::schedule::{parse_timezone, IntervalUnit, Schedule};
use crate::services::StorageService;

/// Cron monitors: marks the check-ins that were missed or ran past their max
/// runtime, once a minute, the granularity of every monitor schedule.
pub struct MonitorSweepJob {
    processors: web::Data<Processors>,
}

impl MonitorSweepJob {
    pub const KEY: &'static str = "monitor_sweep";

    pub fn new(processors: web::Data<Processors>) -> Self {
        Self { processors }
    }
}

#[async_trait]
impl ScheduledJob for MonitorSweepJob {
    fn key(&self) -> &'static str {
        Self::KEY
    }

    fn schedule(&self, _settings: &Value) -> Option<(Schedule, Tz)> {
        Some((
            Schedule::Interval {
                value: 1,
                unit: IntervalUnit::Minute,
            },
            Tz::UTC,
        ))
    }

    /// The sweep reads the clock itself and catches up on its own: one run
    /// covers any downtime.
    fn misfire(&self) -> Misfire {
        Misfire::RunOnce
    }

    async fn run(&self, ctx: JobContext<'_>) -> AppResult<Value> {
        let failures =
            crate::routes::ingest::run_monitor_sweep(&self.processors, ctx.pool, ctx.now).await?;
        Ok(json!({ "failures": failures }))
    }
}

/// Automatic storage cleanup, on the schedule and for the categories an
/// admin chose in Settings → Storage. Off until turned on.
pub struct StorageCleanupJob;

impl StorageCleanupJob {
    pub const KEY: &'static str = "storage_cleanup";

    /// Stored settings, with defaults for anything missing or unreadable.
    pub fn settings(stored: &Value) -> AutoCleanupSettings {
        serde_json::from_value(stored.clone()).unwrap_or_default()
    }
}

#[async_trait]
impl ScheduledJob for StorageCleanupJob {
    fn key(&self) -> &'static str {
        Self::KEY
    }

    fn schedule(&self, settings: &Value) -> Option<(Schedule, Tz)> {
        let settings = Self::settings(settings);
        if !settings.enabled || settings.validate().is_err() {
            return None;
        }
        let tz = parse_timezone(&settings.timezone).ok()?;
        Some((settings.schedule, tz))
    }

    /// A missed cleanup runs once: catching up on every night it slept
    /// through would only repeat the same purge.
    fn misfire(&self) -> Misfire {
        Misfire::RunOnce
    }

    async fn run(&self, ctx: JobContext<'_>) -> AppResult<Value> {
        let settings = Self::settings(ctx.settings);
        let removed = StorageService::execute_cleanup(
            ctx.pool,
            settings.older_than_days,
            None,
            settings.filter(),
        )
        .await?;
        Ok(serde_json::to_value(removed).unwrap_or_default())
    }
}
