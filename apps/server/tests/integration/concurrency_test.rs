//! Concurrency tests for the Digest process
//!
//! Tests that verify the advisory lock mechanism properly handles concurrent
//! event processing without race conditions.

use crate::common::create_test_project;
use crate::common::process_error_event;
use crate::common::TestDb;
use chrono::Utc;
use rustrak::config::RateLimitConfig;
#[cfg(feature = "sqlite")]
use rustrak::ingest::read_event;
use rustrak::ingest::EventMetadata;
use rustrak::pagination::{IssueSort, SortOrder};
use rustrak::services::IssueService;
use serde_json::json;
use std::collections::HashSet;
use std::sync::Arc;
use tempfile::TempDir;
use uuid::Uuid;

fn create_rate_limit_config() -> RateLimitConfig {
    RateLimitConfig {
        max_events_per_minute: 10000,
        max_events_per_hour: 100000,
        max_events_per_project_per_minute: 5000,
        max_events_per_project_per_hour: 50000,
    }
}

fn create_unique_event_json(error_type: &str, error_msg: &str) -> (String, serde_json::Value) {
    let event_id = Uuid::new_v4().to_string().replace("-", "");
    let event_json = json!({
        "event_id": &event_id,
        "timestamp": Utc::now().timestamp() as f64,
        "platform": "rust",
        "level": "error",
        "exception": {
            "values": [{
                "type": error_type,
                "value": error_msg
            }]
        }
    });
    (event_id, event_json)
}

/// Test that concurrent events with DIFFERENT error types create separate issues
/// with consecutive digest_order values (no gaps, no duplicates)
#[actix_web::test]
async fn test_concurrent_different_errors_same_project_creates_sequential_issues() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Concurrent Different Errors").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let ingest_dir = temp_dir.path().to_path_buf();
    let rate_limit_config = Arc::new(create_rate_limit_config());
    let pool = Arc::new(db.pool.clone());

    // Create 10 events with different error types (will create 10 issues)
    let num_events = 10;
    let mut handles = Vec::new();

    for i in 0..num_events {
        let pool_clone = Arc::clone(&pool);
        let ingest_dir_clone = ingest_dir.clone();
        let rate_limit_config_clone = Arc::clone(&rate_limit_config);
        let project_id = project.id;

        let handle = tokio::spawn(async move {
            let (event_id, event_json) =
                create_unique_event_json(&format!("Error{}", i), &format!("Message {}", i));
            let event_bytes = serde_json::to_vec(&event_json).unwrap();

            crate::common::store_event(&ingest_dir_clone, project_id, &event_id, &event_bytes)
                .await
                .expect("Failed to store event");

            let metadata = EventMetadata {
                event_id: event_id.clone(),
                project_id,
                ingested_at: Utc::now(),
                remote_addr: None,
            };

            process_error_event(
                &pool_clone,
                &metadata,
                &ingest_dir_clone,
                &rate_limit_config_clone,
                crate::common::null_sourcemap_provider(),
            )
            .await
            .expect("Failed to process event");
        });

        handles.push(handle);
    }

    // Wait for all concurrent tasks to complete
    for handle in handles {
        handle.await.expect("Task panicked");
    }

    // Verify we have exactly 10 issues
    let (issues, _) = IssueService::list_offset(
        &db.pool,
        project.id,
        IssueSort::DigestOrder,
        SortOrder::Asc,
        rustrak::pagination::IssueFilter::All,
        rustrak::pagination::IssueCategory::Error,
        1,
        100,
        None,
        rustrak::services::scope::Scope::default(),
        &rustrak::services::issue::IssueFilters::default(),
    )
    .await
    .expect("Failed to list issues");

    assert_eq!(
        issues.len(),
        num_events,
        "Expected {} issues, got {}",
        num_events,
        issues.len()
    );

    // Verify digest_order values are consecutive (1, 2, 3, ..., 10) with no gaps
    let digest_orders: Vec<i32> = issues.iter().map(|i| i.digest_order).collect();
    let expected_orders: Vec<i32> = (1..=num_events as i32).collect();

    // Check that we have all expected values (order doesn't matter due to concurrency)
    let digest_orders_set: HashSet<i32> = digest_orders.iter().cloned().collect();
    let expected_set: HashSet<i32> = expected_orders.iter().cloned().collect();

    assert_eq!(
        digest_orders_set, expected_set,
        "digest_order values should be consecutive 1-{} without gaps. Got: {:?}",
        num_events, digest_orders
    );
}

/// Test that concurrent events with SAME error type are grouped into one issue
#[actix_web::test]
async fn test_concurrent_same_errors_same_project_groups_into_one_issue() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Concurrent Same Errors").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let ingest_dir = temp_dir.path().to_path_buf();
    let rate_limit_config = Arc::new(create_rate_limit_config());
    let pool = Arc::new(db.pool.clone());

    // Create 10 events with SAME error type (should all be grouped into 1 issue)
    let num_events = 10;
    let mut handles = Vec::new();

    for _i in 0..num_events {
        let pool_clone = Arc::clone(&pool);
        let ingest_dir_clone = ingest_dir.clone();
        let rate_limit_config_clone = Arc::clone(&rate_limit_config);
        let project_id = project.id;

        let handle = tokio::spawn(async move {
            let (event_id, event_json) =
                create_unique_event_json("SameError", "Same message for grouping");
            let event_bytes = serde_json::to_vec(&event_json).unwrap();

            crate::common::store_event(&ingest_dir_clone, project_id, &event_id, &event_bytes)
                .await
                .expect("Failed to store event");

            let metadata = EventMetadata {
                event_id: event_id.clone(),
                project_id,
                ingested_at: Utc::now(),
                remote_addr: None,
            };

            process_error_event(
                &pool_clone,
                &metadata,
                &ingest_dir_clone,
                &rate_limit_config_clone,
                crate::common::null_sourcemap_provider(),
            )
            .await
            .expect("Failed to process event");
        });

        handles.push(handle);
    }

    // Wait for all concurrent tasks to complete
    for handle in handles {
        handle.await.expect("Task panicked");
    }

    // Verify we have exactly 1 issue with 10 events
    let (issues, _) = IssueService::list_offset(
        &db.pool,
        project.id,
        IssueSort::DigestOrder,
        SortOrder::Asc,
        rustrak::pagination::IssueFilter::All,
        rustrak::pagination::IssueCategory::Error,
        1,
        100,
        None,
        rustrak::services::scope::Scope::default(),
        &rustrak::services::issue::IssueFilters::default(),
    )
    .await
    .expect("Failed to list issues");

    assert_eq!(issues.len(), 1, "Expected 1 issue, got {}", issues.len());
    assert_eq!(
        issues[0].digested_event_count, num_events,
        "Expected {} events in issue, got {}",
        num_events, issues[0].digested_event_count
    );
}

