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
    let project = ProjectService::get_by_id(pool, project_id).await.unwrap();
    RateLimitService::check_quota(pool, &project, quotas)
        .await
        .unwrap()
        .is_some()
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
    let processors = rustrak::digest::processors::Processors::new(
        ingest_dir.path().to_path_buf(),
        quotas,
        null_sourcemap_provider(),
        None,
    )
    .with_counters(counters);

    for _ in 0..2 {
        let metadata = accept_event(ingest_dir.path(), project.id).await;
        rustrak::routes::ingest::digest_stored_event(&processors, &db.pool, &metadata).await;
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
    let processors = actix_web::web::Data::new(
        rustrak::digest::processors::Processors::new(
            ingest_dir.path().to_path_buf(),
            quotas,
            null_sourcemap_provider(),
            None,
        )
        .with_counters(counters),
    );
    for _ in 0..2 {
        accept_event(ingest_dir.path(), project.id).await;
    }

    rustrak::routes::ingest::recover_pending_events_once(
        db.pool.clone(),
        processors,
        ingest_dir.path().to_path_buf(),
    )
    .await;

    assert_eq!(counters.snapshot().digest.rate_limited, 1);
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
