//! An ingest acknowledgement must survive a quota change before digest.
mod common;

use chrono::{Duration, Utc};
use common::{null_sourcemap_provider, process_error_event, TestDb};
use rustrak::config::RateLimitConfig;
use rustrak::ingest::{store_event_with_metadata, EventMetadata};
use rustrak::models::CreateProject;
use rustrak::services::ProjectService;
use uuid::Uuid;

#[tokio::test]
async fn acknowledged_event_waits_for_quota_instead_of_being_deleted() {
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "Quota retention".into(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let metadata = EventMetadata {
        event_id: Uuid::new_v4().simple().to_string(),
        project_id: project.id,
        ingested_at: Utc::now(),
        remote_addr: None,
    };
    let event = serde_json::to_vec(&serde_json::json!({"event_id":metadata.event_id,"platform":"javascript","message":"retained quota proof"})).unwrap();
    store_event_with_metadata(directory.path(), &metadata.event_id, &event, &metadata)
        .await
        .unwrap();

    // Model quota becoming unavailable after ingest persisted the accepted event.
    sqlx::query(
        "UPDATE projects SET quota_exceeded_until = $1, next_quota_check = $2 WHERE id = $3",
    )
    .bind(Utc::now() + Duration::minutes(1))
    .bind(100_i64)
    .bind(project.id)
    .execute(&db.pool)
    .await
    .unwrap();
    let quotas = RateLimitConfig {
        max_events_per_minute: 10000,
        max_events_per_hour: 10000,
        max_events_per_project_per_minute: 10000,
        max_events_per_project_per_hour: 10000,
    };
    let result = process_error_event(
        &db.pool,
        &metadata,
        directory.path(),
        &quotas,
        null_sourcemap_provider(),
    )
    .await;
    assert!(
        result.is_err(),
        "deferred work must remain visible as incomplete"
    );
    assert!(result.unwrap_err().to_string().contains("remains queued"));
    assert_eq!(
        std::fs::read_dir(directory.path()).unwrap().count(),
        1,
        "accepted pending payload must survive quota rejection"
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE project_id = $1")
        .bind(project.id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "quota must still defer the database write");

    // Retry the same durable record, as the recovery worker does after quota resets.
    sqlx::query("UPDATE projects SET quota_exceeded_until = NULL WHERE id = $1")
        .bind(project.id)
        .execute(&db.pool)
        .await
        .unwrap();
    process_error_event(
        &db.pool,
        &metadata,
        directory.path(),
        &quotas,
        null_sourcemap_provider(),
    )
    .await
    .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE project_id = $1")
        .bind(project.id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}
