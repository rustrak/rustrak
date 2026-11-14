//! Integration tests for the Digest process
//!
//! Tests the complete event digest workflow: ingest -> grouping -> issue creation.

use crate::common::create_test_project;
use crate::common::process_error_event;
use crate::common::TestDb;
use chrono::Utc;
use rustrak::config::RateLimitConfig;
use rustrak::ingest::EventMetadata;
use rustrak::models::{CleanupFilter, CreateRelease};
use rustrak::services::scope::Scope;
use rustrak::services::{
    EventService, IssueService, IssueSocialService, ProjectService, ReleaseService, StorageService,
};
use serde_json::json;
use tempfile::TempDir;
use uuid::Uuid;

fn create_event_json(event_id: &str) -> serde_json::Value {
    json!({
        "event_id": event_id,
        "timestamp": Utc::now().timestamp() as f64,
        "platform": "rust",
        "level": "error",
        "transaction": "/api/users",
        "exception": {
            "values": [{
                "type": "TypeError",
                "value": "Cannot read property 'x' of null",
                "stacktrace": {
                    "frames": [{
                        "filename": "app.rs",
                        "function": "handle_request",
                        "lineno": 42,
                        "in_app": true
                    }]
                }
            }]
        }
    })
}

#[actix_web::test]
async fn test_digest_creates_issue_and_event() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Digest Test Project").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let ingest_dir = temp_dir.path();
    let rate_limit_config = crate::common::rate_limit();

    let event_id = Uuid::new_v4().to_string().replace("-", "");
    let event_json = create_event_json(&event_id);
    let event_bytes = serde_json::to_vec(&event_json).unwrap();

    crate::common::store_event(ingest_dir, project.id, &event_id, &event_bytes)
        .await
        .expect("Failed to store event");

    let metadata = EventMetadata {
        event_id: event_id.clone(),
        project_id: project.id,
        ingested_at: Utc::now(),
        remote_addr: None,
    };

    process_error_event(
        &db.pool,
        &metadata,
        ingest_dir,
        &rate_limit_config,
        crate::common::null_sourcemap_provider(),
    )
    .await
    .expect("Failed to process event");

    let (issues, _) = IssueService::list_offset(
        &db.pool,
        &rustrak::models::rbac::ProjectScope::Only(vec![project.id]),
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
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

    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].calculated_type, "TypeError");
    assert!(issues[0].calculated_value.contains("Cannot read property"));
    assert_eq!(issues[0].digested_event_count, 1);

    let event_uuid = Uuid::parse_str(&event_id).expect("Invalid event_id");
    let exists = EventService::exists(&db.pool, project.id, event_uuid)
        .await
        .expect("Failed to check event existence");
    assert!(exists);
}

#[actix_web::test]
async fn test_error_processor_impl_creates_issue_and_event() {
    // ErrorProcessor::digest must produce the exact same outcome as the
    // process_error_event() helper.
    use rustrak::digest::processors::ErrorProcessor;

    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "ErrorProcessor Trait Test").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let ingest_dir = temp_dir.path();
    let rate_limit_config = crate::common::rate_limit();

    let event_id = Uuid::new_v4().to_string().replace("-", "");
    let event_json = create_event_json(&event_id);
    let event_bytes = serde_json::to_vec(&event_json).unwrap();
    crate::common::store_event(ingest_dir, project.id, &event_id, &event_bytes)
        .await
        .expect("Failed to store event");

    let metadata = EventMetadata {
        event_id: event_id.clone(),
        project_id: project.id,
        ingested_at: Utc::now(),
        remote_addr: None,
    };

    let processor = ErrorProcessor::new(
        &rustrak::config::Config {
            ingest_dir: ingest_dir.to_path_buf(),
            rate_limit: rate_limit_config,
            ..crate::common::config()
        },
        crate::common::null_sourcemap_provider(),
    );

    processor
        .digest(&db.pool, &metadata)
        .await
        .expect("ErrorProcessor failed to process event");

    let (issues, _) = IssueService::list_offset(
        &db.pool,
        &rustrak::models::rbac::ProjectScope::Only(vec![project.id]),
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
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
        1,
        "ErrorProcessor must create exactly one issue"
    );
    assert_eq!(issues[0].calculated_type, "TypeError");
    assert!(issues[0].calculated_value.contains("Cannot read property"));

    let event_uuid = Uuid::parse_str(&event_id).expect("Invalid event_id");
    let exists = EventService::exists(&db.pool, project.id, event_uuid)
        .await
        .expect("Failed to check event existence");
    assert!(exists, "ErrorProcessor must store the event");
}

#[actix_web::test]
async fn test_digest_groups_similar_events() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Grouping Test Project").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let ingest_dir = temp_dir.path();
    let rate_limit_config = crate::common::rate_limit();

    for _i in 0..2 {
        let event_id = Uuid::new_v4().to_string().replace("-", "");
        let event_json = json!({
            "event_id": &event_id,
            "timestamp": Utc::now().timestamp() as f64,
            "platform": "rust",
            "level": "error",
            "transaction": "/api/users",
            "exception": {
                "values": [{
                    "type": "DatabaseError",
                    "value": "Connection refused"
                }]
            }
        });
        let event_bytes = serde_json::to_vec(&event_json).unwrap();

        crate::common::store_event(ingest_dir, project.id, &event_id, &event_bytes)
            .await
            .expect("Failed to store event");

        let metadata = EventMetadata {
            event_id: event_id.clone(),
            project_id: project.id,
            ingested_at: Utc::now(),
            remote_addr: None,
        };

        process_error_event(
            &db.pool,
            &metadata,
            ingest_dir,
            &rate_limit_config,
            crate::common::null_sourcemap_provider(),
        )
        .await
        .expect("Failed to process event");
    }

    // Should have only 1 issue with 2 events
    let (issues, _) = IssueService::list_offset(
        &db.pool,
        &rustrak::models::rbac::ProjectScope::Only(vec![project.id]),
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
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

    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].digested_event_count, 2);
}

#[actix_web::test]
async fn test_digest_creates_separate_issues_for_different_errors() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Different Errors Project").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let ingest_dir = temp_dir.path();
    let rate_limit_config = crate::common::rate_limit();

    let errors = vec![
        ("TypeError", "Cannot read property"),
        ("ValueError", "Invalid value"),
        ("IOError", "File not found"),
    ];

    for (error_type, error_msg) in errors {
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
        let event_bytes = serde_json::to_vec(&event_json).unwrap();

        crate::common::store_event(ingest_dir, project.id, &event_id, &event_bytes)
            .await
            .expect("Failed to store event");

        let metadata = EventMetadata {
            event_id: event_id.clone(),
            project_id: project.id,
            ingested_at: Utc::now(),
            remote_addr: None,
        };

        process_error_event(
            &db.pool,
            &metadata,
            ingest_dir,
            &rate_limit_config,
            crate::common::null_sourcemap_provider(),
        )
        .await
        .expect("Failed to process event");
    }

    // Should have 3 separate issues
    let (issues, _) = IssueService::list_offset(
        &db.pool,
        &rustrak::models::rbac::ProjectScope::Only(vec![project.id]),
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
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

    assert_eq!(issues.len(), 3);
    let mut numeric_ids: Vec<i64> = issues.iter().map(|i| i.numeric_id).collect();
    numeric_ids.sort();
    numeric_ids.dedup();
    assert_eq!(
        numeric_ids.len(),
        3,
        "digest gives each issue its own numeric id"
    );
}

#[actix_web::test]
async fn test_digest_handles_custom_fingerprint() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Custom Fingerprint Project").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let ingest_dir = temp_dir.path();
    let rate_limit_config = crate::common::rate_limit();

    // Create two events with same fingerprint but different errors
    for i in 0..2 {
        let event_id = Uuid::new_v4().to_string().replace("-", "");
        let event_json = json!({
            "event_id": &event_id,
            "timestamp": Utc::now().timestamp() as f64,
            "platform": "rust",
            "level": "error",
            "fingerprint": ["custom-group-key"],
            "exception": {
                "values": [{
                    "type": format!("Error{}", i),
                    "value": format!("Different error {}", i)
                }]
            }
        });
        let event_bytes = serde_json::to_vec(&event_json).unwrap();

        crate::common::store_event(ingest_dir, project.id, &event_id, &event_bytes)
            .await
            .expect("Failed to store event");

        let metadata = EventMetadata {
            event_id: event_id.clone(),
            project_id: project.id,
            ingested_at: Utc::now(),
            remote_addr: None,
        };

        process_error_event(
            &db.pool,
            &metadata,
            ingest_dir,
            &rate_limit_config,
            crate::common::null_sourcemap_provider(),
        )
        .await
        .expect("Failed to process event");
    }

    // Should have 1 issue because of custom fingerprint
    let (issues, _) = IssueService::list_offset(
        &db.pool,
        &rustrak::models::rbac::ProjectScope::Only(vec![project.id]),
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
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

    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].digested_event_count, 2);
}

#[actix_web::test]
async fn test_digest_empty_fingerprint_does_not_collapse_issues() {
    // Issue #290: sentry-ruby always sends `"fingerprint": []`. Distinct
    // exceptions must still land in distinct issues with a non-empty key.
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Empty Fingerprint Project").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let ingest_dir = temp_dir.path();
    let rate_limit_config = crate::common::rate_limit();

    for exc_type in [
        "Rack::Multipart::BoundaryTooLongError",
        "Rack::Multipart::EmptyContentError",
    ] {
        let event_id = Uuid::new_v4().to_string().replace("-", "");
        let event_json = json!({
            "event_id": &event_id,
            "timestamp": Utc::now().timestamp() as f64,
            "platform": "ruby",
            "level": "error",
            "transaction": "/upload",
            "fingerprint": [],
            "exception": {
                "values": [{ "type": exc_type, "value": "boom" }]
            }
        });
        let event_bytes = serde_json::to_vec(&event_json).unwrap();

        crate::common::store_event(ingest_dir, project.id, &event_id, &event_bytes)
            .await
            .expect("Failed to store event");

        let metadata = EventMetadata {
            event_id: event_id.clone(),
            project_id: project.id,
            ingested_at: Utc::now(),
            remote_addr: None,
        };

        process_error_event(
            &db.pool,
            &metadata,
            ingest_dir,
            &rate_limit_config,
            crate::common::null_sourcemap_provider(),
        )
        .await
        .expect("Failed to process event");
    }

    let (issues, _) = IssueService::list_offset(
        &db.pool,
        &rustrak::models::rbac::ProjectScope::Only(vec![project.id]),
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
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

    assert_eq!(issues.len(), 2);

    let keys: Vec<String> = sqlx::query_scalar("SELECT grouping_key FROM groupings")
        .fetch_all(&db.pool)
        .await
        .expect("Failed to read groupings");
    assert_eq!(keys.len(), 2);
    assert!(
        keys.iter().all(|k| !k.is_empty()),
        "grouping keys: {keys:?}"
    );
}

#[actix_web::test]
async fn test_digest_handles_default_fingerprint_placeholder() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Default Fingerprint Project").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let ingest_dir = temp_dir.path();
    let rate_limit_config = crate::common::rate_limit();

    // Event with {{ default }} fingerprint should use default grouping
    let event_id = Uuid::new_v4().to_string().replace("-", "");
    let event_json = json!({
        "event_id": &event_id,
        "timestamp": Utc::now().timestamp() as f64,
        "platform": "rust",
        "level": "error",
        "fingerprint": ["{{ default }}", "extra-context"],
        "exception": {
            "values": [{
                "type": "TestError",
                "value": "Test message"
            }]
        }
    });
    let event_bytes = serde_json::to_vec(&event_json).unwrap();

    crate::common::store_event(ingest_dir, project.id, &event_id, &event_bytes)
        .await
        .expect("Failed to store event");

    let metadata = EventMetadata {
        event_id: event_id.clone(),
        project_id: project.id,
        ingested_at: Utc::now(),
        remote_addr: None,
    };

    process_error_event(
        &db.pool,
        &metadata,
        ingest_dir,
        &rate_limit_config,
        crate::common::null_sourcemap_provider(),
    )
    .await
    .expect("Failed to process event");

    let (issues, _) = IssueService::list_offset(
        &db.pool,
        &rustrak::models::rbac::ProjectScope::Only(vec![project.id]),
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
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

    assert_eq!(issues.len(), 1);
}

#[actix_web::test]
async fn test_digest_ignores_duplicate_event_id() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Duplicate Event Project").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let ingest_dir = temp_dir.path();
    let rate_limit_config = crate::common::rate_limit();

    let event_id = Uuid::new_v4().to_string().replace("-", "");
    let event_json = create_event_json(&event_id);
    let event_bytes = serde_json::to_vec(&event_json).unwrap();

    for _ in 0..2 {
        crate::common::store_event(ingest_dir, project.id, &event_id, &event_bytes)
            .await
            .expect("Failed to store event");

        let metadata = EventMetadata {
            event_id: event_id.clone(),
            project_id: project.id,
            ingested_at: Utc::now(),
            remote_addr: None,
        };

        // The second time is a duplicate, which is not an error.
        process_error_event(
            &db.pool,
            &metadata,
            ingest_dir,
            &rate_limit_config,
            crate::common::null_sourcemap_provider(),
        )
        .await
        .expect("a duplicate is ignored, not an error");
    }

    // Should only have 1 issue with 1 event
    let (issues, _) = IssueService::list_offset(
        &db.pool,
        &rustrak::models::rbac::ProjectScope::Only(vec![project.id]),
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
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

    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].digested_event_count, 1);
}

/// Digests one error event grouped onto `create_event_json`'s fixed
/// exception/transaction, with an explicit `timestamp` (the event's SDK time,
/// used for `(timestamp, id)` ordering) and `ingested_at` (the retention key
/// in `services/storage.rs`) instead of both defaulting to "now".
async fn ingest_error_event_at(
    pool: &rustrak::db::DbPool,
    project_id: i32,
    ingest_dir: &std::path::Path,
    rate_limit_config: &RateLimitConfig,
    event_id: &str,
    event_timestamp: chrono::DateTime<Utc>,
    ingested_at: chrono::DateTime<Utc>,
) {
    let mut event_json = create_event_json(event_id);
    event_json["timestamp"] = json!(event_timestamp.timestamp() as f64);
    let event_bytes = serde_json::to_vec(&event_json).unwrap();
    crate::common::store_event(ingest_dir, project_id, event_id, &event_bytes)
        .await
        .expect("store event");
    let metadata = EventMetadata {
        event_id: event_id.to_string(),
        project_id,
        ingested_at,
        remote_addr: None,
    };
    process_error_event(
        pool,
        &metadata,
        ingest_dir,
        rate_limit_config,
        crate::common::null_sourcemap_provider(),
    )
    .await
    .expect("process event");
}

