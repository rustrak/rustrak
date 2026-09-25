//! Level 3 integration tests for typed envelope dispatch (spec: spec-transaction-processing.md)
//!
//! Full HTTP → dispatch → DB path. Tests the transaction spawn path in routes/ingest.rs.

use crate::common::TestDb;
use actix_web::{test, web, App};
use rustrak::config::{Config, DashboardConfig, DatabaseConfig, RateLimitConfig};
use rustrak::ingest::{store_event_with_metadata, EventMetadata};
use rustrak::routes;
use rustrak::services::{DbSourceMapProvider, LocalSourceMapStore, ProjectService};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use uuid::Uuid;

fn create_test_config() -> Config {
    Config {
        host: "127.0.0.1".to_string(),
        port: 0,
        database: DatabaseConfig {
            url: "sqlite::memory:".to_string(),
            max_connections: 5,
            min_connections: 1,
            acquire_timeout: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(60),
            max_lifetime: Duration::from_secs(300),
        },
        rate_limit: RateLimitConfig {
            max_events_per_minute: 1000,
            max_events_per_hour: 10000,
            max_events_per_project_per_minute: 500,
            max_events_per_project_per_hour: 5000,
        },
        security: rustrak::config::SecurityConfig {
            ssl_proxy: false,
            session_secret_key: None,
        },
        ingest_dir: Some("/tmp/rustrak_test_ingest_v2".to_string()),
        public_url: None,
        sourcemap_storage_path: "/tmp/test_sourcemaps_v2".to_string(),
        sourcemap_cache_bytes: 64 * 1024 * 1024,
        max_chunk_size_bytes: 10 * 1024 * 1024,
        session_flush_interval_secs: 30,
        session_cardinality_cap: 10_000,
        dashboard: DashboardConfig {
            dir: "./static".to_string(),
            enabled: true,
            url: None,
        },
        telemetry: rustrak::config::TelemetryConfig {
            enabled: false,
            do_not_track: false,
        },
    }
}

async fn create_test_project(pool: &rustrak::db::DbPool, name: &str) -> (i32, String) {
    let project = ProjectService::create(
        pool,
        rustrak::models::CreateProject {
            name: name.to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .expect("Failed to create test project");
    (project.id, project.sentry_key.to_string())
}

fn transaction_envelope(event_id: &str, project_id: i32, sentry_key: &str) -> Vec<u8> {
    let txn = json!({
        "event_id": event_id,
        "type": "transaction",
        "transaction": "/api/test",
        "start_timestamp": 1704801590.0_f64,
        "timestamp": 1704801600.0_f64,
        "spans": [{"op": "db", "description": "SELECT 1"}]
    });
    let txn_str = txn.to_string();
    let envelope = format!(
        "{}\n{}\n{}\n",
        json!({ "event_id": event_id }),
        json!({ "type": "transaction", "length": txn_str.len() }),
        txn_str
    );
    let _ = (project_id, sentry_key); // used via URL/header
    envelope.into_bytes()
}

fn error_envelope(event_id: &str) -> Vec<u8> {
    let event = json!({
        "event_id": event_id,
        "timestamp": 1704801600.0_f64,
        "platform": "python",
        "level": "error",
        "exception": {
            "values": [{
                "type": "ValueError",
                "value": "something went wrong"
            }]
        }
    });
    let event_str = event.to_string();
    format!(
        "{}\n{}\n{}\n",
        json!({ "event_id": event_id }),
        json!({ "type": "event", "length": event_str.len() }),
        event_str
    )
    .into_bytes()
}

/// Regression guard: error envelope must still work after EnvelopeItemKind refactor.
#[actix_web::test]
async fn test_error_envelope_still_works_after_refactor() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_test_project(&db.pool, "Error Regression Test").await;
    let config = create_test_config();

    let sourcemap_store = Arc::new(LocalSourceMapStore::new(&config.sourcemap_storage_path))
        as Arc<dyn rustrak::services::SourceMapStore>;
    let sourcemap_provider = Arc::new(DbSourceMapProvider::new(db.pool.clone(), sourcemap_store))
        as Arc<dyn rustrak::services::SourceMapProvider>;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data(web::Data::new(sourcemap_provider))
            .app_data(web::Data::new(
                rustrak::digest::processors::Processors::new(
                    rustrak::ingest::get_ingest_dir(config.ingest_dir.as_deref()),
                    config.rate_limit.clone(),
                    crate::common::null_sourcemap_provider(),
                    None,
                ),
            ))
            .configure(routes::ingest::configure),
    )
    .await;

    let event_id = Uuid::new_v4().to_string().replace("-", "");
    let body = error_envelope(&event_id);

    let req = test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", project_id))
        .insert_header(("X-Sentry-Auth", format!("Sentry sentry_key={}", sentry_key)))
        .insert_header(("Content-Type", "application/x-sentry-envelope"))
        .set_payload(body)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status().as_u16(),
        200,
        "error envelope must return 200"
    );

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body.get("id").is_some(), "response must include event id");
}