/// Test that concurrent events in DIFFERENT projects can be processed in parallel
/// (advisory locks are per-project, so they shouldn't block each other)
#[actix_web::test]
async fn test_concurrent_different_projects_process_in_parallel() {
    let db = TestDb::new().await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let ingest_dir = temp_dir.path().to_path_buf();
    let rate_limit_config = Arc::new(create_rate_limit_config());
    let pool = Arc::new(db.pool.clone());

    let num_projects = 5;
    let mut projects = Vec::new();
    for i in 0..num_projects {
        let project = create_test_project(&db.pool, &format!("Project {}", i)).await;
        projects.push(project);
    }

    // Create 5 events per project (25 total), all with different error types
    let events_per_project = 5;
    let mut handles = Vec::new();

    for project in &projects {
        for j in 0..events_per_project {
            let pool_clone = Arc::clone(&pool);
            let ingest_dir_clone = ingest_dir.clone();
            let rate_limit_config_clone = Arc::clone(&rate_limit_config);
            let project_id = project.id;

            let handle = tokio::spawn(async move {
                let (event_id, event_json) = create_unique_event_json(
                    &format!("Error_P{}_E{}", project_id, j),
                    &format!("Message {} {}", project_id, j),
                );
                let event_bytes = serde_json::to_vec(&event_json).unwrap();

                crate::common::store_event(&ingest_dir_clone, project_id, &event_id, &event_bytes)
                    .await
                    .expect("Failed to store event");

                let metadata = EventMetadata {
                    event_id: event_id.clone(),
                    project_id,
                    ingested_at: Utc::now(),
                    remote_addr: None,
                };

                process_error_event(
                    &pool_clone,
                    &metadata,
                    &ingest_dir_clone,
                    &rate_limit_config_clone,
                    crate::common::null_sourcemap_provider(),
                )
                .await
                .expect("Failed to process event");
            });

            handles.push(handle);
        }
    }

    // Wait for all concurrent tasks to complete
    for handle in handles {
        handle.await.expect("Task panicked");
    }

    // Verify each project has exactly 5 issues with consecutive digest_order
    for project in &projects {
        let (issues, _) = IssueService::list_offset(
            &db.pool,
            project.id,
            IssueSort::DigestOrder,
            SortOrder::Asc,
            rustrak::pagination::IssueFilter::All,
            rustrak::pagination::IssueCategory::Error,
            1,
            100,
            None,
            rustrak::services::scope::Scope::default(),
            &rustrak::services::issue::IssueFilters::default(),
        )
        .await
        .expect("Failed to list issues");

        assert_eq!(
            issues.len(),
            events_per_project,
            "Project {} should have {} issues, got {}",
            project.id,
            events_per_project,
            issues.len()
        );

        let digest_orders: HashSet<i32> = issues.iter().map(|i| i.digest_order).collect();
        let expected: HashSet<i32> = (1..=events_per_project as i32).collect();

        assert_eq!(
            digest_orders, expected,
            "Project {} digest_orders should be 1-{}, got {:?}",
            project.id, events_per_project, digest_orders
        );
    }
}

/// Stress test with high concurrency to ensure no race conditions
#[actix_web::test]
async fn test_high_concurrency_stress_test() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Stress Test Project").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let ingest_dir = temp_dir.path().to_path_buf();
    let rate_limit_config = Arc::new(create_rate_limit_config());
    let pool = Arc::new(db.pool.clone());

    let num_events = 50;
    let mut handles = Vec::new();

    for i in 0..num_events {
        let pool_clone = Arc::clone(&pool);
        let ingest_dir_clone = ingest_dir.clone();
        let rate_limit_config_clone = Arc::clone(&rate_limit_config);
        let project_id = project.id;

        let handle = tokio::spawn(async move {
            let (event_id, event_json) = create_unique_event_json(
                &format!("StressError{}", i),
                &format!("Stress msg {}", i),
            );
            let event_bytes = serde_json::to_vec(&event_json).unwrap();

            crate::common::store_event(&ingest_dir_clone, project_id, &event_id, &event_bytes)
                .await
                .expect("Failed to store event");

            let metadata = EventMetadata {
                event_id: event_id.clone(),
                project_id,
                ingested_at: Utc::now(),
                remote_addr: None,
            };

            process_error_event(
                &pool_clone,
                &metadata,
                &ingest_dir_clone,
                &rate_limit_config_clone,
                crate::common::null_sourcemap_provider(),
            )
            .await
            .expect("Failed to process event");
        });

        handles.push(handle);
    }

    // Wait for all concurrent tasks to complete
    for handle in handles {
        handle.await.expect("Task panicked");
    }

    // Verify we have exactly 50 issues
    let (issues, _) = IssueService::list_offset(
        &db.pool,
        project.id,
        IssueSort::DigestOrder,
        SortOrder::Asc,
        rustrak::pagination::IssueFilter::All,
        rustrak::pagination::IssueCategory::Error,
        1,
        100,
        None,
        rustrak::services::scope::Scope::default(),
        &rustrak::services::issue::IssueFilters::default(),
    )
    .await
    .expect("Failed to list issues");

    assert_eq!(
        issues.len(),
        num_events,
        "Expected {} issues, got {}",
        num_events,
        issues.len()
    );

    // Verify NO GAPS in digest_order (all values 1 through 50 must exist)
    let digest_orders: HashSet<i32> = issues.iter().map(|i| i.digest_order).collect();
    let expected: HashSet<i32> = (1..=num_events as i32).collect();

    assert_eq!(
        digest_orders, expected,
        "digest_order should be consecutive 1-{} with NO GAPS. Got: {:?}",
        num_events, digest_orders
    );

    // Verify NO DUPLICATES (set size should equal vector size)
    let digest_orders_vec: Vec<i32> = issues.iter().map(|i| i.digest_order).collect();
    assert_eq!(
        digest_orders_vec.len(),
        digest_orders.len(),
        "There should be no duplicate digest_order values"
    );
}

