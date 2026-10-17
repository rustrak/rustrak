//! The jobs the server schedules.

use actix_web::web;
use async_trait::async_trait;
use chrono_tz::Tz;
use serde_json::{json, Value};

use crate::digest::processors::Processors;
use crate::error::AppResult;
use crate::scheduler::engine::{JobContext, Misfire, ScheduledJob};
use crate::scheduler::schedule::{IntervalUnit, Schedule};

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