/// Transaction-only envelope must return 200 with an id, then the spawned
/// processor stores the row. We verify the response and poll (bounded) for the
/// asynchronously-stored `events` row.
#[actix_web::test]
async fn test_transaction_envelope_returns_200_with_id() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_test_project(&db.pool, "Transaction Ingest Test").await;
    let config = create_test_config();

    let sourcemap_store = Arc::new(LocalSourceMapStore::new(&config.sourcemap_storage_path))
        as Arc<dyn rustrak::services::SourceMapStore>;
    let sourcemap_provider = Arc::new(DbSourceMapProvider::new(db.pool.clone(), sourcemap_store))
        as Arc<dyn rustrak::services::SourceMapProvider>;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data(web::Data::new(sourcemap_provider))
            .app_data(web::Data::new(
                rustrak::digest::processors::Processors::new(
                    rustrak::ingest::get_ingest_dir(config.ingest_dir.as_deref()),
                    config.rate_limit.clone(),
                    crate::common::null_sourcemap_provider(),
                    None,
                ),
            ))
            .configure(routes::ingest::configure),
    )
    .await;

    let event_id = Uuid::new_v4().to_string().replace("-", "");
    let body = transaction_envelope(&event_id, project_id, &sentry_key);

    let req = test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", project_id))
        .insert_header(("X-Sentry-Auth", format!("Sentry sentry_key={}", sentry_key)))
        .insert_header(("Content-Type", "application/x-sentry-envelope"))
        .set_payload(body)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status().as_u16(),
        200,
        "transaction envelope must return 200"
    );

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body.get("id").is_some(), "response must include event id");

    // The processor runs in a spawned task; poll (bounded) instead of a fixed
    // sleep so the assertion isn't flaky on slower CI runners.
    #[cfg(feature = "postgres")]
    const COUNT_QUERY: &str = "SELECT COUNT(*) FROM transactions WHERE project_id = $1";
    #[cfg(not(feature = "postgres"))]
    const COUNT_QUERY: &str = "SELECT COUNT(*) FROM transactions WHERE project_id = ?";

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    let stored: i64 = loop {
        let count: i64 = sqlx::query_scalar(COUNT_QUERY)
            .bind(project_id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        if count > 0 || tokio::time::Instant::now() >= deadline {
            break count;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    };

    assert_eq!(
        stored, 1,
        "transaction must be stored in the dedicated transactions table"
    );
}

#[actix_web::test]
async fn same_event_id_isolated_between_projects_over_http() {
    let db = TestDb::new().await;
    let (first_project, first_key) = create_test_project(&db.pool, "Scoped Event One").await;
    let (second_project, second_key) = create_test_project(&db.pool, "Scoped Event Two").await;
    let temp_dir = TempDir::new().unwrap();
    let mut config = create_test_config();
    config.ingest_dir = Some(temp_dir.path().to_string_lossy().into_owned());

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data(web::Data::new(crate::common::null_sourcemap_provider()))
            .app_data(web::Data::new(
                rustrak::digest::processors::Processors::new(
                    rustrak::ingest::get_ingest_dir(config.ingest_dir.as_deref()),
                    config.rate_limit.clone(),
                    crate::common::null_sourcemap_provider(),
                    None,
                ),
            ))
            .configure(routes::ingest::configure),
    )
    .await;

    let event_id = Uuid::new_v4().simple().to_string();
    for (project_id, sentry_key) in [(first_project, first_key), (second_project, second_key)] {
        let req = test::TestRequest::post()
            .uri(&format!("/api/{project_id}/envelope/"))
            .insert_header(("X-Sentry-Auth", format!("Sentry sentry_key={sentry_key}")))
            .insert_header(("Content-Type", "application/x-sentry-envelope"))
            .set_payload(error_envelope(&event_id))
            .to_request();
        assert_eq!(test::call_service(&app, req).await.status(), 200);
    }

    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        let first_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE project_id = $1")
                .bind(first_project)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        let second_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE project_id = $1")
                .bind(second_project)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        if first_count == 1 && second_count == 1 {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "both project events must be stored"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[actix_web::test]
async fn pending_event_recovery_replays_a_scoped_event() {
    let db = TestDb::new().await;
    let (project, _) = create_test_project(&db.pool, "Recovery Replay Test").await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config();
    let event_id = Uuid::new_v4().simple().to_string();
    let metadata = EventMetadata {
        event_id: event_id.clone(),
        project_id: project,
        ingested_at: chrono::Utc::now(),
        remote_addr: None,
    };
    let event_data = serde_json::json!({
        "event_id": event_id,
        "timestamp": 1704801600.0_f64,
        "platform": "rust",
        "level": "error",
        "exception": {"values": [{"type": "Recovered", "value": "after restart"}]}
    });
    store_event_with_metadata(
        temp_dir.path(),
        &metadata.event_id,
        &serde_json::to_vec(&event_data).unwrap(),
        &metadata,
    )
    .await
    .unwrap();

    let processors = web::Data::new(rustrak::digest::processors::Processors::new(
        temp_dir.path().to_path_buf(),
        config.rate_limit,
        crate::common::null_sourcemap_provider(),
        None,
    ));
    routes::ingest::recover_pending_events_once(
        db.pool.clone(),
        processors,
        temp_dir.path().to_path_buf(),
    )
    .await;

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE project_id = $1")
        .bind(project)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

/// Writes one scoped pending event and returns its metadata.
async fn store_scoped_event(dir: &std::path::Path, project: i32) -> EventMetadata {
    let event_id = Uuid::new_v4().simple().to_string();
    let metadata = EventMetadata {
        event_id: event_id.clone(),
        project_id: project,
        ingested_at: chrono::Utc::now(),
        remote_addr: None,
    };
    let event_data = serde_json::json!({
        "event_id": event_id,
        "timestamp": 1704801600.0_f64,
        "platform": "rust",
        "level": "error",
        "exception": {"values": [{"type": "Owned", "value": "by a task"}]}
    });
    store_event_with_metadata(
        dir,
        &metadata.event_id,
        &serde_json::to_vec(&event_data).unwrap(),
        &metadata,
    )
    .await
    .unwrap();
    metadata
}

fn pending_files(dir: &std::path::Path) -> usize {
    std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .ends_with(".pending.json")
        })
        .count()
}

/// The recovery scan must leave alone a file some task in this process still
/// owns. Before the in-flight registry, a scan during a backlog replayed the
/// whole queue on top of the spawned digests: the same event digested twice,
/// and thousands of failed reads for files the tasks had just deleted.
#[actix_web::test]
async fn pending_event_recovery_skips_events_owned_by_this_process() {
    let db = TestDb::new().await;
    let (project, _) = create_test_project(&db.pool, "Recovery Skips Owned").await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config();
    let metadata = store_scoped_event(temp_dir.path(), project).await;

    let processors = web::Data::new(rustrak::digest::processors::Processors::new(
        temp_dir.path().to_path_buf(),
        config.rate_limit,
        crate::common::null_sourcemap_provider(),
        None,
    ));
    let owner = processors
        .errors
        .in_flight()
        .register(project, Uuid::parse_str(&metadata.event_id).unwrap());

    routes::ingest::recover_pending_events_once(
        db.pool.clone(),
        processors.clone(),
        temp_dir.path().to_path_buf(),
    )
    .await;

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE project_id = $1")
        .bind(project)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "an owned event must not be replayed");
    assert_eq!(pending_files(temp_dir.path()), 1, "and its file must stay");

    // Once the owner is gone the file is a genuine leftover again.
    drop(owner);
    routes::ingest::recover_pending_events_once(
        db.pool.clone(),
        processors,
        temp_dir.path().to_path_buf(),
    )
    .await;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE project_id = $1")
        .bind(project)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

/// On SQLite a scoped file is deleted only after a durability checkpoint that
/// started after its commit. That checkpoint runs on a background queue, so
/// the digest returns first and the file follows shortly after; the event
/// stays registered as in flight until then.
#[cfg(feature = "sqlite")]
#[actix_web::test]
async fn scoped_event_file_is_deleted_after_the_durability_checkpoint() {
    use rustrak::digest::processors::{Processor, ProcessorCtx};

    let db = TestDb::new().await;
    let (project, _) = create_test_project(&db.pool, "Durability Queue").await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config();
    let processors = rustrak::digest::processors::Processors::new(
        temp_dir.path().to_path_buf(),
        config.rate_limit,
        crate::common::null_sourcemap_provider(),
        None,
    );

    let first = store_scoped_event(temp_dir.path(), project).await;
    let second = store_scoped_event(temp_dir.path(), project).await;
    for metadata in [&first, &second] {
        let ctx = ProcessorCtx {
            pool: db.pool.clone(),
            project_id: project,
            event_id: Uuid::parse_str(&metadata.event_id).unwrap(),
            ingested_at: metadata.ingested_at,
            remote_addr: None,
        };
        processors
            .errors
            .process(metadata.clone(), &ctx)
            .await
            .expect("digest must succeed");
    }

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE project_id = $1")
        .bind(project)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(count, 2, "both digests have committed");

    // The worker deletes the files and then drops the batch's guards; this
    // task can be polled between the two, so wait for both together.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while pending_files(temp_dir.path()) > 0 || !processors.errors.in_flight().is_empty() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the durability queue must delete both files after its checkpoint and release their in-flight entries"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Relay parity: a malformed item payload never fails the envelope — the
/// invalid item is dropped (Relay records an Invalid outcome) and every
/// sibling item is still processed, with the endpoint returning 200.
/// (relay-server/src/processing/relay.rs run_one: "This is not a fatal error
/// case ... other items from the same original envelope must still be
/// processed.")
#[actix_web::test]
async fn malformed_log_item_is_dropped_and_sibling_event_still_ingests() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_test_project(&db.pool, "Malformed Log Sibling").await;
    let temp_dir = TempDir::new().unwrap();
    let mut config = create_test_config();
    config.ingest_dir = Some(temp_dir.path().to_string_lossy().into_owned());

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data(web::Data::new(crate::common::null_sourcemap_provider()))
            .app_data(web::Data::new(
                rustrak::digest::processors::Processors::new(
                    rustrak::ingest::get_ingest_dir(config.ingest_dir.as_deref()),
                    config.rate_limit.clone(),
                    crate::common::null_sourcemap_provider(),
                    None,
                ),
            ))
            .configure(routes::ingest::configure),
    )
    .await;

    let event_id = Uuid::new_v4().simple().to_string();
    let event = json!({
        "event_id": event_id,
        "timestamp": 1704801600.0_f64,
        "platform": "python",
        "level": "error",
        "exception": {
            "values": [{ "type": "ValueError", "value": "sibling survives" }]
        }
    })
    .to_string();
    let broken_log_container = "{this is not json";
    let envelope = format!(
        "{}\n{}\n{}\n{}\n{}\n",
        json!({ "event_id": event_id }),
        json!({ "type": "log", "item_count": 1, "content_type": "application/vnd.sentry.items.log+json" }),
        broken_log_container,
        json!({ "type": "event" }),
        event,
    );

    let req = test::TestRequest::post()
        .uri(&format!("/api/{project_id}/envelope/"))
        .insert_header(("X-Sentry-Auth", format!("Sentry sentry_key={sentry_key}")))
        .insert_header(("Content-Type", "application/x-sentry-envelope"))
        .set_payload(envelope)
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        200,
        "a malformed log item must be dropped, not fail the envelope"
    );

    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE project_id = $1")
            .bind(project_id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        if count == 1 {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the sibling event must still be ingested"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// A new issue's alert links into the dashboard at the address the processors
/// were given, which `main` resolves from `DASHBOARD_URL` or `PUBLIC_URL`.
/// Before 0.15 the digest read `DASHBOARD_URL` itself and fell back to
/// `http://localhost:3000`, the port of a dashboard container that no longer
/// exists.
#[actix_web::test]
async fn new_issue_alert_links_to_the_dashboard_the_processors_were_given() {
    use rustrak::digest::processors::{Processor, ProcessorCtx};
    use rustrak::models::{
        AlertRuleChannelInput, AlertType, ChannelType, CreateAlertRule, CreateNotificationChannel,
    };
    use rustrak::services::AlertService;

    let db = TestDb::new().await;
    let (project, _) = create_test_project(&db.pool, "Alert Links").await;
    let channel = AlertService::create_channel(
        &db.pool,
        CreateNotificationChannel {
            name: "Links Webhook".to_string(),
            provider_type: ChannelType::Webhook,
            credentials: json!({ "url": "https://example.com/webhook" }),
            is_enabled: true,
        },
    )
    .await
    .unwrap();
    let rule = AlertService::create_rule(
        &db.pool,
        project,
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
    // In cooldown the alert is recorded with its payload but never delivered,
    // so this test adds no failed delivery to the process-wide telemetry
    // counters other tests read.
    sqlx::query("UPDATE alert_rules SET last_triggered_at = $1 WHERE id = $2")
        .bind(chrono::Utc::now())
        .bind(rule.id)
        .execute(&db.pool)
        .await
        .unwrap();

    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config();
    let processors = rustrak::digest::processors::Processors::new(
        temp_dir.path().to_path_buf(),
        config.rate_limit,
        crate::common::null_sourcemap_provider(),
        None,
    )
    .with_dashboard_url("https://rustrak.example.com");

    let metadata = store_scoped_event(temp_dir.path(), project).await;
    let ctx = ProcessorCtx {
        pool: db.pool.clone(),
        project_id: project,
        event_id: Uuid::parse_str(&metadata.event_id).unwrap(),
        ingested_at: metadata.ingested_at,
        remote_addr: None,
    };
    processors
        .errors
        .process(metadata, &ctx)
        .await
        .expect("digest must succeed");

    let payload: serde_json::Value =
        sqlx::query_scalar("SELECT payload FROM alert_history WHERE project_id = $1")
            .bind(project)
            .fetch_one(&db.pool)
            .await
            .expect("the new issue must have queued an alert");
    let issue_url = payload["issue_url"].as_str().unwrap();
    assert!(
        issue_url.starts_with(&format!(
            "https://rustrak.example.com/projects/{project}/issues/"
        )),
        "issue_url was {issue_url}"
    );
}