/// Regression test for the `events(issue_id, digest_order)` unique-violation
/// storm: the pre-fix codebase derived a new event's `digest_order` from
/// `issues.digested_event_count`, but retention cleanup
/// (`StorageService::execute_cleanup`) decrements that same counter when
/// purging old events. Since old (low-`digest_order`) events get purged
/// while recent (high-`digest_order`) ones survive, the decremented counter
/// could fall back into `digest_order` territory a surviving row still
/// occupied -- the next event for that issue then collided on insert and was
/// silently lost. `events.digest_order` no longer exists (events order on
/// `(timestamp, id)` instead), so this exercises the exact repro end to end
/// through the real digest and retention paths and confirms the fix holds:
/// purge the oldest events for an issue via `StorageService::execute_cleanup`
/// (the real path, not a hand-rolled DELETE), then digest one more event for
/// that same issue and confirm it inserts cleanly and sorts after the
/// newest surviving event.
#[actix_web::test]
async fn test_new_event_after_retention_purge_does_not_collide() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Retention Purge Project").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let ingest_dir = temp_dir.path();
    let rate_limit_config = crate::common::rate_limit();

    // 3 old events (well past the 30-day retention window used below) plus 2
    // recent ones, all grouping onto the same issue (create_event_json's
    // fixed exception type/message/transaction), at strictly increasing
    // timestamps.
    let old_base = Utc::now() - chrono::Duration::days(60);
    for i in 0..3 {
        let event_id = Uuid::new_v4().to_string().replace("-", "");
        let at = old_base + chrono::Duration::minutes(i);
        ingest_error_event_at(
            &db.pool,
            project.id,
            ingest_dir,
            &rate_limit_config,
            &event_id,
            at,
            at,
        )
        .await;
    }
    for i in 0..2 {
        let event_id = Uuid::new_v4().to_string().replace("-", "");
        let at = Utc::now() - chrono::Duration::minutes(2 - i);
        ingest_error_event_at(
            &db.pool,
            project.id,
            ingest_dir,
            &rate_limit_config,
            &event_id,
            at,
            at,
        )
        .await;
    }

    let (issues, _) = IssueService::list_offset(
        &db.pool,
        &rustrak::models::rbac::ProjectScope::Only(vec![project.id]),
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
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
    assert_eq!(issues.len(), 1, "all 5 events grouped onto one issue");
    let issue_id = issues[0].id;
    assert_eq!(issues[0].digested_event_count, 5);

    // Purge everything older than 30 days -- removes the oldest 3, the issue
    // survives with its 2 recent events. This is the exact operation that
    // decremented issues.digested_event_count in the pre-fix codebase and
    // produced a digest_order collision for the next event digested onto
    // this issue.
    let counts = StorageService::execute_cleanup(&db.pool, 30, None, CleanupFilter::all())
        .await
        .expect("retention cleanup should succeed");
    assert_eq!(counts.events, 3, "the 3 old events are purged");
    assert_eq!(
        counts.issues_removed, 0,
        "the issue still has 2 surviving events, so it is not removed"
    );

    let issue_after_purge = IssueService::get_by_id(&db.pool, issue_id)
        .await
        .expect("issue should survive the purge");
    assert_eq!(issue_after_purge.digested_event_count, 2);

    // Digest one more event for the same issue (same grouping key). Before
    // this fix, its digest_order would have been derived from the
    // decremented digested_event_count and could collide with a surviving
    // event's digest_order -- inserting must now succeed unconditionally,
    // since there is no counter-derived value left to collide on at all.
    let new_event_id = Uuid::new_v4().to_string().replace("-", "");
    let new_timestamp = Utc::now();
    ingest_error_event_at(
        &db.pool,
        project.id,
        ingest_dir,
        &rate_limit_config,
        &new_event_id,
        new_timestamp,
        new_timestamp,
    )
    .await;

    // Still the same issue -- no spurious extra issue, no silently dropped
    // event.
    let (issues_after, _) = IssueService::list_offset(
        &db.pool,
        &rustrak::models::rbac::ProjectScope::Only(vec![project.id]),
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
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
    assert_eq!(issues_after.len(), 1, "no spurious extra issue was created");
    assert_eq!(issues_after[0].id, issue_id);
    assert_eq!(
        issues_after[0].digested_event_count, 3,
        "2 surviving + 1 new"
    );

    let new_event_uuid = Uuid::parse_str(&new_event_id).expect("Invalid event_id");
    assert!(
        EventService::exists(&db.pool, project.id, new_event_uuid)
            .await
            .unwrap(),
        "the new event was not silently lost to a unique-constraint collision"
    );

    // Sorts after the newest surviving event: a DESC listing puts the new
    // event first, exercising the (timestamp, id) keyset end to end.
    let (events, has_more) = EventService::list_paginated(
        &db.pool,
        issue_id,
        rustrak::pagination::SortOrder::Desc,
        None,
        10,
        None,
    )
    .await
    .expect("Failed to list events");
    assert_eq!(events.len(), 3, "2 surviving + 1 new event");
    assert!(!has_more);
    assert_eq!(
        events[0].event_id, new_event_uuid,
        "the new event sorts first (newest) in DESC order"
    );
    assert!(events[1].timestamp <= events[0].timestamp);
    assert!(events[2].timestamp <= events[1].timestamp);

    // Also exercises the ASC keyset branch (both no-cursor and with-cursor)
    // against the same purge-then-digest scenario, so an operator-flip bug
    // in the ASC comparison (`>` vs `<`) wouldn't only be caught by the DESC
    // assertions above.
    let (events_asc_page1, has_more_asc_page1) = EventService::list_paginated(
        &db.pool,
        issue_id,
        rustrak::pagination::SortOrder::Asc,
        None,
        1,
        None,
    )
    .await
    .expect("Failed to list events (ASC, page 1)");
    assert_eq!(events_asc_page1.len(), 1);
    assert!(has_more_asc_page1);

    let asc_cursor = rustrak::pagination::EventCursor::new(
        "asc",
        events_asc_page1[0].timestamp,
        events_asc_page1[0].id,
    );
    let (events_asc_page2, has_more_asc_page2) = EventService::list_paginated(
        &db.pool,
        issue_id,
        rustrak::pagination::SortOrder::Asc,
        Some(&asc_cursor),
        10,
        None,
    )
    .await
    .expect("Failed to list events (ASC, page 2)");
    assert_eq!(
        events_asc_page2.len(),
        2,
        "1 surviving + 1 new event remain"
    );
    assert!(!has_more_asc_page2);
    assert_eq!(
        events_asc_page2[1].event_id, new_event_uuid,
        "the new event sorts last (oldest-first) in ASC order"
    );
    assert!(events_asc_page2[0].timestamp <= events_asc_page2[1].timestamp);
}

/// Digests one event into `project`, returning nothing; the caller reads the
/// issue back. `event` is merged over the minimum an event needs.
async fn digest(
    pool: &rustrak::db::DbPool,
    ingest_dir: &std::path::Path,
    project_id: i32,
    at: chrono::DateTime<Utc>,
    mut event: serde_json::Value,
) {
    let event_id = Uuid::new_v4().to_string().replace("-", "");
    let obj = event.as_object_mut().unwrap();
    obj.insert("event_id".into(), json!(&event_id));
    obj.insert("timestamp".into(), json!(at.timestamp() as f64));

    crate::common::store_event(
        ingest_dir,
        project_id,
        &event_id,
        &serde_json::to_vec(&event).unwrap(),
    )
    .await
    .expect("Failed to store event");

    process_error_event(
        pool,
        &EventMetadata {
            event_id,
            project_id,
            ingested_at: at,
            remote_addr: None,
        },
        ingest_dir,
        &crate::common::rate_limit(),
        crate::common::null_sourcemap_provider(),
    )
    .await
    .expect("Failed to process event");
}

async fn only_issue(pool: &rustrak::db::DbPool, project_id: i32) -> rustrak::models::Issue {
    let (issues, _) = IssueService::list_offset(
        pool,
        &rustrak::models::rbac::ProjectScope::Only(vec![project_id]),
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
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
    assert_eq!(issues.len(), 1, "expected the events to share one issue");
    issues.into_iter().next().unwrap()
}

#[actix_web::test]
async fn test_issue_title_follows_the_latest_event() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Title Follows").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let dir = temp_dir.path();
    let now = Utc::now();

    let event = |param: &str| {
        json!({
            "platform": "elixir",
            "level": "warning",
            "logentry": { "message": "User %s logged in", "params": [param] }
        })
    };

    digest(&db.pool, dir, project.id, now, event("john")).await;
    digest(&db.pool, dir, project.id, now, event("jane")).await;

    let issue = only_issue(&db.pool, project.id).await;
    assert_eq!(issue.digested_event_count, 2);
    assert_eq!(issue.title(), "Log Message: User jane logged in");
}

#[actix_web::test]
async fn test_placeholder_title_does_not_overwrite_a_real_one() {
    // Sentry keeps the existing title when the incoming one is a placeholder
    // (`_get_updated_group_title`). Rustrak's placeholders are "Unknown", for an
    // event with neither exception nor message, and a bare "Error".
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Placeholder Title").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let dir = temp_dir.path();
    let now = Utc::now();

    // A custom fingerprint pins both events to one issue regardless of content.
    let real = json!({
        "platform": "rust",
        "fingerprint": ["pinned"],
        "logentry": { "formatted": "Something specific happened" }
    });
    let placeholder = json!({
        "platform": "rust",
        "fingerprint": ["pinned"]
    });

    digest(&db.pool, dir, project.id, now, real).await;
    digest(&db.pool, dir, project.id, now, placeholder).await;

    let issue = only_issue(&db.pool, project.id).await;
    assert_eq!(issue.digested_event_count, 2);
    assert_eq!(issue.title(), "Log Message: Something specific happened");
}

#[actix_web::test]
async fn test_issue_level_and_culprit_follow_the_latest_event() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Level Follows").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let dir = temp_dir.path();
    let now = Utc::now();

    let event = |level: &str, function: &str| {
        json!({
            "platform": "rust",
            "level": level,
            "fingerprint": ["pinned"],
            "exception": { "values": [{
                "type": "TypeError",
                "value": "boom",
                "stacktrace": { "frames": [{
                    "filename": "app.rs", "function": function, "in_app": true
                }]}
            }]}
        })
    };

    digest(
        &db.pool,
        dir,
        project.id,
        now,
        event("warning", "first_path"),
    )
    .await;
    digest(
        &db.pool,
        dir,
        project.id,
        now,
        event("error", "second_path"),
    )
    .await;

    let issue = only_issue(&db.pool, project.id).await;
    assert_eq!(issue.level.as_deref(), Some("error"));
    assert_eq!(issue.culprit, "second_path");
}

#[actix_web::test]
async fn test_issue_keeps_creation_only_fields_across_events() {
    // Sentry's `_process_existing_aggregate` never touches platform, logger or
    // first_release, and sets priority only at creation.
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Creation Only").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let dir = temp_dir.path();
    let now = Utc::now();

    digest(
        &db.pool,
        dir,
        project.id,
        now,
        json!({
            "platform": "rust", "level": "warning", "logger": "first.logger",
            "release": "1.0.0", "fingerprint": ["pinned"],
            "logentry": { "formatted": "boom" }
        }),
    )
    .await;
    digest(
        &db.pool,
        dir,
        project.id,
        now,
        json!({
            "platform": "python", "level": "fatal", "logger": "second.logger",
            "release": "2.0.0", "fingerprint": ["pinned"],
            "logentry": { "formatted": "boom" }
        }),
    )
    .await;

    let issue = only_issue(&db.pool, project.id).await;
    assert_eq!(issue.platform.as_deref(), Some("rust"));
    assert_eq!(issue.logger, "first.logger");
    assert_eq!(issue.first_release, "1.0.0");
    assert_eq!(issue.last_release, "2.0.0");
    assert_eq!(
        issue.priority.as_deref(),
        Some("medium"),
        "priority is derived once, at creation"
    );
}

#[actix_web::test]
async fn test_first_seen_moves_back_for_an_older_event() {
    // Clock skew between hosts, or a retried envelope, can deliver an event
    // older than the issue. Sentry moves `first_seen` back; `last_seen` only
    // ever moves forward.
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "First Seen").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let dir = temp_dir.path();

    let now = Utc::now();
    let earlier = now - chrono::Duration::hours(2);
    let event = json!({
        "platform": "rust",
        "logentry": { "formatted": "boom" }
    });

    digest(&db.pool, dir, project.id, now, event.clone()).await;
    digest(&db.pool, dir, project.id, earlier, event).await;

    let issue = only_issue(&db.pool, project.id).await;
    assert!(
        (issue.first_seen - earlier).num_seconds().abs() <= 1,
        "first_seen should move back to the older event, got {}",
        issue.first_seen
    );
    assert!(
        (issue.last_seen - now).num_seconds().abs() <= 1,
        "last_seen should stay at the newest event, got {}",
        issue.last_seen
    );
}

#[actix_web::test]
async fn test_automatic_regression_is_recorded_in_the_activity_log() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Regression Activity").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let dir = temp_dir.path();
    let now = Utc::now();
    let event = json!({ "platform": "rust", "logentry": { "formatted": "boom" } });

    digest(&db.pool, dir, project.id, now, event.clone()).await;
    let issue = only_issue(&db.pool, project.id).await;
    IssueService::set_status(&db.pool, issue.id, "resolved", None)
        .await
        .expect("resolve");
    let before = rustrak::services::IssueSocialService::list_activity(&db.pool, issue.id)
        .await
        .expect("activity");

    digest(
        &db.pool,
        dir,
        project.id,
        now + chrono::Duration::hours(3),
        event,
    )
    .await;

    let after = rustrak::services::IssueSocialService::list_activity(&db.pool, issue.id)
        .await
        .expect("activity");
    assert_eq!(
        after.len(),
        before.len() + 1,
        "the automatic regression should add exactly one activity entry"
    );
    let entry = &after[0];
    assert_eq!(entry.activity_type, "set_regression");
    assert!(entry.user_id.is_none(), "no human performed this change");
}

#[actix_web::test]
async fn test_an_ordinary_event_records_no_activity() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "No Noise").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let dir = temp_dir.path();
    let now = Utc::now();
    let event = json!({ "platform": "rust", "logentry": { "formatted": "boom" } });

    digest(&db.pool, dir, project.id, now, event.clone()).await;
    let issue = only_issue(&db.pool, project.id).await;
    digest(&db.pool, dir, project.id, now, event).await;

    let entries = rustrak::services::IssueSocialService::list_activity(&db.pool, issue.id)
        .await
        .expect("activity");
    assert!(
        entries.is_empty(),
        "only lifecycle changes belong in the log, got {entries:?}"
    );
}

