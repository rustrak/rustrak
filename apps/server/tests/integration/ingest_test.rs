//! Integration tests for the Ingest API
//!
//! Tests event ingestion via the Sentry-compatible envelope endpoint.

use crate::common::TestDb;
use actix_web::{test, web, App};
use rustrak::config::{Config, DashboardConfig, DatabaseConfig, RateLimitConfig};
use rustrak::routes;
use rustrak::services::{
    DbSourceMapProvider, LocalSourceMapStore, ProjectService, SourceMapProvider, SourceMapStore,
};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

/// Creates a test config
fn create_test_config() -> Config {
    Config {
        host: "127.0.0.1".to_string(),
        port: 0,
        database: DatabaseConfig {
            url: "postgres://test:test@localhost/test".to_string(),
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
        ingest_dir: Some("/tmp/rustrak_test_ingest".to_string()),
        public_url: None,
        sourcemap_storage_path: "/tmp/test_sourcemaps".to_string(),
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

/// Creates a test project and returns its sentry_key
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

/// Creates a minimal valid Sentry envelope
fn create_envelope(event_id: &str, event_json: &str) -> Vec<u8> {
    let envelope = format!(
        r#"{{"event_id":"{}"}}
{{"type":"event","length":{}}}
{}"#,
        event_id,
        event_json.len(),
        event_json
    );
    envelope.into_bytes()
}

// =============================================================================
// Log item ingestion (Sentry "log" item type — OurLog container)
// =============================================================================

#[actix_web::test]
async fn test_ingest_log_envelope_stores_rows() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_test_project(&db.pool, "Logs Project").await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data({
                let store: Arc<dyn SourceMapStore> =
                    Arc::new(LocalSourceMapStore::new("/tmp/test_sourcemaps"));
                let provider: Arc<dyn SourceMapProvider> =
                    Arc::new(DbSourceMapProvider::new(db.pool.clone(), store));
                web::Data::new(provider)
            })
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

    // A log-only envelope: no event item, no event_id required. The single log
    // item carries a container with two OurLog records.
    let container = r#"{"items":[{"timestamp":1704801600.0,"trace_id":"5b8efff798038103d269b633813fc60c","level":"error","body":"boom"},{"timestamp":1704801601.0,"trace_id":"5b8efff798038103d269b633813fc60c","level":"info","body":"ok"}]}"#;
    let envelope = format!(
        "{{}}\n{{\"type\":\"log\",\"content_type\":\"application/vnd.sentry.items.log+json\",\"length\":{}}}\n{}",
        container.len(),
        container
    );

    let req = test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", project_id))
        .insert_header((
            "X-Sentry-Auth",
            format!("Sentry sentry_key={}, sentry_version=7", sentry_key),
        ))
        .insert_header(("Content-Type", "application/x-sentry-envelope"))
        .set_payload(envelope.into_bytes())
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success(), "log envelope should return 200");

    #[cfg(feature = "postgres")]
    const COUNT_QUERY: &str = "SELECT COUNT(*) FROM logs WHERE project_id = $1";
    #[cfg(not(feature = "postgres"))]
    const COUNT_QUERY: &str = "SELECT COUNT(*) FROM logs WHERE project_id = ?";

    let count: i64 = sqlx::query_scalar(COUNT_QUERY)
        .bind(project_id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(count, 2, "two-item log container → two stored rows");
}

// =============================================================================
// Standalone span item ingestion (Sentry "span" item type — story-span-ingestion.md)
// =============================================================================

#[actix_web::test]
async fn test_ingest_standalone_span_envelope_without_trace_header_stores_row() {
    // AC #3: a standalone span must be accepted WITHOUT a `trace` envelope
    // header (DSC) present — matches Relay's default legacy_spans pipeline,
    // which real SDKs rely on today (unlike the opt-in SpanV2 pipeline).
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_test_project(&db.pool, "Spans Project").await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data({
                let store: Arc<dyn SourceMapStore> =
                    Arc::new(LocalSourceMapStore::new("/tmp/test_sourcemaps"));
                let provider: Arc<dyn SourceMapProvider> =
                    Arc::new(DbSourceMapProvider::new(db.pool.clone(), store));
                web::Data::new(provider)
            })
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

    // No `trace` field anywhere in the envelope headers — DSC intentionally absent.
    let span_json = r#"{"span_id":"9fd17741416e8e4e","trace_id":"d3d20f000885466b8c8f947c9b92b8d3","op":"http.client","start_timestamp":1234567890.0,"timestamp":1234567890.5}"#;
    let envelope = format!(
        "{{}}\n{{\"type\":\"span\",\"length\":{}}}\n{}",
        span_json.len(),
        span_json
    );

    let req = test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", project_id))
        .insert_header((
            "X-Sentry-Auth",
            format!("Sentry sentry_key={}, sentry_version=7", sentry_key),
        ))
        .insert_header(("Content-Type", "application/x-sentry-envelope"))
        .set_payload(envelope.into_bytes())
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status().is_success(),
        "standalone span envelope without a trace header must be accepted"
    );

    // The processor runs in a spawned task — give it a beat to land.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    #[cfg(feature = "postgres")]
    const COUNT_QUERY: &str =
        "SELECT COUNT(*) FROM spans WHERE project_id = $1 AND transaction_id IS NULL";
    #[cfg(not(feature = "postgres"))]
    const COUNT_QUERY: &str =
        "SELECT COUNT(*) FROM spans WHERE project_id = ? AND transaction_id IS NULL";

    let count: i64 = sqlx::query_scalar(COUNT_QUERY)
        .bind(project_id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(
        count, 1,
        "standalone span must be stored with no transaction_id"
    );
}

#[actix_web::test]
async fn test_ingest_multiple_standalone_spans_in_one_envelope() {
    // Standalone spans are NOT containerized like logs — an envelope may
    // carry several separate "span" items, each one flat span object.
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_test_project(&db.pool, "Multi Span Project").await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data({
                let store: Arc<dyn SourceMapStore> =
                    Arc::new(LocalSourceMapStore::new("/tmp/test_sourcemaps"));
                let provider: Arc<dyn SourceMapProvider> =
                    Arc::new(DbSourceMapProvider::new(db.pool.clone(), store));
                web::Data::new(provider)
            })
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

    let span1 = r#"{"span_id":"1111111111111111","trace_id":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","op":"http.client","start_timestamp":1.0,"timestamp":1.5}"#;
    let span2 = r#"{"span_id":"2222222222222222","trace_id":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","op":"db.query","start_timestamp":1.5,"timestamp":1.8}"#;
    let envelope = format!(
        "{{}}\n{{\"type\":\"span\",\"length\":{}}}\n{}\n{{\"type\":\"span\",\"length\":{}}}\n{}",
        span1.len(),
        span1,
        span2.len(),
        span2
    );

    let req = test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", project_id))
        .insert_header((
            "X-Sentry-Auth",
            format!("Sentry sentry_key={}, sentry_version=7", sentry_key),
        ))
        .insert_header(("Content-Type", "application/x-sentry-envelope"))
        .set_payload(envelope.into_bytes())
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    #[cfg(feature = "postgres")]
    const COUNT_QUERY: &str = "SELECT COUNT(*) FROM spans WHERE project_id = $1";
    #[cfg(not(feature = "postgres"))]
    const COUNT_QUERY: &str = "SELECT COUNT(*) FROM spans WHERE project_id = ?";

    let count: i64 = sqlx::query_scalar(COUNT_QUERY)
        .bind(project_id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(count, 2, "both standalone span items must be stored");
}

// =============================================================================
// Basic Ingestion Tests
// =============================================================================

#[actix_web::test]
async fn test_ingest_basic_event() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_test_project(&db.pool, "Test Project").await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data({
                let store: Arc<dyn SourceMapStore> =
                    Arc::new(LocalSourceMapStore::new("/tmp/test_sourcemaps"));
                let provider: Arc<dyn SourceMapProvider> =
                    Arc::new(DbSourceMapProvider::new(db.pool.clone(), store));
                web::Data::new(provider)
            })
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
    let event_json = json!({
        "event_id": event_id,
        "timestamp": 1704801600.0,
        "level": "error",
        "platform": "python",
        "exception": {
            "values": [{
                "type": "ValueError",
                "value": "Invalid input"
            }]
        }
    })
    .to_string();

    let envelope = create_envelope(&event_id, &event_json);
    let compressed_envelope = zstd::stream::encode_all(envelope.as_slice(), 0).unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", project_id))
        .insert_header((
            "X-Sentry-Auth",
            format!("Sentry sentry_key={}, sentry_version=7", sentry_key),
        ))
        .insert_header(("Content-Type", "application/x-sentry-envelope"))
        .insert_header(("Content-Encoding", "zstd"))
        .set_payload(compressed_envelope)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["id"], event_id);
}

#[actix_web::test]
async fn test_ingest_event_is_digested_by_the_spawned_task() {
    // The production digest path: the route spawns a detached task that
    // takes the processing permit, reads the stored file and lands the
    // event + issue. Other tests drive the same processor directly via
    // process_error_event, so this one pins the spawn wiring itself.
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_test_project(&db.pool, "Spawned Digest").await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data({
                let store: Arc<dyn SourceMapStore> =
                    Arc::new(LocalSourceMapStore::new("/tmp/test_sourcemaps"));
                let provider: Arc<dyn SourceMapProvider> =
                    Arc::new(DbSourceMapProvider::new(db.pool.clone(), store));
                web::Data::new(provider)
            })
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
    let event_json = json!({
        "event_id": event_id,
        "timestamp": 1704801600.0,
        "level": "error",
        "platform": "python",
        "exception": {
            "values": [{ "type": "ValueError", "value": "Invalid input" }]
        }
    })
    .to_string();
    let envelope = create_envelope(&event_id, &event_json);

    let req = test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", project_id))
        .insert_header((
            "X-Sentry-Auth",
            format!("Sentry sentry_key={}, sentry_version=7", sentry_key),
        ))
        .insert_header(("Content-Type", "application/x-sentry-envelope"))
        .set_payload(envelope)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    // The spawned task is detached, so poll for its outcome instead of
    // joining: the event row appears only after permit → file read →
    // parse → grouping → insert all ran.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let (issue_id, _): (Uuid, Uuid) = loop {
        if let Some(row) = sqlx::query_as(
            "SELECT issue_id, id FROM events WHERE project_id = $1 AND event_id = $2",
        )
        .bind(project_id)
        .bind(Uuid::parse_str(&event_id).unwrap())
        .fetch_optional(&db.pool)
        .await
        .unwrap()
        {
            break row;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the spawned digest task should store the event within 5s"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    let issue_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM issues WHERE id = $1")
        .bind(issue_id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(issue_count, 1, "the digest must create the issue");
}

#[actix_web::test]
async fn test_transaction_ingest_is_not_blocked_by_a_saturated_digest_queue() {
    // Transactions and spans are persisted inline during the request. The
    // digest permit bounds the spawned error-digest tasks only: a burst of
    // errors being grouped must never make an SDK wait on its transaction.
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_test_project(&db.pool, "Saturated Digest").await;
    let config = create_test_config();
    let processors = web::Data::new(rustrak::digest::processors::Processors::new(
        rustrak::ingest::get_ingest_dir(config.ingest_dir.as_deref()),
        config.rate_limit.clone(),
        crate::common::null_sourcemap_provider(),
        None,
    ));
    // Consume every digest permit for the lifetime of the test.
    let slots = processors.processing_slot.available_permits() as u32;
    processors
        .processing_slot
        .try_acquire_many(slots)
        .expect("all permits should be free")
        .forget();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data(processors.clone())
            .configure(routes::ingest::configure),
    )
    .await;

    let event_id = Uuid::new_v4().to_string().replace("-", "");
    let transaction_json = json!({
        "event_id": event_id,
        "type": "transaction",
        "transaction": "GET /checkout",
        "start_timestamp": 1704801600.0,
        "timestamp": 1704801600.5,
        "contexts": {"trace": {"trace_id": "a".repeat(32), "span_id": "b".repeat(16), "op": "http.server"}}
    })
    .to_string();
    let envelope = format!(
        "{{\"event_id\":\"{event_id}\"}}\n{{\"type\":\"transaction\",\"length\":{}}}\n{transaction_json}",
        transaction_json.len()
    );

    let req = test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", project_id))
        .insert_header((
            "X-Sentry-Auth",
            format!("Sentry sentry_key={}, sentry_version=7", sentry_key),
        ))
        .insert_header(("Content-Type", "application/x-sentry-envelope"))
        .set_payload(envelope)
        .to_request();
    let resp = tokio::time::timeout(Duration::from_secs(2), test::call_service(&app, req))
        .await
        .expect("a transaction envelope must be accepted while the digest queue is saturated");
    assert!(resp.status().is_success());

    let stored: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM transactions WHERE project_id = $1")
        .bind(project_id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(stored, 1);
}

#[actix_web::test]
async fn test_ingest_accepts_utf8_content_encoding_from_old_sdks() {
    // sentry.java.android 2.0.0 sends `Content-Encoding: UTF-8` on a plain
    // body; Relay ignores it and so must we.
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_test_project(&db.pool, "UTF-8 Encoding").await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
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
    let event_json = json!({"event_id": event_id, "message": "plain body"}).to_string();
    let envelope = create_envelope(&event_id, &event_json);

    let req = test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", project_id))
        .insert_header((
            "X-Sentry-Auth",
            format!("Sentry sentry_key={}, sentry_version=7", sentry_key),
        ))
        .insert_header(("Content-Type", "application/x-sentry-envelope"))
        .insert_header(("Content-Encoding", "UTF-8"))
        .set_payload(envelope)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["id"], event_id);
}

#[actix_web::test]
async fn test_ingest_with_query_param_auth() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_test_project(&db.pool, "Query Auth Project").await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data({
                let store: Arc<dyn SourceMapStore> =
                    Arc::new(LocalSourceMapStore::new("/tmp/test_sourcemaps"));
                let provider: Arc<dyn SourceMapProvider> =
                    Arc::new(DbSourceMapProvider::new(db.pool.clone(), store));
                web::Data::new(provider)
            })
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
    let event_json = json!({
        "event_id": event_id,
        "level": "error"
    })
    .to_string();

    let envelope = create_envelope(&event_id, &event_json);

    // Use query parameter for auth instead of header
    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/{}/envelope/?sentry_key={}",
            project_id, sentry_key
        ))
        .insert_header(("Content-Type", "application/x-sentry-envelope"))
        .set_payload(envelope)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
}