/// Reproduces #131: with SQLite in file mode and `max_connections > 1`
/// (the self-hosted production config), concurrent digests must not fail
/// with "database is locked" nor drop events. Drives the real production
/// pool — WAL + busy_timeout alone are not enough: the read-then-write
/// digest needs `BEGIN IMMEDIATE`.
#[cfg(feature = "sqlite")]
#[actix_web::test]
async fn test_concurrent_digests_sqlite_file_mode_no_lock_errors_or_loss() {
    use rustrak::config::DatabaseConfig;
    use std::time::Duration;

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("rustrak_concurrency.db");
    let config = DatabaseConfig {
        url: format!("sqlite://{}?mode=rwc", db_path.display()),
        max_connections: 5,
        min_connections: 1,
        acquire_timeout: Duration::from_secs(10),
        idle_timeout: Duration::from_secs(60),
        max_lifetime: Duration::from_secs(300),
    };
    let pool = rustrak::db::create_pool(&config)
        .await
        .expect("Failed to create file-mode SQLite pool");
    rustrak::db::run_migrations(&pool)
        .await
        .expect("Failed to run migrations");

    let project = create_test_project(&pool, "Concurrent File Mode").await;
    let ingest_dir = temp_dir.path().to_path_buf();
    let rate_limit_config = Arc::new(create_rate_limit_config());
    let pool = Arc::new(pool);

    // 50 concurrent events with distinct error types (50 new issues).
    let num_events = 50;
    let mut handles = Vec::new();
    for i in 0..num_events {
        let pool_clone = Arc::clone(&pool);
        let ingest_dir_clone = ingest_dir.clone();
        let rate_limit_config_clone = Arc::clone(&rate_limit_config);
        let project_id = project.id;

        let handle = tokio::spawn(async move {
            let (event_id, event_json) =
                create_unique_event_json(&format!("FileError{}", i), &format!("Msg {}", i));
            let event_bytes = serde_json::to_vec(&event_json).unwrap();

            crate::common::store_event(&ingest_dir_clone, project_id, &event_id, &event_bytes)
                .await
                .expect("Failed to store event");

            let metadata = EventMetadata {
                event_id,
                project_id,
                ingested_at: Utc::now(),
                remote_addr: None,
            };

            // Return the Result instead of unwrapping so we can assert no loss.
            process_error_event(
                &pool_clone,
                &metadata,
                &ingest_dir_clone,
                &rate_limit_config_clone,
                crate::common::null_sourcemap_provider(),
            )
            .await
        });
        handles.push(handle);
    }

    let mut failures = Vec::new();
    for handle in handles {
        if let Err(e) = handle.await.expect("Task panicked") {
            failures.push(format!("{:?}", e));
        }
    }

    assert!(
        failures.is_empty(),
        "Concurrent digests must not fail under SQLite file mode (events would be \
         dropped). {} of {} failed: {:#?}",
        failures.len(),
        num_events,
        failures
    );

    // No event loss: all 50 issues exist with sequential digest_order 1..=50.
    let (issues, _) = IssueService::list_offset(
        &pool,
        project.id,
        IssueSort::DigestOrder,
        SortOrder::Asc,
        rustrak::pagination::IssueFilter::All,
        rustrak::pagination::IssueCategory::Error,
        1,
        100,
        None,
        rustrak::services::scope::Scope::default(),
        &rustrak::services::issue::IssueFilters::default(),
    )
    .await
    .expect("Failed to list issues");

    assert_eq!(
        issues.len(),
        num_events,
        "Expected {} issues, got {} (events were dropped)",
        num_events,
        issues.len()
    );

    let digest_orders: HashSet<i32> = issues.iter().map(|i| i.digest_order).collect();
    let expected: HashSet<i32> = (1..=num_events as i32).collect();
    assert_eq!(
        digest_orders, expected,
        "digest_order must be 1-{} with no gaps/duplicates",
        num_events
    );
}

/// Test mixed operations: some events create new issues, others update existing ones
#[actix_web::test]
async fn test_concurrent_mixed_create_and_update() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Mixed Operations Project").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let ingest_dir = temp_dir.path().to_path_buf();
    let rate_limit_config = Arc::new(create_rate_limit_config());
    let pool = Arc::new(db.pool.clone());

    // Create events:
    // - 5 unique error types (create 5 new issues)
    // - 3 copies of each error type (update existing issues)
    // Total: 20 events, 5 issues, 4 events per issue
    let error_types = vec!["ErrorA", "ErrorB", "ErrorC", "ErrorD", "ErrorE"];
    let copies_per_error = 4;
    let mut handles = Vec::new();

    for error_type in &error_types {
        for _copy in 0..copies_per_error {
            let pool_clone = Arc::clone(&pool);
            let ingest_dir_clone = ingest_dir.clone();
            let rate_limit_config_clone = Arc::clone(&rate_limit_config);
            let project_id = project.id;
            let error_type = error_type.to_string();

            let handle = tokio::spawn(async move {
                let (event_id, event_json) =
                    create_unique_event_json(&error_type, "Same message for grouping");
                let event_bytes = serde_json::to_vec(&event_json).unwrap();

                crate::common::store_event(&ingest_dir_clone, project_id, &event_id, &event_bytes)
                    .await
                    .expect("Failed to store event");

                let metadata = EventMetadata {
                    event_id: event_id.clone(),
                    project_id,
                    ingested_at: Utc::now(),
                    remote_addr: None,
                };

                process_error_event(
                    &pool_clone,
                    &metadata,
                    &ingest_dir_clone,
                    &rate_limit_config_clone,
                    crate::common::null_sourcemap_provider(),
                )
                .await
                .expect("Failed to process event");
            });

            handles.push(handle);
        }
    }

    // Wait for all concurrent tasks to complete
    for handle in handles {
        handle.await.expect("Task panicked");
    }

    // Verify we have exactly 5 issues
    let (issues, _) = IssueService::list_offset(
        &db.pool,
        project.id,
        IssueSort::DigestOrder,
        SortOrder::Asc,
        rustrak::pagination::IssueFilter::All,
        rustrak::pagination::IssueCategory::Error,
        1,
        100,
        None,
        rustrak::services::scope::Scope::default(),
        &rustrak::services::issue::IssueFilters::default(),
    )
    .await
    .expect("Failed to list issues");

    assert_eq!(issues.len(), 5, "Expected 5 issues, got {}", issues.len());

    for issue in &issues {
        assert_eq!(
            issue.digested_event_count, copies_per_error,
            "Each issue should have {} events, issue {} has {}",
            copies_per_error, issue.id, issue.digested_event_count
        );
    }

    let digest_orders: HashSet<i32> = issues.iter().map(|i| i.digest_order).collect();
    let expected: HashSet<i32> = (1..=5).collect();
    assert_eq!(
        digest_orders, expected,
        "digest_order should be 1-5, got {:?}",
        digest_orders
    );
}