#[actix_web::test]
async fn test_issue_search_finds_the_frame_filename_and_module() {
    use rustrak::pagination::{IssueFilter, IssueSort, SortOrder};
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Search Frames").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let dir = temp_dir.path();

    // The frame carries a function, so the culprit takes it and neither the
    // filename nor the module is reachable through any other column.
    digest(
        &db.pool,
        dir,
        project.id,
        Utc::now(),
        json!({
            "platform": "rust",
            "exception": { "values": [{
                "type": "PaymentError",
                "value": "card declined",
                "stacktrace": { "frames": [{
                    "filename": "billing/stripe.rs",
                    "module": "billing::stripe",
                    "function": "charge_customer",
                    "in_app": true
                }]}
            }]}
        }),
    )
    .await;

    for term in ["billing/stripe.rs", "billing::stripe"] {
        let (hits, total) = IssueService::list_offset(
            &db.pool,
            &rustrak::models::rbac::ProjectScope::Only(vec![project.id]),
            IssueSort::DigestOrder,
            SortOrder::Desc,
            IssueFilter::All,
            rustrak::pagination::IssueCategory::Error,
            1,
            20,
            Some(term),
            Scope::default(),
            &rustrak::services::issue::IssueFilters::default(),
        )
        .await
        .expect("search");
        assert_eq!(hits.len(), 1, "searching {term:?} should find the issue");
        assert_eq!(total, 1, "the count query must agree for {term:?}");
    }
}

#[actix_web::test]
async fn test_search_follows_the_frame_of_the_latest_event() {
    use rustrak::pagination::{IssueFilter, IssueSort, SortOrder};
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Search Freshness").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let dir = temp_dir.path();
    let now = Utc::now();

    let event = |file: &str, module: &str| {
        json!({
            "platform": "rust",
            "fingerprint": ["pinned"],
            "exception": { "values": [{
                "type": "PaymentError", "value": "card declined",
                "stacktrace": { "frames": [{
                    "filename": file, "module": module,
                    "function": "charge_customer", "in_app": true
                }]}
            }]}
        })
    };

    digest(
        &db.pool,
        dir,
        project.id,
        now,
        event("old/path.rs", "old::path"),
    )
    .await;
    digest(
        &db.pool,
        dir,
        project.id,
        now,
        event("new/path.rs", "new::path"),
    )
    .await;

    let find = |term: &'static str| {
        let pool = db.pool.clone();
        async move {
            IssueService::list_offset(
                &pool,
                &rustrak::models::rbac::ProjectScope::Only(vec![project.id]),
                IssueSort::DigestOrder,
                SortOrder::Desc,
                IssueFilter::All,
                rustrak::pagination::IssueCategory::Error,
                1,
                20,
                Some(term),
                Scope::default(),
                &rustrak::services::issue::IssueFilters::default(),
            )
            .await
            .expect("search")
            .0
            .len()
        }
    };

    assert_eq!(
        find("new/path.rs").await,
        1,
        "the newest filename must be searchable"
    );
    assert_eq!(
        find("new::path").await,
        1,
        "the newest module must be searchable"
    );
}

#[actix_web::test]
async fn test_a_frameless_event_does_not_erase_a_known_frame() {
    // Sentry merges incoming metadata over existing, so a key the new event
    // lacks keeps its old value rather than being blanked.
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Frame Merge").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let dir = temp_dir.path();
    let now = Utc::now();

    digest(
        &db.pool,
        dir,
        project.id,
        now,
        json!({
            "platform": "rust",
            "fingerprint": ["pinned"],
            "exception": { "values": [{
                "type": "PaymentError", "value": "card declined",
                "stacktrace": { "frames": [{
                    "filename": "billing/stripe.rs", "module": "billing::stripe",
                    "function": "charge_customer", "in_app": true
                }]}
            }]}
        }),
    )
    .await;
    digest(
        &db.pool,
        dir,
        project.id,
        now,
        json!({
            "platform": "rust",
            "fingerprint": ["pinned"],
            "exception": { "values": [{ "type": "PaymentError", "value": "card declined" }]}
        }),
    )
    .await;

    let issue = only_issue(&db.pool, project.id).await;
    assert_eq!(issue.last_frame_filename, "billing/stripe.rs");
    assert_eq!(issue.last_frame_module, "billing::stripe");
    assert_eq!(issue.last_frame_function, "charge_customer");
}

#[actix_web::test]
async fn test_an_event_without_a_level_does_not_erase_the_issue_level() {
    // Sentry SDKs omit `level` when it is the default, so an issue that has a
    // level must not lose it to the next event that leaves it out.
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Level Merge").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let dir = temp_dir.path();
    let now = Utc::now();

    digest(
        &db.pool,
        dir,
        project.id,
        now,
        json!({
            "platform": "rust", "level": "warning", "fingerprint": ["pinned"],
            "logentry": { "formatted": "boom" }
        }),
    )
    .await;
    digest(
        &db.pool,
        dir,
        project.id,
        now,
        json!({
            "platform": "rust", "fingerprint": ["pinned"],
            "logentry": { "formatted": "boom" }
        }),
    )
    .await;

    let issue = only_issue(&db.pool, project.id).await;
    assert_eq!(issue.level.as_deref(), Some("warning"));
}

/// Digests one event under the v0.14.10 grouping key, standing in for an issue
/// that already existed before the upgrade.
async fn digest_as_pre_upgrade(
    pool: &rustrak::db::DbPool,
    ingest_dir: &std::path::Path,
    project_id: i32,
    event: serde_json::Value,
) -> rustrak::models::Issue {
    let key = rustrak::services::calculate_grouping_key_v1(&event);
    let hash = rustrak::services::hash_grouping_key(&key);
    let denormalized = rustrak::services::get_denormalized_fields(&event);
    let issue = IssueService::create(
        pool,
        project_id,
        Utc::now(),
        &denormalized,
        event.get("level").and_then(|l| l.as_str()),
        event.get("platform").and_then(|p| p.as_str()),
    )
    .await
    .expect("create issue");
    sqlx::query(
        "INSERT INTO groupings (project_id, issue_id, grouping_key, grouping_key_hash)          VALUES ($1, $2, $3, $4)",
    )
    .bind(project_id)
    .bind(issue.id)
    .bind(&key)
    .bind(&hash)
    .execute(pool)
    .await
    .expect("insert grouping");
    let _ = ingest_dir;
    issue
}

#[actix_web::test]
async fn test_an_event_finds_its_pre_upgrade_issue() {
    // The exception-chain and normalization changes move this event to a new
    // key. Without the fallback it would open a second issue and leave the
    // first frozen.
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Upgrade Path").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let dir = temp_dir.path();

    let event = json!({
        "platform": "python",
        "exception": { "values": [
            { "type": "IOError", "value": "disk full" },
            { "type": "RuntimeError", "value": "save failed for order 4213" }
        ]}
    });
    let old = digest_as_pre_upgrade(&db.pool, dir, project.id, event.clone()).await;

    digest(&db.pool, dir, project.id, Utc::now(), event).await;

    let (issues, _) = IssueService::list_offset(
        &db.pool,
        &rustrak::models::rbac::ProjectScope::Only(vec![project.id]),
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
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

    assert_eq!(issues.len(), 1, "the event opened a second issue");
    assert_eq!(issues[0].id, old.id);
    assert_eq!(issues[0].digested_event_count, 2);
}

#[actix_web::test]
async fn test_the_issue_migrates_to_the_current_key() {
    // After the first event lands through the fallback, the issue carries both
    // hashes, so later events resolve directly.
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Upgrade Migrates").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let dir = temp_dir.path();

    let event = json!({
        "platform": "python",
        "exception": { "values": [
            { "type": "IOError", "value": "disk full" },
            { "type": "RuntimeError", "value": "save failed for order 4213" }
        ]}
    });
    let old = digest_as_pre_upgrade(&db.pool, dir, project.id, event.clone()).await;
    digest(&db.pool, dir, project.id, Utc::now(), event.clone()).await;

    let hashes = IssueService::list_hashes(&db.pool, old.id)
        .await
        .expect("list hashes");
    assert_eq!(hashes.len(), 2, "the current key should have been recorded");
    let current =
        rustrak::services::hash_grouping_key(&rustrak::services::calculate_grouping_key(&event));
    assert!(hashes.iter().any(|h| h.grouping_key_hash == current));

    // A later event, now differing only by the order id, still lands there.
    let later = json!({
        "platform": "python",
        "exception": { "values": [
            { "type": "IOError", "value": "disk full" },
            { "type": "RuntimeError", "value": "save failed for order 9981" }
        ]}
    });
    digest(&db.pool, dir, project.id, Utc::now(), later).await;

    let issue = only_issue(&db.pool, project.id).await;
    assert_eq!(issue.id, old.id);
    assert_eq!(issue.digested_event_count, 3);
}

#[actix_web::test]
async fn test_a_genuinely_new_event_still_opens_its_own_issue() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Upgrade New").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let dir = temp_dir.path();

    digest_as_pre_upgrade(
        &db.pool,
        dir,
        project.id,
        json!({
            "platform": "python",
            "exception": { "values": [{ "type": "IOError", "value": "disk full" }] }
        }),
    )
    .await;

    digest(
        &db.pool,
        dir,
        project.id,
        Utc::now(),
        json!({
            "platform": "python",
            "exception": { "values": [{ "type": "ValueError", "value": "bad input" }] }
        }),
    )
    .await;

    let (issues, _) = IssueService::list_offset(
        &db.pool,
        &rustrak::models::rbac::ProjectScope::Only(vec![project.id]),
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
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
    assert_eq!(issues.len(), 2);
}

#[actix_web::test]
async fn test_digest_message_only_events_get_their_own_issue_and_title() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Elixir Warnings").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let ingest_dir = temp_dir.path();
    let rate_limit_config = crate::common::rate_limit();

    for warning in ["Disk almost full", "Cache miss storm"] {
        let event_id = Uuid::new_v4().to_string().replace("-", "");
        let event_json = json!({
            "event_id": &event_id,
            "timestamp": Utc::now().timestamp() as f64,
            "platform": "elixir",
            "level": "warning",
            "exception": [],
            "fingerprint": ["{{ default }}"],
            "transaction": null,
            "message": { "formatted": warning, "message": null, "params": null }
        });
        let event_bytes = serde_json::to_vec(&event_json).unwrap();

        crate::common::store_event(ingest_dir, project.id, &event_id, &event_bytes)
            .await
            .expect("Failed to store event");

        let metadata = EventMetadata {
            event_id: event_id.clone(),
            project_id: project.id,
            ingested_at: Utc::now(),
            remote_addr: None,
        };

        process_error_event(
            &db.pool,
            &metadata,
            ingest_dir,
            &rate_limit_config,
            crate::common::null_sourcemap_provider(),
        )
        .await
        .expect("Failed to process event");
    }

    let (issues, _) = IssueService::list_offset(
        &db.pool,
        &rustrak::models::rbac::ProjectScope::Only(vec![project.id]),
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
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

    assert_eq!(issues.len(), 2, "the two warnings collapsed into one issue");

    let mut titles: Vec<String> = issues.iter().map(|i| i.title()).collect();
    titles.sort();
    assert_eq!(
        titles,
        vec![
            "Log Message: Cache miss storm".to_string(),
            "Log Message: Disk almost full".to_string(),
        ]
    );
}

#[actix_web::test]
async fn test_digest_groups_log_messages() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Log Message Project").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let ingest_dir = temp_dir.path();
    let rate_limit_config = crate::common::rate_limit();

    // Create events with same log message (no exception)
    for _ in 0..2 {
        let event_id = Uuid::new_v4().to_string().replace("-", "");
        let event_json = json!({
            "event_id": &event_id,
            "timestamp": Utc::now().timestamp() as f64,
            "platform": "rust",
            "level": "warning",
            "logentry": {
                "message": "User %s failed to authenticate",
                "formatted": "User john@example.com failed to authenticate"
            }
        });
        let event_bytes = serde_json::to_vec(&event_json).unwrap();

        crate::common::store_event(ingest_dir, project.id, &event_id, &event_bytes)
            .await
            .expect("Failed to store event");

        let metadata = EventMetadata {
            event_id: event_id.clone(),
            project_id: project.id,
            ingested_at: Utc::now(),
            remote_addr: None,
        };

        process_error_event(
            &db.pool,
            &metadata,
            ingest_dir,
            &rate_limit_config,
            crate::common::null_sourcemap_provider(),
        )
        .await
        .expect("Failed to process event");
    }

    // Should have 1 issue grouped by log message
    let (issues, _) = IssueService::list_offset(
        &db.pool,
        &rustrak::models::rbac::ProjectScope::Only(vec![project.id]),
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
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

    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].calculated_type, "Log Message");
    assert_eq!(issues[0].digested_event_count, 2);
}

