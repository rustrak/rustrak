//! Check-in processing and the missed/timeout sweep, against a real database.
//! Behaviour mirrors Sentry's `monitor_consumer.py`, `logic/incidents.py` and
//! `clock_tasks/check_missed.py` / `check_timeout.py`.

use crate::common::TestDb;
use chrono::{DateTime, Duration, Utc};
use rustrak::db::DbPool;
use rustrak::models::monitor::{
    CheckIn, CheckInStatus, FailureReason, MonitorConfigInput, MonitorEnvironmentStatus, Schedule,
};
use rustrak::models::CreateProject;
use rustrak::services::monitor::MonitorService;
use rustrak::services::ProjectService;
use uuid::Uuid;

fn at(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

async fn project(pool: &DbPool) -> i32 {
    ProjectService::create(
        pool,
        CreateProject {
            name: format!("Crons {}", Uuid::new_v4()),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap()
    .id
}

fn hourly(threshold: u64) -> MonitorConfigInput {
    MonitorConfigInput {
        schedule: Some(Schedule::Crontab {
            value: "0 * * * *".to_string(),
        }),
        failure_issue_threshold: Some(threshold),
        ..Default::default()
    }
}

fn check_in(id: Uuid, status: CheckInStatus, config: Option<MonitorConfigInput>) -> CheckIn {
    CheckIn::new(id, "backup", status, None, None, config, None).unwrap()
}

/// Creates the `backup` monitor (hourly) with one ok check-in at `when`.
async fn upsert(pool: &DbPool, project_id: i32, threshold: u64, when: DateTime<Utc>) {
    MonitorService::process_check_in(
        pool,
        project_id,
        check_in(Uuid::new_v4(), CheckInStatus::Ok, Some(hourly(threshold))),
        when,
    )
    .await
    .unwrap();
}

#[actix_web::test]
async fn upsert_creates_monitor_environment_and_check_in() {
    let db = TestDb::new().await;
    let pid = project(&db.pool).await;

    upsert(&db.pool, pid, 1, at("2026-01-01T05:00:10Z")).await;

    let monitor = MonitorService::get_by_slug(&db.pool, pid, "backup")
        .await
        .unwrap();
    assert_eq!(monitor.name, "backup");
    assert_eq!(monitor.environments.len(), 1);
    let env = &monitor.environments[0];
    assert_eq!(env.environment, "production");
    assert_eq!(env.status, MonitorEnvironmentStatus::Ok);
    assert_eq!(env.next_checkin, Some(at("2026-01-01T06:00:00Z")));
    assert_eq!(env.next_checkin_latest, Some(at("2026-01-01T06:01:00Z")));
    assert_eq!(env.last_checkin, Some(at("2026-01-01T05:00:10Z")));
    assert_eq!(env.recent_checkins.len(), 1);
    assert_eq!(env.recent_checkins[0].status, CheckInStatus::Ok);
}

#[actix_web::test]
async fn check_in_for_unknown_monitor_without_config_is_dropped() {
    let db = TestDb::new().await;
    let pid = project(&db.pool).await;

    let result = MonitorService::process_check_in(
        &db.pool,
        pid,
        check_in(Uuid::new_v4(), CheckInStatus::Ok, None),
        Utc::now(),
    )
    .await;

    assert!(result.is_err());
    assert!(MonitorService::list(&db.pool, pid)
        .await
        .unwrap()
        .is_empty());
}

#[actix_web::test]
async fn in_progress_then_ok_with_the_same_id_is_one_check_in() {
    let db = TestDb::new().await;
    let pid = project(&db.pool).await;
    let id = Uuid::new_v4();
    let start = at("2026-01-01T05:00:00Z");

    MonitorService::process_check_in(
        &db.pool,
        pid,
        check_in(id, CheckInStatus::InProgress, Some(hourly(1))),
        start,
    )
    .await
    .unwrap();
    MonitorService::process_check_in(
        &db.pool,
        pid,
        check_in(id, CheckInStatus::Ok, None),
        start + Duration::seconds(90),
    )
    .await
    .unwrap();

    let page = MonitorService::list_checkins(&db.pool, pid, "backup", None, 1, 50)
        .await
        .unwrap();
    assert_eq!(page.1, 1);
    let only = &page.0[0];
    assert_eq!(only.id, id);
    assert_eq!(only.status, CheckInStatus::Ok);
    // No duration sent: computed from when the check-in was opened.
    assert_eq!(only.duration_ms, Some(90_000));
}

#[actix_web::test]
async fn nil_id_closes_the_latest_in_progress_check_in() {
    let db = TestDb::new().await;
    let pid = project(&db.pool).await;
    let id = Uuid::new_v4();
    let start = at("2026-01-01T05:00:00Z");

    MonitorService::process_check_in(
        &db.pool,
        pid,
        check_in(id, CheckInStatus::InProgress, Some(hourly(1))),
        start,
    )
    .await
    .unwrap();
    let mut done = check_in(Uuid::nil(), CheckInStatus::Error, None);
    done.duration = Some(12.5);
    MonitorService::process_check_in(&db.pool, pid, done, start + Duration::minutes(1))
        .await
        .unwrap();

    let (rows, total) = MonitorService::list_checkins(&db.pool, pid, "backup", None, 1, 50)
        .await
        .unwrap();
    assert_eq!(total, 1);
    assert_eq!(rows[0].id, id);
    assert_eq!(rows[0].status, CheckInStatus::Error);
    assert_eq!(rows[0].duration_ms, Some(12_500));
}

#[actix_web::test]
async fn a_finished_check_in_cannot_change_status() {
    let db = TestDb::new().await;
    let pid = project(&db.pool).await;
    let id = Uuid::new_v4();
    let start = at("2026-01-01T05:00:00Z");

    MonitorService::process_check_in(
        &db.pool,
        pid,
        check_in(id, CheckInStatus::Ok, Some(hourly(1))),
        start,
    )
    .await
    .unwrap();
    let again = MonitorService::process_check_in(
        &db.pool,
        pid,
        check_in(id, CheckInStatus::Error, None),
        start + Duration::minutes(1),
    )
    .await;

    assert!(again.is_err());
    let (rows, _) = MonitorService::list_checkins(&db.pool, pid, "backup", None, 1, 50)
        .await
        .unwrap();
    assert_eq!(rows[0].status, CheckInStatus::Ok);
}

#[actix_web::test]
async fn error_check_in_reports_a_failure_at_threshold_one() {
    let db = TestDb::new().await;
    let pid = project(&db.pool).await;
    upsert(&db.pool, pid, 1, at("2026-01-01T05:00:00Z")).await;

    let failures = MonitorService::process_check_in(
        &db.pool,
        pid,
        check_in(Uuid::new_v4(), CheckInStatus::Error, None),
        at("2026-01-01T06:00:05Z"),
    )
    .await
    .unwrap();

    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].reason, FailureReason::Error);
    assert_eq!(failures[0].monitor_slug, "backup");
    let monitor = MonitorService::get_by_slug(&db.pool, pid, "backup")
        .await
        .unwrap();
    assert_eq!(
        monitor.environments[0].status,
        MonitorEnvironmentStatus::Error
    );
}

#[actix_web::test]
async fn failures_below_the_threshold_open_no_incident() {
    let db = TestDb::new().await;
    let pid = project(&db.pool).await;
    upsert(&db.pool, pid, 2, at("2026-01-01T05:00:00Z")).await;

    let first = MonitorService::process_check_in(
        &db.pool,
        pid,
        check_in(Uuid::new_v4(), CheckInStatus::Error, None),
        at("2026-01-01T06:00:00Z"),
    )
    .await
    .unwrap();
    assert!(first.is_empty());

    let second = MonitorService::process_check_in(
        &db.pool,
        pid,
        check_in(Uuid::new_v4(), CheckInStatus::Error, None),
        at("2026-01-01T07:00:00Z"),
    )
    .await
    .unwrap();
    assert_eq!(second.len(), 1);
}

#[actix_web::test]
async fn every_failure_during_an_incident_is_reported() {
    let db = TestDb::new().await;
    let pid = project(&db.pool).await;
    upsert(&db.pool, pid, 1, at("2026-01-01T05:00:00Z")).await;

    for hour in 6..8 {
        let failures = MonitorService::process_check_in(
            &db.pool,
            pid,
            check_in(Uuid::new_v4(), CheckInStatus::Error, None),
            at(&format!("2026-01-01T0{hour}:00:00Z")),
        )
        .await
        .unwrap();
        assert_eq!(failures.len(), 1);
    }
}

#[actix_web::test]
async fn ok_after_an_incident_recovers_the_environment() {
    let db = TestDb::new().await;
    let pid = project(&db.pool).await;
    upsert(&db.pool, pid, 1, at("2026-01-01T05:00:00Z")).await;
    MonitorService::process_check_in(
        &db.pool,
        pid,
        check_in(Uuid::new_v4(), CheckInStatus::Error, None),
        at("2026-01-01T06:00:00Z"),
    )
    .await
    .unwrap();

    MonitorService::process_check_in(
        &db.pool,
        pid,
        check_in(Uuid::new_v4(), CheckInStatus::Ok, None),
        at("2026-01-01T07:00:00Z"),
    )
    .await
    .unwrap();

    let monitor = MonitorService::get_by_slug(&db.pool, pid, "backup")
        .await
        .unwrap();
    assert_eq!(monitor.environments[0].status, MonitorEnvironmentStatus::Ok);
}

#[actix_web::test]
async fn muted_monitor_records_the_incident_but_reports_nothing() {
    let db = TestDb::new().await;
    let pid = project(&db.pool).await;
    upsert(&db.pool, pid, 1, at("2026-01-01T05:00:00Z")).await;
    MonitorService::set_muted(&db.pool, pid, "backup", true)
        .await
        .unwrap();

    let failures = MonitorService::process_check_in(
        &db.pool,
        pid,
        check_in(Uuid::new_v4(), CheckInStatus::Error, None),
        at("2026-01-01T06:00:00Z"),
    )
    .await
    .unwrap();

    assert!(failures.is_empty());
    let monitor = MonitorService::get_by_slug(&db.pool, pid, "backup")
        .await
        .unwrap();
    assert_eq!(
        monitor.environments[0].status,
        MonitorEnvironmentStatus::Error
    );
}

#[actix_web::test]
async fn sweep_marks_a_missed_check_in_and_moves_the_schedule_forward() {
    let db = TestDb::new().await;
    let pid = project(&db.pool).await;
    upsert(&db.pool, pid, 1, at("2026-01-01T05:00:00Z")).await;

    // Due 06:00, latest 06:01. Nothing yet at 06:00:30.
    assert!(MonitorService::sweep(&db.pool, at("2026-01-01T06:00:30Z"))
        .await
        .unwrap()
        .is_empty());

    let failures = MonitorService::sweep(&db.pool, at("2026-01-01T06:01:00Z"))
        .await
        .unwrap();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].reason, FailureReason::Missed);

    let monitor = MonitorService::get_by_slug(&db.pool, pid, "backup")
        .await
        .unwrap();
    let env = &monitor.environments[0];
    assert_eq!(env.status, MonitorEnvironmentStatus::Error);
    assert_eq!(env.next_checkin, Some(at("2026-01-01T07:00:00Z")));
    // A synthetic check-in does not count as the job checking in.
    assert_eq!(env.last_checkin, Some(at("2026-01-01T05:00:00Z")));

    let (rows, _) = MonitorService::list_checkins(&db.pool, pid, "backup", None, 1, 50)
        .await
        .unwrap();
    let missed = rows
        .iter()
        .find(|c| c.status == CheckInStatus::Missed)
        .unwrap();
    assert_eq!(missed.date_added, at("2026-01-01T06:00:00Z"));
    assert_eq!(missed.expected_time, Some(at("2026-01-01T06:00:00Z")));

    // Running again at the same instant must not miss the same slot twice.
    assert!(MonitorService::sweep(&db.pool, at("2026-01-01T06:01:00Z"))
        .await
        .unwrap()
        .is_empty());
}