/// A holder that outlasts the write lock's budget fails the synchronous
/// digest, which makes one attempt, with the busy error (not a loop), keeps
/// the file for a replay and leaves no partial issue behind.
#[cfg(feature = "sqlite")]
#[actix_web::test]
async fn test_digest_fails_after_exhausting_sqlite_write_retries() {
    use sqlx::sqlite::{
        SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous,
    };
    use std::str::FromStr;
    use std::time::Duration;

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("rustrak_lock_holder_fail.db");

    let opts = SqliteConnectOptions::from_str(&format!("sqlite://{}?mode=rwc", db_path.display()))
        .expect("valid sqlite url")
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_millis(200));
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .min_connections(1)
        .connect_with(opts)
        .await
        .expect("Failed to create file-mode SQLite pool");
    rustrak::db::run_migrations(&pool)
        .await
        .expect("Failed to run migrations");

    let project = create_test_project(&pool, "Lock Holder Fail").await;
    let ingest_dir = temp_dir.path().to_path_buf();
    let rate_limit_config = Arc::new(create_rate_limit_config());
    let pool = Arc::new(pool);

    let (lock_held_tx, lock_held_rx) = tokio::sync::oneshot::channel();
    let holder_pool = pool.clone();
    let holder = tokio::spawn(async move {
        let tx = rustrak::db::begin_write(&holder_pool)
            .await
            .expect("holder must take the write lock");
        let _ = lock_held_tx.send(());
        tokio::time::sleep(Duration::from_millis(2000)).await;
        tx.commit().await.expect("holder commit must succeed");
    });

    lock_held_rx.await.expect("holder must signal");

    // One digest whose three attempts (~750ms total) all land inside the
    // 2000ms lock-hold: it must error out, not hang or loop forever.
    let (event_id, event_json) = create_unique_event_json("HoldFailError", "Msg");
    let event_bytes = serde_json::to_vec(&event_json).unwrap();
    crate::common::store_event(&ingest_dir, project.id, &event_id, &event_bytes)
        .await
        .expect("Failed to store event");
    let metadata = EventMetadata {
        event_id,
        project_id: project.id,
        ingested_at: Utc::now(),
        remote_addr: None,
    };
    let result = process_error_event(
        &pool,
        &metadata,
        &ingest_dir,
        &rate_limit_config,
        crate::common::null_sourcemap_provider(),
    )
    .await;

    holder.await.expect("holder task panicked");

    assert!(
        result.is_err(),
        "exhausted retries must fail, not drop the event silently"
    );
    assert!(
        read_event(&ingest_dir, metadata.project_id, &metadata.event_id)
            .await
            .unwrap()
            .is_some(),
        "a retryable failure must retain the durable event file"
    );

    // The rolled-back transaction left no partial issue/grouping behind.
    let (issues, _) = IssueService::list_offset(
        &pool,
        project.id,
        IssueSort::DigestOrder,
        SortOrder::Asc,
        rustrak::pagination::IssueFilter::All,
        rustrak::pagination::IssueCategory::Error,
        1,
        100,
        None,
        rustrak::services::scope::Scope::default(),
        &rustrak::services::issue::IssueFilters::default(),
    )
    .await
    .expect("Failed to list issues");
    assert!(
        issues.is_empty(),
        "a failed digest must not leave partial rows"
    );
}

/// The production shape: one shared `ErrorProcessor` driving many concurrent
/// events — guards against deadlock/livelock; every digest must still land
/// its issue exactly once.
#[cfg(feature = "sqlite")]
#[actix_web::test]
async fn test_shared_processor_digests_concurrent_events() {
    use sqlx::sqlite::{
        SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous,
    };
    use std::str::FromStr;
    use std::time::Duration;

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("rustrak_shared_processor.db");

    let opts = SqliteConnectOptions::from_str(&format!("sqlite://{}?mode=rwc", db_path.display()))
        .expect("valid sqlite url")
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_millis(200));
    let pool = Arc::new(
        SqlitePoolOptions::new()
            .max_connections(10)
            .min_connections(1)
            .connect_with(opts)
            .await
            .expect("Failed to create file-mode SQLite pool"),
    );
    rustrak::db::run_migrations(&pool)
        .await
        .expect("Failed to run migrations");

    let project = create_test_project(&pool, "Shared Processor").await;
    let ingest_dir = temp_dir.path().to_path_buf();
    let rate_limit_config = Arc::new(create_rate_limit_config());

    // One processor shared by every event — the production shape.
    let processor = Arc::new(rustrak::digest::processors::ErrorProcessor::new(
        &rustrak::config::Config {
            ingest_dir: ingest_dir.clone(),
            rate_limit: (*rate_limit_config).clone(),
            ..crate::common::config()
        },
        crate::common::null_sourcemap_provider(),
    ));

    let num_events = 10;
    let mut handles = Vec::new();
    for i in 0..num_events {
        let pool = pool.clone();
        let ingest_dir = ingest_dir.clone();
        let processor = processor.clone();
        let project_id = project.id;

        let handle = tokio::spawn(async move {
            let (event_id, event_json) =
                create_unique_event_json(&format!("Shared{}", i), &format!("Msg {}", i));
            let event_bytes = serde_json::to_vec(&event_json).unwrap();
            crate::common::store_event(&ingest_dir, project_id, &event_id, &event_bytes)
                .await
                .expect("Failed to store event");
            let metadata = EventMetadata {
                event_id,
                project_id,
                ingested_at: Utc::now(),
                remote_addr: None,
            };
            processor.digest(&pool, &metadata).await
        });
        handles.push(handle);
    }

    let mut failures = Vec::new();
    for handle in handles {
        if let Err(e) = handle.await.expect("Task panicked") {
            failures.push(format!("{:?}", e));
        }
    }
    assert!(
        failures.is_empty(),
        "a shared processor must digest all events without deadlock: {failures:#?}"
    );

    let (issues, _) = IssueService::list_offset(
        &pool,
        project.id,
        IssueSort::DigestOrder,
        SortOrder::Asc,
        rustrak::pagination::IssueFilter::All,
        rustrak::pagination::IssueCategory::Error,
        1,
        100,
        None,
        rustrak::services::scope::Scope::default(),
        &rustrak::services::issue::IssueFilters::default(),
    )
    .await
    .expect("Failed to list issues");
    assert_eq!(issues.len(), num_events, "no event may be dropped");
    let digest_orders: HashSet<i32> = issues.iter().map(|i| i.digest_order).collect();
    let expected: HashSet<i32> = (1..=num_events as i32).collect();
    assert_eq!(
        digest_orders, expected,
        "digest_order must be 1-{num_events} with no gaps/duplicates"
    );
}

/// Builds a pool through the production path (`db::create_pool`), so the test
/// pins the settings the server actually ships rather than its own overrides.
#[cfg(feature = "sqlite")]
async fn production_pool(db_path: &std::path::Path, max_connections: u32) -> rustrak::db::DbPool {
    let config = rustrak::config::DatabaseConfig {
        url: format!("sqlite://{}?mode=rwc", db_path.display()),
        max_connections,
        min_connections: 1,
        acquire_timeout: std::time::Duration::from_secs(5),
        idle_timeout: std::time::Duration::from_secs(600),
        max_lifetime: std::time::Duration::from_secs(1800),
    };
    let pool = rustrak::db::create_pool(&config)
        .await
        .expect("Failed to create production SQLite pool");
    rustrak::db::run_migrations(&pool)
        .await
        .expect("Failed to run migrations");
    pool
}