#[actix_web::test]
async fn test_digest_updates_issue_last_seen() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Last Seen Project").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let ingest_dir = temp_dir.path();
    let rate_limit_config = crate::common::rate_limit();

    let event_id1 = Uuid::new_v4().to_string().replace("-", "");
    let event_json1 = json!({
        "event_id": &event_id1,
        "timestamp": Utc::now().timestamp() as f64,
        "platform": "rust",
        "level": "error",
        "exception": {
            "values": [{
                "type": "TestError",
                "value": "Test"
            }]
        }
    });
    let event_bytes1 = serde_json::to_vec(&event_json1).unwrap();

    crate::common::store_event(ingest_dir, project.id, &event_id1, &event_bytes1)
        .await
        .expect("Failed to store event");

    let metadata1 = EventMetadata {
        event_id: event_id1.clone(),
        project_id: project.id,
        ingested_at: Utc::now(),
        remote_addr: None,
    };

    process_error_event(
        &db.pool,
        &metadata1,
        ingest_dir,
        &rate_limit_config,
        crate::common::null_sourcemap_provider(),
    )
    .await
    .expect("Failed to process event");

    let (issues_before, _) = IssueService::list_offset(
        &db.pool,
        &rustrak::models::rbac::ProjectScope::Only(vec![project.id]),
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
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

    let first_seen = issues_before[0].first_seen;
    let last_seen_before = issues_before[0].last_seen;

    let later = Utc::now() + chrono::Duration::seconds(60);

    // Create second event (same error type)
    let event_id2 = Uuid::new_v4().to_string().replace("-", "");
    let event_json2 = json!({
        "event_id": &event_id2,
        "timestamp": later.timestamp() as f64,
        "platform": "rust",
        "level": "error",
        "exception": {
            "values": [{
                "type": "TestError",
                "value": "Test"
            }]
        }
    });
    let event_bytes2 = serde_json::to_vec(&event_json2).unwrap();

    crate::common::store_event(ingest_dir, project.id, &event_id2, &event_bytes2)
        .await
        .expect("Failed to store event");

    let metadata2 = EventMetadata {
        event_id: event_id2.clone(),
        project_id: project.id,
        ingested_at: later,
        remote_addr: None,
    };

    process_error_event(
        &db.pool,
        &metadata2,
        ingest_dir,
        &rate_limit_config,
        crate::common::null_sourcemap_provider(),
    )
    .await
    .expect("Failed to process event");

    let (issues_after, _) = IssueService::list_offset(
        &db.pool,
        &rustrak::models::rbac::ProjectScope::Only(vec![project.id]),
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
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

    assert_eq!(issues_after[0].first_seen, first_seen);
    assert!(issues_after[0].last_seen > last_seen_before);
}

#[actix_web::test]
async fn test_digest_updates_project_counters() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Counter Test Project").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let ingest_dir = temp_dir.path();
    let rate_limit_config = crate::common::rate_limit();

    let initial_count = project.stored_event_count;

    for _ in 0..3 {
        let event_id = Uuid::new_v4().to_string().replace("-", "");
        let event_json = create_event_json(&event_id);
        let event_bytes = serde_json::to_vec(&event_json).unwrap();

        crate::common::store_event(ingest_dir, project.id, &event_id, &event_bytes)
            .await
            .expect("Failed to store event");

        let metadata = EventMetadata {
            event_id: event_id.clone(),
            project_id: project.id,
            ingested_at: Utc::now(),
            remote_addr: None,
        };

        process_error_event(
            &db.pool,
            &metadata,
            ingest_dir,
            &rate_limit_config,
            crate::common::null_sourcemap_provider(),
        )
        .await
        .expect("Failed to process event");
    }

    let updated_project = ProjectService::get_by_id(&db.pool, project.id)
        .await
        .expect("Failed to get project");

    assert_eq!(updated_project.stored_event_count, initial_count + 3);
}

// =============================================================================
// Project Platform Auto-Detection Tests (Sentry parity)
//
// Mirrors sentry.event_manager._set_project_platform_if_needed: set once from
// the first event whose top-level `platform` is a valid Relay VALID_PLATFORMS
// value, never overwritten by later events. A manual override IS possible via
// ProjectService::update() (see projects_api_test.rs) — that's a distinct,
// user-driven path and intentionally bypasses this "never overwritten" rule.
// =============================================================================

#[actix_web::test]
async fn test_digest_sets_project_platform_from_first_valid_event() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Platform Detect Project").await;
    assert_eq!(project.platform, None);

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let ingest_dir = temp_dir.path();
    let rate_limit_config = crate::common::rate_limit();

    let event_id = Uuid::new_v4().to_string().replace("-", "");
    let mut event_json = create_event_json(&event_id);
    event_json["platform"] = json!("python");
    let event_bytes = serde_json::to_vec(&event_json).unwrap();

    crate::common::store_event(ingest_dir, project.id, &event_id, &event_bytes)
        .await
        .expect("Failed to store event");

    let metadata = EventMetadata {
        event_id: event_id.clone(),
        project_id: project.id,
        ingested_at: Utc::now(),
        remote_addr: None,
    };

    process_error_event(
        &db.pool,
        &metadata,
        ingest_dir,
        &rate_limit_config,
        crate::common::null_sourcemap_provider(),
    )
    .await
    .expect("Failed to process event");

    let updated_project = ProjectService::get_by_id(&db.pool, project.id)
        .await
        .expect("Failed to get project");

    assert_eq!(updated_project.platform, Some("python".to_string()));
}

#[actix_web::test]
async fn test_digest_does_not_overwrite_existing_project_platform() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Platform No Overwrite Project").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let ingest_dir = temp_dir.path();
    let rate_limit_config = crate::common::rate_limit();

    // First event sets platform to "python".
    let event_id_1 = Uuid::new_v4().to_string().replace("-", "");
    let mut event_json_1 = create_event_json(&event_id_1);
    event_json_1["platform"] = json!("python");
    crate::common::store_event(
        ingest_dir,
        project.id,
        &event_id_1,
        &serde_json::to_vec(&event_json_1).unwrap(),
    )
    .await
    .expect("Failed to store event");
    process_error_event(
        &db.pool,
        &EventMetadata {
            event_id: event_id_1,
            project_id: project.id,
            ingested_at: Utc::now(),
            remote_addr: None,
        },
        ingest_dir,
        &rate_limit_config,
        crate::common::null_sourcemap_provider(),
    )
    .await
    .expect("Failed to process first event");

    // Second event, different valid platform, must NOT change it.
    let event_id_2 = Uuid::new_v4().to_string().replace("-", "");
    let mut event_json_2 = create_event_json(&event_id_2);
    event_json_2["platform"] = json!("php");
    crate::common::store_event(
        ingest_dir,
        project.id,
        &event_id_2,
        &serde_json::to_vec(&event_json_2).unwrap(),
    )
    .await
    .expect("Failed to store event");
    process_error_event(
        &db.pool,
        &EventMetadata {
            event_id: event_id_2,
            project_id: project.id,
            ingested_at: Utc::now(),
            remote_addr: None,
        },
        ingest_dir,
        &rate_limit_config,
        crate::common::null_sourcemap_provider(),
    )
    .await
    .expect("Failed to process second event");

    let updated_project = ProjectService::get_by_id(&db.pool, project.id)
        .await
        .expect("Failed to get project");

    assert_eq!(updated_project.platform, Some("python".to_string()));
}

#[actix_web::test]
async fn test_digest_ignores_invalid_platform_for_project_platform_detection() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Invalid Platform Project").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let ingest_dir = temp_dir.path();
    let rate_limit_config = crate::common::rate_limit();

    // create_event_json() already uses "platform": "rust", which is NOT in
    // Relay's VALID_PLATFORMS (confirmed against relay-event-schema source).
    let event_id = Uuid::new_v4().to_string().replace("-", "");
    let event_json = create_event_json(&event_id);
    crate::common::store_event(
        ingest_dir,
        project.id,
        &event_id,
        &serde_json::to_vec(&event_json).unwrap(),
    )
    .await
    .expect("Failed to store event");
    process_error_event(
        &db.pool,
        &EventMetadata {
            event_id,
            project_id: project.id,
            ingested_at: Utc::now(),
            remote_addr: None,
        },
        ingest_dir,
        &rate_limit_config,
        crate::common::null_sourcemap_provider(),
    )
    .await
    .expect("Failed to process event");

    let updated_project = ProjectService::get_by_id(&db.pool, project.id)
        .await
        .expect("Failed to get project");

    assert_eq!(updated_project.platform, None);
}

/// Guards the boundary between the two platform lists. `javascript-nextjs` is
/// a legal *project* platform (`SELECTABLE_PLATFORMS`, chosen by a user in
/// settings) but never a legal *event* platform: Relay strips anything outside
/// `VALID_PLATFORMS` from an event and substitutes `"other"`. Auto-detection
/// reads event platforms, so it must keep using the narrow list. This test
/// fails the moment someone points `infer_platform_from_event` at the
/// selectable list.
#[actix_web::test]
async fn test_digest_ignores_framework_specific_platform_for_detection() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Framework Platform Detect Project").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let ingest_dir = temp_dir.path();
    let rate_limit_config = crate::common::rate_limit();

    let event_id = Uuid::new_v4().to_string().replace("-", "");
    let mut event_json = create_event_json(&event_id);
    event_json["platform"] = json!("javascript-nextjs");
    crate::common::store_event(
        ingest_dir,
        project.id,
        &event_id,
        &serde_json::to_vec(&event_json).unwrap(),
    )
    .await
    .expect("Failed to store event");
    process_error_event(
        &db.pool,
        &EventMetadata {
            event_id,
            project_id: project.id,
            ingested_at: Utc::now(),
            remote_addr: None,
        },
        ingest_dir,
        &rate_limit_config,
        crate::common::null_sourcemap_provider(),
    )
    .await
    .expect("Failed to process event");

    let updated_project = ProjectService::get_by_id(&db.pool, project.id)
        .await
        .expect("Failed to get project");

    assert_eq!(updated_project.platform, None);
}

#[actix_web::test]
async fn test_process_event_cleans_up_temp_file_on_failure() {
    let db = TestDb::new().await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let ingest_dir = temp_dir.path();
    let rate_limit_config = crate::common::rate_limit();

    let event_id = Uuid::new_v4().to_string().replace("-", "");
    let event_json = create_event_json(&event_id);
    let event_bytes = serde_json::to_vec(&event_json).unwrap();

    let event_path = crate::common::store_event(ingest_dir, 99999, &event_id, &event_bytes)
        .await
        .expect("Failed to store event");

    assert!(
        event_path.exists(),
        "file must exist before calling process_event"
    );

    // project_id 99999 does not exist → process_event returns Err immediately
    let metadata = EventMetadata {
        event_id: event_id.clone(),
        project_id: 99999,
        ingested_at: Utc::now(),
        remote_addr: None,
    };

    let result = process_error_event(
        &db.pool,
        &metadata,
        ingest_dir,
        &rate_limit_config,
        crate::common::null_sourcemap_provider(),
    )
    .await;
    assert!(
        result.is_err(),
        "process_event must fail for non-existent project"
    );

    assert!(
        !event_path.exists(),
        "temp file must be deleted even when process_event fails"
    );
}

#[actix_web::test]
async fn test_digest_handles_missing_exception() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "No Exception Project").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let ingest_dir = temp_dir.path();
    let rate_limit_config = crate::common::rate_limit();

    // Event without exception or message
    let event_id = Uuid::new_v4().to_string().replace("-", "");
    let event_json = json!({
        "event_id": &event_id,
        "timestamp": Utc::now().timestamp() as f64,
        "platform": "rust",
        "level": "error"
    });
    let event_bytes = serde_json::to_vec(&event_json).unwrap();

    crate::common::store_event(ingest_dir, project.id, &event_id, &event_bytes)
        .await
        .expect("Failed to store event");

    let metadata = EventMetadata {
        event_id: event_id.clone(),
        project_id: project.id,
        ingested_at: Utc::now(),
        remote_addr: None,
    };

    // Should still process successfully with fallback grouping
    process_error_event(
        &db.pool,
        &metadata,
        ingest_dir,
        &rate_limit_config,
        crate::common::null_sourcemap_provider(),
    )
    .await
    .expect("Failed to process event");

    let (issues, _) = IssueService::list_offset(
        &db.pool,
        &rustrak::models::rbac::ProjectScope::Only(vec![project.id]),
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
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

    assert_eq!(issues.len(), 1);
}

#[actix_web::test]
async fn test_digest_handles_multiline_error_value() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Multiline Error Project").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let ingest_dir = temp_dir.path();
    let rate_limit_config = crate::common::rate_limit();

    let event_id = Uuid::new_v4().to_string().replace("-", "");
    let event_json = json!({
        "event_id": &event_id,
        "timestamp": Utc::now().timestamp() as f64,
        "platform": "rust",
        "level": "error",
        "exception": {
            "values": [{
                "type": "AssertionError",
                "value": "Expected 1 but got 2\n  at test.rs:42\n  at main.rs:10"
            }]
        }
    });
    let event_bytes = serde_json::to_vec(&event_json).unwrap();

    crate::common::store_event(ingest_dir, project.id, &event_id, &event_bytes)
        .await
        .expect("Failed to store event");

    let metadata = EventMetadata {
        event_id: event_id.clone(),
        project_id: project.id,
        ingested_at: Utc::now(),
        remote_addr: None,
    };

    process_error_event(
        &db.pool,
        &metadata,
        ingest_dir,
        &rate_limit_config,
        crate::common::null_sourcemap_provider(),
    )
    .await
    .expect("Failed to process event");

    let (issues, _) = IssueService::list_offset(
        &db.pool,
        &rustrak::models::rbac::ProjectScope::Only(vec![project.id]),
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
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

    assert_eq!(issues.len(), 1);
    // Should only use first line in title
    assert!(!issues[0].title().contains('\n'));
}

#[actix_web::test]
async fn test_digest_cleans_up_temp_file() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Cleanup Test Project").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let ingest_dir = temp_dir.path();
    let rate_limit_config = crate::common::rate_limit();

    let event_id = Uuid::new_v4().to_string().replace("-", "");
    let event_json = create_event_json(&event_id);
    let event_bytes = serde_json::to_vec(&event_json).unwrap();

    let file_path = crate::common::store_event(ingest_dir, project.id, &event_id, &event_bytes)
        .await
        .expect("Failed to store event");

    assert!(file_path.exists());

    let metadata = EventMetadata {
        event_id: event_id.clone(),
        project_id: project.id,
        ingested_at: Utc::now(),
        remote_addr: None,
    };

    process_error_event(
        &db.pool,
        &metadata,
        ingest_dir,
        &rate_limit_config,
        crate::common::null_sourcemap_provider(),
    )
    .await
    .expect("Failed to process event");

    // Deleted after processing, once its commit is durable.
    crate::common::eventually(|| async { (!file_path.exists()).then_some(()) }).await;
}

/// Ingests a single error event with the given id through the digest pipeline.
async fn ingest_error_event(
    pool: &rustrak::db::DbPool,
    project_id: i32,
    ingest_dir: &std::path::Path,
    rate_limit_config: &RateLimitConfig,
    event_id: &str,
) {
    let event_json = create_event_json(event_id);
    let event_bytes = serde_json::to_vec(&event_json).unwrap();
    crate::common::store_event(ingest_dir, project_id, event_id, &event_bytes)
        .await
        .expect("store event");
    let metadata = EventMetadata {
        event_id: event_id.to_string(),
        project_id,
        ingested_at: Utc::now(),
        remote_addr: None,
    };
    process_error_event(
        pool,
        &metadata,
        ingest_dir,
        rate_limit_config,
        crate::common::null_sourcemap_provider(),
    )
    .await
    .expect("process event");
}

#[actix_web::test]
async fn test_new_issue_has_status_priority_and_culprit() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Status Create Project").await;
    let temp_dir = TempDir::new().unwrap();
    let cfg = crate::common::rate_limit();

    let event_id = Uuid::new_v4().to_string().replace("-", "");
    ingest_error_event(&db.pool, project.id, temp_dir.path(), &cfg, &event_id).await;

    let issue = IssueService::get_by_id(
        &db.pool,
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM issues WHERE project_id = $1")
            .bind(project.id)
            .fetch_one(&db.pool)
            .await
            .unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(issue.status, "unresolved");
    assert_eq!(issue.substatus.as_deref(), Some("new"));
    // level "error" -> high priority
    assert_eq!(issue.priority.as_deref(), Some("high"));
    // culprit derived from the in-app frame's function
    assert_eq!(issue.culprit, "handle_request");
}