// =============================================================================
// Authentication Error Tests
// =============================================================================

#[actix_web::test]
async fn test_ingest_missing_auth() {
    let db = TestDb::new().await;
    let (project_id, _) = create_test_project(&db.pool, "No Auth Project").await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data({
                let store: Arc<dyn SourceMapStore> =
                    Arc::new(LocalSourceMapStore::new("/tmp/test_sourcemaps"));
                let provider: Arc<dyn SourceMapProvider> =
                    Arc::new(DbSourceMapProvider::new(db.pool.clone(), store));
                web::Data::new(provider)
            })
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
    let event_json = json!({"event_id": event_id}).to_string();
    let envelope = create_envelope(&event_id, &event_json);

    let req = test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", project_id))
        .insert_header(("Content-Type", "application/x-sentry-envelope"))
        .set_payload(envelope)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn test_ingest_invalid_sentry_key() {
    let db = TestDb::new().await;
    let (project_id, _) = create_test_project(&db.pool, "Invalid Key Project").await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data({
                let store: Arc<dyn SourceMapStore> =
                    Arc::new(LocalSourceMapStore::new("/tmp/test_sourcemaps"));
                let provider: Arc<dyn SourceMapProvider> =
                    Arc::new(DbSourceMapProvider::new(db.pool.clone(), store));
                web::Data::new(provider)
            })
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
    let event_json = json!({"event_id": event_id}).to_string();
    let envelope = create_envelope(&event_id, &event_json);

    let req = test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", project_id))
        .insert_header((
            "X-Sentry-Auth",
            "Sentry sentry_key=00000000-0000-0000-0000-000000000000, sentry_version=7",
        ))
        .set_payload(envelope)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn test_ingest_wrong_project_id() {
    let db = TestDb::new().await;
    let (_, sentry_key) = create_test_project(&db.pool, "Wrong ID Project").await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data({
                let store: Arc<dyn SourceMapStore> =
                    Arc::new(LocalSourceMapStore::new("/tmp/test_sourcemaps"));
                let provider: Arc<dyn SourceMapProvider> =
                    Arc::new(DbSourceMapProvider::new(db.pool.clone(), store));
                web::Data::new(provider)
            })
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
    let event_json = json!({"event_id": event_id}).to_string();
    let envelope = create_envelope(&event_id, &event_json);

    // Use wrong project_id (99999) - project doesn't exist
    let req = test::TestRequest::post()
        .uri("/api/99999/envelope/")
        .insert_header((
            "X-Sentry-Auth",
            format!("Sentry sentry_key={}, sentry_version=7", sentry_key),
        ))
        .set_payload(envelope)
        .to_request();

    let resp = test::call_service(&app, req).await;
    // Project doesn't exist, so auth fails - could be 401 or 404 depending on implementation
    // Our auth checks project_id + sentry_key together, so it returns 404 when project not found
    assert!(resp.status() == 401 || resp.status() == 404);
}

// =============================================================================
// Envelope Validation Tests
// =============================================================================

#[actix_web::test]
async fn test_ingest_missing_event_id() {
    // Relay behavior: if an event item is present but event_id is absent from headers,
    // the server must auto-generate a UUID (not reject with 400).
    // relay-server/src/envelope/mod.rs:300 — get_or_insert_with(EventId::new)
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_test_project(&db.pool, "Missing Event ID").await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data({
                let store: Arc<dyn SourceMapStore> =
                    Arc::new(LocalSourceMapStore::new("/tmp/test_sourcemaps"));
                let provider: Arc<dyn SourceMapProvider> =
                    Arc::new(DbSourceMapProvider::new(db.pool.clone(), store));
                web::Data::new(provider)
            })
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

    // Envelope without event_id in headers — valid per Sentry protocol
    let envelope = br#"{}
{"type":"event","length":2}
{}"#;

    let req = test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", project_id))
        .insert_header((
            "X-Sentry-Auth",
            format!("Sentry sentry_key={}, sentry_version=7", sentry_key),
        ))
        .set_payload(envelope.to_vec())
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        200,
        "event without event_id must be accepted"
    );

    let body: Value = test::read_body_json(resp).await;
    let id = body["id"]
        .as_str()
        .expect("response must contain id when event item is present");
    assert_eq!(id.len(), 32, "auto-generated event_id must be 32-char hex");
    assert!(
        id.chars().all(|c| c.is_ascii_hexdigit()),
        "auto-generated event_id must be valid hex: {}",
        id
    );
}

#[actix_web::test]
async fn test_ingest_invalid_event_id_format() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_test_project(&db.pool, "Invalid Event ID").await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data({
                let store: Arc<dyn SourceMapStore> =
                    Arc::new(LocalSourceMapStore::new("/tmp/test_sourcemaps"));
                let provider: Arc<dyn SourceMapProvider> =
                    Arc::new(DbSourceMapProvider::new(db.pool.clone(), store));
                web::Data::new(provider)
            })
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

    // Envelope with invalid event_id (not a UUID)
    let envelope = br#"{"event_id":"not-a-uuid"}
{"type":"event","length":2}
{}"#;

    let req = test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", project_id))
        .insert_header((
            "X-Sentry-Auth",
            format!("Sentry sentry_key={}, sentry_version=7", sentry_key),
        ))
        .set_payload(envelope.to_vec())
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn test_ingest_invalid_json_payload() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_test_project(&db.pool, "Invalid JSON").await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data({
                let store: Arc<dyn SourceMapStore> =
                    Arc::new(LocalSourceMapStore::new("/tmp/test_sourcemaps"));
                let provider: Arc<dyn SourceMapProvider> =
                    Arc::new(DbSourceMapProvider::new(db.pool.clone(), store));
                web::Data::new(provider)
            })
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
    // Invalid JSON payload
    let envelope = format!(
        r#"{{"event_id":"{}"}}
{{"type":"event","length":12}}
not valid json"#,
        event_id
    );

    let req = test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", project_id))
        .insert_header((
            "X-Sentry-Auth",
            format!("Sentry sentry_key={}, sentry_version=7", sentry_key),
        ))
        .set_payload(envelope.into_bytes())
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

// =============================================================================
// Session Tracking Protocol Tests
// =============================================================================

#[actix_web::test]
async fn test_session_only_without_event_id_accepted() {
    // Sentry Node.js SDK sends session-only envelopes with empty headers: {}
    // Relay protocol: session items do NOT require event_id (requires_event() == false)
    // Expected: 200, response body {} (no id field — mirrors Relay StoreResponse)
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_test_project(&db.pool, "Session Only No ID").await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data({
                let store: Arc<dyn SourceMapStore> =
                    Arc::new(LocalSourceMapStore::new("/tmp/test_sourcemaps"));
                let provider: Arc<dyn SourceMapProvider> =
                    Arc::new(DbSourceMapProvider::new(db.pool.clone(), store));
                web::Data::new(provider)
            })
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

    let session_json =
        r#"{"started":"2020-02-07T14:16:00Z","attrs":{"release":"sentry-test@1.0.0"}}"#;
    let item_header = format!(r#"{{"type":"session","length":{}}}"#, session_json.len());
    let envelope = format!("{}\n{}\n{}\n", "{}", item_header, session_json);

    let req = test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", project_id))
        .insert_header((
            "X-Sentry-Auth",
            format!("Sentry sentry_key={}, sentry_version=7", sentry_key),
        ))
        .insert_header(("Content-Type", "application/x-sentry-envelope"))
        .set_payload(envelope.into_bytes())
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200, "session-only envelope must be accepted");

    let body: Value = test::read_body_json(resp).await;
    assert!(
        body.get("id").is_none(),
        "session-only envelope must not have id in response, got: {}",
        body
    );
}

#[actix_web::test]
async fn test_session_only_with_event_id_in_headers_echoes_it() {
    // When SDK provides event_id in envelope headers on a session-only envelope,
    // the server must echo it back (but still not reject if absent).
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_test_project(&db.pool, "Session With ID").await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data({
                let store: Arc<dyn SourceMapStore> =
                    Arc::new(LocalSourceMapStore::new("/tmp/test_sourcemaps"));
                let provider: Arc<dyn SourceMapProvider> =
                    Arc::new(DbSourceMapProvider::new(db.pool.clone(), store));
                web::Data::new(provider)
            })
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
    let session_json =
        r#"{"started":"2020-02-07T14:16:00Z","attrs":{"release":"sentry-test@1.0.0"}}"#;
    let item_header = format!(r#"{{"type":"session","length":{}}}"#, session_json.len());
    let envelope = format!(r#"{{"event_id":"{}"}}"#, event_id)
        + &format!("\n{}\n{}\n", item_header, session_json);

    let req = test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", project_id))
        .insert_header((
            "X-Sentry-Auth",
            format!("Sentry sentry_key={}, sentry_version=7", sentry_key),
        ))
        .insert_header(("Content-Type", "application/x-sentry-envelope"))
        .set_payload(envelope.into_bytes())
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["id"], event_id);
}

#[actix_web::test]
async fn test_event_without_event_id_auto_generates_uuid() {
    // Relay behavior: if envelope has an event item but no event_id in headers,
    // auto-generate a UUID (get_or_insert_with(EventId::new)).
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_test_project(&db.pool, "Auto Generate ID").await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data({
                let store: Arc<dyn SourceMapStore> =
                    Arc::new(LocalSourceMapStore::new("/tmp/test_sourcemaps"));
                let provider: Arc<dyn SourceMapProvider> =
                    Arc::new(DbSourceMapProvider::new(db.pool.clone(), store));
                web::Data::new(provider)
            })
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

    let event_json =
        r#"{"level":"error","exception":{"values":[{"type":"Error","value":"test"}]}}"#;
    let item_header = format!(r#"{{"type":"event","length":{}}}"#, event_json.len());
    let envelope = format!("{}\n{}\n{}\n", "{}", item_header, event_json);

    let req = test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", project_id))
        .insert_header((
            "X-Sentry-Auth",
            format!("Sentry sentry_key={}, sentry_version=7", sentry_key),
        ))
        .insert_header(("Content-Type", "application/x-sentry-envelope"))
        .set_payload(envelope.into_bytes())
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        200,
        "event without event_id must be accepted"
    );

    let body: Value = test::read_body_json(resp).await;
    let id = body["id"]
        .as_str()
        .expect("response must contain id field for event items");
    assert_eq!(id.len(), 32, "auto-generated id must be 32-char hex");
    assert!(
        id.chars().all(|c| c.is_ascii_hexdigit()),
        "auto-generated id must be valid hex: {}",
        id
    );
}

#[actix_web::test]
async fn test_mixed_session_and_event_without_event_id_auto_generates() {
    // Mixed envelope: session + event items, no event_id in headers.
    // Server must auto-generate event_id (event item requires one).
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_test_project(&db.pool, "Mixed Envelope").await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data({
                let store: Arc<dyn SourceMapStore> =
                    Arc::new(LocalSourceMapStore::new("/tmp/test_sourcemaps"));
                let provider: Arc<dyn SourceMapProvider> =
                    Arc::new(DbSourceMapProvider::new(db.pool.clone(), store));
                web::Data::new(provider)
            })
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

    let session_json =
        r#"{"started":"2020-02-07T14:16:00Z","attrs":{"release":"sentry-test@1.0.0"}}"#;
    let event_json =
        r#"{"level":"error","exception":{"values":[{"type":"Error","value":"mixed"}]}}"#;
    let envelope = format!(
        "{}\n{}\n{}\n{}\n{}\n",
        "{}",
        format_args!(r#"{{"type":"session","length":{}}}"#, session_json.len()),
        session_json,
        format_args!(r#"{{"type":"event","length":{}}}"#, event_json.len()),
        event_json,
    );

    let req = test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", project_id))
        .insert_header((
            "X-Sentry-Auth",
            format!("Sentry sentry_key={}, sentry_version=7", sentry_key),
        ))
        .insert_header(("Content-Type", "application/x-sentry-envelope"))
        .set_payload(envelope.into_bytes())
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    let id = body["id"]
        .as_str()
        .expect("mixed envelope with event must have id in response");
    assert_eq!(id.len(), 32, "auto-generated id must be 32-char hex");
}

// =============================================================================
// Special Cases Tests
// =============================================================================

#[actix_web::test]
async fn test_ingest_envelope_without_event_item() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_test_project(&db.pool, "No Event Item").await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data({
                let store: Arc<dyn SourceMapStore> =
                    Arc::new(LocalSourceMapStore::new("/tmp/test_sourcemaps"));
                let provider: Arc<dyn SourceMapProvider> =
                    Arc::new(DbSourceMapProvider::new(db.pool.clone(), store));
                web::Data::new(provider)
            })
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
    // Envelope with only session item, no event
    let envelope = format!(
        r#"{{"event_id":"{}"}}
{{"type":"session","length":2}}
{{}}"#,
        event_id
    );

    let req = test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", project_id))
        .insert_header((
            "X-Sentry-Auth",
            format!("Sentry sentry_key={}, sentry_version=7", sentry_key),
        ))
        .set_payload(envelope.into_bytes())
        .to_request();

    let resp = test::call_service(&app, req).await;
    // Should still return 200, but with no event stored
    assert!(resp.status().is_success());

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["id"], event_id);
}

#[actix_web::test]
async fn test_ingest_empty_body() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_test_project(&db.pool, "Empty Body").await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data({
                let store: Arc<dyn SourceMapStore> =
                    Arc::new(LocalSourceMapStore::new("/tmp/test_sourcemaps"));
                let provider: Arc<dyn SourceMapProvider> =
                    Arc::new(DbSourceMapProvider::new(db.pool.clone(), store));
                web::Data::new(provider)
            })
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

    let req = test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", project_id))
        .insert_header((
            "X-Sentry-Auth",
            format!("Sentry sentry_key={}, sentry_version=7", sentry_key),
        ))
        .set_payload(Vec::new())
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

// =============================================================================
// CORS Tests
// =============================================================================

#[actix_web::test]
async fn test_ingest_cors_preflight() {
    let db = TestDb::new().await;
    let (project_id, _) = create_test_project(&db.pool, "CORS Project").await;
    let config = create_test_config();

    // CORS is handled by middleware, so we need to include it in the test
    let cors = actix_cors::Cors::default()
        .allow_any_origin()
        .allowed_methods(vec!["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"])
        .allowed_headers(vec![
            actix_web::http::header::AUTHORIZATION,
            actix_web::http::header::ACCEPT,
            actix_web::http::header::CONTENT_TYPE,
            actix_web::http::header::CONTENT_ENCODING,
            actix_web::http::header::HeaderName::from_static("x-sentry-auth"),
        ])
        .max_age(3600);

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data({
                let store: Arc<dyn SourceMapStore> =
                    Arc::new(LocalSourceMapStore::new("/tmp/test_sourcemaps"));
                let provider: Arc<dyn SourceMapProvider> =
                    Arc::new(DbSourceMapProvider::new(db.pool.clone(), store));
                web::Data::new(provider)
            })
            .wrap(cors)
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

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::OPTIONS)
        .uri(&format!("/api/{}/envelope/", project_id))
        .insert_header(("Origin", "https://example.com"))
        .insert_header(("Access-Control-Request-Method", "POST"))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    // Check CORS headers
    let headers = resp.headers();
    assert!(headers.contains_key("access-control-allow-origin"));
    assert!(headers.contains_key("access-control-allow-methods"));
}

#[actix_web::test]
async fn test_ingest_response_has_cors_headers() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_test_project(&db.pool, "CORS Response Project").await;
    let config = create_test_config();

    // CORS is handled by middleware, so we need to include it in the test
    let cors = actix_cors::Cors::default()
        .allow_any_origin()
        .allowed_methods(vec!["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"])
        .allowed_headers(vec![
            actix_web::http::header::AUTHORIZATION,
            actix_web::http::header::ACCEPT,
            actix_web::http::header::CONTENT_TYPE,
            actix_web::http::header::CONTENT_ENCODING,
            actix_web::http::header::HeaderName::from_static("x-sentry-auth"),
        ])
        .max_age(3600);

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data({
                let store: Arc<dyn SourceMapStore> =
                    Arc::new(LocalSourceMapStore::new("/tmp/test_sourcemaps"));
                let provider: Arc<dyn SourceMapProvider> =
                    Arc::new(DbSourceMapProvider::new(db.pool.clone(), store));
                web::Data::new(provider)
            })
            .wrap(cors)
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
    let event_json = json!({"event_id": event_id}).to_string();
    let envelope = create_envelope(&event_id, &event_json);

    let req = test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", project_id))
        .insert_header(("Origin", "https://example.com"))
        .insert_header((
            "X-Sentry-Auth",
            format!("Sentry sentry_key={}, sentry_version=7", sentry_key),
        ))
        .set_payload(envelope)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let headers = resp.headers();
    // CORS reflects the Origin header back
    assert_eq!(
        headers.get("access-control-allow-origin").unwrap(),
        "https://example.com"
    );
}