/// `SQLITE_FULL` inside a transaction can roll the whole transaction back by
/// itself. sqlx's own `ROLLBACK` then fails, its count of open transactions
/// stays at one, and the connection goes back to the pool looking idle: the
/// next `BEGIN IMMEDIATE` on it failed with `InvalidSavePointStatement`.
#[cfg(feature = "sqlite")]
#[actix_web::test]
async fn a_full_disk_does_not_leave_a_pooled_connection_inside_a_transaction() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let pool = production_pool(&temp_dir.path().join("rustrak_full.db"), 1).await;
    sqlx::query("CREATE TABLE filler (data BLOB)")
        .execute(&pool)
        .await
        .unwrap();
    let pages: i64 = sqlx::query_scalar("PRAGMA page_count")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "PRAGMA max_page_count = {}",
        pages + 2
    )))
    .execute(&pool)
    .await
    .unwrap();

    let mut tx = rustrak::db::begin_write(&pool).await.unwrap();
    let full = sqlx::query("INSERT INTO filler VALUES (zeroblob(1000000))")
        .execute(&mut *tx)
        .await
        .expect_err("the insert must not fit");
    let code = full
        .as_database_error()
        .and_then(|e| e.code())
        .map(|c| c.into_owned());
    assert_eq!(
        code.as_deref(),
        Some("13"),
        "expected SQLITE_FULL: {full:?}"
    );
    let _ = tx.rollback().await;

    let tx = rustrak::db::begin_write(&pool)
        .await
        .expect("the next write transaction must start");
    tx.rollback().await.unwrap();
}

/// Pool connections never checkpoint, so no request (a transaction, a log, a
/// span) runs the whole log's checkpoint, and a reset log is truncated back
/// to 64 MiB. The digest commits on its own connection: every commit fsyncs
/// the log (`synchronous=FULL`, so a spool file can go right after it) and
/// its batches checkpoint late. Everything else is the pool's,
/// `busy_timeout` included.
#[cfg(feature = "sqlite")]
#[actix_web::test]
async fn test_only_the_durable_committer_checkpoints() {
    use sqlx::Connection as _;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let pool = production_pool(&temp_dir.path().join("rustrak_wal.db"), 2).await;
    let mut conn = rustrak::db::connect_committer(&pool).await.unwrap();
    let pragma = |name: &str| sqlx::AssertSqlSafe(format!("PRAGMA {name}"));

    let mut got: Vec<i64> = Vec::new();
    for name in ["wal_autocheckpoint", "journal_size_limit"] {
        got.push(
            sqlx::query_scalar(pragma(name))
                .fetch_one(&pool)
                .await
                .unwrap(),
        );
    }
    for name in ["synchronous", "wal_autocheckpoint", "busy_timeout"] {
        got.push(
            sqlx::query_scalar(pragma(name))
                .fetch_one(&mut conn)
                .await
                .unwrap(),
        );
    }
    assert_eq!(got, [0, 64 * 1024 * 1024, 2, 10_000, 5_000]);
    conn.close().await.unwrap();
}

/// Writes that never reach the digest (a deployment that only sends
/// transactions, logs or spans) stay in the log, never checkpointed by the
/// request that wrote them, until the digest's timer checkpoints them once it
/// is caught up: then the database holds them and the next write starts the
/// log over, so it stays bounded.
#[cfg(feature = "sqlite")]
#[actix_web::test]
async fn test_pool_only_writes_are_checkpointed_by_the_digest() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let path = temp_dir.path().join("rustrak_pool_only.db");
    let pool = production_pool(&path, 2).await;
    let spool = TempDir::new().unwrap();
    let processors = crate::common::processors(&pool, spool.path());
    sqlx::query("CREATE TABLE filler (b BLOB)")
        .execute(&pool)
        .await
        .unwrap();
    let db_bytes = || std::fs::metadata(&path).unwrap().len();
    let before = db_bytes();

    // ~2 000 pages, past SQLite's own 1 000-page autocheckpoint.
    sqlx::query(
        "INSERT INTO filler SELECT zeroblob(4000) FROM \
         (WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM n WHERE x < 2000) \
          SELECT x FROM n)",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(
        db_bytes() < before + 1024 * 1024,
        "the pool's write checkpointed itself"
    );

    processors
        .errors
        .checkpoint_when_caught_up(&pool)
        .await
        .unwrap();
    assert!(db_bytes() >= before + 2000 * 4000, "not checkpointed");
    sqlx::query("INSERT INTO filler VALUES (x'00')")
        .execute(&pool)
        .await
        .unwrap();
    let (_, logged, _): (i64, i64, i64) = sqlx::query_as("PRAGMA wal_checkpoint(PASSIVE)")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(logged < 16, "the log did not start over: {logged} frames");
}

/// Once the live digest is idle nothing it keeps open pins a read snapshot:
/// a `TRUNCATE` checkpoint gets the whole log back at once, with the
/// committer's connection still open.
#[cfg(feature = "sqlite")]
#[actix_web::test]
async fn test_an_idle_digest_pins_no_snapshot_of_the_log() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let pool = production_pool(&temp_dir.path().join("rustrak_truncate.db"), 4).await;
    let project = create_test_project(&pool, "Truncate").await;
    let spool = TempDir::new().unwrap();
    let processors = Arc::new(crate::common::processors(&pool, spool.path()));
    for i in 0..64 {
        let (event_id, event) = create_unique_event_json("TruncateError", &format!("e{}", i % 4));
        rustrak::ingest::pipeline::enqueue_digest(
            &processors,
            &pool,
            Uuid::parse_str(&event_id).unwrap(),
            EventMetadata {
                event_id,
                project_id: project.id,
                ingested_at: Utc::now(),
                remote_addr: None,
            },
            serde_json::to_vec(&event).unwrap().into(),
        )
        .await
        .unwrap();
    }
    crate::common::eventually(|| async {
        (!crate::common::has_files(spool.path()) && processors.errors.in_flight().is_empty())
            .then_some(())
    })
    .await;

    let mut conn = pool.acquire().await.unwrap();
    sqlx::query("PRAGMA busy_timeout = 100")
        .execute(&mut *conn)
        .await
        .unwrap();
    let (busy, log, _): (i64, i64, i64) = sqlx::query_as("PRAGMA wal_checkpoint(TRUNCATE)")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert_eq!((busy, log), (0, 0), "the log stays pinned");
}