#[actix_web::test]
async fn test_resolved_issue_regresses_on_new_event() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Regression Project").await;
    let temp_dir = TempDir::new().unwrap();
    let cfg = crate::common::rate_limit();

    // First event creates the issue.
    let e1 = Uuid::new_v4().to_string().replace("-", "");
    ingest_error_event(&db.pool, project.id, temp_dir.path(), &cfg, &e1).await;

    let issue_id = sqlx::query_scalar::<_, Uuid>("SELECT id FROM issues WHERE project_id = $1")
        .bind(project.id)
        .fetch_one(&db.pool)
        .await
        .unwrap();

    // Resolve it.
    IssueService::resolve(&db.pool, issue_id).await.unwrap();
    let resolved = IssueService::get_by_id(&db.pool, issue_id).await.unwrap();
    assert_eq!(resolved.status, "resolved");

    // A second event with the same grouping must reopen it as regressed.
    let e2 = Uuid::new_v4().to_string().replace("-", "");
    ingest_error_event(&db.pool, project.id, temp_dir.path(), &cfg, &e2).await;

    let reopened = IssueService::get_by_id(&db.pool, issue_id).await.unwrap();
    assert_eq!(reopened.status, "unresolved");
    assert_eq!(reopened.substatus.as_deref(), Some("regressed"));
    assert_eq!(reopened.digested_event_count, 2);
}

#[actix_web::test]
async fn test_unresolved_issue_does_not_regress() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "No Regression Project").await;
    let temp_dir = TempDir::new().unwrap();
    let cfg = crate::common::rate_limit();

    let e1 = Uuid::new_v4().to_string().replace("-", "");
    ingest_error_event(&db.pool, project.id, temp_dir.path(), &cfg, &e1).await;
    let issue_id = sqlx::query_scalar::<_, Uuid>("SELECT id FROM issues WHERE project_id = $1")
        .bind(project.id)
        .fetch_one(&db.pool)
        .await
        .unwrap();

    let e2 = Uuid::new_v4().to_string().replace("-", "");
    ingest_error_event(&db.pool, project.id, temp_dir.path(), &cfg, &e2).await;

    let issue = IssueService::get_by_id(&db.pool, issue_id).await.unwrap();
    // Substatus stays whatever it was (new), never flips to regressed.
    assert_eq!(issue.status, "unresolved");
    assert_ne!(issue.substatus.as_deref(), Some("regressed"));
}

#[actix_web::test]
async fn test_set_status_rejects_substatus_not_valid_for_status() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Invalid Pair Project").await;
    let temp_dir = TempDir::new().unwrap();
    let cfg = crate::common::rate_limit();

    let e1 = Uuid::new_v4().to_string().replace("-", "");
    ingest_error_event(&db.pool, project.id, temp_dir.path(), &cfg, &e1).await;
    let issue_id = sqlx::query_scalar::<_, Uuid>("SELECT id FROM issues WHERE project_id = $1")
        .bind(project.id)
        .fetch_one(&db.pool)
        .await
        .unwrap();

    // "escalating" is only a legal substatus under `unresolved`, never `resolved`.
    let err = IssueService::set_status(&db.pool, issue_id, "resolved", Some("escalating"))
        .await
        .expect_err("resolved+escalating must be rejected");
    assert!(matches!(err, rustrak::error::AppError::Validation(_)));

    // The issue must be left untouched by the rejected update.
    let issue = IssueService::get_by_id(&db.pool, issue_id).await.unwrap();
    assert_eq!(issue.status, "unresolved");

    // An `ignored`-only substatus under `unresolved` must be rejected too.
    let err = IssueService::set_status(&db.pool, issue_id, "unresolved", Some("archived_forever"))
        .await
        .expect_err("unresolved+archived_forever must be rejected");
    assert!(matches!(err, rustrak::error::AppError::Validation(_)));

    // A legal pairing must still succeed.
    let issue = IssueService::set_status(&db.pool, issue_id, "ignored", Some("archived_forever"))
        .await
        .expect("ignored+archived_forever is a legal pairing");
    assert_eq!(issue.status, "ignored");
    assert_eq!(issue.substatus.as_deref(), Some("archived_forever"));
}

/// Ingests an event with explicit tags + user, keeping the grouping stable so
/// all events land on the same issue.
async fn ingest_event_with_tags(
    pool: &rustrak::db::DbPool,
    project_id: i32,
    ingest_dir: &std::path::Path,
    cfg: &RateLimitConfig,
    tags: serde_json::Value,
    user: serde_json::Value,
) {
    let event_id = Uuid::new_v4().to_string().replace("-", "");
    let event_json = json!({
        "event_id": event_id,
        "timestamp": Utc::now().timestamp() as f64,
        "platform": "rust",
        "level": "error",
        "transaction": "/api/users",
        "release": "v1.0.0",
        "tags": tags,
        "user": user,
        "exception": {
            "values": [{
                "type": "TypeError",
                "value": "Cannot read property 'x' of null",
                "stacktrace": { "frames": [{
                    "filename": "app.rs", "function": "handle_request",
                    "lineno": 42, "in_app": true
                }]}
            }]
        }
    });
    let bytes = serde_json::to_vec(&event_json).unwrap();
    crate::common::store_event(ingest_dir, project_id, &event_id, &bytes)
        .await
        .unwrap();
    let metadata = EventMetadata {
        event_id: event_id.clone(),
        project_id,
        ingested_at: Utc::now(),
        remote_addr: None,
    };
    process_error_event(
        pool,
        &metadata,
        ingest_dir,
        cfg,
        crate::common::null_sourcemap_provider(),
    )
    .await
    .unwrap();
}

async fn only_issue_id(pool: &rustrak::db::DbPool, project_id: i32) -> Uuid {
    sqlx::query_scalar::<_, Uuid>("SELECT id FROM issues WHERE project_id = $1 LIMIT 1")
        .bind(project_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[actix_web::test]
async fn test_list_hashes_returns_grouping() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Hashes Project").await;
    let temp_dir = TempDir::new().unwrap();
    let cfg = crate::common::rate_limit();

    let e1 = Uuid::new_v4().to_string().replace("-", "");
    ingest_error_event(&db.pool, project.id, temp_dir.path(), &cfg, &e1).await;
    let issue_id = only_issue_id(&db.pool, project.id).await;

    let hashes = IssueService::list_hashes(&db.pool, issue_id).await.unwrap();
    assert_eq!(hashes.len(), 1);
    assert_eq!(hashes[0].issue_id, issue_id);
    assert_eq!(hashes[0].grouping_key_hash.len(), 64);
}

#[actix_web::test]
async fn test_release_populated_on_issue() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Release Project").await;
    let temp_dir = TempDir::new().unwrap();
    let cfg = crate::common::rate_limit();

    ingest_event_with_tags(
        &db.pool,
        project.id,
        temp_dir.path(),
        &cfg,
        json!({}),
        json!({"id": "u1"}),
    )
    .await;
    let issue_id = only_issue_id(&db.pool, project.id).await;
    let issue = IssueService::get_by_id(&db.pool, issue_id).await.unwrap();
    assert_eq!(issue.first_release, "v1.0.0");
    assert_eq!(issue.last_release, "v1.0.0");
}

/// A bulk status change is recorded in each issue's activity, like a single
/// update; an id of another project is neither changed nor recorded.
#[actix_web::test]
async fn test_bulk_status_change_records_activity() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Bulk Activity").await;
    let other = create_test_project(&db.pool, "Bulk Activity Other").await;
    use rustrak::services::grouping::get_denormalized_fields;
    let fields =
        get_denormalized_fields(&json!({"exception":{"values":[{"type":"A","value":"a"}]}}));
    let mine = IssueService::create(
        &db.pool,
        project.id,
        Utc::now(),
        &fields,
        Some("error"),
        None,
    )
    .await
    .unwrap();
    let theirs = IssueService::create(&db.pool, other.id, Utc::now(), &fields, Some("error"), None)
        .await
        .unwrap();

    let updated = IssueService::bulk_update(
        &db.pool,
        project.id,
        &[mine.id, theirs.id],
        Some("resolvedInNextRelease"),
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(updated, 1);

    let activity = IssueSocialService::list_activity(&db.pool, mine.id)
        .await
        .unwrap();
    assert_eq!(activity.len(), 1);
    assert_eq!(activity[0].activity_type, "set_status");
    assert!(activity[0].data.contains("resolvedInNextRelease"));
    assert!(IssueSocialService::list_activity(&db.pool, theirs.id)
        .await
        .unwrap()
        .is_empty());
}

#[actix_web::test]
async fn test_bulk_set_status_and_delete() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Bulk Project").await;

    // Two distinct issues via direct service create.
    use rustrak::services::grouping::get_denormalized_fields;
    let d1 = get_denormalized_fields(&json!({"exception":{"values":[{"type":"A","value":"a"}]}}));
    let d2 = get_denormalized_fields(&json!({"exception":{"values":[{"type":"B","value":"b"}]}}));
    let i1 = IssueService::create(
        &db.pool,
        project.id,
        Utc::now(),
        &d1,
        Some("error"),
        Some("rust"),
    )
    .await
    .unwrap();
    let i2 = IssueService::create(
        &db.pool,
        project.id,
        Utc::now(),
        &d2,
        Some("error"),
        Some("rust"),
    )
    .await
    .unwrap();

    let mut tx = db.pool.begin().await.unwrap();
    let updated =
        IssueService::bulk_set_status(&mut tx, project.id, &[i1.id, i2.id], "resolved", None)
            .await
            .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(updated, 2);
    assert_eq!(
        IssueService::get_by_id(&db.pool, i1.id)
            .await
            .unwrap()
            .status,
        "resolved"
    );
    assert_eq!(
        IssueService::get_by_id(&db.pool, i2.id)
            .await
            .unwrap()
            .status,
        "resolved"
    );

    let errors =
        crate::common::processors_with_config(&db.pool, &rustrak::config::Config::default()).errors;
    let deleted = IssueService::bulk_delete(&db.pool, project.id, &[i1.id, i2.id], &errors)
        .await
        .unwrap();
    assert_eq!(deleted, 2);
    assert!(IssueService::get_by_id(&db.pool, i1.id).await.is_err());
}

#[actix_web::test]
async fn test_bulk_set_priority_updates_only_ids_in_project() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Bulk Priority Project").await;
    let other_project = create_test_project(&db.pool, "Other Project").await;

    use rustrak::services::grouping::get_denormalized_fields;
    let d = get_denormalized_fields(&json!({"exception":{"values":[{"type":"A","value":"a"}]}}));
    let i1 = IssueService::create(
        &db.pool,
        project.id,
        Utc::now(),
        &d,
        Some("error"),
        Some("rust"),
    )
    .await
    .unwrap();
    let i2 = IssueService::create(
        &db.pool,
        project.id,
        Utc::now(),
        &d,
        Some("error"),
        Some("rust"),
    )
    .await
    .unwrap();
    // An issue in a different project must not be touched even if its id is
    // (mistakenly or maliciously) included in the request. Created with
    // level "info" (derives to "low" priority) so a leaked update to "high"
    // is actually observable, rather than starting at "high" already.
    let other = IssueService::create(
        &db.pool,
        other_project.id,
        Utc::now(),
        &d,
        Some("info"),
        Some("rust"),
    )
    .await
    .unwrap();

    let mut tx = db.pool.begin().await.unwrap();
    let updated =
        IssueService::bulk_set_priority(&mut tx, project.id, &[i1.id, i2.id, other.id], "high")
            .await
            .unwrap();
    tx.commit().await.unwrap();

    // Only the two issues actually in `project` are counted/updated.
    assert_eq!(updated, 2);
    assert_eq!(
        IssueService::get_by_id(&db.pool, i1.id)
            .await
            .unwrap()
            .priority
            .as_deref(),
        Some("high")
    );
    assert_eq!(
        IssueService::get_by_id(&db.pool, i2.id)
            .await
            .unwrap()
            .priority
            .as_deref(),
        Some("high")
    );
    assert_ne!(
        IssueService::get_by_id(&db.pool, other.id)
            .await
            .unwrap()
            .priority
            .as_deref(),
        Some("high")
    );
}

#[actix_web::test]
async fn test_bulk_set_priority_rejects_invalid_priority() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Bulk Priority Invalid Project").await;

    use rustrak::services::grouping::get_denormalized_fields;
    let d = get_denormalized_fields(&json!({"exception":{"values":[{"type":"A","value":"a"}]}}));
    let i1 = IssueService::create(
        &db.pool,
        project.id,
        Utc::now(),
        &d,
        Some("error"),
        Some("rust"),
    )
    .await
    .unwrap();

    let mut tx = db.pool.begin().await.unwrap();
    let err = IssueService::bulk_set_priority(&mut tx, project.id, &[i1.id], "urgent")
        .await
        .expect_err("invalid priority must be rejected");
    assert!(matches!(err, rustrak::error::AppError::Validation(_)));
}

#[actix_web::test]
async fn test_tag_values_and_aggregates() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Tags Project").await;
    let temp_dir = TempDir::new().unwrap();
    let cfg = crate::common::rate_limit();

    ingest_event_with_tags(
        &db.pool,
        project.id,
        temp_dir.path(),
        &cfg,
        json!({"browser": "chrome"}),
        json!({"id": "user-1"}),
    )
    .await;
    ingest_event_with_tags(
        &db.pool,
        project.id,
        temp_dir.path(),
        &cfg,
        json!({"browser": "chrome"}),
        json!({"id": "user-2"}),
    )
    .await;
    ingest_event_with_tags(
        &db.pool,
        project.id,
        temp_dir.path(),
        &cfg,
        json!({"browser": "firefox"}),
        json!({"id": "user-1"}),
    )
    .await;

    let issue_id = only_issue_id(&db.pool, project.id).await;

    let values = IssueService::tag_values(&db.pool, issue_id, "browser")
        .await
        .unwrap();
    // chrome (2) ranks before firefox (1)
    assert_eq!(values[0].value, "chrome");
    assert_eq!(values[0].count, 2);
    assert!(values.iter().any(|v| v.value == "firefox" && v.count == 1));
    // Sentry-compatible shape: each value carries its own key/name and a
    // seen range, not just value+count.
    for v in &values {
        assert_eq!(v.key, "browser");
        assert_eq!(v.name, "browser");
        assert!(v.first_seen <= v.last_seen);
    }

    let agg = IssueService::aggregates(&db.pool, issue_id).await.unwrap();
    assert_eq!(agg.user_count, 2); // user-1, user-2
    let browser = agg.tags.iter().find(|t| t.key == "browser").unwrap();
    // Sentry's `totalValues`: the events carrying the tag (chrome 2 +
    // firefox 1), not how many distinct values it has.
    assert_eq!(browser.total_values, 3);
}