#[actix_web::test]
async fn sweep_after_downtime_records_one_miss_and_resumes_from_now() {
    let db = TestDb::new().await;
    let pid = project(&db.pool).await;
    upsert(&db.pool, pid, 1, at("2026-01-01T05:00:00Z")).await;

    let failures = MonitorService::sweep(&db.pool, at("2026-01-01T10:30:00Z"))
        .await
        .unwrap();
    assert_eq!(failures.len(), 1);

    let monitor = MonitorService::get_by_slug(&db.pool, pid, "backup")
        .await
        .unwrap();
    assert_eq!(
        monitor.environments[0].next_checkin,
        Some(at("2026-01-01T11:00:00Z"))
    );
}

#[actix_web::test]
async fn sweep_times_out_a_check_in_open_past_max_runtime() {
    let db = TestDb::new().await;
    let pid = project(&db.pool).await;
    let id = Uuid::new_v4();
    let mut config = hourly(1);
    config.max_runtime = Some(10);
    MonitorService::process_check_in(
        &db.pool,
        pid,
        check_in(id, CheckInStatus::InProgress, Some(config)),
        at("2026-01-01T05:00:20Z"),
    )
    .await
    .unwrap();

    assert!(MonitorService::sweep(&db.pool, at("2026-01-01T05:09:00Z"))
        .await
        .unwrap()
        .is_empty());
    let failures = MonitorService::sweep(&db.pool, at("2026-01-01T05:10:00Z"))
        .await
        .unwrap();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].reason, FailureReason::Timeout);

    let (rows, _) = MonitorService::list_checkins(&db.pool, pid, "backup", None, 1, 50)
        .await
        .unwrap();
    assert_eq!(rows[0].status, CheckInStatus::Timeout);

    // The late ok only records the duration; the run still timed out.
    MonitorService::process_check_in(
        &db.pool,
        pid,
        check_in(id, CheckInStatus::Ok, None),
        at("2026-01-01T05:20:20Z"),
    )
    .await
    .unwrap();
    let (rows, _) = MonitorService::list_checkins(&db.pool, pid, "backup", None, 1, 50)
        .await
        .unwrap();
    assert_eq!(rows[0].status, CheckInStatus::Timeout);
    assert_eq!(rows[0].duration_ms, Some(1_200_000));
}

