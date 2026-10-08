//! TDD tests for the Storage feature — StorageService stats (Fase 1).
//!
//! Level 2 — service DB behavior tests, #[tokio::test] + TestDb.
//! Default test backend is SQLite (see Cargo.toml `default = ["sqlite"]`).

use crate::common::TestDb;
use bytes::Bytes;
use chrono::Utc;
use rustrak::error::AppError;
use rustrak::ingest::{delete_event, read_event_for_project as read_scoped, store_event};
use rustrak::models::{CleanupCounts, CleanupFilter, CleanupState, CleanupStatus, CreateProject};
use rustrak::services::sourcemap_store::{LocalSourceMapStore, SourceMapStore};
use rustrak::services::{CleanupJob, ProjectService, StorageService};
use std::sync::atomic::{AtomicI32, Ordering};
use tempfile::tempdir;
use uuid::Uuid;

/// Monotonic source of `digest_order` so seeded issues never collide on the
/// `UNIQUE(project_id, digest_order)` constraint within a project.
static DIGEST_ORDER: AtomicI32 = AtomicI32::new(1);

#[tokio::test]
async fn ingest_legacy_event_cleanup_removes_the_payload() {
    let dir = tempdir().unwrap();
    let event_id = Uuid::new_v4().to_string();
    let payload = br#"{"legacy":true}"#;
    store_event(dir.path(), &event_id, payload).await.unwrap();
    let got = read_scoped(dir.path(), 99999, &event_id).await.unwrap();
    assert_eq!(got, payload);
    delete_event(dir.path(), &event_id).await.unwrap();
    assert!(read_scoped(dir.path(), 99999, &event_id).await.is_err());
}

/// Inserts a chunk row directly (CAS table: checksum PK, size, data BYTEA).
async fn insert_chunk(pool: &rustrak::db::DbPool, checksum: &str, size: i64) {
    sqlx::query("INSERT INTO chunk (checksum, size, data) VALUES ($1, $2, $3)")
        .bind(checksum)
        .bind(size)
        .bind(Vec::<u8>::new())
        .execute(pool)
        .await
        .unwrap();
}

/// Inserts a source_file row directly (CAS table on disk: id, checksum, size,
/// storage_path). `storage_path == checksum` mirrors production: the checksum IS
/// the CAS key. No metadata row, so it counts as an orphan for GC.
async fn insert_source_file(pool: &rustrak::db::DbPool, checksum: &str, size: i64) {
    sqlx::query(
        "INSERT INTO source_file (id, checksum, size, storage_path) VALUES ($1, $2, $3, $4)",
    )
    .bind(Uuid::new_v4())
    .bind(checksum)
    .bind(size)
    .bind(checksum)
    .execute(pool)
    .await
    .unwrap();
}

/// Seeds a source map owned by a project: a `source_file` row plus the
/// `source_file_metadata` link that ties it to the project via debug_id.
async fn seed_project_source_map(pool: &rustrak::db::DbPool, project_id: i32, checksum: &str) {
    let file_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO source_file (id, checksum, size, storage_path) VALUES ($1, $2, $3, $4)",
    )
    .bind(file_id)
    .bind(checksum)
    .bind(300_i64)
    .bind(format!("/store/{checksum}"))
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO source_file_metadata (id, project_id, debug_id, file_type, file_id) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(project_id)
    .bind(Uuid::new_v4())
    .bind("source_map")
    .bind(file_id)
    .execute(pool)
    .await
    .unwrap();
}