#[actix_web::test]
async fn test_aggregates_count_a_repeated_tag_key_once_per_event() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Repeated Tags Project").await;
    let temp_dir = TempDir::new().unwrap();
    let cfg = crate::common::rate_limit();

    // One event sending `browser` twice, in the list form of `tags`.
    ingest_event_with_tags(
        &db.pool,
        project.id,
        temp_dir.path(),
        &cfg,
        json!([["browser", "chrome"], ["browser", "firefox"]]),
        json!({"id": "user-1"}),
    )
    .await;

    let issue_id = only_issue_id(&db.pool, project.id).await;
    let agg = IssueService::aggregates(&db.pool, issue_id).await.unwrap();
    let browser = agg.tags.iter().find(|t| t.key == "browser").unwrap();

    // Relay deduplicates tags and keeps the first: one event, one value.
    assert_eq!(browser.total_values, 1);
    assert_eq!(browser.top_values.len(), 1);
    assert_eq!(browser.top_values[0].value, "chrome");
}

#[actix_web::test]
async fn test_list_stats_counts_distinct_users_per_issue() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "List Stats Project").await;
    let temp_dir = TempDir::new().unwrap();
    let cfg = crate::common::rate_limit();

    ingest_event_with_tags(
        &db.pool,
        project.id,
        temp_dir.path(),
        &cfg,
        json!({}),
        json!({"id": "user-1"}),
    )
    .await;
    ingest_event_with_tags(
        &db.pool,
        project.id,
        temp_dir.path(),
        &cfg,
        json!({}),
        json!({"id": "user-2"}),
    )
    .await;

    let issue_id = only_issue_id(&db.pool, project.id).await;

    let stats = IssueService::list_stats(&db.pool, &[issue_id], Default::default())
        .await
        .unwrap();
    let entry = stats.get(&issue_id).expect("stats for the issue");
    assert_eq!(entry.user_count, 2);
}

#[actix_web::test]
async fn test_list_stats_trend_has_24_buckets_summing_to_event_count() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Trend Project").await;
    let temp_dir = TempDir::new().unwrap();
    let cfg = crate::common::rate_limit();

    let e1 = Uuid::new_v4().to_string().replace("-", "");
    ingest_error_event(&db.pool, project.id, temp_dir.path(), &cfg, &e1).await;
    let issue_id = only_issue_id(&db.pool, project.id).await;

    let stats = IssueService::list_stats(&db.pool, &[issue_id], Default::default())
        .await
        .unwrap();
    let entry = stats.get(&issue_id).expect("stats for the issue");

    assert_eq!(entry.trend.len(), 24);
    let total: i64 = entry.trend.iter().sum();
    assert_eq!(total, 1, "the one event should be counted exactly once");
    // Freshly ingested, so it lands in one of the most recent buckets.
    let recent: i64 = entry.trend.iter().rev().take(2).sum();
    assert_eq!(recent, 1);
}

#[actix_web::test]
async fn test_list_stats_counts_by_email_when_id_is_absent() {
    // list_stats's SQL projects just `data`'s `user` sub-field (not the
    // whole event) for the issue-list hot path — this locks in that the
    // id > email > username > ip_address fallback still works once only the
    // `user` object survives the projection, on events that otherwise carry
    // a large unrelated payload (exception/stacktrace) alongside it.
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "List Stats Email Fallback Project").await;
    let temp_dir = TempDir::new().unwrap();
    let cfg = crate::common::rate_limit();

    ingest_event_with_tags(
        &db.pool,
        project.id,
        temp_dir.path(),
        &cfg,
        json!({}),
        json!({"email": "a@example.com"}),
    )
    .await;
    ingest_event_with_tags(
        &db.pool,
        project.id,
        temp_dir.path(),
        &cfg,
        json!({}),
        json!({"email": "a@example.com"}),
    )
    .await;
    ingest_event_with_tags(
        &db.pool,
        project.id,
        temp_dir.path(),
        &cfg,
        json!({}),
        json!({"email": "b@example.com"}),
    )
    .await;
    // No `user` field at all — must not be counted, and must not error the
    // NULL projection path.
    ingest_error_event(
        &db.pool,
        project.id,
        temp_dir.path(),
        &cfg,
        &Uuid::new_v4().to_string().replace("-", ""),
    )
    .await;

    let issue_id = only_issue_id(&db.pool, project.id).await;

    let stats = IssueService::list_stats(&db.pool, &[issue_id], Default::default())
        .await
        .unwrap();
    let entry = stats.get(&issue_id).expect("stats for the issue");
    assert_eq!(entry.user_count, 2, "a@example.com and b@example.com");
}

#[actix_web::test]
async fn test_list_stats_counts_users_by_the_first_non_empty_string_field() {
    // Only a non-empty string counts: a numeric id falls back to the email,
    // an empty one to the username. The same user seen twice counts once.
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "List Stats Identity Project").await;
    let temp_dir = TempDir::new().unwrap();
    let cfg = crate::common::rate_limit();

    for user in [
        json!({"id": 42, "email": "a@example.com"}),
        json!({"email": "a@example.com"}),
        json!({"id": "", "username": "bob"}),
        json!({"ip_address": "10.0.0.1"}),
        json!("not an object"),
    ] {
        ingest_event_with_tags(&db.pool, project.id, temp_dir.path(), &cfg, json!({}), user).await;
    }

    let issue_id = only_issue_id(&db.pool, project.id).await;
    let stats = IssueService::list_stats(&db.pool, &[issue_id], Default::default())
        .await
        .unwrap();
    let entry = stats.get(&issue_id).expect("stats for the issue");
    assert_eq!(
        entry.user_count, 3,
        "email:a@, username:bob, ip_address:10.0.0.1"
    );
    assert_eq!(entry.trend.iter().sum::<i64>(), 5);
}

#[actix_web::test]
async fn test_list_offset_search_filters_by_text() {
    use rustrak::pagination::{IssueFilter, IssueSort, SortOrder};
    use rustrak::services::grouping::get_denormalized_fields;
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Search Project").await;

    let d1 = get_denormalized_fields(
        &json!({"exception":{"values":[{"type":"DatabaseError","value":"connection refused"}]}}),
    );
    let d2 = get_denormalized_fields(
        &json!({"exception":{"values":[{"type":"TypeError","value":"undefined is not a function"}]}}),
    );
    IssueService::create(
        &db.pool,
        project.id,
        Utc::now(),
        &d1,
        Some("error"),
        Some("rust"),
    )
    .await
    .unwrap();
    IssueService::create(
        &db.pool,
        project.id,
        Utc::now(),
        &d2,
        Some("error"),
        Some("rust"),
    )
    .await
    .unwrap();

    // Case-insensitive match on type.
    let (hits, total) = IssueService::list_offset(
        &db.pool,
        &rustrak::models::rbac::ProjectScope::Only(vec![project.id]),
        IssueSort::DigestOrder,
        SortOrder::Desc,
        IssueFilter::All,
        rustrak::pagination::IssueCategory::Error,
        1,
        50,
        Some("databaseerror"),
        Scope::default(),
        &rustrak::services::issue::IssueFilters::default(),
    )
    .await
    .unwrap();
    assert_eq!(total, 1);
    assert_eq!(hits[0].calculated_type, "DatabaseError");

    // Match on value substring.
    let (hits, _) = IssueService::list_offset(
        &db.pool,
        &rustrak::models::rbac::ProjectScope::Only(vec![project.id]),
        IssueSort::DigestOrder,
        SortOrder::Desc,
        IssueFilter::All,
        rustrak::pagination::IssueCategory::Error,
        1,
        50,
        Some("undefined"),
        Scope::default(),
        &rustrak::services::issue::IssueFilters::default(),
    )
    .await
    .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].calculated_type, "TypeError");

    // No match.
    let (_, total) = IssueService::list_offset(
        &db.pool,
        &rustrak::models::rbac::ProjectScope::Only(vec![project.id]),
        IssueSort::DigestOrder,
        SortOrder::Desc,
        IssueFilter::All,
        rustrak::pagination::IssueCategory::Error,
        1,
        50,
        Some("nonexistent"),
        Scope::default(),
        &rustrak::services::issue::IssueFilters::default(),
    )
    .await
    .unwrap();
    assert_eq!(total, 0);
}

#[actix_web::test]
async fn test_list_offset_search_escapes_like_wildcards() {
    // `_` is a single-char LIKE wildcard; an unescaped search for a literal
    // underscore would also match any other single character in that
    // position. Two issues differing only by "_" vs "x" in that spot must
    // NOT both match a search for the literal underscore.
    use rustrak::pagination::{IssueFilter, IssueSort, SortOrder};
    use rustrak::services::grouping::get_denormalized_fields;
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Search Escape Project").await;

    let with_underscore = get_denormalized_fields(
        &json!({"exception":{"values":[{"type":"Error","value":"database_error"}]}}),
    );
    let without_underscore = get_denormalized_fields(
        &json!({"exception":{"values":[{"type":"Error","value":"databasexerror"}]}}),
    );
    IssueService::create(
        &db.pool,
        project.id,
        Utc::now(),
        &with_underscore,
        Some("error"),
        Some("rust"),
    )
    .await
    .unwrap();
    IssueService::create(
        &db.pool,
        project.id,
        Utc::now(),
        &without_underscore,
        Some("error"),
        Some("rust"),
    )
    .await
    .unwrap();

    let (hits, total) = IssueService::list_offset(
        &db.pool,
        &rustrak::models::rbac::ProjectScope::Only(vec![project.id]),
        IssueSort::DigestOrder,
        SortOrder::Desc,
        IssueFilter::All,
        rustrak::pagination::IssueCategory::Error,
        1,
        50,
        Some("database_error"),
        Scope::default(),
        &rustrak::services::issue::IssueFilters::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        total, 1,
        "literal '_' in the search term must not act as a LIKE wildcard"
    );
    assert_eq!(hits[0].calculated_value, "database_error");
}

#[actix_web::test]
async fn test_list_offset_rejects_page_below_one() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Page Clamp Project").await;

    let err = IssueService::list_offset(
        &db.pool,
        &rustrak::models::rbac::ProjectScope::Only(vec![project.id]),
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
        rustrak::pagination::IssueFilter::All,
        rustrak::pagination::IssueCategory::Error,
        0,
        20,
        None,
        Scope::default(),
        &rustrak::services::issue::IssueFilters::default(),
    )
    .await
    .expect_err("page=0 must be rejected, not produce a negative SQL OFFSET");
    assert!(matches!(err, rustrak::error::AppError::Validation(_)));

    let err = IssueService::list_offset(
        &db.pool,
        &rustrak::models::rbac::ProjectScope::Only(vec![project.id]),
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
        rustrak::pagination::IssueFilter::All,
        rustrak::pagination::IssueCategory::Error,
        -5,
        20,
        None,
        Scope::default(),
        &rustrak::services::issue::IssueFilters::default(),
    )
    .await
    .expect_err("negative page must be rejected");
    assert!(matches!(err, rustrak::error::AppError::Validation(_)));
}

#[actix_web::test]
async fn test_list_offset_rejects_per_page_out_of_range() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Per Page Clamp Project").await;

    let err = IssueService::list_offset(
        &db.pool,
        &rustrak::models::rbac::ProjectScope::Only(vec![project.id]),
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
        rustrak::pagination::IssueFilter::All,
        rustrak::pagination::IssueCategory::Error,
        1,
        0,
        None,
        Scope::default(),
        &rustrak::services::issue::IssueFilters::default(),
    )
    .await
    .expect_err("per_page=0 must be rejected");
    assert!(matches!(err, rustrak::error::AppError::Validation(_)));

    let err = IssueService::list_offset(
        &db.pool,
        &rustrak::models::rbac::ProjectScope::Only(vec![project.id]),
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
        rustrak::pagination::IssueFilter::All,
        rustrak::pagination::IssueCategory::Error,
        1,
        101,
        None,
        Scope::default(),
        &rustrak::services::issue::IssueFilters::default(),
    )
    .await
    .expect_err("per_page over the documented max of 100 must be rejected");
    assert!(matches!(err, rustrak::error::AppError::Validation(_)));
}

async fn seed_user_for_social(pool: &rustrak::db::DbPool, email: &str) -> i32 {
    use rustrak::models::{CreateUserRequest, UserRole};
    use rustrak::services::users::UsersService;
    UsersService::create_user(
        pool,
        &CreateUserRequest {
            email: email.to_string(),
            password: "password123".to_string(),
        },
        UserRole::Admin,
    )
    .await
    .unwrap()
    .id
}

async fn seed_issue_direct(pool: &rustrak::db::DbPool, project_id: i32, t: &str) -> Uuid {
    use rustrak::services::grouping::get_denormalized_fields;
    let d = get_denormalized_fields(&json!({"exception":{"values":[{"type":t,"value":"x"}]}}));
    IssueService::create(
        pool,
        project_id,
        Utc::now(),
        &d,
        Some("error"),
        Some("rust"),
    )
    .await
    .unwrap()
    .id
}

#[actix_web::test]
async fn test_activity_log_and_comments() {
    use rustrak::services::IssueSocialService;
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Activity Project").await;
    let user_id = seed_user_for_social(&db.pool, "act@x.com").await;
    let issue_id = seed_issue_direct(&db.pool, project.id, "ActErr").await;

    IssueSocialService::record_status_change(&db.pool, issue_id, Some(user_id), "resolved")
        .await
        .unwrap();
    let note =
        IssueSocialService::add_comment(&db.pool, issue_id, Some(user_id), "looking into it")
            .await
            .unwrap();
    assert_eq!(note.activity_type, "note");
    assert!(note.data.contains("looking into it"));

    let activity = IssueSocialService::list_activity(&db.pool, issue_id)
        .await
        .unwrap();
    assert_eq!(activity.len(), 2);
    // newest first → the note
    assert_eq!(activity[0].activity_type, "note");
    assert_eq!(activity[1].activity_type, "set_status");
}

#[actix_web::test]
async fn test_bookmark_and_subscription_toggle() {
    use rustrak::services::IssueSocialService;
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Bookmark Project").await;
    let user_id = seed_user_for_social(&db.pool, "bm@x.com").await;
    let issue_id = seed_issue_direct(&db.pool, project.id, "BmErr").await;

    assert!(
        !IssueSocialService::is_bookmarked(&db.pool, issue_id, user_id)
            .await
            .unwrap()
    );
    IssueSocialService::set_bookmark(&db.pool, issue_id, user_id, true)
        .await
        .unwrap();
    assert!(
        IssueSocialService::is_bookmarked(&db.pool, issue_id, user_id)
            .await
            .unwrap()
    );
    // idempotent
    IssueSocialService::set_bookmark(&db.pool, issue_id, user_id, true)
        .await
        .unwrap();
    IssueSocialService::set_bookmark(&db.pool, issue_id, user_id, false)
        .await
        .unwrap();
    assert!(
        !IssueSocialService::is_bookmarked(&db.pool, issue_id, user_id)
            .await
            .unwrap()
    );

    assert!(
        !IssueSocialService::is_subscribed(&db.pool, issue_id, user_id)
            .await
            .unwrap()
    );
    IssueSocialService::set_subscription(&db.pool, issue_id, user_id, true, "manual")
        .await
        .unwrap();
    assert!(
        IssueSocialService::is_subscribed(&db.pool, issue_id, user_id)
            .await
            .unwrap()
    );
}