/// A digest that keeps up checkpoints each commit right after it, while the
/// commit is small, so the next write starts the log over.
#[cfg(feature = "sqlite")]
#[actix_web::test]
async fn test_a_caught_up_digest_checkpoints_each_commit() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let pool = production_pool(&temp_dir.path().join("rustrak_caught_up.db"), 4).await;
    let project = create_test_project(&pool, "Caught up").await;
    let spool = TempDir::new().unwrap();
    let processors = Arc::new(crate::common::processors(&pool, spool.path()));
    let (event_id, event) = create_unique_event_json("CaughtUpError", "e");
    rustrak::ingest::pipeline::enqueue_digest(
        &processors,
        &pool,
        Uuid::parse_str(&event_id).unwrap(),
        EventMetadata {
            event_id,
            project_id: project.id,
            ingested_at: Utc::now(),
            remote_addr: None,
        },
        serde_json::to_vec(&event).unwrap().into(),
    )
    .await
    .unwrap();
    crate::common::eventually(|| async {
        (!crate::common::has_files(spool.path()) && processors.errors.in_flight().is_empty())
            .then_some(())
    })
    .await;

    sqlx::query("CREATE TABLE after_the_digest (x INTEGER)")
        .execute(&pool)
        .await
        .unwrap();
    let (_, logged, _): (i64, i64, i64) = sqlx::query_as("PRAGMA wal_checkpoint(PASSIVE)")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(
        logged < 16,
        "the digest's commit stayed in the log: {logged} frames"
    );
}

/// The digest retry loop covers the digest only. Every other writer —
/// sourcemap assembly, the storage purge, bulk issue updates, alert rules,
/// the transaction/span/log processors — has a single shot at the write
/// lock, so the shipped `busy_timeout` is their entire tolerance. A holder
/// that outlasts it turns an ordinary write into a 500, and there is no
/// retry to heal it.
#[cfg(feature = "sqlite")]
#[actix_web::test]
async fn test_non_digest_write_survives_a_lock_holder_the_digest_survives() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let pool = Arc::new(production_pool(&temp_dir.path().join("rustrak_plain_write.db"), 5).await);
    let project = create_test_project(&pool, "Plain Write").await;

    // 3s sits between the two values this test has to tell apart: far above
    // any sub-second busy_timeout (500ms already failed this write), and far
    // below the 5s the server ships, leaving ~2s of slack against a measured
    // jitter of tens of milliseconds. Shortening it would let a regression
    // that halves the timeout slip through.
    let (lock_held_tx, lock_held_rx) = tokio::sync::oneshot::channel();
    let holder_pool = Arc::clone(&pool);
    let holder = tokio::spawn(async move {
        let tx = rustrak::db::begin_write(&holder_pool)
            .await
            .expect("holder must take the write lock");
        let _ = lock_held_tx.send(());
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        tx.commit().await.expect("holder commit must succeed");
    });
    lock_held_rx.await.expect("holder must signal");

    let result = sqlx::query(
        "UPDATE projects SET stored_event_count = stored_event_count + 1 WHERE id = $1",
    )
    .bind(project.id)
    .execute(pool.as_ref())
    .await;

    holder.await.expect("holder task panicked");

    assert!(
        result.is_ok(),
        "an ordinary write must wait out a lock holder, not fail with \
         \"database is locked\": {:?}",
        result.err()
    );
}

/// One slow write must not take the queued digests down with it. Serializing
/// digests behind an app-level slot with its own deadline turns a single slow
/// holder into a mass drop: every digest that cannot reach the front of the
/// queue before its own budget expires is discarded, even though the database
/// was available the whole time. The digest's tolerance for a holder must come
/// from the write lock it is actually waiting on.
#[cfg(feature = "sqlite")]
#[actix_web::test]
async fn test_a_slow_write_does_not_drop_queued_digests() {
    use rustrak::digest::processors::ErrorProcessor;

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let pool = Arc::new(production_pool(&temp_dir.path().join("rustrak_queued.db"), 8).await);
    let project = create_test_project(&pool, "Queued Digests").await;
    let ingest_dir = temp_dir.path().to_path_buf();

    // One processor shared by every event: the production shape.
    let processor = Arc::new(ErrorProcessor::new(
        &rustrak::config::Config {
            ingest_dir: ingest_dir.clone(),
            rate_limit: create_rate_limit_config(),
            ..crate::common::config()
        },
        crate::common::null_sourcemap_provider(),
    ));

    // 4s: longer than any app-level queueing deadline would tolerate, but well
    // inside the write lock's own wait. Every digest must still land.
    let (lock_held_tx, lock_held_rx) = tokio::sync::oneshot::channel();
    let holder_pool = Arc::clone(&pool);
    let holder = tokio::spawn(async move {
        let tx = rustrak::db::begin_write(&holder_pool)
            .await
            .expect("holder must take the write lock");
        let _ = lock_held_tx.send(());
        tokio::time::sleep(std::time::Duration::from_secs(4)).await;
        tx.commit().await.expect("holder commit must succeed");
    });
    lock_held_rx.await.expect("holder must signal");

    let num_events = 5;
    let mut handles = Vec::new();
    for i in 0..num_events {
        let pool = Arc::clone(&pool);
        let ingest_dir = ingest_dir.clone();
        let processor = Arc::clone(&processor);
        let project_id = project.id;

        handles.push(tokio::spawn(async move {
            let (event_id, event_json) =
                create_unique_event_json(&format!("Queued{}", i), &format!("Msg {}", i));
            let event_bytes = serde_json::to_vec(&event_json).unwrap();
            crate::common::store_event(&ingest_dir, project_id, &event_id, &event_bytes)
                .await
                .expect("Failed to store event");
            let metadata = EventMetadata {
                event_id,
                project_id,
                ingested_at: Utc::now(),
                remote_addr: None,
            };
            processor.digest(&pool, &metadata).await
        }));
    }

    let mut failures = Vec::new();
    for handle in handles {
        if let Err(e) = handle.await.expect("Task panicked") {
            failures.push(format!("{:?}", e));
        }
    }
    holder.await.expect("holder task panicked");

    assert!(
        failures.is_empty(),
        "queueing behind one slow write must not drop digests. {} of {} failed: {:#?}",
        failures.len(),
        num_events,
        failures
    );

    let (issues, _) = IssueService::list_offset(
        &pool,
        project.id,
        IssueSort::DigestOrder,
        SortOrder::Asc,
        rustrak::pagination::IssueFilter::All,
        rustrak::pagination::IssueCategory::Error,
        1,
        100,
        None,
        rustrak::services::scope::Scope::default(),
        &rustrak::services::issue::IssueFilters::default(),
    )
    .await
    .expect("Failed to list issues");
    assert_eq!(issues.len(), num_events, "no event may be dropped");
}