// =============================================================================
// Payload Size Tests
// =============================================================================

#[actix_web::test]
async fn test_ingest_large_payload_above_256kb_is_accepted() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_test_project(&db.pool, "Large Payload Project").await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data({
                let store: Arc<dyn SourceMapStore> =
                    Arc::new(LocalSourceMapStore::new("/tmp/test_sourcemaps"));
                let provider: Arc<dyn SourceMapProvider> =
                    Arc::new(DbSourceMapProvider::new(db.pool.clone(), store));
                web::Data::new(provider)
            })
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
    // Build an event whose JSON exceeds actix-web's default 256KB body limit.
    // Without an explicit PayloadConfig, the framework rejects this before the
    // handler runs, returning 413 instead of 200.
    let padding = "x".repeat(300_000);
    let event_json = json!({
        "event_id": event_id,
        "timestamp": 1704801600.0,
        "level": "error",
        "platform": "python",
        "exception": {
            "values": [{"type": "ValueError", "value": "large stack trace"}]
        },
        "_padding": padding
    })
    .to_string();

    assert!(
        event_json.len() > 256 * 1024,
        "test payload must exceed 256KB, got {} bytes",
        event_json.len()
    );

    let envelope = create_envelope(&event_id, &event_json);

    let req = test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", project_id))
        .insert_header((
            "X-Sentry-Auth",
            format!("Sentry sentry_key={}, sentry_version=7", sentry_key),
        ))
        .insert_header(("Content-Type", "application/x-sentry-envelope"))
        .set_payload(envelope)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        200,
        "payloads above 256KB must be accepted by the ingest endpoint"
    );
}

// =============================================================================
// Legacy Store Endpoint Tests
// =============================================================================

#[actix_web::test]
async fn test_store_endpoint_deprecated() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_test_project(&db.pool, "Store Project").await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data({
                let store: Arc<dyn SourceMapStore> =
                    Arc::new(LocalSourceMapStore::new("/tmp/test_sourcemaps"));
                let provider: Arc<dyn SourceMapProvider> =
                    Arc::new(DbSourceMapProvider::new(db.pool.clone(), store));
                web::Data::new(provider)
            })
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

    let req = test::TestRequest::post()
        .uri(&format!("/api/{}/store/", project_id))
        .insert_header((
            "X-Sentry-Auth",
            format!("Sentry sentry_key={}, sentry_version=7", sentry_key),
        ))
        .set_payload(b"{}".to_vec())
        .to_request();

    let resp = test::call_service(&app, req).await;
    // Should return 400 because store is deprecated
    assert_eq!(resp.status(), 400);
}