#[actix_web::test]
async fn environments_are_tracked_separately() {
    let db = TestDb::new().await;
    let pid = project(&db.pool).await;
    upsert(&db.pool, pid, 1, at("2026-01-01T05:00:00Z")).await;

    let staging = CheckIn::new(
        Uuid::new_v4(),
        "backup",
        CheckInStatus::Error,
        Some("staging".to_string()),
        None,
        None,
        None,
    )
    .unwrap();
    MonitorService::process_check_in(&db.pool, pid, staging, at("2026-01-01T05:30:00Z"))
        .await
        .unwrap();

    let monitor = MonitorService::get_by_slug(&db.pool, pid, "backup")
        .await
        .unwrap();
    assert_eq!(monitor.environments.len(), 2);
    let status = |name: &str| {
        monitor
            .environments
            .iter()
            .find(|e| e.environment == name)
            .unwrap()
            .status
    };
    assert_eq!(status("production"), MonitorEnvironmentStatus::Ok);
    assert_eq!(status("staging"), MonitorEnvironmentStatus::Error);
}

#[actix_web::test]
async fn sweep_drops_check_ins_older_than_the_retention() {
    let db = TestDb::new().await;
    let pid = project(&db.pool).await;
    upsert(&db.pool, pid, 1, at("2026-01-01T05:00:00Z")).await;
    MonitorService::process_check_in(
        &db.pool,
        pid,
        check_in(Uuid::new_v4(), CheckInStatus::Ok, None),
        at("2026-03-20T05:00:00Z"),
    )
    .await
    .unwrap();

    // 2026-04-05 is 94 days after the first check-in, 16 after the second.
    MonitorService::sweep(&db.pool, at("2026-04-05T05:00:00Z"))
        .await
        .unwrap();

    let (rows, _) = MonitorService::list_checkins(&db.pool, pid, "backup", None, 1, 50)
        .await
        .unwrap();
    assert!(rows
        .iter()
        .all(|c| c.date_added > at("2026-03-01T00:00:00Z")));
    assert!(rows
        .iter()
        .any(|c| c.date_added == at("2026-03-20T05:00:00Z")));
}