/// A digest is one write or none. The issue row, its counters and the event
/// row have to land together: if the event cannot be written, an issue whose
/// `stored_event_count` already claims it must not survive. Otherwise the
/// counters describe an event nobody can open, and nothing ever repairs them
/// because `process()` deletes the temp file on failure.
///
/// The failure is injected with a trigger rather than timed, so the test
/// pins the invariant instead of a race window.
#[cfg(feature = "sqlite")]
#[actix_web::test]
async fn test_a_digest_that_cannot_store_its_event_leaves_no_issue_behind() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let pool = Arc::new(production_pool(&temp_dir.path().join("rustrak_atomic.db"), 5).await);
    let project = create_test_project(&pool, "Atomic Digest").await;
    let ingest_dir = temp_dir.path().to_path_buf();

    sqlx::query(
        "CREATE TRIGGER reject_event_insert BEFORE INSERT ON events \
         BEGIN SELECT RAISE(ABORT, 'injected: event store unavailable'); END",
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to install the fault-injection trigger");

    let (event_id, event_json) = create_unique_event_json("AtomicError", "Msg");
    let event_bytes = serde_json::to_vec(&event_json).unwrap();
    crate::common::store_event(&ingest_dir, project.id, &event_id, &event_bytes)
        .await
        .expect("Failed to store event");
    let metadata = EventMetadata {
        event_id,
        project_id: project.id,
        ingested_at: Utc::now(),
        remote_addr: None,
    };

    let result = process_error_event(
        &pool,
        &metadata,
        &ingest_dir,
        &create_rate_limit_config(),
        crate::common::null_sourcemap_provider(),
    )
    .await;

    assert!(
        result.is_err(),
        "a digest that cannot store its event must fail"
    );

    let (issues, _) = IssueService::list_offset(
        &pool,
        project.id,
        IssueSort::DigestOrder,
        SortOrder::Asc,
        rustrak::pagination::IssueFilter::All,
        rustrak::pagination::IssueCategory::Error,
        1,
        100,
        None,
        rustrak::services::scope::Scope::default(),
        &rustrak::services::issue::IssueFilters::default(),
    )
    .await
    .expect("Failed to list issues");
    assert!(
        issues.is_empty(),
        "the issue must roll back with the event, not survive counting an \
         event that was never stored: {:?}",
        issues
            .iter()
            .map(|i| (i.digest_order, i.stored_event_count))
            .collect::<Vec<_>>()
    );

    let groupings: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM groupings WHERE project_id = $1")
        .bind(project.id)
        .fetch_one(pool.as_ref())
        .await
        .expect("Failed to count groupings");
    assert_eq!(groupings, 0, "the grouping must roll back with the issue");
}

/// A post-commit platform refresh must not turn an already durable digest into
/// a failed processor result.
#[cfg(feature = "sqlite")]
#[actix_web::test]
async fn test_post_commit_platform_refresh_failure_does_not_fail_digest() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let pool = Arc::new(production_pool(&temp_dir.path().join("rustrak_quota.db"), 5).await);
    let project = create_test_project(&pool, "Post Commit Platform").await;
    let ingest_dir = temp_dir.path().to_path_buf();

    sqlx::query(
        "CREATE TRIGGER reject_platform_inference BEFORE UPDATE OF platform ON projects \
         BEGIN SELECT RAISE(ABORT, 'injected: platform inference unavailable'); END",
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to install the platform fault-injection trigger");

    let (event_id, event_json) = create_unique_event_json("PlatformRefreshError", "Msg");
    let mut event_json = event_json;
    event_json["platform"] = serde_json::json!("python");
    crate::common::store_event(
        &ingest_dir,
        project.id,
        &event_id,
        &serde_json::to_vec(&event_json).unwrap(),
    )
    .await
    .expect("Failed to store event");
    let metadata = EventMetadata {
        event_id,
        project_id: project.id,
        ingested_at: Utc::now(),
        remote_addr: None,
    };

    process_error_event(
        &pool,
        &metadata,
        &ingest_dir,
        &create_rate_limit_config(),
        crate::common::null_sourcemap_provider(),
    )
    .await
    .expect("post-commit platform refresh failure must not fail the digest");

    let events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE project_id = $1")
        .bind(project.id)
        .fetch_one(pool.as_ref())
        .await
        .expect("Failed to count durable events");
    assert_eq!(events, 1, "the digest must remain durable");
}

/// Quota counters are part of the digest transaction: a partial counter write
/// must roll back the event, issue, and both counter values together.
#[cfg(feature = "sqlite")]
#[actix_web::test]
async fn test_quota_counter_failure_rolls_back_digest_and_counters() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let pool = Arc::new(production_pool(&temp_dir.path().join("rustrak_counter.db"), 5).await);
    let project = create_test_project(&pool, "Atomic Quota Counters").await;
    let initial_project = rustrak::services::ProjectService::get_by_id(&pool, project.id)
        .await
        .unwrap();
    let initial_installation: i64 =
        sqlx::query_scalar("SELECT digested_event_count FROM installation WHERE id = 1")
            .fetch_one(pool.as_ref())
            .await
            .unwrap();
    let ingest_dir = temp_dir.path().to_path_buf();

    sqlx::query(
        "CREATE TRIGGER reject_project_counter BEFORE UPDATE OF digested_event_count ON projects \
         BEGIN SELECT RAISE(ABORT, 'injected: project counter unavailable'); END",
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to install the counter fault-injection trigger");

    let (event_id, event_json) = create_unique_event_json("AtomicCounterError", "Msg");
    crate::common::store_event(
        &ingest_dir,
        project.id,
        &event_id,
        &serde_json::to_vec(&event_json).unwrap(),
    )
    .await
    .unwrap();
    let metadata = EventMetadata {
        event_id,
        project_id: project.id,
        ingested_at: Utc::now(),
        remote_addr: None,
    };

    assert!(process_error_event(
        &pool,
        &metadata,
        &ingest_dir,
        &create_rate_limit_config(),
        crate::common::null_sourcemap_provider(),
    )
    .await
    .is_err());

    let updated_project = rustrak::services::ProjectService::get_by_id(&pool, project.id)
        .await
        .unwrap();
    let installation_count: i64 =
        sqlx::query_scalar("SELECT digested_event_count FROM installation WHERE id = 1")
            .fetch_one(pool.as_ref())
            .await
            .unwrap();
    let events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE project_id = $1")
        .bind(project.id)
        .fetch_one(pool.as_ref())
        .await
        .unwrap();

    assert_eq!(
        updated_project.stored_event_count,
        initial_project.stored_event_count
    );
    assert_eq!(
        updated_project.digested_event_count,
        initial_project.digested_event_count
    );
    assert_eq!(installation_count, initial_installation);
    assert_eq!(events, 0);
}