#[actix_web::test]
async fn test_seen_tracking_resets_on_new_event() {
    use rustrak::services::IssueSocialService;
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Seen Project").await;
    let user_id = seed_user_for_social(&db.pool, "seen@x.com").await;
    let temp_dir = TempDir::new().unwrap();
    let cfg = crate::common::rate_limit();

    let e1 = Uuid::new_v4().to_string().replace("-", "");
    ingest_error_event(&db.pool, project.id, temp_dir.path(), &cfg, &e1).await;
    let issue_id = only_issue_id(&db.pool, project.id).await;

    assert!(!IssueSocialService::has_seen(&db.pool, issue_id, user_id)
        .await
        .unwrap());
    IssueSocialService::mark_seen(&db.pool, issue_id, user_id)
        .await
        .unwrap();
    assert!(IssueSocialService::has_seen(&db.pool, issue_id, user_id)
        .await
        .unwrap());

    // A new event bumps last_seen → no longer "seen".
    let e2 = Uuid::new_v4().to_string().replace("-", "");
    let later = Utc::now() + chrono::Duration::seconds(60);
    ingest_error_event_at(
        &db.pool,
        project.id,
        temp_dir.path(),
        &cfg,
        &e2,
        later,
        later,
    )
    .await;
    assert!(!IssueSocialService::has_seen(&db.pool, issue_id, user_id)
        .await
        .unwrap());
}

#[actix_web::test]
async fn test_user_reports() {
    use rustrak::services::IssueSocialService;
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Reports Project").await;
    let issue_id = seed_issue_direct(&db.pool, project.id, "RepErr").await;

    assert_eq!(
        IssueSocialService::user_report_count(&db.pool, issue_id)
            .await
            .unwrap(),
        0
    );
    IssueSocialService::create_user_report(
        &db.pool,
        project.id,
        Some(issue_id),
        None,
        "Jane",
        "jane@x.com",
        "it broke when I clicked save",
    )
    .await
    .unwrap();

    let reports = IssueSocialService::list_user_reports(&db.pool, issue_id)
        .await
        .unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].name, "Jane");
    assert_eq!(
        IssueSocialService::user_report_count(&db.pool, issue_id)
            .await
            .unwrap(),
        1
    );
}

#[actix_web::test]
async fn test_issue_stats_timeseries() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Stats Project").await;
    let temp_dir = TempDir::new().unwrap();
    let cfg = crate::common::rate_limit();

    let e1 = Uuid::new_v4().to_string().replace("-", "");
    ingest_error_event(&db.pool, project.id, temp_dir.path(), &cfg, &e1).await;
    let issue_id = only_issue_id(&db.pool, project.id).await;

    let series = IssueService::stats(&db.pool, issue_id, 3600, 24)
        .await
        .unwrap();
    assert_eq!(series.len(), 24);
    let total: i64 = series.iter().map(|(_, c)| *c).sum();
    assert_eq!(total, 1, "the one event should be counted exactly once");
    // It lands in one of the most recent buckets (alignment is relative to the
    // window start, so allow the last two).
    let recent: i64 = series.iter().rev().take(2).map(|(_, c)| *c).sum();
    assert_eq!(recent, 1);
    assert!(series[0].0 < series[23].0, "buckets ascend in time");
}

/// An event of `issue_id` stored as the digest stores one, with `data` and
/// `ingested_at` of the test's choosing.
async fn insert_issue_event(
    pool: &rustrak::db::DbPool,
    project_id: i32,
    issue_id: Uuid,
    data: serde_json::Value,
    ingested_at: chrono::DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO events (id, event_id, project_id, issue_id, data, timestamp, ingested_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(Uuid::new_v4())
    .bind(Uuid::new_v4())
    .bind(project_id)
    .bind(issue_id)
    .bind(sqlx::types::Json(data))
    .bind(ingested_at)
    .bind(ingested_at)
    .execute(pool)
    .await
    .unwrap();
}

#[actix_web::test]
async fn test_issue_stats_buckets_each_event_once() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Stats Buckets").await;
    let temp_dir = TempDir::new().unwrap();
    let cfg = crate::common::rate_limit();
    let e1 = Uuid::new_v4().to_string().replace("-", "");
    ingest_error_event(&db.pool, project.id, temp_dir.path(), &cfg, &e1).await;
    let issue_id = only_issue_id(&db.pool, project.id).await;

    // Half an hour off every bucket edge, so the call's own `now` (a little
    // later than this one) lands each in the bucket counted below.
    let now = Utc::now();
    let hours_ago = |h: i64| now - chrono::Duration::minutes(h * 60 + 30);
    for at in [
        hours_ago(0),
        hours_ago(5),
        hours_ago(23),
        hours_ago(24),                       // before the window
        now + chrono::Duration::seconds(10), // past `now`: the last bucket
    ] {
        insert_issue_event(&db.pool, project.id, issue_id, json!({}), at).await;
    }

    let series = IssueService::stats(&db.pool, issue_id, 3600, 24)
        .await
        .unwrap();
    let counts: Vec<i64> = series.iter().map(|(_, c)| *c).collect();
    let mut expected = vec![0; 24];
    expected[23] = 3; // the ingested one, half an hour ago, and the future one
    expected[18] = 1;
    expected[0] = 1;
    assert_eq!(counts, expected);
    let start = series[0].0;
    assert!(series
        .iter()
        .enumerate()
        .all(|(i, (t, _))| *t == start + i as i64 * 3600));
}

/// The tag and user aggregates read only `tags` and `user`, whatever else an
/// event carries, and an event whose `tags` or `user` is not an object is
/// skipped rather than failing the request.
#[actix_web::test]
async fn test_issue_aggregates_read_tags_and_user_of_large_events() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Aggregates Large").await;
    let temp_dir = TempDir::new().unwrap();
    let cfg = crate::common::rate_limit();
    let e1 = Uuid::new_v4().to_string().replace("-", "");
    ingest_error_event(&db.pool, project.id, temp_dir.path(), &cfg, &e1).await;
    let issue_id = only_issue_id(&db.pool, project.id).await;
    sqlx::query("DELETE FROM events WHERE issue_id = $1")
        .bind(issue_id)
        .execute(&db.pool)
        .await
        .unwrap();

    let blob = "x".repeat(512 * 1024);
    let t = |s: i64| Utc::now() - chrono::Duration::seconds(s);
    for (data, at) in [
        (
            json!({"tags": {"browser": "Firefox", "os": "Linux", "n": 3, "b": true},
                   "user": {"id": "u1", "email": "a@x.com"}, "extra": {"blob": blob}}),
            t(30),
        ),
        (
            json!({"tags": [["browser", "Chrome"], ["browser", "Edge"], {"key": "os", "value": "Mac"}],
                   "user": {"email": "b@x.com"}, "breadcrumbs": [blob.clone()]}),
            t(20),
        ),
        (
            json!({"tags": "not-tags", "user": "not-a-user", "extra": {"blob": blob}}),
            t(10),
        ),
        (
            json!({"tags": null, "user": {"ip_address": "1.2.3.4"}}),
            t(5),
        ),
    ] {
        insert_issue_event(&db.pool, project.id, issue_id, data, at).await;
    }

    let aggregates = IssueService::aggregates(&db.pool, issue_id).await.unwrap();
    assert_eq!(
        serde_json::to_value(&aggregates).unwrap(),
        json!({
            "user_count": 3,
            "tags": [
                {"key": "b", "total_values": 1, "top_values": [{"value": "true", "count": 1}]},
                {"key": "browser", "total_values": 2,
                 "top_values": [{"value": "Chrome", "count": 1}, {"value": "Firefox", "count": 1}]},
                {"key": "n", "total_values": 1, "top_values": [{"value": "3", "count": 1}]},
                {"key": "os", "total_values": 2,
                 "top_values": [{"value": "Linux", "count": 1}, {"value": "Mac", "count": 1}]},
            ]
        })
    );

    let values = IssueService::tag_values(&db.pool, issue_id, "browser")
        .await
        .unwrap();
    let got: Vec<(String, i64)> = values.iter().map(|v| (v.value.clone(), v.count)).collect();
    assert_eq!(got, [("Chrome".to_string(), 1), ("Firefox".to_string(), 1)]);
    assert!(values.iter().all(|v| v.first_seen == v.last_seen));
}

#[actix_web::test]
async fn test_resolve_in_next_release_suppresses_same_release_regression() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "RINR Project").await;
    let temp_dir = TempDir::new().unwrap();
    let cfg = crate::common::rate_limit();

    // First event (release v1.0.0) creates the issue.
    ingest_event_with_tags(
        &db.pool,
        project.id,
        temp_dir.path(),
        &cfg,
        json!({}),
        json!({"id": "u1"}),
    )
    .await;
    let issue_id = only_issue_id(&db.pool, project.id).await;

    // Resolve in next release.
    IssueService::resolve_in_next_release(&db.pool, issue_id)
        .await
        .unwrap();
    let i = IssueService::get_by_id(&db.pool, issue_id).await.unwrap();
    assert_eq!(i.status, "resolved");
    assert!(i.status_details.contains("in_next_release"));

    // A new event from the SAME release must NOT regress it.
    ingest_event_with_tags(
        &db.pool,
        project.id,
        temp_dir.path(),
        &cfg,
        json!({}),
        json!({"id": "u2"}),
    )
    .await;
    let i = IssueService::get_by_id(&db.pool, issue_id).await.unwrap();
    assert_eq!(
        i.status, "resolved",
        "same-release event should not regress"
    );
}

#[actix_web::test]
async fn test_finalize_release_clears_marker() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Deploy Project").await;
    let temp_dir = TempDir::new().unwrap();
    let cfg = crate::common::rate_limit();

    ingest_event_with_tags(
        &db.pool,
        project.id,
        temp_dir.path(),
        &cfg,
        json!({}),
        json!({"id": "u1"}),
    )
    .await; // last_release = v1.0.0
    let issue_id = only_issue_id(&db.pool, project.id).await;
    IssueService::resolve_in_next_release(&db.pool, issue_id)
        .await
        .unwrap();

    // Register the release the issue's last_release denormalizes to, so
    // finalize_release has a `releases` row to chronologically compare against.
    let (v1, _) = ReleaseService::create(
        &db.pool,
        project.id,
        CreateRelease {
            version: "v1.0.0".to_string(),
            reference: None,
            url: None,
        },
    )
    .await
    .unwrap();

    // Deploy a newer release (created strictly after v1.0.0) → marker cleared.
    let finalized = IssueService::finalize_release(
        &db.pool,
        project.id,
        v1.date_created + chrono::Duration::seconds(1),
    )
    .await
    .unwrap();
    assert_eq!(finalized, 1);
    let i = IssueService::get_by_id(&db.pool, issue_id).await.unwrap();
    assert!(!i.status_details.contains("in_next_release"));
    assert_eq!(i.status, "resolved");
}

/// Stores `event` in the spool as ingest would, at `at`, under `event_id`.
async fn spool_event(
    ingest_dir: &std::path::Path,
    project_id: i32,
    event_id: Uuid,
    at: chrono::DateTime<Utc>,
    mut event: serde_json::Value,
) {
    let raw = event_id.simple().to_string();
    let obj = event.as_object_mut().unwrap();
    obj.insert("event_id".into(), json!(&raw));
    obj.insert("timestamp".into(), json!(at.timestamp() as f64));
    rustrak::ingest::store_event_with_metadata(
        ingest_dir,
        &raw,
        &serde_json::to_vec(&event).unwrap(),
        &EventMetadata {
            event_id: raw.clone(),
            project_id,
            ingested_at: at,
            remote_addr: None,
        },
    )
    .await
    .expect("store event");
}

/// A spool file of `project_id`, owned for a digest.
fn spool_ref(
    processors: &rustrak::digest::processors::Processors,
    project_id: i32,
    event_id: Uuid,
) -> rustrak::digest::processors::SpoolRef {
    rustrak::digest::processors::SpoolRef {
        project_id,
        event_id,
        size: 0,
        owner: processors.errors.in_flight().register(project_id, event_id),
    }
}

/// Digests `events` of `project_id`, each at its time, as one batch.
async fn digest_in_order(
    pool: &rustrak::db::DbPool,
    ingest_dir: &std::path::Path,
    project_id: i32,
    events: Vec<(chrono::DateTime<Utc>, serde_json::Value)>,
) {
    let processors = std::sync::Arc::new(crate::common::processors(pool, ingest_dir));
    let mut refs = Vec::new();
    for (at, event) in events {
        let event_id = Uuid::new_v4();
        spool_event(ingest_dir, project_id, event_id, at, event).await;
        refs.push(spool_ref(&processors, project_id, event_id));
    }
    for result in rustrak::ingest::pipeline::digest_refs(&processors, pool, refs).await {
        result.expect("digest");
    }
}

/// A resolved issue that gets several events at once reopens once: one
/// `set_regression` activity, and only the first event is the regression.
#[actix_web::test]
async fn a_resolved_issue_regresses_once_per_batch() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Regress Once").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let dir = temp_dir.path();
    let now = Utc::now();
    let event = json!({ "platform": "rust", "logentry": { "formatted": "boom" } });

    digest(&db.pool, dir, project.id, now, event.clone()).await;
    let issue = only_issue(&db.pool, project.id).await;
    IssueService::set_status(&db.pool, issue.id, "resolved", None)
        .await
        .expect("resolve");
    let before = rustrak::services::IssueSocialService::list_activity(&db.pool, issue.id)
        .await
        .expect("activity");

    digest_in_order(
        &db.pool,
        dir,
        project.id,
        (1..=3)
            .map(|h| (now + chrono::Duration::hours(h), event.clone()))
            .collect(),
    )
    .await;

    let after = rustrak::services::IssueSocialService::list_activity(&db.pool, issue.id)
        .await
        .expect("activity");
    assert_eq!(after.len(), before.len() + 1, "one regression activity");
    assert_eq!(after[0].activity_type, "set_regression");
    let alert_types: Vec<Option<rustrak::models::AlertType>> = sqlx::query_scalar(
        "SELECT alert_type FROM events WHERE issue_id = $1 ORDER BY timestamp, id",
    )
    .bind(issue.id)
    .fetch_all(&db.pool)
    .await
    .unwrap();
    assert_eq!(
        alert_types,
        [
            Some(rustrak::models::AlertType::NewIssue),
            Some(rustrak::models::AlertType::Regression),
            None,
            None
        ]
    );
    let issue = IssueService::get_by_id(&db.pool, issue.id).await.unwrap();
    assert_eq!(
        (issue.status.as_str(), issue.substatus.as_deref()),
        ("unresolved", Some("regressed"))
    );
    assert_eq!(issue.digested_event_count, 4);
}

