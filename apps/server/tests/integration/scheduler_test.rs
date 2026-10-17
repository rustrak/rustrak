//! The internal scheduler: when jobs run, what it records, and how it
//! behaves across downtime, overlap and settings changes. Driven with an
//! explicit `now`, so nothing here waits on a clock.

use crate::common::TestDb;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use rustrak::db::DbPool;
use rustrak::error::{AppError, AppResult};
use rustrak::scheduler::engine::{JobContext, JobRunStatus, Misfire, ScheduledJob, Scheduler};
use rustrak::scheduler::schedule::Schedule;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

fn at(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

/// Runs hourly unless its settings say `{"enabled": false}`; records every
/// run; fails when its settings say so.
struct FakeJob {
    misfire: Misfire,
    runs: Mutex<Vec<DateTime<Utc>>>,
    gate: Option<Arc<tokio::sync::Notify>>,
}

impl FakeJob {
    fn new(misfire: Misfire) -> Arc<Self> {
        Arc::new(Self {
            misfire,
            runs: Mutex::new(Vec::new()),
            gate: None,
        })
    }

    fn runs(&self) -> Vec<DateTime<Utc>> {
        self.runs.lock().unwrap().clone()
    }
}

#[async_trait]
impl ScheduledJob for FakeJob {
    fn key(&self) -> &'static str {
        "fake"
    }

    fn schedule(&self, settings: &Value) -> Option<(Schedule, Tz)> {
        if settings["enabled"] == json!(false) {
            return None;
        }
        let value = settings["crontab"]
            .as_str()
            .unwrap_or("0 * * * *")
            .to_string();
        Some((Schedule::Crontab { value }, Tz::UTC))
    }

    fn misfire(&self) -> Misfire {
        self.misfire
    }

    async fn run(&self, ctx: JobContext<'_>) -> AppResult<Value> {
        let settings = ctx.settings;
        self.runs.lock().unwrap().push(ctx.scheduled_for);
        if let Some(gate) = &self.gate {
            gate.notified().await;
        }
        if settings["fail"] == json!(true) {
            return Err(AppError::Internal("disk on fire".to_string()));
        }
        Ok(json!({"deleted": 3}))
    }
}

async fn tick(scheduler: &Scheduler, pool: &DbPool, now: &str) {
    for handle in scheduler.tick(pool, at(now)).await.unwrap() {
        handle.await.unwrap();
    }
}

#[actix_web::test]
async fn the_first_tick_schedules_without_running() {
    let db = TestDb::new().await;
    let job = FakeJob::new(Misfire::RunOnce);
    let scheduler = Scheduler::new(vec![job.clone()]);

    tick(&scheduler, &db.pool, "2026-01-01T05:30:00Z").await;

    assert!(job.runs().is_empty());
    let state = Scheduler::job(&db.pool, "fake").await.unwrap();
    assert_eq!(state.next_run_at, Some(at("2026-01-01T06:00:00Z")));
}

#[actix_web::test]
async fn a_due_job_runs_once_and_records_the_result() {
    let db = TestDb::new().await;
    let job = FakeJob::new(Misfire::RunOnce);
    let scheduler = Scheduler::new(vec![job.clone()]);
    tick(&scheduler, &db.pool, "2026-01-01T05:30:00Z").await;

    tick(&scheduler, &db.pool, "2026-01-01T05:59:00Z").await;
    assert!(job.runs().is_empty());

    tick(&scheduler, &db.pool, "2026-01-01T06:00:20Z").await;
    assert_eq!(job.runs(), vec![at("2026-01-01T06:00:00Z")]);

    let state = Scheduler::job(&db.pool, "fake").await.unwrap();
    assert_eq!(state.last_status, Some(JobRunStatus::Ok));
    assert_eq!(state.last_result, Some(json!({"deleted": 3})));
    assert_eq!(state.next_run_at, Some(at("2026-01-01T07:00:00Z")));
    assert!(state.last_finished_at.is_some());

    // Same minute again: already done.
    tick(&scheduler, &db.pool, "2026-01-01T06:00:50Z").await;
    assert_eq!(job.runs().len(), 1);
}

#[actix_web::test]
async fn a_failing_run_records_the_error_and_keeps_the_schedule() {
    let db = TestDb::new().await;
    let job = FakeJob::new(Misfire::RunOnce);
    let scheduler = Scheduler::new(vec![job.clone()]);
    tick(&scheduler, &db.pool, "2026-01-01T05:30:00Z").await;
    Scheduler::update_settings(&db.pool, "fake", json!({"fail": true}))
        .await
        .unwrap();
    tick(&scheduler, &db.pool, "2026-01-01T05:31:00Z").await;

    tick(&scheduler, &db.pool, "2026-01-01T06:00:00Z").await;

    let state = Scheduler::job(&db.pool, "fake").await.unwrap();
    assert_eq!(state.last_status, Some(JobRunStatus::Error));
    assert!(state.last_error.unwrap().contains("disk on fire"));
    assert_eq!(state.next_run_at, Some(at("2026-01-01T07:00:00Z")));
}