/// A post-commit quota refresh must not turn an already durable digest into a
/// failed processor result.
#[cfg(feature = "sqlite")]
#[actix_web::test]
async fn test_post_commit_quota_refresh_failure_does_not_fail_digest() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let pool = Arc::new(production_pool(&temp_dir.path().join("rustrak_quota.db"), 5).await);
    let project = create_test_project(&pool, "Post Commit Quota").await;
    let ingest_dir = temp_dir.path().to_path_buf();

    sqlx::query(
        "CREATE TRIGGER reject_quota_refresh BEFORE UPDATE OF next_quota_check ON installation \
         BEGIN SELECT RAISE(ABORT, 'injected: quota refresh unavailable'); END",
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to install the quota fault-injection trigger");

    let (event_id, event_json) = create_unique_event_json("QuotaRefreshError", "Msg");
    crate::common::store_event(
        &ingest_dir,
        project.id,
        &event_id,
        &serde_json::to_vec(&event_json).unwrap(),
    )
    .await
    .expect("Failed to store event");
    let metadata = EventMetadata {
        event_id,
        project_id: project.id,
        ingested_at: Utc::now(),
        remote_addr: None,
    };

    process_error_event(
        &pool,
        &metadata,
        &ingest_dir,
        &create_rate_limit_config(),
        crate::common::null_sourcemap_provider(),
    )
    .await
    .expect("post-commit quota refresh failure must not fail the digest");

    let events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE project_id = $1")
        .bind(project.id)
        .fetch_one(pool.as_ref())
        .await
        .expect("Failed to count durable events");
    assert_eq!(events, 1, "the digest must remain durable");
}

/// SQLite has one writer, and the digests take turns at it in process. A
/// transaction stored inline on the ingest path has to queue in that same
/// line: left to SQLite's busy handler, which polls with growing sleeps, it
/// lost every race to the digests handing the lock to each other and failed
/// after `busy_timeout` with "database is locked", a 500 for the SDK.
#[cfg(feature = "sqlite")]
#[actix_web::test]
async fn test_an_inline_write_waits_its_turn_behind_the_digests() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let pool = production_pool(&temp_dir.path().join("rustrak_turns.db"), 5).await;
    let project = create_test_project(&pool, "Write Turns").await;
    let processors = Arc::new(crate::common::processors(&pool, temp_dir.path()));

    let event_id = Uuid::new_v4().simple().to_string();
    let transaction = json!({
        "event_id": event_id,
        "type": "transaction",
        "transaction": "GET /checkout",
        "start_timestamp": 1704801600.0,
        "timestamp": 1704801600.5,
        "contexts": {"trace": {"trace_id": "a".repeat(32), "span_id": "b".repeat(16)}}
    })
    .to_string();
    let envelope = format!(
        "{{\"event_id\":\"{event_id}\"}}\n{{\"type\":\"transaction\",\"length\":{}}}\n{transaction}",
        transaction.len()
    );

    // A large body's slot: held through the inline writes that wait their
    // turn, since the items still borrow the body; given back after.
    let large: &'static tokio::sync::Semaphore =
        Box::leak(Box::new(tokio::sync::Semaphore::new(1)));
    let slot = large.acquire().await.unwrap();

    let turn = processors.errors.write_turn().await;
    let ingest = tokio::spawn({
        let (processors, pool, project) = (Arc::clone(&processors), pool.clone(), project.clone());
        async move {
            rustrak::ingest::pipeline::process_envelope(
                &processors,
                &pool,
                &project,
                envelope.into(),
                None,
                Utc::now(),
                Some(slot),
            )
            .await
        }
    });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(
        !ingest.is_finished(),
        "the transaction must wait for the digest holding the write turn"
    );
    assert_eq!(
        large.available_permits(),
        0,
        "the large-body slot is held while the body waits to be written"
    );

    drop(turn);
    ingest
        .await
        .expect("ingest task panicked")
        .expect("the transaction must be stored once its turn comes");
    assert_eq!(large.available_permits(), 1, "the slot is given back");
    let stored: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM transactions WHERE project_id = $1")
        .bind(project.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored, 1);
}

/// Postgres: an API write to an issue that commits while a digest of one of
/// its events waits for the issue's row is what the digest folds into. The
/// digest reads the issue under the row lock, so a resolve that landed first
/// is regressed, not overwritten by a stale read.
#[cfg(feature = "postgres")]
#[actix_web::test]
async fn an_api_resolve_committed_while_the_digest_waits_is_seen() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Resolve Race").await;
    let temp_dir = TempDir::new().unwrap();
    let processors = Arc::new(crate::common::processors(&db.pool, temp_dir.path()));
    let accept = |message: &'static str| {
        let dir = temp_dir.path().to_path_buf();
        async move {
            let (event_id, event_json) = create_unique_event_json("RaceError", message);
            crate::common::store_event(
                &dir,
                project.id,
                &event_id,
                &serde_json::to_vec(&event_json).unwrap(),
            )
            .await
            .unwrap();
            EventMetadata {
                event_id,
                project_id: project.id,
                ingested_at: Utc::now(),
                remote_addr: None,
            }
        }
    };
    let first = accept("same").await;
    processors.errors.digest(&db.pool, &first).await.unwrap();
    let issue_id: Uuid = sqlx::query_scalar("SELECT id FROM issues WHERE project_id = $1")
        .bind(project.id)
        .fetch_one(&db.pool)
        .await
        .unwrap();

    let mut resolve = db.pool.begin().await.unwrap();
    sqlx::query("UPDATE issues SET status = 'resolved' WHERE id = $1")
        .bind(issue_id)
        .execute(&mut *resolve)
        .await
        .unwrap();
    let second = accept("same").await;
    let digest = tokio::spawn({
        let (processors, pool) = (Arc::clone(&processors), db.pool.clone());
        async move { processors.errors.digest(&pool, &second).await }
    });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(!digest.is_finished(), "the digest waits for the issue row");
    resolve.commit().await.unwrap();
    digest.await.unwrap().unwrap();

    let issue = IssueService::get_by_id(&db.pool, issue_id).await.unwrap();
    assert_eq!(
        (issue.status.as_str(), issue.substatus.as_deref()),
        ("unresolved", Some("regressed"))
    );
    let activities = rustrak::services::IssueSocialService::list_activity(&db.pool, issue_id)
        .await
        .unwrap();
    assert!(activities
        .iter()
        .any(|a| a.activity_type == "set_regression"));
}