/// Two events of one batch with different current keys but the same
/// pre-upgrade key both land on the pre-upgrade issue, as they would one at
/// a time.
#[actix_web::test]
async fn events_sharing_a_legacy_key_in_one_batch_claim_one_issue() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Legacy Batch").await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let dir = temp_dir.path();
    let chain = |cause: &str| {
        json!({
            "platform": "python",
            "exception": { "values": [
                { "type": cause, "value": "underlying" },
                { "type": "RuntimeError", "value": "save failed" }
            ]}
        })
    };
    let old = digest_as_pre_upgrade(&db.pool, dir, project.id, chain("IOError")).await;

    let now = Utc::now();
    digest_in_order(
        &db.pool,
        dir,
        project.id,
        vec![(now, chain("IOError")), (now, chain("KeyError"))],
    )
    .await;

    let issue = only_issue(&db.pool, project.id).await;
    assert_eq!(issue.id, old.id, "the second event opened its own issue");
    assert_eq!(issue.digested_event_count, 3);
    let hashes = IssueService::list_hashes(&db.pool, old.id)
        .await
        .expect("list hashes");
    assert_eq!(hashes.len(), 3, "the legacy key and both current keys");
}

/// The barrier that makes the pool's commits power-safe writes to the
/// write-ahead log: a commit that changed no page writes nothing, so its
/// `synchronous=FULL` would sync nothing.
#[cfg(feature = "sqlite")]
#[actix_web::test]
async fn the_sync_barrier_writes_to_the_wal() {
    let db = TestDb::new().await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    sqlx::query("PRAGMA journal_mode = WAL")
        .execute(&db.pool)
        .await
        .unwrap();
    let processors = crate::common::processors(&db.pool, temp_dir.path());
    // (busy, frames in the WAL, frames checkpointed)
    let checkpoint = |mode: &'static str| {
        let pool = db.pool.clone();
        async move {
            sqlx::query_as::<_, (i64, i64, i64)>(sqlx::AssertSqlSafe(format!(
                "PRAGMA wal_checkpoint({mode})"
            )))
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    assert_eq!(checkpoint("TRUNCATE").await, (0, 0, 0), "the WAL is empty");

    processors
        .errors
        .sync_barrier_for_test(&db.pool)
        .await
        .unwrap();

    let (_, frames, _) = checkpoint("PASSIVE").await;
    assert!(frames > 0, "the barrier wrote nothing to the WAL");
}

/// (installation digested, project stored, project digested)
async fn digest_counters(pool: &rustrak::db::DbPool, project_id: i32) -> (i64, i64, i64) {
    let installation: i64 = sqlx::query_scalar(
        "SELECT CAST(digested_event_count AS BIGINT) FROM installation WHERE id = 1",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let project = ProjectService::get_by_id(pool, project_id).await.unwrap();
    (
        installation,
        project.stored_event_count as i64,
        project.digested_event_count as i64,
    )
}

/// The same spool file twice in one batch (a ref queued while a replay of it
/// was too) is one event: one row, counted once, both refs settled as stored.
#[actix_web::test]
async fn a_duplicate_inside_one_batch_is_stored_once() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Batch Duplicate").await;
    let temp_dir = TempDir::new().unwrap();
    let dir = temp_dir.path();
    let processors = std::sync::Arc::new(crate::common::processors(&db.pool, dir));
    let before = digest_counters(&db.pool, project.id).await;
    let event_id = Uuid::new_v4();
    spool_event(
        dir,
        project.id,
        event_id,
        Utc::now(),
        json!({ "message": "twice" }),
    )
    .await;
    processors.errors.pending().insert(project.id, event_id);

    let results = rustrak::ingest::pipeline::digest_refs(
        &processors,
        &db.pool,
        vec![
            spool_ref(&processors, project.id, event_id),
            spool_ref(&processors, project.id, event_id),
        ],
    )
    .await;

    let results: Vec<_> = results.into_iter().map(Result::unwrap).collect();
    assert_eq!(
        results,
        [
            rustrak::digest::processors::Digested::Stored,
            rustrak::digest::processors::Digested::Stored
        ]
    );
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE event_id = $1")
        .bind(event_id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(rows, 1);
    assert_eq!(
        only_issue(&db.pool, project.id).await.digested_event_count,
        1
    );
    assert_eq!(
        digest_counters(&db.pool, project.id).await,
        (before.0 + 1, before.1 + 1, before.2 + 1)
    );
    assert_eq!(
        processors.errors.pending().counts(project.id).installation,
        0
    );
    crate::common::eventually(|| async {
        (!crate::common::has_files(dir) && processors.errors.in_flight().is_empty()).then_some(())
    })
    .await;
}

/// Everything an ambiguous-commit replay could get wrong, as rows that do
/// not depend on random ids: issues are named by their `digest_order`.
async fn mixed_batch_snapshot(pool: &rustrak::db::DbPool, project_id: i32) -> Vec<String> {
    use rustrak::models::AlertType;
    let mut rows = Vec::new();
    macro_rules! table {
        ($name:literal, $ty:ty, $sql:literal) => {
            let found: Vec<$ty> = sqlx::query_as($sql).fetch_all(pool).await.unwrap();
            rows.extend(found.into_iter().map(|r| format!("{}: {r:?}", $name)));
        };
    }
    table!(
        "event",
        (Uuid, i32, Option<AlertType>, Option<i32>, String),
        "SELECT e.event_id, i.digest_order, e.alert_type, e.issue_event_count, e.environment \
         FROM events e JOIN issues i ON i.id = e.issue_id ORDER BY i.digest_order, e.event_id"
    );
    table!(
        "issue",
        (i32, String, Option<String>, i64, i64, String),
        "SELECT digest_order, status, substatus, CAST(digested_event_count AS BIGINT), \
         CAST(stored_event_count AS BIGINT), issue_category FROM issues ORDER BY digest_order"
    );
    table!(
        "environment",
        (i32, String, i64),
        "SELECT i.digest_order, ie.environment, CAST(ie.event_count AS BIGINT) \
         FROM issue_environments ie JOIN issues i ON i.id = ie.issue_id ORDER BY 1, 2"
    );
    table!(
        "installation",
        (i64, i64, i64),
        "SELECT CAST(digested_event_count AS BIGINT), CAST(quota_minute_count AS BIGINT), \
         CAST(quota_hour_count AS BIGINT) FROM installation"
    );
    table!(
        "user_report",
        (Uuid, Option<i32>, Option<i32>),
        "SELECT ur.event_id, ei.digest_order, fi.digest_order FROM user_reports ur \
         LEFT JOIN issues ei ON ei.id = ur.issue_id \
         LEFT JOIN issues fi ON fi.id = ur.feedback_issue_id ORDER BY 1"
    );
    table!(
        "attachment",
        (Uuid, String, i64),
        "SELECT e.event_id, a.filename, CAST(a.size AS BIGINT) FROM event_attachments a \
         JOIN events e ON e.id = a.event_id ORDER BY 1, 2"
    );
    table!(
        "activity",
        (i32, String),
        "SELECT i.digest_order, a.type FROM issue_activity a \
         JOIN issues i ON i.id = a.issue_id ORDER BY 1, 2"
    );
    table!(
        "alert",
        (String,),
        "SELECT idempotency_key FROM alert_history ORDER BY 1"
    );
    let project: (i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT CAST(stored_event_count AS BIGINT), CAST(digested_event_count AS BIGINT), \
         CAST(rate_limited_event_count AS BIGINT), CAST(quota_minute_count AS BIGINT), \
         CAST(quota_hour_count AS BIGINT) FROM projects WHERE id = $1",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await
    .unwrap();
    rows.push(format!("project: {project:?}"));
    rows
}

/// One run of the mixed batch, in a database of its own: two issues
/// digested first (one then resolved), then a batch with a new issue, an
/// existing one, a regression, an error event and the feedback about it,
/// two environments, an attachment, under a new-issue rule. An ambiguous
/// run makes the batch's commit land and its acknowledgement fail.
async fn run_mixed_batch(ids: &[Uuid], run: MixedRun) -> Vec<String> {
    use rustrak::models::{
        AlertRuleChannelInput, AlertType, CreateAlertIntegration, CreateAlertRule, ProviderType,
    };
    use rustrak::services::AlertService;

    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Ambiguous Commit").await;
    let channel = AlertService::create_channel(
        &db.pool,
        &Default::default(),
        CreateAlertIntegration {
            name: "Webhook".to_string(),
            provider_type: ProviderType::Webhook,
            credentials: json!({ "url": "https://example.com/webhook" }),
            is_enabled: true,
        },
    )
    .await
    .unwrap();
    let rule = AlertService::create_rule(
        &db.pool,
        project.id,
        CreateAlertRule {
            name: "New issues".to_string(),
            alert_type: AlertType::NewIssue,
            channels: vec![AlertRuleChannelInput {
                integration_id: channel.id,
                routing_override: json!({}),
            }],
            conditions: json!({}),
            cooldown_minutes: 60,
        },
    )
    .await
    .unwrap();
    // In cooldown: recorded, never delivered.
    sqlx::query("UPDATE alert_rules SET last_triggered_at = $1 WHERE id = $2")
        .bind(Utc::now())
        .bind(rule.id)
        .execute(&db.pool)
        .await
        .unwrap();

    let temp_dir = TempDir::new().unwrap();
    let dir = temp_dir.path();
    let processors = std::sync::Arc::new(crate::common::processors(&db.pool, dir));
    let now = Utc::now();
    let digest_batch = |events: Vec<(Uuid, serde_json::Value)>| {
        let (processors, pool) = (std::sync::Arc::clone(&processors), db.pool.clone());
        async move {
            let mut refs = Vec::new();
            for (event_id, event) in events {
                spool_event(dir, project.id, event_id, now, event).await;
                processors.errors.pending().insert(project.id, event_id);
                refs.push(spool_ref(&processors, project.id, event_id));
            }
            rustrak::ingest::pipeline::digest_refs(&processors, &pool, refs).await
        }
    };

    for result in digest_batch(vec![
        (ids[0], json!({ "message": "existing" })),
        (ids[1], json!({ "message": "resolved" })),
    ])
    .await
    {
        result.unwrap();
    }
    let resolved: Uuid = sqlx::query_scalar(
        "SELECT i.id FROM issues i JOIN events e ON e.issue_id = i.id WHERE e.event_id = $1",
    )
    .bind(ids[1])
    .fetch_one(&db.pool)
    .await
    .unwrap();
    IssueService::resolve(&db.pool, resolved).await.unwrap();

    use base64::Engine as _;
    if run != MixedRun::Clean {
        processors.errors.fail_after_commit_once_for_test();
    }
    let batch = vec![
        (
            ids[2],
            json!({ "message": "brand new", "environment": "production" }),
        ),
        (
            ids[3],
            json!({ "message": "existing", "environment": "staging" }),
        ),
        (
            ids[4],
            json!({ "message": "resolved", "environment": "production" }),
        ),
        (
            ids[5],
            json!({ "message": "reported", "environment": "production" }),
        ),
        (
            ids[6],
            json!({
                "type": "feedback",
                "contexts": { "feedback": {
                    "message": "it broke",
                    "contact_email": "ann@example.com",
                    "name": "Ann",
                    "associated_event_id": ids[5].simple().to_string(),
                }},
            }),
        ),
        (
            ids[7],
            json!({
                "message": "with a file",
                "_rustrak_attachments": [{
                    "filename": "log.txt",
                    "content_type": "text/plain",
                    "data": base64::engine::general_purpose::STANDARD.encode(b"hello"),
                }],
            }),
        ),
    ];
    match run {
        MixedRun::Clean => {
            for result in digest_batch(batch).await {
                result.unwrap();
            }
        }
        // Recovery replays the files.
        MixedRun::AmbiguousOnce => {
            let results = digest_batch(batch).await;
            assert!(results.iter().all(Result::is_err), "{results:?}");
            assert!(
                !rustrak::ingest::pipeline::recover_pending_events_once(
                    db.pool.clone(),
                    std::sync::Arc::clone(&processors),
                    dir.to_path_buf(),
                )
                .await
            );
        }
        // The live committer retries the batch itself: no recovery runs.
        MixedRun::AmbiguousLive => {
            for (event_id, mut event) in batch {
                let raw = event_id.simple().to_string();
                let obj = event.as_object_mut().unwrap();
                obj.insert("event_id".into(), json!(&raw));
                obj.insert("timestamp".into(), json!(now.timestamp() as f64));
                rustrak::ingest::pipeline::enqueue_digest(
                    &processors,
                    &db.pool,
                    event_id,
                    EventMetadata {
                        event_id: raw,
                        project_id: project.id,
                        ingested_at: now,
                        remote_addr: None,
                    },
                    serde_json::to_vec(&event).unwrap().into(),
                )
                .await
                .unwrap();
            }
        }
    }
    crate::common::eventually(|| async { (!crate::common::has_files(dir)).then_some(()) }).await;
    assert_eq!(
        processors.errors.pending().counts(project.id).installation,
        0
    );
    mixed_batch_snapshot(&db.pool, project.id).await
}

/// A commit that lands while its acknowledgement is lost (the connection
/// drops during COMMIT) leaves its files; recovery replays them into exactly
/// the database a clean run leaves: nothing counted, consumed, inserted or
/// alerted twice. The same in the live digest, which retries the batch whole
/// on its own.
#[actix_web::test]
async fn an_ambiguous_commit_replays_to_an_identical_database() {
    let ids: Vec<Uuid> = (0..8).map(|_| Uuid::new_v4()).collect();
    let control = run_mixed_batch(&ids, MixedRun::Clean).await;
    let replayed = run_mixed_batch(&ids, MixedRun::AmbiguousOnce).await;
    assert_eq!(replayed, control, "recovery replay");
    let retried = run_mixed_batch(&ids, MixedRun::AmbiguousLive).await;
    assert_eq!(retried, control, "live retry");
}

#[derive(Clone, Copy, PartialEq)]
enum MixedRun {
    Clean,
    AmbiguousOnce,
    AmbiguousLive,
}