#[actix_web::test]
async fn a_disabled_job_never_runs() {
    let db = TestDb::new().await;
    let job = FakeJob::new(Misfire::RunOnce);
    let scheduler = Scheduler::new(vec![job.clone()]);
    tick(&scheduler, &db.pool, "2026-01-01T05:30:00Z").await;

    Scheduler::update_settings(&db.pool, "fake", json!({"enabled": false}))
        .await
        .unwrap();
    tick(&scheduler, &db.pool, "2026-01-01T06:00:00Z").await;
    tick(&scheduler, &db.pool, "2026-01-01T09:00:00Z").await;

    assert!(job.runs().is_empty());
    let state = Scheduler::job(&db.pool, "fake").await.unwrap();
    assert_eq!(state.next_run_at, None);
}

#[actix_web::test]
async fn new_settings_reschedule_from_the_next_tick() {
    let db = TestDb::new().await;
    let job = FakeJob::new(Misfire::RunOnce);
    let scheduler = Scheduler::new(vec![job.clone()]);
    tick(&scheduler, &db.pool, "2026-01-01T05:30:00Z").await;

    Scheduler::update_settings(&db.pool, "fake", json!({"crontab": "45 5 * * *"}))
        .await
        .unwrap();
    tick(&scheduler, &db.pool, "2026-01-01T05:31:00Z").await;

    let state = Scheduler::job(&db.pool, "fake").await.unwrap();
    assert_eq!(state.settings, json!({"crontab": "45 5 * * *"}));
    assert_eq!(state.next_run_at, Some(at("2026-01-01T05:45:00Z")));
}

#[actix_web::test]
async fn after_downtime_run_once_runs_a_single_time() {
    let db = TestDb::new().await;
    let job = FakeJob::new(Misfire::RunOnce);
    let scheduler = Scheduler::new(vec![job.clone()]);
    tick(&scheduler, &db.pool, "2026-01-01T05:30:00Z").await;

    // Down from 05:30 to 10:15: four hourly runs were missed.
    tick(&scheduler, &db.pool, "2026-01-01T10:15:00Z").await;

    assert_eq!(job.runs(), vec![at("2026-01-01T06:00:00Z")]);
    let state = Scheduler::job(&db.pool, "fake").await.unwrap();
    assert_eq!(state.next_run_at, Some(at("2026-01-01T11:00:00Z")));
}

#[actix_web::test]
async fn after_downtime_skip_waits_for_the_next_slot() {
    let db = TestDb::new().await;
    let job = FakeJob::new(Misfire::Skip);
    let scheduler = Scheduler::new(vec![job.clone()]);
    tick(&scheduler, &db.pool, "2026-01-01T05:30:00Z").await;

    tick(&scheduler, &db.pool, "2026-01-01T10:15:00Z").await;

    assert!(job.runs().is_empty());
    let state = Scheduler::job(&db.pool, "fake").await.unwrap();
    assert_eq!(state.next_run_at, Some(at("2026-01-01T11:00:00Z")));
}

#[actix_web::test]
async fn a_run_still_going_is_not_started_twice() {
    let db = TestDb::new().await;
    let gate = Arc::new(tokio::sync::Notify::new());
    let job = Arc::new(FakeJob {
        misfire: Misfire::RunOnce,
        runs: Mutex::new(Vec::new()),
        gate: Some(gate.clone()),
    });
    let scheduler = Scheduler::new(vec![job.clone()]);
    Scheduler::update_settings(&db.pool, "fake", json!({"crontab": "* * * * *"}))
        .await
        .unwrap();
    tick(&scheduler, &db.pool, "2026-01-01T05:30:00Z").await;

    let first = scheduler
        .tick(&db.pool, at("2026-01-01T05:31:00Z"))
        .await
        .unwrap();
    let second = scheduler
        .tick(&db.pool, at("2026-01-01T05:32:00Z"))
        .await
        .unwrap();
    assert_eq!(first.len(), 1);
    assert!(second.is_empty(), "the 05:31 run has not finished");

    gate.notify_one();
    for handle in first {
        handle.await.unwrap();
    }
    assert_eq!(job.runs().len(), 1);
}
