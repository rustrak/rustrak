//! The digest stores exactly the quota, the way Relay's consistent rate
//! limiter does: check and count in one step, never past the limit.

use crate::common::{null_sourcemap_provider, process_error_event, TestDb};
use chrono::Utc;
use rustrak::config::RateLimitConfig;
use rustrak::ingest::{store_event_with_metadata, EventMetadata};
use rustrak::models::{CreateProject, Project};
use rustrak::services::{ProjectService, RateLimitService};
use std::path::Path;
use uuid::Uuid;

fn project_minute_quota(limit: i64) -> RateLimitConfig {
    RateLimitConfig {
        max_events_per_minute: 100_000,
        max_events_per_hour: 100_000,
        max_events_per_project_per_minute: limit,
        max_events_per_project_per_hour: 100_000,
    }
}

async fn create_project(pool: &rustrak::db::DbPool) -> Project {
    ProjectService::create(
        pool,
        CreateProject {
            name: "Quota".into(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap()
}

/// Persists an accepted event the way ingest does and returns its metadata.
async fn accept_event(ingest_dir: &Path, project_id: i32) -> EventMetadata {
    accept_event_with_message(ingest_dir, project_id, "quota proof").await
}

async fn accept_event_with_message(
    ingest_dir: &Path,
    project_id: i32,
    message: &str,
) -> EventMetadata {
    let metadata = EventMetadata {
        event_id: Uuid::new_v4().simple().to_string(),
        project_id,
        ingested_at: Utc::now(),
        remote_addr: None,
    };
    let event = serde_json::json!({
        "event_id": metadata.event_id,
        "platform": "javascript",
        "level": "error",
        "message": message,
    });
    store_event_with_metadata(
        ingest_dir,
        &metadata.event_id,
        &serde_json::to_vec(&event).unwrap(),
        &metadata,
    )
    .await
    .unwrap();
    metadata
}

/// Quota windows are fixed minutes: a test that counts within one must not
/// straddle a rollover.
async fn wait_clear_of_minute_rollover() {
    let second = Utc::now().timestamp().rem_euclid(60);
    if second >= 55 {
        tokio::time::sleep(std::time::Duration::from_secs((61 - second) as u64)).await;
    }
}

async fn stored_events(pool: &rustrak::db::DbPool, project_id: i32) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE project_id = $1")
        .bind(project_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn digest_stores_exactly_the_project_minute_quota() {
    let db = TestDb::new().await;
    let project = create_project(&db.pool).await;
    let ingest_dir = tempfile::tempdir().unwrap();
    let quotas = project_minute_quota(10);
    wait_clear_of_minute_rollover().await;

    for _ in 0..15 {
        let metadata = accept_event(ingest_dir.path(), project.id).await;
        process_error_event(
            &db.pool,
            &metadata,
            ingest_dir.path(),
            &quotas,
            null_sourcemap_provider(),
        )
        .await
        .unwrap();
    }

    assert_eq!(stored_events(&db.pool, project.id).await, 10);
}

/// The race from rustrak/rustrak#339: digests that run at the same time must
/// not all read "room left" and all write.
#[tokio::test]
async fn concurrent_digests_never_store_past_the_quota() {
    let db = TestDb::new().await;
    let project = create_project(&db.pool).await;
    let ingest_dir = tempfile::tempdir().unwrap();
    let quotas = project_minute_quota(10);
    wait_clear_of_minute_rollover().await;

    let mut accepted = Vec::new();
    for _ in 0..25 {
        accepted.push(accept_event(ingest_dir.path(), project.id).await);
    }
    let digests = accepted.iter().map(|metadata| {
        process_error_event(
            &db.pool,
            metadata,
            ingest_dir.path(),
            &quotas,
            null_sourcemap_provider(),
        )
    });
    // A digest that lost the write lock stays pending; replay it the way the
    // recovery worker would, until nothing is left.
    let results = futures_util::future::join_all(digests).await;
    for (metadata, result) in accepted.iter().zip(results) {
        if result.is_err() {
            process_error_event(
                &db.pool,
                metadata,
                ingest_dir.path(),
                &quotas,
                null_sourcemap_provider(),
            )
            .await
            .unwrap();
        }
    }

    assert_eq!(stored_events(&db.pool, project.id).await, 10);
}

async fn digest(
    pool: &rustrak::db::DbPool,
    ingest_dir: &Path,
    project_id: i32,
    quotas: &RateLimitConfig,
) {
    let metadata = accept_event(ingest_dir, project_id).await;
    process_error_event(
        pool,
        &metadata,
        ingest_dir,
        quotas,
        null_sourcemap_provider(),
    )
    .await
    .unwrap();
}

async fn admission_rejects(
    pool: &rustrak::db::DbPool,
    project_id: i32,
    quotas: &RateLimitConfig,
) -> bool {
    let (project, installation) = ProjectService::get_with_installation_quota(pool, project_id)
        .await
        .unwrap();
    RateLimitService::check_quota(&installation, &project, quotas, Default::default()).is_some()
}

/// Ingest answers 429 from cached state; that state has to agree with the
/// counter the digest enforces, or it turns events away while there is room.
#[tokio::test]
async fn admission_closes_exactly_when_the_quota_fills() {
    let db = TestDb::new().await;
    let project = create_project(&db.pool).await;
    let ingest_dir = tempfile::tempdir().unwrap();
    let quotas = project_minute_quota(10);
    wait_clear_of_minute_rollover().await;

    for _ in 0..9 {
        digest(&db.pool, ingest_dir.path(), project.id, &quotas).await;
    }
    assert!(
        !admission_rejects(&db.pool, project.id, &quotas).await,
        "one slot is still free"
    );

    digest(&db.pool, ingest_dir.path(), project.id, &quotas).await;
    assert!(
        admission_rejects(&db.pool, project.id, &quotas).await,
        "the quota is full"
    );
}

/// A dropped event is accounted for, the way Relay records a `RateLimited`
/// outcome: the operator can see how much the quota turned away.
#[tokio::test]
async fn events_the_quota_drops_are_counted_on_the_project() {
    let db = TestDb::new().await;
    let project = create_project(&db.pool).await;
    let ingest_dir = tempfile::tempdir().unwrap();
    let quotas = project_minute_quota(10);
    wait_clear_of_minute_rollover().await;

    for _ in 0..15 {
        digest(&db.pool, ingest_dir.path(), project.id, &quotas).await;
    }

    let project = ProjectService::get_by_id(&db.pool, project.id)
        .await
        .unwrap();
    assert_eq!(project.rate_limited_event_count, 5);
}

/// Telemetry has to tell a drop from a stored event: counting it as a
/// successful digest would hide exactly the case operators hit.
#[tokio::test]
async fn a_dropped_event_is_reported_as_rate_limited_not_as_a_digest() {
    let db = TestDb::new().await;
    let project = create_project(&db.pool).await;
    let ingest_dir = tempfile::tempdir().unwrap();
    let quotas = project_minute_quota(1);
    wait_clear_of_minute_rollover().await;
    let counters: &'static rustrak::telemetry::Counters =
        Box::leak(Box::new(rustrak::telemetry::Counters::new()));
    let processors = crate::common::processors_with_config(
        &db.pool,
        &rustrak::config::Config {
            ingest_dir: ingest_dir.path().to_path_buf(),
            rate_limit: quotas,
            ..crate::common::config()
        },
    )
    .with_counters(counters);

    for _ in 0..2 {
        let metadata = accept_event(ingest_dir.path(), project.id).await;
        rustrak::ingest::pipeline::digest_stored_event(&processors, &db.pool, &metadata).await;
    }

    let digest = counters.snapshot().digest;
    assert_eq!((digest.ok, digest.rate_limited, digest.failed), (1, 1, 0));
}

/// A project's own limit, like Sentry's per-DSN rate limit, can only make its
/// quota tighter than the operator's.
#[tokio::test]
async fn a_project_limit_below_the_operators_is_the_one_enforced() {
    let db = TestDb::new().await;
    let project = create_project(&db.pool).await;
    let ingest_dir = tempfile::tempdir().unwrap();
    let quotas = project_minute_quota(10);
    wait_clear_of_minute_rollover().await;
    ProjectService::update(
        &db.pool,
        project.id,
        rustrak::models::UpdateProject {
            rate_limit_per_minute: Some(Some(3)),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    for _ in 0..5 {
        digest(&db.pool, ingest_dir.path(), project.id, &quotas).await;
    }

    assert_eq!(stored_events(&db.pool, project.id).await, 3);
    assert!(admission_rejects(&db.pool, project.id, &quotas).await);
}

#[tokio::test]
async fn a_project_limit_cannot_raise_the_operators() {
    let db = TestDb::new().await;
    let project = create_project(&db.pool).await;
    let ingest_dir = tempfile::tempdir().unwrap();
    let quotas = project_minute_quota(3);
    wait_clear_of_minute_rollover().await;
    ProjectService::update(
        &db.pool,
        project.id,
        rustrak::models::UpdateProject {
            rate_limit_per_minute: Some(Some(100)),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    for _ in 0..5 {
        digest(&db.pool, ingest_dir.path(), project.id, &quotas).await;
    }

    assert_eq!(stored_events(&db.pool, project.id).await, 3);
}

/// The digest loads the project row before its transaction. A limit lowered
/// in between must still hold: the count is checked against the limit the
/// row has now, not the one the digest read.
#[tokio::test]
async fn a_limit_lowered_during_a_digest_still_holds() {
    let db = TestDb::new().await;
    let stale = create_project(&db.pool).await;
    let quotas = project_minute_quota(10);
    wait_clear_of_minute_rollover().await;
    ProjectService::update(
        &db.pool,
        stale.id,
        rustrak::models::UpdateProject {
            rate_limit_per_minute: Some(Some(1)),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let mut conn = db.pool.acquire().await.unwrap();
    let mut consume =
        async || RateLimitService::try_consume(&mut conn, &stale, &quotas, Utc::now()).await;
    assert!(consume().await.unwrap().is_none(), "the first event fits");
    assert!(
        consume().await.unwrap().is_some(),
        "the second is past the limit set after the row was read"
    );
}

/// The quota is counted at the end of the digest transaction, so the rows
/// written before it (a new issue, its grouping) have to go with the rollback.
#[tokio::test]
async fn a_dropped_event_leaves_no_issue_behind() {
    let db = TestDb::new().await;
    let project = create_project(&db.pool).await;
    let ingest_dir = tempfile::tempdir().unwrap();
    let quotas = project_minute_quota(1);
    wait_clear_of_minute_rollover().await;

    for message in ["first error", "a different error"] {
        let metadata = accept_event_with_message(ingest_dir.path(), project.id, message).await;
        process_error_event(
            &db.pool,
            &metadata,
            ingest_dir.path(),
            &quotas,
            null_sourcemap_provider(),
        )
        .await
        .unwrap();
    }

    let issues: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM issues WHERE project_id = $1")
        .bind(project.id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(issues, 1);
}

/// The recovery worker replays pending events on its own path; a drop there
/// has to reach telemetry as well, or the count understates exactly the
/// backlog case.
#[tokio::test]
async fn a_drop_during_recovery_is_reported_as_rate_limited() {
    let db = TestDb::new().await;
    let project = create_project(&db.pool).await;
    let ingest_dir = tempfile::tempdir().unwrap();
    let quotas = project_minute_quota(1);
    wait_clear_of_minute_rollover().await;
    let counters: &'static rustrak::telemetry::Counters =
        Box::leak(Box::new(rustrak::telemetry::Counters::new()));
    let processors = std::sync::Arc::new(
        crate::common::processors_with_config(
            &db.pool,
            &rustrak::config::Config {
                ingest_dir: ingest_dir.path().to_path_buf(),
                rate_limit: quotas,
                ..crate::common::config()
            },
        )
        .with_counters(counters),
    );
    for _ in 0..2 {
        accept_event(ingest_dir.path(), project.id).await;
    }

    rustrak::ingest::pipeline::recover_pending_events_once(
        db.pool.clone(),
        processors,
        ingest_dir.path().to_path_buf(),
    )
    .await;

    assert_eq!(counters.snapshot().digest.rate_limited, 1);
}

/// (installation digested, project stored, project digested, project rate limited)
async fn event_counters(pool: &rustrak::db::DbPool, project_id: i32) -> (i64, i64, i64, i64) {
    let installation: i64 =
        sqlx::query_scalar("SELECT digested_event_count FROM installation WHERE id = 1")
            .fetch_one(pool)
            .await
            .unwrap();
    let project = ProjectService::get_by_id(pool, project_id).await.unwrap();
    (
        installation,
        project.stored_event_count as i64,
        project.digested_event_count as i64,
        project.rate_limited_event_count,
    )
}

/// Each path counts what it always has: an accepted event once everywhere, a
/// duplicate nowhere, a dropped one only as rate limited.
#[tokio::test]
async fn accepted_duplicate_and_dropped_events_count_as_before() {
    let db = TestDb::new().await;
    let project = create_project(&db.pool).await;
    let ingest_dir = tempfile::tempdir().unwrap();
    let quotas = project_minute_quota(2);
    wait_clear_of_minute_rollover().await;
    let before = event_counters(&db.pool, project.id).await;

    let first = accept_event(ingest_dir.path(), project.id).await;
    process_error_event(
        &db.pool,
        &first,
        ingest_dir.path(),
        &quotas,
        null_sourcemap_provider(),
    )
    .await
    .unwrap();
    assert_eq!(
        event_counters(&db.pool, project.id).await,
        (before.0 + 1, 1, 1, 0),
        "accepted"
    );

    // The same event again, as an SDK retry or a replay would send it.
    let event = serde_json::json!({ "event_id": first.event_id, "message": "quota proof" });
    store_event_with_metadata(
        ingest_dir.path(),
        &first.event_id,
        &serde_json::to_vec(&event).unwrap(),
        &first,
    )
    .await
    .unwrap();
    process_error_event(
        &db.pool,
        &first,
        ingest_dir.path(),
        &quotas,
        null_sourcemap_provider(),
    )
    .await
    .unwrap();
    assert_eq!(
        event_counters(&db.pool, project.id).await,
        (before.0 + 1, 1, 1, 0),
        "duplicate"
    );

    for _ in 0..2 {
        digest(&db.pool, ingest_dir.path(), project.id, &quotas).await;
    }
    assert_eq!(
        event_counters(&db.pool, project.id).await,
        (before.0 + 2, 2, 2, 1),
        "one accepted, one dropped"
    );
}

/// Digests racing for the last slots: the losers roll back inside the
/// transaction, and their counters with them.
#[tokio::test]
async fn racing_digests_count_only_the_events_they_store() {
    let db = TestDb::new().await;
    let project = create_project(&db.pool).await;
    let ingest_dir = tempfile::tempdir().unwrap();
    let quotas = project_minute_quota(10);
    wait_clear_of_minute_rollover().await;
    let before = event_counters(&db.pool, project.id).await;

    let mut accepted = Vec::new();
    for _ in 0..25 {
        accepted.push(accept_event(ingest_dir.path(), project.id).await);
    }
    let results = futures_util::future::join_all(accepted.iter().map(|metadata| {
        process_error_event(
            &db.pool,
            metadata,
            ingest_dir.path(),
            &quotas,
            null_sourcemap_provider(),
        )
    }))
    .await;
    for (metadata, result) in accepted.iter().zip(results) {
        if result.is_err() {
            process_error_event(
                &db.pool,
                metadata,
                ingest_dir.path(),
                &quotas,
                null_sourcemap_provider(),
            )
            .await
            .unwrap();
        }
    }

    assert_eq!(
        event_counters(&db.pool, project.id).await,
        (before.0 + 10, 10, 10, 15)
    );
}

/// Like Relay's `CombinedRateLimiter`, the cheap cached check runs first: an
/// event the counters already show has no room is dropped before the digest
/// takes the write lock and writes rows only to roll them back.
#[cfg(feature = "sqlite")]
#[tokio::test]
async fn a_drop_known_from_the_counters_never_reaches_the_write_path() {
    let db = TestDb::new().await;
    let project = create_project(&db.pool).await;
    let ingest_dir = tempfile::tempdir().unwrap();
    let quotas = project_minute_quota(1);
    wait_clear_of_minute_rollover().await;
    digest(&db.pool, ingest_dir.path(), project.id, &quotas).await;
    // Any write the digest attempts from here on fails.
    sqlx::query(
        "CREATE TRIGGER no_more_events BEFORE INSERT ON events \
         BEGIN SELECT RAISE(ABORT, 'the write path was taken'); END",
    )
    .execute(&db.pool)
    .await
    .unwrap();

    let metadata = accept_event(ingest_dir.path(), project.id).await;
    let result = process_error_event(
        &db.pool,
        &metadata,
        ingest_dir.path(),
        &quotas,
        null_sourcemap_provider(),
    )
    .await;

    assert!(result.is_ok(), "{result:?}");
    let project = ProjectService::get_by_id(&db.pool, project.id)
        .await
        .unwrap();
    assert_eq!(project.rate_limited_event_count, 1);
}

// --- Admission counts what is accepted but not digested yet ---------------

/// A test server over its own ingest directory, with `quotas`.
async fn ingest_state(
    pool: &rustrak::db::DbPool,
    ingest_dir: &Path,
    quotas: RateLimitConfig,
) -> rustrak::app::AppState {
    crate::common::state(
        pool,
        rustrak::config::Config {
            rate_limit: quotas,
            ..crate::common::config_with_ingest_dir(ingest_dir)
        },
    )
    .await
}

/// Takes every digest slot: accepted events wait in the spool until the
/// returned permit is dropped, as they do behind a digest that lags.
fn pause_digests(state: &rustrak::app::AppState) -> tokio::sync::OwnedSemaphorePermit {
    let slot = std::sync::Arc::clone(&state.processors.processing_slot);
    let slots = slot.available_permits() as u32;
    slot.try_acquire_many_owned(slots).unwrap()
}

fn error_envelope() -> String {
    let event_id = Uuid::new_v4().simple().to_string();
    let event = serde_json::json!({"event_id": event_id, "message": "burst"}).to_string();
    format!(
        "{{\"event_id\":\"{event_id}\"}}\n{{\"type\":\"event\",\"length\":{}}}\n{event}",
        event.len()
    )
}

fn transaction_envelope() -> String {
    let event_id = Uuid::new_v4().simple().to_string();
    let event = serde_json::json!({
        "event_id": event_id,
        "type": "transaction",
        "transaction": "GET /",
        "start_timestamp": 1704801600.0,
        "timestamp": 1704801600.5,
        "contexts": {"trace": {"trace_id": "a".repeat(32), "span_id": "b".repeat(16), "op": "http.server"}}
    })
    .to_string();
    format!(
        "{{\"event_id\":\"{event_id}\"}}\n{{\"type\":\"transaction\",\"length\":{}}}\n{event}",
        event.len()
    )
}

async fn send<S>(app: &S, project: &Project, envelope: String) -> actix_web::dev::ServiceResponse
where
    S: actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
{
    let req = actix_web::test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", project.id))
        .insert_header((
            "X-Sentry-Auth",
            format!("Sentry sentry_key={}, sentry_version=7", project.sentry_key),
        ))
        .set_payload(envelope)
        .to_request();
    actix_web::test::call_service(app, req).await
}

/// A burst past the quota, faster than the digest: the door counts what it
/// accepted and the digest has not stored yet, so it answers 429 the way
/// Sentry does and keeps every event it acknowledged.
#[actix_web::test]
async fn a_burst_past_the_quota_is_refused_at_the_door_not_dropped_after() {
    let db = TestDb::new().await;
    let project = create_project(&db.pool).await;
    let ingest_dir = tempfile::tempdir().unwrap();
    let state = ingest_state(&db.pool, ingest_dir.path(), project_minute_quota(10)).await;
    let app = crate::common::app_with(&state).await;
    wait_clear_of_minute_rollover().await;
    let paused = pause_digests(&state);

    let mut accepted = 0;
    for _ in 0..30 {
        let resp = send(&app, &project, error_envelope()).await;
        if resp.status() == 200 {
            accepted += 1;
            continue;
        }
        assert_eq!(resp.status(), 429);
        let retry_after: u64 = resp
            .headers()
            .get("retry-after")
            .unwrap()
            .to_str()
            .unwrap()
            .parse()
            .unwrap();
        assert!((1..=60).contains(&retry_after));
        assert_eq!(
            resp.headers()
                .get("x-sentry-rate-limits")
                .unwrap()
                .to_str()
                .unwrap(),
            format!("{retry_after}::project")
        );
        let body: serde_json::Value = actix_web::test::read_body_json(resp).await;
        assert_eq!(body["error"], "rate_limit_exceeded");
    }
    assert_eq!(accepted, 10, "the door admits exactly the quota");

    drop(paused);
    crate::common::eventually(|| async {
        (!crate::common::has_files(ingest_dir.path())).then_some(())
    })
    .await;
    let (_, stored, _, rate_limited) = event_counters(&db.pool, project.id).await;
    assert_eq!(
        (stored, rate_limited),
        (10, 0),
        "every acknowledged event is kept"
    );
    assert_eq!(stored_events(&db.pool, project.id).await, 10);
    assert_eq!(
        state.processors.errors.pending().counts(project.id),
        Default::default(),
        "nothing is left pending once the digest drains"
    );
}

/// The installation's quota is Relay's organization scope and counts pending
/// events from every project.
#[actix_web::test]
async fn pending_events_of_every_project_count_against_the_installation() {
    let db = TestDb::new().await;
    let first = create_project(&db.pool).await;
    let second = crate::common::create_test_project(&db.pool, "Second").await;
    let ingest_dir = tempfile::tempdir().unwrap();
    let quotas = RateLimitConfig {
        max_events_per_minute: 100_000,
        max_events_per_hour: 3,
        ..project_minute_quota(100_000)
    };
    let state = ingest_state(&db.pool, ingest_dir.path(), quotas).await;
    let app = crate::common::app_with(&state).await;
    let _paused = pause_digests(&state);

    for _ in 0..2 {
        assert_eq!(send(&app, &first, error_envelope()).await.status(), 200);
    }
    assert_eq!(send(&app, &second, error_envelope()).await.status(), 200);
    let resp = send(&app, &second, error_envelope()).await;
    assert_eq!(resp.status(), 429);
    assert!(resp
        .headers()
        .get("x-sentry-rate-limits")
        .unwrap()
        .to_str()
        .unwrap()
        .ends_with("::organization"));
}

/// Transactions are stored inline and never count against this quota, so
/// they never take a slot an error event would need.
#[actix_web::test]
async fn items_outside_the_quota_take_no_pending_slot() {
    let db = TestDb::new().await;
    let project = create_project(&db.pool).await;
    let ingest_dir = tempfile::tempdir().unwrap();
    let state = ingest_state(&db.pool, ingest_dir.path(), project_minute_quota(2)).await;
    let app = crate::common::app_with(&state).await;
    wait_clear_of_minute_rollover().await;
    let _paused = pause_digests(&state);

    for _ in 0..5 {
        assert_eq!(
            send(&app, &project, transaction_envelope()).await.status(),
            200
        );
    }
    for _ in 0..2 {
        assert_eq!(send(&app, &project, error_envelope()).await.status(), 200);
    }
    assert_eq!(send(&app, &project, error_envelope()).await.status(), 429);
}

/// A restart with a backlog in the spool: the new process counts those
/// events before it answers anyone, or it would accept a full quota on top of
/// a backlog the digest will then have no room for.
#[actix_web::test]
async fn a_backlog_left_by_a_previous_process_counts_at_startup() {
    let db = TestDb::new().await;
    let project = create_project(&db.pool).await;
    let ingest_dir = tempfile::tempdir().unwrap();
    for _ in 0..3 {
        accept_event(ingest_dir.path(), project.id).await;
    }
    let state = ingest_state(&db.pool, ingest_dir.path(), project_minute_quota(3)).await;
    let app = crate::common::app_with(&state).await;

    assert_eq!(
        state.processors.errors.pending().counts(project.id),
        rustrak::services::Pending {
            installation: 3,
            project: 3
        }
    );
    assert_eq!(send(&app, &project, error_envelope()).await.status(), 429);

    // Replayed by the recovery worker: the backlog stops counting as pending
    // once each event is digested, here into the quota it was counted against.
    rustrak::ingest::pipeline::recover_pending_events_once(
        db.pool.clone(),
        state.processors.clone().into_inner(),
        ingest_dir.path().to_path_buf(),
    )
    .await;
    assert_eq!(
        state.processors.errors.pending().counts(project.id),
        Default::default()
    );
    let (_, stored, _, rate_limited) = event_counters(&db.pool, project.id).await;
    assert_eq!((stored, rate_limited), (3, 0));
}

/// A digest that fails in a way worth replaying keeps its file, and the
/// event keeps its pending slot until the replay finishes it.
#[cfg(feature = "sqlite")]
#[tokio::test]
async fn a_retained_event_stays_pending_until_its_replay() {
    let db = TestDb::new().await;
    let project = create_project(&db.pool).await;
    let ingest_dir = tempfile::tempdir().unwrap();
    let processors = crate::common::processors(&db.pool, ingest_dir.path());
    let metadata = accept_event(ingest_dir.path(), project.id).await;
    let event_id = Uuid::parse_str(&metadata.event_id).unwrap();
    processors.errors.pending().insert(project.id, event_id);
    // A write the digest cannot finish, the way a locked database fails it.
    sqlx::query(
        "CREATE TRIGGER busy BEFORE INSERT ON events \
         BEGIN SELECT RAISE(ABORT, 'database is locked'); END",
    )
    .execute(&db.pool)
    .await
    .unwrap();

    let result = processors.errors.digest(&db.pool, &metadata).await;
    assert!(result.is_err());
    assert!(
        crate::common::has_files(ingest_dir.path()),
        "retained for replay"
    );
    assert_eq!(processors.errors.pending().counts(project.id).project, 1);

    sqlx::query("DROP TRIGGER busy")
        .execute(&db.pool)
        .await
        .unwrap();
    processors.errors.digest(&db.pool, &metadata).await.unwrap();
    assert_eq!(
        processors.errors.pending().counts(project.id),
        Default::default()
    );
}

// --- One batch against the quota --------------------------------------------

/// The pipeline over `ingest_dir`, with `quotas`.
fn batch_processors(
    pool: &rustrak::db::DbPool,
    ingest_dir: &Path,
    quotas: RateLimitConfig,
) -> std::sync::Arc<rustrak::digest::processors::Processors> {
    std::sync::Arc::new(crate::common::processors_with_config(
        pool,
        &rustrak::config::Config {
            rate_limit: quotas,
            ..crate::common::config_with_ingest_dir(ingest_dir)
        },
    ))
}

/// Digests the accepted events in one batch, in this order.
async fn digest_as_one_batch(
    processors: &std::sync::Arc<rustrak::digest::processors::Processors>,
    pool: &rustrak::db::DbPool,
    accepted: &[EventMetadata],
) {
    let refs = accepted
        .iter()
        .map(|m| {
            let event_id = Uuid::parse_str(&m.event_id).unwrap();
            rustrak::digest::processors::SpoolRef {
                project_id: m.project_id,
                event_id,
                size: 0,
                owner: processors
                    .errors
                    .in_flight()
                    .register(m.project_id, event_id),
            }
        })
        .collect();
    for result in rustrak::ingest::pipeline::digest_refs(processors, pool, refs).await {
        result.unwrap();
    }
}

async fn stored_ids(pool: &rustrak::db::DbPool) -> std::collections::HashSet<Uuid> {
    sqlx::query_scalar("SELECT event_id FROM events")
        .fetch_all(pool)
        .await
        .unwrap()
        .into_iter()
        .collect()
}

/// A batch decides its events in order, as one digest each would have: the
/// first ones with room are stored, every later one is dropped, counted as
/// rate limited, and leaves no issue behind.
#[tokio::test]
async fn a_batch_past_the_quota_drops_the_later_events() {
    let db = TestDb::new().await;
    let project = create_project(&db.pool).await;
    let ingest_dir = tempfile::tempdir().unwrap();
    let quotas = project_minute_quota(10);
    wait_clear_of_minute_rollover().await;
    let before = event_counters(&db.pool, project.id).await;
    let mut accepted = Vec::new();
    for i in 0..25 {
        accepted.push(
            accept_event_with_message(
                ingest_dir.path(),
                project.id,
                &format!("error {}", "x".repeat(i + 1)),
            )
            .await,
        );
    }

    let processors = batch_processors(&db.pool, ingest_dir.path(), quotas);
    digest_as_one_batch(&processors, &db.pool, &accepted).await;

    let first: std::collections::HashSet<Uuid> = accepted[..10]
        .iter()
        .map(|m| Uuid::parse_str(&m.event_id).unwrap())
        .collect();
    assert_eq!(stored_ids(&db.pool).await, first);
    assert_eq!(
        event_counters(&db.pool, project.id).await,
        (before.0 + 10, 10, 10, 15)
    );
    let issues: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM issues WHERE project_id = $1")
        .bind(project.id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(issues, 10, "no issue for a dropped event");
    crate::common::eventually(|| async {
        (!crate::common::has_files(ingest_dir.path())).then_some(())
    })
    .await;
}

/// The installation's quota is shared: two projects interleaved in one batch
/// get the first slots in batch order, whichever project they belong to.
#[tokio::test]
async fn a_batch_past_the_installation_quota_admits_the_first_in_order() {
    let db = TestDb::new().await;
    let a = create_project(&db.pool).await;
    let b = crate::common::create_test_project(&db.pool, "Quota B").await;
    let ingest_dir = tempfile::tempdir().unwrap();
    let quotas = RateLimitConfig {
        max_events_per_minute: 5,
        ..project_minute_quota(100)
    };
    wait_clear_of_minute_rollover().await;
    let mut accepted = Vec::new();
    for i in 0..10 {
        let project = if i % 2 == 0 { &a } else { &b };
        accepted.push(
            accept_event_with_message(
                ingest_dir.path(),
                project.id,
                &format!("error {}", "x".repeat(i + 1)),
            )
            .await,
        );
    }

    let processors = batch_processors(&db.pool, ingest_dir.path(), quotas);
    digest_as_one_batch(&processors, &db.pool, &accepted).await;

    let first: std::collections::HashSet<Uuid> = accepted[..5]
        .iter()
        .map(|m| Uuid::parse_str(&m.event_id).unwrap())
        .collect();
    assert_eq!(stored_ids(&db.pool).await, first);
}