/// Seeds one error event by hand (project → issue → grouping → event) stamped at
/// `ingested_at`. The full FK chain is the price of an exact `events` COUNT
/// without the ingest pipeline.
async fn seed_event_at(
    pool: &rustrak::db::DbPool,
    project_id: i32,
    ingested_at: chrono::DateTime<Utc>,
) {
    let issue_id = Uuid::new_v4();
    let now = ingested_at;
    let digest_order = DIGEST_ORDER.fetch_add(1, Ordering::Relaxed);
    sqlx::query(
        "INSERT INTO issues (id, project_id, digest_order, first_seen, last_seen) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(issue_id)
    .bind(project_id)
    .bind(digest_order)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();

    let grouping_id: i32 = sqlx::query_scalar(
        "INSERT INTO groupings (project_id, issue_id, grouping_key, grouping_key_hash) \
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(project_id)
    .bind(issue_id)
    .bind(format!("key-{digest_order}"))
    .bind(format!("{digest_order:0>64}")) // unique 64-char hash per seeded event
    .fetch_one(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO events \
         (id, event_id, project_id, issue_id, grouping_id, data, timestamp, ingested_at, digested_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(Uuid::new_v4())
    .bind(Uuid::new_v4())
    .bind(project_id)
    .bind(issue_id)
    .bind(grouping_id)
    .bind(serde_json::json!({}))
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
}

/// Seeds an issue-less event row of an arbitrary `event_type` directly into the
/// `events` table (NULL `issue_id`/`grouping_id`) stamped at `ingested_at`. This
/// is the shape of non-error rows: legacy `transaction` rows stranded from before
/// the dedicated table, or future types like `log`. None of them ever
/// incremented a counter.
async fn seed_issueless_event_at(
    pool: &rustrak::db::DbPool,
    project_id: i32,
    event_type: &str,
    ingested_at: chrono::DateTime<Utc>,
) {
    let now = ingested_at;
    sqlx::query(
        "INSERT INTO events \
         (id, event_id, project_id, data, timestamp, ingested_at, digested_at, event_type) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(Uuid::new_v4())
    .bind(Uuid::new_v4())
    .bind(project_id)
    .bind(serde_json::json!({ "event_type": event_type }))
    .bind(now)
    .bind(now)
    .bind(now)
    .bind(event_type)
    .execute(pool)
    .await
    .unwrap();
}

/// Convenience: the legacy `transaction`-in-`events` shape.
async fn seed_legacy_transaction_event_at(
    pool: &rustrak::db::DbPool,
    project_id: i32,
    ingested_at: chrono::DateTime<Utc>,
) {
    seed_issueless_event_at(pool, project_id, "transaction", ingested_at).await;
}

/// Seeds one transaction (stamped at `ingested_at`) plus `span_count` spans
/// cascaded under it.
async fn seed_transaction_with_spans_at(
    pool: &rustrak::db::DbPool,
    project_id: i32,
    span_count: usize,
    ingested_at: chrono::DateTime<Utc>,
) {
    let txn_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO transactions (id, event_id, project_id, timestamp, ingested_at, data) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(txn_id)
    .bind(Uuid::new_v4())
    .bind(project_id)
    .bind(ingested_at)
    .bind(ingested_at)
    .bind(serde_json::json!({}))
    .execute(pool)
    .await
    .unwrap();

    for _ in 0..span_count {
        sqlx::query(
            "INSERT INTO spans (id, transaction_id, project_id, data) VALUES ($1, $2, $3, $4)",
        )
        .bind(Uuid::new_v4())
        .bind(txn_id)
        .bind(project_id)
        .bind(serde_json::json!({}))
        .execute(pool)
        .await
        .unwrap();
    }
}

/// Convenience wrappers stamping data at "now" for tests that don't care about age.
async fn seed_event(pool: &rustrak::db::DbPool, project_id: i32) {
    seed_event_at(pool, project_id, Utc::now()).await;
}

async fn seed_transaction_with_spans(
    pool: &rustrak::db::DbPool,
    project_id: i32,
    span_count: usize,
) {
    seed_transaction_with_spans_at(pool, project_id, span_count, Utc::now()).await;
}

#[tokio::test]
async fn test_preview_cleanup_counts_old_rows_without_mutating() {
    // A dry-run must report exactly what an execute would remove — old rows only —
    // and touch nothing. It's the safety net before a destructive delete.
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "preview-proj".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    let old = Utc::now() - chrono::Duration::days(60);
    let now = Utc::now();
    seed_event_at(&db.pool, project.id, old).await; // its issue will be emptied
    seed_event_at(&db.pool, project.id, now).await;
    seed_transaction_with_spans_at(&db.pool, project.id, 2, old).await;
    seed_transaction_with_spans_at(&db.pool, project.id, 1, now).await;

    let preview = StorageService::preview_cleanup(&db.pool, 30, None, CleanupFilter::all())
        .await
        .unwrap();

    assert_eq!(preview.events, 1, "one event older than 30d");
    assert_eq!(preview.transactions, 1);
    assert_eq!(preview.spans, 2, "spans under the old transaction");
    assert_eq!(
        preview.issues_removed, 1,
        "the old event's issue empties out"
    );

    // Nothing was deleted.
    let summary = StorageService::global_summary(&db.pool).await.unwrap();
    assert_eq!(summary.events_count, 2);
    assert_eq!(summary.transactions_count, 2);
    assert_eq!(summary.spans_count, 3);
}

/// Inserts a log row with an explicit `ingested_at` (the retention key).
async fn seed_log_at(
    pool: &rustrak::db::DbPool,
    project_id: i32,
    ingested_at: chrono::DateTime<Utc>,
) {
    sqlx::query(
        r#"
        INSERT INTO logs (id, project_id, trace_id, span_id, level, severity_number,
                          body, attributes, timestamp, ingested_at)
        VALUES ($1, $2, $3, NULL, $4, $5, $6, $7, $8, $9)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(project_id)
    .bind("aaaa")
    .bind("info")
    .bind(9_i16)
    .bind("hello")
    .bind(serde_json::json!({}))
    .bind(ingested_at)
    .bind(ingested_at)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_cleanup_counts_and_deletes_old_logs() {
    // Logs must participate in retention: preview reports old log rows and
    // execute deletes them, leaving recent logs untouched.
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "logs-cleanup".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    let old = Utc::now() - chrono::Duration::days(60);
    let now = Utc::now();
    seed_log_at(&db.pool, project.id, old).await;
    seed_log_at(&db.pool, project.id, now).await;

    let preview = StorageService::preview_cleanup(&db.pool, 30, None, CleanupFilter::all())
        .await
        .unwrap();
    assert_eq!(preview.logs, 1, "one log older than 30d");

    StorageService::execute_cleanup(&db.pool, 30, None, CleanupFilter::all())
        .await
        .unwrap();

    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM logs WHERE project_id = $1")
        .bind(project.id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(remaining, 1, "only the recent log survives");
}

#[tokio::test]
async fn test_storage_summary_and_by_project_include_logs() {
    // The Storage page surfaces a per-category row count; logs must appear in
    // both the global summary and the per-project breakdown.
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "logs-storage-count".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    seed_log_at(&db.pool, project.id, Utc::now()).await;
    seed_log_at(&db.pool, project.id, Utc::now()).await;

    let summary = StorageService::global_summary(&db.pool).await.unwrap();
    assert_eq!(summary.logs_count, 2);

    let rows = StorageService::by_project(&db.pool).await.unwrap();
    let p = rows
        .iter()
        .find(|r| r.project_id == project.id)
        .expect("project in breakdown");
    assert_eq!(p.logs_count, 2);
}

#[tokio::test]
async fn test_cleanup_rejects_nonpositive_retention_window() {
    // A window of 0 puts the cutoff at "now" and a negative one in the future —
    // either turns the cleanup into a full data wipe. Both preview and execute
    // must reject anything below 1 day before computing a cutoff, so a direct API
    // caller can't bypass the client-side `min(1)` and lose everything.
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "retention-guard".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    seed_event(&db.pool, project.id).await;

    for bad in [0_i64, -1, -365] {
        assert!(
            StorageService::preview_cleanup(&db.pool, bad, None, CleanupFilter::all())
                .await
                .is_err(),
            "preview must reject older_than_days = {bad}"
        );
        assert!(
            StorageService::execute_cleanup(&db.pool, bad, None, CleanupFilter::all())
                .await
                .is_err(),
            "execute must reject older_than_days = {bad}"
        );
    }

    // The rejected execute calls deleted nothing.
    let summary = StorageService::global_summary(&db.pool).await.unwrap();
    assert_eq!(summary.events_count, 1, "no data was purged");
}

#[tokio::test]
async fn test_gc_source_maps_keeps_file_that_is_referenced_at_delete_time() {
    // The orphan list is a point-in-time snapshot, but the delete re-checks
    // `NOT EXISTS(metadata)` atomically. A source_file that has a metadata row is
    // never removed — guarding the window where a concurrent upload re-references
    // a row the snapshot saw as an orphan.
    let db = TestDb::new().await;
    let tmp = tempdir().unwrap();
    let store = LocalSourceMapStore::new(tmp.path());

    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "gc-recheck".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    let referenced = "d".repeat(40);
    seed_project_source_map(&db.pool, project.id, &referenced).await;
    store
        .put(&referenced, Bytes::from_static(b"ref"))
        .await
        .unwrap();

    let result = StorageService::gc_source_maps(&db.pool, &store)
        .await
        .unwrap();

    assert_eq!(
        result.files_removed, 0,
        "referenced file is never collected"
    );
    assert_eq!(result.bytes_freed, 0);
    assert!(
        store.exists(&referenced).await.unwrap(),
        "referenced file kept on disk"
    );
}

#[tokio::test]
async fn test_execute_cleanup_deletes_old_cascades_spans_and_removes_empty_issues() {
    // Execute is the destructive twin of preview: old rows gone, spans cascade
    // away with their transaction, recent data untouched, and the issue left with
    // no events is deleted — no ghost issues.
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "execute-proj".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    let old = Utc::now() - chrono::Duration::days(60);
    let now = Utc::now();
    seed_event_at(&db.pool, project.id, old).await;
    seed_event_at(&db.pool, project.id, now).await;
    seed_transaction_with_spans_at(&db.pool, project.id, 2, old).await;
    seed_transaction_with_spans_at(&db.pool, project.id, 1, now).await;

    let result = StorageService::execute_cleanup(&db.pool, 30, None, CleanupFilter::all())
        .await
        .unwrap();

    assert_eq!(result.events, 1);
    assert_eq!(result.transactions, 1);
    assert_eq!(result.spans, 2);
    assert_eq!(result.issues_removed, 1);

    // Only the recent rows survive.
    let summary = StorageService::global_summary(&db.pool).await.unwrap();
    assert_eq!(summary.events_count, 1, "recent event survives");
    assert_eq!(summary.transactions_count, 1);
    assert_eq!(summary.spans_count, 1, "recent transaction's span survives");

    let issues_left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM issues")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(issues_left, 1, "emptied issue removed, recent issue kept");
}

#[tokio::test]
async fn test_execute_cleanup_decrements_project_event_counters() {
    // Regression guard: deleting events must keep the denormalized
    // projects.{stored,digested}_event_count in sync — same contract as
    // IssueService::delete. A raw DELETE that skips this leaves stale counts.
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "counter-proj".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    let old = Utc::now() - chrono::Duration::days(60);
    let now = Utc::now();
    seed_event_at(&db.pool, project.id, old).await; // its issue empties out
    seed_event_at(&db.pool, project.id, now).await; // survives

    // Reflect the two seeded events on the project counters (the digest path
    // would have done this in production).
    sqlx::query(
        "UPDATE projects SET stored_event_count = 2, digested_event_count = 2 WHERE id = $1",
    )
    .bind(project.id)
    .execute(&db.pool)
    .await
    .unwrap();

    StorageService::execute_cleanup(&db.pool, 30, None, CleanupFilter::all())
        .await
        .unwrap();

    let (stored, digested): (i32, i32) = sqlx::query_as(
        "SELECT stored_event_count, digested_event_count FROM projects WHERE id = $1",
    )
    .bind(project.id)
    .fetch_one(&db.pool)
    .await
    .unwrap();

    assert_eq!(stored, 1, "one of two events removed → count drops to 1");
    assert_eq!(digested, 1);
}

#[tokio::test]
async fn test_execute_cleanup_purges_legacy_transaction_events_without_underflowing_counters() {
    // Legacy `event_type='transaction'` rows never incremented the project
    // counters. A cleanup that catches them must still physically purge the rows
    // while leaving the project's error-event counters correct (the counters are
    // rebuilt from the surviving issues, so deleting issue-less rows can't drive
    // them negative).
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "tx-underflow".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    let old = Utc::now() - chrono::Duration::days(60);
    seed_event(&db.pool, project.id).await; // one recent error event, survives
    seed_legacy_transaction_event_at(&db.pool, project.id, old).await;
    seed_legacy_transaction_event_at(&db.pool, project.id, old).await;

    // Counter reflects the single error event the digest path would have counted.
    sqlx::query(
        "UPDATE projects SET stored_event_count = 1, digested_event_count = 1 WHERE id = $1",
    )
    .bind(project.id)
    .execute(&db.pool)
    .await
    .unwrap();

    StorageService::execute_cleanup(&db.pool, 30, None, CleanupFilter::all())
        .await
        .unwrap();

    let (stored, digested): (i32, i32) = sqlx::query_as(
        "SELECT stored_event_count, digested_event_count FROM projects WHERE id = $1",
    )
    .bind(project.id)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(
        stored, 1,
        "error-event counter untouched by transaction purge"
    );
    assert_eq!(digested, 1);

    // The legacy transaction rows are gone; the error event remains.
    let tx_left: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE event_type = 'transaction'")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(
        tx_left, 0,
        "old legacy transaction events physically purged"
    );

    let summary = StorageService::global_summary(&db.pool).await.unwrap();
    assert_eq!(summary.events_count, 1, "the error event survives");
}

#[tokio::test]
async fn test_execute_cleanup_deletes_every_old_row_regardless_of_type() {
    // The user-facing contract: a cleanup deletes EVERYTHING older than the cutoff
    // with no exceptions — errors and legacy transaction rows alike — and the
    // surviving project counter still equals the surviving error events.
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "delete-all".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    let old = Utc::now() - chrono::Duration::days(60);
    let now = Utc::now();
    seed_event_at(&db.pool, project.id, old).await; // old error → deleted
    seed_event_at(&db.pool, project.id, now).await; // recent error → survives
    seed_legacy_transaction_event_at(&db.pool, project.id, old).await; // deleted
    seed_legacy_transaction_event_at(&db.pool, project.id, old).await; // deleted

    // Two error events were counted by the digest path.
    sqlx::query(
        "UPDATE projects SET stored_event_count = 2, digested_event_count = 2 WHERE id = $1",
    )
    .bind(project.id)
    .execute(&db.pool)
    .await
    .unwrap();

    StorageService::execute_cleanup(&db.pool, 30, None, CleanupFilter::all())
        .await
        .unwrap();

    // Every old row gone (1 error + 2 transactions); only the recent error remains.
    let total_events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(
        total_events, 1,
        "all old rows deleted, recent error survives"
    );

    let (stored, digested): (i32, i32) = sqlx::query_as(
        "SELECT stored_event_count, digested_event_count FROM projects WHERE id = $1",
    )
    .bind(project.id)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(stored, 1, "counter tracks the one surviving error event");
    assert_eq!(digested, 1);
}

#[tokio::test]
async fn test_execute_cleanup_with_only_events_selected_spares_transactions_and_logs() {
    // The mirror case: selecting events only must purge old error events and the
    // issue they emptied, while every transaction, span and log — of any age —
    // survives untouched. Guards the issue-removal/counter path under a filter.
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "filter-events-only".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    let old = Utc::now() - chrono::Duration::days(60);
    seed_event_at(&db.pool, project.id, old).await; // its issue empties out
    seed_transaction_with_spans_at(&db.pool, project.id, 2, old).await;
    seed_log_at(&db.pool, project.id, old).await;

    let counts = StorageService::execute_cleanup(
        &db.pool,
        30,
        None,
        CleanupFilter {
            include_events: true,
            include_transactions: false,
            include_logs: false,
        },
    )
    .await
    .unwrap();

    assert_eq!(counts.events, 1, "the old error event is deleted");
    assert_eq!(counts.issues_removed, 1, "its emptied issue is removed");
    assert_eq!(counts.transactions, 0, "transactions out of scope → zero");
    assert_eq!(counts.spans, 0);
    assert_eq!(counts.logs, 0, "logs out of scope → zero");

    let summary = StorageService::global_summary(&db.pool).await.unwrap();
    assert_eq!(summary.events_count, 0, "old event purged");
    assert_eq!(summary.transactions_count, 1, "transaction survives");
    assert_eq!(summary.spans_count, 2, "spans survive");
    assert_eq!(summary.logs_count, 1, "log survives");
}

#[tokio::test]
async fn test_preview_cleanup_respects_filter_without_mutating() {
    // Preview must honour the same filter as execute: a transactions-only dry-run
    // reports transactions+spans, zeroes events and logs, and changes nothing.
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "preview-filter".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    let old = Utc::now() - chrono::Duration::days(60);
    seed_event_at(&db.pool, project.id, old).await;
    seed_transaction_with_spans_at(&db.pool, project.id, 3, old).await;
    seed_log_at(&db.pool, project.id, old).await;

    let preview = StorageService::preview_cleanup(
        &db.pool,
        30,
        None,
        CleanupFilter {
            include_events: false,
            include_transactions: true,
            include_logs: false,
        },
    )
    .await
    .unwrap();

    assert_eq!(preview.transactions, 1);
    assert_eq!(preview.spans, 3, "spans under the old transaction");
    assert_eq!(preview.events, 0, "events excluded → not reported");
    assert_eq!(preview.logs, 0, "logs excluded → not reported");
    assert_eq!(
        preview.issues_removed, 0,
        "no issue emptied without event scope"
    );

    // Nothing was deleted.
    let summary = StorageService::global_summary(&db.pool).await.unwrap();
    assert_eq!(summary.events_count, 1);
    assert_eq!(summary.transactions_count, 1);
    assert_eq!(summary.logs_count, 1);
}

#[tokio::test]
async fn test_execute_cleanup_with_only_logs_selected_spares_events_and_transactions() {
    // Granular selection: an admin who wants to reclaim log volume without losing
    // error history picks logs only. Execute must delete the old logs and leave
    // every event, transaction and span — of any age — untouched, and report the
    // skipped categories as zero so the UI never claims it removed them.
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "filter-logs-only".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    let old = Utc::now() - chrono::Duration::days(60);
    seed_event_at(&db.pool, project.id, old).await; // would be deleted if events were in scope
    seed_transaction_with_spans_at(&db.pool, project.id, 2, old).await;
    seed_log_at(&db.pool, project.id, old).await;

    let counts = StorageService::execute_cleanup(
        &db.pool,
        30,
        None,
        CleanupFilter {
            include_events: false,
            include_transactions: false,
            include_logs: true,
        },
    )
    .await
    .unwrap();

    assert_eq!(counts.logs, 1, "the one old log is deleted");
    assert_eq!(counts.events, 0, "events out of scope → reported zero");
    assert_eq!(counts.transactions, 0, "transactions out of scope → zero");
    assert_eq!(counts.spans, 0, "spans follow their transaction → zero");
    assert_eq!(
        counts.issues_removed, 0,
        "no issue emptied when events are spared"
    );

    let summary = StorageService::global_summary(&db.pool).await.unwrap();
    assert_eq!(summary.events_count, 1, "event survives");
    assert_eq!(summary.transactions_count, 1, "transaction survives");
    assert_eq!(
        summary.spans_count, 2,
        "spans survive with their transaction"
    );
    assert_eq!(summary.logs_count, 0, "old log purged");
}

#[tokio::test]
async fn test_execute_cleanup_scoped_to_project_spares_other_projects() {
    // Safety guard: a project-scoped purge must never reach into a sibling
    // project's data, no matter how old that data is.
    let db = TestDb::new().await;
    let proj_a = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "scope-a".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();
    let proj_b = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "scope-b".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    let old = Utc::now() - chrono::Duration::days(60);
    seed_event_at(&db.pool, proj_a.id, old).await;
    seed_transaction_with_spans_at(&db.pool, proj_a.id, 1, old).await;
    seed_event_at(&db.pool, proj_b.id, old).await;
    seed_transaction_with_spans_at(&db.pool, proj_b.id, 1, old).await;

    StorageService::execute_cleanup(&db.pool, 30, Some(proj_a.id), CleanupFilter::all())
        .await
        .unwrap();

    let rows = StorageService::by_project(&db.pool).await.unwrap();
    let a = rows.iter().find(|r| r.project_id == proj_a.id).unwrap();
    let b = rows.iter().find(|r| r.project_id == proj_b.id).unwrap();

    assert_eq!(a.events_count, 0, "scoped project purged");
    assert_eq!(a.transactions_count, 0);
    assert_eq!(b.events_count, 1, "sibling project untouched");
    assert_eq!(b.transactions_count, 1);
    assert_eq!(b.spans_count, 1);
}

#[tokio::test]
async fn test_storage_event_count_reflects_every_stored_event_row() {
    // The storage page shows the truth of what's on disk: every `events` row
    // counts, no exceptions — errors, future types (e.g. `log`), and legacy
    // `transaction` rows alike. Admins must see the real total so they know
    // there's data to reclaim; a cleanup then deletes it all by date.
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "mixed-types".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    seed_event(&db.pool, project.id).await; // error event (has an issue)
    seed_issueless_event_at(&db.pool, project.id, "log", Utc::now()).await;
    seed_legacy_transaction_event_at(&db.pool, project.id, Utc::now()).await;
    seed_legacy_transaction_event_at(&db.pool, project.id, Utc::now()).await;

    let summary = StorageService::global_summary(&db.pool).await.unwrap();
    assert_eq!(
        summary.events_count, 4,
        "every stored event row counts: error + log + 2 transactions"
    );

    let rows = StorageService::by_project(&db.pool).await.unwrap();
    let p = rows.iter().find(|r| r.project_id == project.id).unwrap();
    assert_eq!(
        p.events_count, 4,
        "per-project count reflects all stored event rows"
    );
}

#[tokio::test]
async fn test_global_summary_counts_rows_across_data_categories() {
    // The summary is the at-a-glance number: exact row counts per data category,
    // exact source-map weight, and a non-zero whole-DB size from the backend.
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "summary-proj".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    seed_event(&db.pool, project.id).await;
    seed_transaction_with_spans(&db.pool, project.id, 2).await;
    seed_transaction_with_spans(&db.pool, project.id, 1).await;
    insert_chunk(&db.pool, "a".repeat(40).as_str(), 100).await;
    insert_source_file(&db.pool, "c".repeat(40).as_str(), 300).await;

    let summary = StorageService::global_summary(&db.pool).await.unwrap();

    assert_eq!(summary.events_count, 1);
    assert_eq!(summary.transactions_count, 2);
    assert_eq!(summary.spans_count, 3, "2 + 1 spans");
    assert_eq!(summary.source_maps.total_bytes, 400, "100 chunk + 300 file");
    assert!(summary.total_db_size_bytes > 0, "backend reports a DB size");
}

#[tokio::test]
async fn test_by_project_breaks_down_counts_per_project_with_isolation() {
    // Per-project view: every project shows up (even empty ones), counts are
    // exact, and one project's data never bleeds into another's row.
    let db = TestDb::new().await;
    let proj_a = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "proj-a".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();
    let proj_b = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "proj-b".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    // Only proj_a gets data; proj_b stays empty.
    seed_event(&db.pool, proj_a.id).await;
    seed_transaction_with_spans(&db.pool, proj_a.id, 2).await;
    seed_project_source_map(&db.pool, proj_a.id, "d".repeat(40).as_str()).await;

    let rows = StorageService::by_project(&db.pool).await.unwrap();

    let a = rows
        .iter()
        .find(|r| r.project_id == proj_a.id)
        .expect("proj-a present");
    assert_eq!(a.project_name, "proj-a");
    assert_eq!(a.events_count, 1);
    assert_eq!(a.transactions_count, 1);
    assert_eq!(a.spans_count, 2);
    assert_eq!(a.source_maps_count, 1);
    assert!(a.estimated_bytes > 0, "proj-a holds payload bytes");

    let b = rows
        .iter()
        .find(|r| r.project_id == proj_b.id)
        .expect("proj-b present");
    assert_eq!(b.events_count, 0);
    assert_eq!(b.transactions_count, 0);
    assert_eq!(b.spans_count, 0);
    assert_eq!(b.source_maps_count, 0);
    assert_eq!(b.estimated_bytes, 0, "empty project weighs nothing");
}

#[tokio::test]
async fn test_by_project_estimates_bytes_from_a_sample_of_recent_rows() {
    // Summing every payload read the whole database on each page load. The
    // estimate scales a sample of the project's most recent rows by its row
    // count instead. Events are uniform, so their estimate equals the exact
    // sum. Logs are 250 old, large rows and 200 recent, small ones: only the
    // newest 200 may drive the estimate, so reading old rows or dropping the
    // sample limit both miss it.
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "sampled".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    let at = Utc::now();
    let old = at - chrono::Duration::days(1);
    for _ in 0..250 {
        seed_log_at(&db.pool, project.id, old).await;
    }
    sqlx::query("UPDATE logs SET body = $1 WHERE project_id = $2")
        .bind("x".repeat(1_000))
        .bind(project.id)
        .execute(&db.pool)
        .await
        .unwrap();
    for _ in 0..200 {
        seed_log_at(&db.pool, project.id, at).await;
    }
    for _ in 0..450 {
        seed_issueless_event_at(&db.pool, project.id, "log", at).await;
    }

    let (events, recent_logs): (i64, i64) = sqlx::query_as(
        "SELECT \
           (SELECT COALESCE(SUM(length(CAST(data AS TEXT))), 0) FROM events WHERE project_id = $1), \
           (SELECT COALESCE(SUM(length(CAST(body AS TEXT)) + length(CAST(attributes AS TEXT))), 0) \
              FROM logs WHERE project_id = $2 AND ingested_at > $3)",
    )
    .bind(project.id)
    .bind(project.id)
    .bind(old)
    .fetch_one(&db.pool)
    .await
    .unwrap();

    let rows = StorageService::by_project(&db.pool).await.unwrap();
    let p = rows.iter().find(|r| r.project_id == project.id).unwrap();
    assert_eq!(p.events_count, 450);
    assert_eq!(p.logs_count, 450);
    assert_eq!(p.estimated_bytes, events + recent_logs * 450 / 200);
}

#[tokio::test]
async fn test_preview_source_map_gc_counts_orphans_without_deleting() {
    // The GC dry-run reports the orphaned files + bytes a real GC would reclaim,
    // and deletes nothing — same safety contract as the time-based preview.
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "gc-preview".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    seed_project_source_map(&db.pool, project.id, &"a".repeat(40)).await; // referenced
    insert_source_file(&db.pool, &"b".repeat(40), 500).await; // orphan

    let preview = StorageService::preview_source_map_gc(&db.pool)
        .await
        .unwrap();

    assert_eq!(preview.files_removed, 1, "one orphan would be removed");
    assert_eq!(preview.bytes_freed, 500);

    // Nothing deleted: both source_file rows still present.
    let storage = StorageService::source_map_storage(&db.pool).await.unwrap();
    assert_eq!(storage.file_count, 2);
}

#[tokio::test]
async fn test_gc_source_maps_removes_orphans_from_db_and_disk() {
    // GC reclaims source_file rows that no metadata references (e.g. left behind
    // when a project was deleted — metadata cascades, the CAS file does not). The
    // orphan's DB row AND its on-disk file must go; referenced files are untouched.
    let db = TestDb::new().await;
    let tmp = tempdir().unwrap();
    let store = LocalSourceMapStore::new(tmp.path());

    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "gc-proj".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    // 40-char hex checksums == valid CAS keys.
    let referenced = "a".repeat(40);
    let orphan = "b".repeat(40);

    seed_project_source_map(&db.pool, project.id, &referenced).await; // has metadata
    insert_source_file(&db.pool, &orphan, 500).await; // no metadata → orphan

    store
        .put(&referenced, Bytes::from_static(b"ref"))
        .await
        .unwrap();
    store
        .put(&orphan, Bytes::from_static(b"orphan"))
        .await
        .unwrap();

    let result = StorageService::gc_source_maps(&db.pool, &store)
        .await
        .unwrap();

    assert_eq!(result.files_removed, 1, "only the orphan is removed");
    assert_eq!(result.bytes_freed, 500);

    // Orphan gone from DB and disk; referenced file survives both.
    assert!(
        !store.exists(&orphan).await.unwrap(),
        "orphan unlinked from disk"
    );
    assert!(
        store.exists(&referenced).await.unwrap(),
        "referenced file kept"
    );

    let storage = StorageService::source_map_storage(&db.pool).await.unwrap();
    assert_eq!(
        storage.file_count, 1,
        "only the referenced source_file row remains"
    );
}

/// Seeds one issue holding one event per entry of `ages`, with the issue's
/// counters set to the number of events (what the digest path would record).
/// Returns the issue id.
async fn seed_issue_with_events_at(
    pool: &rustrak::db::DbPool,
    project_id: i32,
    ages: &[chrono::DateTime<Utc>],
) -> Uuid {
    let issue_id = Uuid::new_v4();
    let digest_order = DIGEST_ORDER.fetch_add(1, Ordering::Relaxed);
    let count = ages.len() as i32;
    sqlx::query(
        "INSERT INTO issues (id, project_id, digest_order, first_seen, last_seen, \
         stored_event_count, digested_event_count) VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(issue_id)
    .bind(project_id)
    .bind(digest_order)
    .bind(Utc::now())
    .bind(Utc::now())
    .bind(count)
    .bind(count)
    .execute(pool)
    .await
    .unwrap();

    let grouping_id: i32 = sqlx::query_scalar(
        "INSERT INTO groupings (project_id, issue_id, grouping_key, grouping_key_hash) \
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(project_id)
    .bind(issue_id)
    .bind(format!("key-{digest_order}"))
    .bind(format!("{digest_order:0>64}"))
    .fetch_one(pool)
    .await
    .unwrap();

    for at in ages {
        sqlx::query(
            "INSERT INTO events \
             (id, event_id, project_id, issue_id, grouping_id, data, timestamp, ingested_at, digested_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(Uuid::new_v4())
        .bind(Uuid::new_v4())
        .bind(project_id)
        .bind(issue_id)
        .bind(grouping_id)
        .bind(serde_json::json!({}))
        .bind(at)
        .bind(at)
        .bind(at)
        .execute(pool)
        .await
        .unwrap();
    }
    issue_id
}

async fn issue_counters(pool: &rustrak::db::DbPool, issue_id: Uuid) -> Option<(i32, i32)> {
    sqlx::query_as("SELECT stored_event_count, digested_event_count FROM issues WHERE id = $1")
        .bind(issue_id)
        .fetch_optional(pool)
        .await
        .unwrap()
}

async fn project_counters(pool: &rustrak::db::DbPool, project_id: i32) -> (i32, i32) {
    sqlx::query_as("SELECT stored_event_count, digested_event_count FROM projects WHERE id = $1")
        .bind(project_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn test_cleanup_keeps_project_counters_right_after_every_batch() {
    // A restart can cut a cleanup short between any two batches, and nothing
    // repairs the project counters at startup: each batch has to leave them
    // right, not only the end of the run.
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "per-batch-counters".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();
    let old = Utc::now() - chrono::Duration::days(60);
    seed_issue_with_events_at(&db.pool, project.id, &[old, old, Utc::now()]).await;
    sqlx::query(
        "UPDATE projects SET stored_event_count = 3, digested_event_count = 3 WHERE id = $1",
    )
    .bind(project.id)
    .execute(&db.pool)
    .await
    .unwrap();

    let mut seen = Vec::new();
    StorageService::execute_cleanup_in_batches(&db.pool, 30, None, CleanupFilter::all(), 1, |c| {
        let counters = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(project_counters(&db.pool, project.id))
        });
        seen.push((c.events, counters));
    })
    .await
    .unwrap();

    assert_eq!(seen, vec![(1, (2, 2)), (2, (1, 1))]);
}

#[tokio::test]
async fn test_execute_cleanup_in_small_batches_removes_everything_and_keeps_counters_exact() {
    // The cleanup deletes in short batches so it never holds one huge write
    // transaction. Batches must add up to exactly what one pass would remove:
    // an issue whose events straddle several batches is decremented once per
    // event and removed only when its last event goes, an issue with a recent
    // event survives with the right counters, spans go with their transaction,
    // and every project in scope is walked.
    let db = TestDb::new().await;
    let create = |name: &str| CreateProject {
        name: name.to_string(),
        slug: None,
        platform: None,
    };
    let a = ProjectService::create(&db.pool, create("batch-a"))
        .await
        .unwrap();
    let b = ProjectService::create(&db.pool, create("batch-b"))
        .await
        .unwrap();

    let old = Utc::now() - chrono::Duration::days(60);
    let now = Utc::now();
    let emptied = seed_issue_with_events_at(&db.pool, a.id, &[old, old, old]).await;
    let survivor = seed_issue_with_events_at(&db.pool, a.id, &[old, old, now]).await;
    let b_issue = seed_issue_with_events_at(&db.pool, b.id, &[old, old]).await;
    for _ in 0..3 {
        seed_transaction_with_spans_at(&db.pool, a.id, 1, old).await;
        seed_log_at(&db.pool, a.id, old).await;
    }
    seed_transaction_with_spans_at(&db.pool, a.id, 1, now).await;
    seed_log_at(&db.pool, b.id, now).await;
    for (project, count) in [(a.id, 6), (b.id, 2)] {
        sqlx::query(
            "UPDATE projects SET stored_event_count = $1, digested_event_count = $1 WHERE id = $2",
        )
        .bind(count)
        .bind(project)
        .execute(&db.pool)
        .await
        .unwrap();
    }

    let mut progress: Vec<CleanupCounts> = Vec::new();
    let counts = StorageService::execute_cleanup_in_batches(
        &db.pool,
        30,
        None,
        CleanupFilter::all(),
        2,
        |c| progress.push(c.clone()),
    )
    .await
    .unwrap();

    assert_eq!(
        counts,
        CleanupCounts {
            events: 7,
            transactions: 3,
            spans: 3,
            logs: 3,
            issues_removed: 2,
        }
    );
    assert!(progress.len() > 3, "progress is reported batch by batch");
    assert!(
        progress
            .windows(2)
            .all(|w| w[0].events <= w[1].events && w[0].logs <= w[1].logs),
        "progress only grows"
    );
    assert_eq!(progress.last(), Some(&counts), "last report is the total");

    assert_eq!(
        issue_counters(&db.pool, emptied).await,
        None,
        "emptied issue removed"
    );
    assert_eq!(issue_counters(&db.pool, b_issue).await, None);
    assert_eq!(
        issue_counters(&db.pool, survivor).await,
        Some((1, 1)),
        "two of three events removed"
    );
    assert_eq!(project_counters(&db.pool, a.id).await, (1, 1));
    assert_eq!(project_counters(&db.pool, b.id).await, (0, 0));

    let summary = StorageService::global_summary(&db.pool).await.unwrap();
    assert_eq!(summary.events_count, 1);
    assert_eq!(summary.transactions_count, 1);
    assert_eq!(summary.spans_count, 1);
    assert_eq!(summary.logs_count, 1);
}

#[tokio::test]
async fn test_execute_cleanup_leaves_issues_it_did_not_touch() {
    // Only issues emptied by this cleanup are removed. An issue that already
    // had no events (or belongs to data out of scope) is not the cleanup's to
    // delete.
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "untouched-issue".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    let empty = seed_issue_with_events_at(&db.pool, project.id, &[]).await;
    seed_event_at(
        &db.pool,
        project.id,
        Utc::now() - chrono::Duration::days(60),
    )
    .await;

    let counts = StorageService::execute_cleanup(&db.pool, 30, None, CleanupFilter::all())
        .await
        .unwrap();

    assert_eq!(counts.issues_removed, 1, "only the emptied issue");
    assert!(issue_counters(&db.pool, empty).await.is_some());
}

#[tokio::test]
async fn test_cleanup_job_runs_in_the_background_and_refuses_a_second_run() {
    // The HTTP request only starts the cleanup; the work runs detached so a
    // client timeout can no longer roll it back. While it runs, a second start
    // is refused rather than racing the first.
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "job".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();
    let old = Utc::now() - chrono::Duration::days(60);
    seed_event_at(&db.pool, project.id, old).await;
    seed_log_at(&db.pool, project.id, old).await;

    let job = CleanupJob::default();
    assert_eq!(job.status().state, CleanupState::Idle);

    let started = job
        .start(db.pool.clone(), 30, None, CleanupFilter::all())
        .unwrap();
    assert_eq!(started.state, CleanupState::Running);
    assert!(started.started_at.is_some());

    let second = job.start(db.pool.clone(), 30, None, CleanupFilter::all());
    assert!(
        matches!(second, Err(AppError::Conflict(_))),
        "a second run is refused while the first is running"
    );

    let done = wait_for_job(&job).await;
    assert_eq!(done.state, CleanupState::Completed);
    assert_eq!(done.removed.events, 1);
    assert_eq!(done.removed.logs, 1);
    assert_eq!(done.removed.issues_removed, 1);
    assert!(done.finished_at.is_some());
    assert!(done.error.is_none());

    // Finished: a new run may start.
    job.start(db.pool.clone(), 30, None, CleanupFilter::all())
        .unwrap();
    assert_eq!(wait_for_job(&job).await.state, CleanupState::Completed);
}

#[tokio::test]
async fn test_cleanup_job_rejects_a_bad_window_without_starting() {
    let db = TestDb::new().await;
    let job = CleanupJob::default();

    let result = job.start(db.pool.clone(), 0, None, CleanupFilter::all());

    assert!(matches!(result, Err(AppError::Validation(_))));
    assert_eq!(job.status().state, CleanupState::Idle, "nothing started");
}

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn test_cleanup_lookups_are_index_range_scans() {
    // The timeouts this cleanup used to hit came from full passes over events.
    // Pin the plans of the batch pick and the preview count (same predicates as
    // StorageService uses) to the (project_id, ingested_at) indexes, so a
    // dropped index or a rewritten predicate fails here, not in production.
    let db = TestDb::new().await;
    let plan = |sql: &'static str| {
        let pool = db.pool.clone();
        async move {
            let rows: Vec<(i64, i64, i64, String)> =
                sqlx::query_as(sqlx::AssertSqlSafe(format!("EXPLAIN QUERY PLAN {sql}")))
                    .fetch_all(&pool)
                    .await
                    .unwrap();
            rows.into_iter()
                .map(|r| r.3)
                .collect::<Vec<_>>()
                .join(" | ")
        }
    };

    for (sql, index) in [
        (
            "SELECT id, issue_id FROM events WHERE project_id = $1 AND ingested_at < $2 LIMIT $3",
            "idx_events_project_ingested",
        ),
        (
            "SELECT id FROM transactions WHERE project_id = $1 AND ingested_at < $2 LIMIT $3",
            "idx_transactions_project_ingested",
        ),
        (
            "SELECT id FROM logs WHERE project_id = $1 AND ingested_at < $2 LIMIT $3",
            "idx_logs_project_ingested",
        ),
        (
            "SELECT COUNT(*) FROM events e WHERE e.ingested_at < $1 \
             AND e.project_id IN (SELECT id FROM projects WHERE $2 IS NULL OR id = $3)",
            "idx_events_project_ingested",
        ),
        // The storage page's size estimate samples each project's newest rows.
        (
            "SELECT data FROM events WHERE project_id = $1 ORDER BY ingested_at DESC LIMIT $2",
            "idx_events_project_ingested",
        ),
        (
            "SELECT data FROM transactions WHERE project_id = $1 ORDER BY ingested_at DESC LIMIT $2",
            "idx_transactions_project_ingested",
        ),
        (
            "SELECT body FROM logs WHERE project_id = $1 ORDER BY ingested_at DESC LIMIT $2",
            "idx_logs_project_ingested",
        ),
        (
            "SELECT COUNT(*) FROM issues i WHERE NOT EXISTS \
             (SELECT 1 FROM events e3 WHERE e3.issue_id = i.id AND e3.ingested_at >= $1)",
            "idx_events_issue_ingested",
        ),
    ] {
        let got = plan(sql).await;
        assert!(got.contains(index), "{sql}\n  expected {index}, got: {got}");
    }
}

/// Polls a cleanup job until it leaves `Running`, failing after 10 seconds.
async fn wait_for_job(job: &CleanupJob) -> CleanupStatus {
    for _ in 0..200 {
        let status = job.status();
        if status.state != CleanupState::Running {
            return status;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("cleanup job did not finish");
}

#[tokio::test]
async fn test_source_map_storage_sums_chunk_and_source_file_sizes() {
    // Source-map storage weight is exact: it's the SUM of the `size` columns on
    // `chunk` (in-DB BYTEA) and `source_file` (on-disk CAS). No filesystem walk.
    let db = TestDb::new().await;

    insert_chunk(&db.pool, "a".repeat(40).as_str(), 100).await;
    insert_chunk(&db.pool, "b".repeat(40).as_str(), 250).await;
    insert_source_file(&db.pool, "c".repeat(40).as_str(), 300).await;

    let storage = StorageService::source_map_storage(&db.pool).await.unwrap();

    assert_eq!(storage.chunk_bytes, 350, "100 + 250");
    assert_eq!(storage.source_file_bytes, 300);
    assert_eq!(storage.total_bytes, 650);
    assert_eq!(storage.file_count, 1, "one source_file row");
}
