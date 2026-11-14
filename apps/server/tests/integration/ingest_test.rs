//! Integration tests for the Ingest API
//!
//! Tests event ingestion via the Sentry-compatible envelope endpoint.

use crate::common::{create_ingest_project, create_test_token, eventually, TestDb};
use actix_web::test;
use rustrak::config::Config;
use serde_json::{json, Value};
use std::time::Duration;
use uuid::Uuid;

fn create_test_config() -> Config {
    crate::common::config_with_ingest_dir(std::path::Path::new("/tmp/rustrak_test_ingest"))
}

/// The project's spans through the API, once `count` of them are stored.
async fn wait_for_spans<S>(
    app: &S,
    pool: &rustrak::db::DbPool,
    project_id: i32,
    count: usize,
) -> Vec<Value>
where
    S: actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
{
    let token = create_test_token(pool).await;
    eventually(|| async {
        let req = test::TestRequest::get()
            .uri(&format!("/api/projects/{project_id}/spans"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let body: Value = test::call_and_read_body_json(app, req).await;
        let items = body["items"].as_array().unwrap().clone();
        (items.len() >= count).then_some(items)
    })
    .await
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

#[actix_web::test]
async fn test_ingest_log_envelope_stores_rows() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "Logs Project").await;
    let config = create_test_config();

    let app = crate::common::app_with_config(&db.pool, config.clone()).await;

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

    const COUNT_QUERY: &str = "SELECT COUNT(*) FROM logs WHERE project_id = $1";

    let count: i64 = sqlx::query_scalar(COUNT_QUERY)
        .bind(project_id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(count, 2, "two-item log container → two stored rows");
}

#[actix_web::test]
async fn test_ingest_standalone_span_envelope_without_trace_header_stores_row() {
    // AC #3: a standalone span must be accepted WITHOUT a `trace` envelope
    // header (DSC) present — matches Relay's default legacy_spans pipeline,
    // which real SDKs rely on today (unlike the opt-in SpanV2 pipeline).
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "Spans Project").await;
    let config = create_test_config();

    let app = crate::common::app_with_config(&db.pool, config.clone()).await;

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

    let spans = wait_for_spans(&app, &db.pool, project_id, 1).await;
    assert_eq!(spans.len(), 1);
    assert!(
        spans[0]["transaction_id"].is_null(),
        "standalone span must be stored with no transaction_id"
    );
}

#[actix_web::test]
async fn test_ingest_multiple_standalone_spans_in_one_envelope() {
    // Standalone spans are NOT containerized like logs — an envelope may
    // carry several separate "span" items, each one flat span object.
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "Multi Span Project").await;
    let config = create_test_config();

    let app = crate::common::app_with_config(&db.pool, config.clone()).await;

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

    let spans = wait_for_spans(&app, &db.pool, project_id, 2).await;
    assert_eq!(spans.len(), 2, "both standalone span items must be stored");
}

#[actix_web::test]
async fn test_ingest_basic_event() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "Test Project").await;
    let config = create_test_config();

    let app = crate::common::app_with_config(&db.pool, config.clone()).await;

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

/// One error event, posted to `project_id` with `sentry_key`.
fn event_request(project_id: i32, sentry_key: &str) -> actix_http::Request {
    let event_id = Uuid::new_v4().simple().to_string();
    let event_json = json!({
        "event_id": event_id,
        "exception": {"values": [{"type": "ValueError", "value": "shed"}]}
    })
    .to_string();
    test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", project_id))
        .insert_header((
            "X-Sentry-Auth",
            format!("Sentry sentry_key={}, sentry_version=7", sentry_key),
        ))
        .set_payload(create_envelope(&event_id, &event_json))
        .to_request()
}

/// While the digest is this far behind, ingest sheds new events with 503 and
/// `Retry-After` instead of letting the spool grow until the disk fills. The
/// rejected event is not spooled; the SDK drops it and pauses sending.
#[actix_web::test]
async fn ingest_sheds_load_while_the_spool_backlog_is_full() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "Backlog").await;
    let spool = tempfile::tempdir().unwrap();
    let config = Config {
        ingest_max_pending_events: 2,
        ..crate::common::config_with_ingest_dir(spool.path())
    };
    let state = crate::common::state(&db.pool, config).await;
    let pending = state.processors.errors.pending();
    pending.insert(project_id, Uuid::new_v4());
    pending.insert(project_id, Uuid::new_v4());
    let app = crate::common::app_with(&state).await;

    let resp = test::call_service(&app, event_request(project_id, &sentry_key)).await;

    assert_eq!(resp.status(), 503);
    assert_eq!(
        resp.headers().get("Retry-After").unwrap(),
        &rustrak::error::RETRY_AFTER_SECS.to_string()
    );
    assert_eq!(pending.counts(project_id).installation, 2);
}

/// Shedding needs no body, so it comes before the body is read and decoded:
/// a body that would not even decode is refused with the 503, not a 400.
#[actix_web::test]
async fn ingest_sheds_before_reading_the_body() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "Shed First").await;
    let config = Config {
        ingest_max_pending_events: 1,
        ..create_test_config()
    };
    let state = crate::common::state(&db.pool, config).await;
    state
        .processors
        .errors
        .pending()
        .insert(project_id, Uuid::new_v4());
    let app = crate::common::app_with(&state).await;

    let req = test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", project_id))
        .insert_header((
            "X-Sentry-Auth",
            format!("Sentry sentry_key={}, sentry_version=7", sentry_key),
        ))
        .insert_header(("Content-Encoding", "gzip"))
        .set_payload("not gzip at all")
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 503);
}

/// Below the free-disk floor on the spool's volume, ingest sheds too. The
/// free space is sampled on a timer, never read per request.
#[actix_web::test]
async fn ingest_sheds_load_below_the_free_disk_floor() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "Disk Floor").await;
    let config = Config {
        ingest_min_free_disk_bytes: u64::MAX,
        ..create_test_config()
    };
    let state = crate::common::state(&db.pool, config).await;
    let app = crate::common::app_with(&state).await;

    // Not sampled yet: nothing is known, nothing is shed.
    let resp = test::call_service(&app, event_request(project_id, &sentry_key)).await;
    assert_eq!(resp.status(), 200);

    state.processors.errors.sample_free_disk().await;
    let resp = test::call_service(&app, event_request(project_id, &sentry_key)).await;
    assert_eq!(resp.status(), 503);
    assert!(resp.headers().get("Retry-After").is_some());
}

/// Each shed request is counted by why, so an operator can tell a slow
/// digest from a filling disk.
#[actix_web::test]
async fn shed_requests_are_counted_by_reason() {
    let db = TestDb::new().await;
    let spool = tempfile::tempdir().unwrap();
    let counters: &'static rustrak::telemetry::Counters =
        Box::leak(Box::new(rustrak::telemetry::Counters::new()));
    let processors = crate::common::processors_with_config(
        &db.pool,
        &Config {
            ingest_max_pending_events: 1,
            ingest_min_free_disk_bytes: u64::MAX,
            ..crate::common::config_with_ingest_dir(spool.path())
        },
    )
    .with_counters(counters);

    assert!(processors.admit(1).is_err(), "the backlog is full");
    processors.errors.sample_free_disk().await;
    assert!(processors.admit(0).is_err(), "the disk is under the floor");

    let metrics = counters.render(Default::default(), None);
    assert!(metrics.contains("rustrak_ingest_shed_total{reason=\"backlog\"} 1"));
    assert!(metrics.contains("rustrak_ingest_shed_total{reason=\"disk\"} 1"));
}

/// The digest keeps writing once ingest sheds, and SQLite's write-ahead log
/// can grow by up to its deferred checkpoint before it is copied back: the
/// floor leaves that much room on top of `INGEST_MIN_FREE_DISK_MB`, so the
/// database cannot fill the disk while ingest still accepts events.
#[cfg(all(feature = "sqlite", unix))]
#[allow(clippy::useless_conversion)] // the statvfs field types differ per platform
#[actix_web::test]
async fn the_free_disk_floor_leaves_room_for_the_write_ahead_log() {
    let db = TestDb::new().await;
    let spool = tempfile::tempdir().unwrap();
    let stat = nix::sys::statvfs::statvfs(spool.path()).unwrap();
    let free = u64::from(stat.blocks_available()) * u64::from(stat.fragment_size());
    let wal = u64::from(rustrak::db::COMMITTER_CHECKPOINT_PAGES) * 4096;
    let processors = crate::common::processors_with_config(
        &db.pool,
        &Config {
            ingest_min_free_disk_bytes: free - wal / 2,
            ..crate::common::config_with_ingest_dir(spool.path())
        },
    );

    processors.errors.sample_free_disk().await;

    assert!(
        processors.admit(0).is_err(),
        "less room than the log may still take, and ingest accepts events"
    );
}

/// SIGTERM in the middle of a backlog: shutdown waits a bounded time for the
/// digests, closes the pool, and every event it could not digest is still in
/// the spool for the next start to recover. Nothing acknowledged is lost.
#[actix_web::test]
async fn a_shutdown_mid_digest_leaves_undigested_events_in_the_spool() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "Shutdown").await;
    let spool = tempfile::tempdir().unwrap();
    let state = crate::common::state(
        &db.pool,
        crate::common::config_with_ingest_dir(spool.path()),
    )
    .await;
    // Every digest is stuck waiting for a slot, as in a backlog.
    let slots = state.processors.processing_slot.available_permits() as u32;
    state
        .processors
        .processing_slot
        .try_acquire_many(slots)
        .unwrap()
        .forget();
    let app = crate::common::app_with(&state).await;
    for _ in 0..5 {
        let resp = test::call_service(&app, event_request(project_id, &sentry_key)).await;
        assert_eq!(resp.status(), 200);
    }

    let started = std::time::Instant::now();
    state.drain(Duration::from_millis(200)).await;
    // Draining, ingest sheds with a 503 the SDK backs off from.
    let resp = test::call_service(&app, event_request(project_id, &sentry_key)).await;
    assert_eq!(resp.status(), 503);
    state.shutdown().await;

    assert!(
        started.elapsed() < Duration::from_secs(5),
        "shutdown is bounded"
    );
    assert!(db.pool.is_closed());
    let spooled = rustrak::ingest::list_pending_event_metadata(spool.path())
        .await
        .unwrap();
    assert_eq!(
        spooled.len(),
        5,
        "every acknowledged event is still spooled"
    );
}

/// `docker stop` sends SIGKILL 10 s after SIGTERM. With a backlog, the drain
/// waits only for the digests already running, not for the queue behind
/// them, which stays in the spool; and the session counts are written at
/// its start, not after it, so a SIGKILL cannot take them.
#[actix_web::test]
async fn a_drain_with_a_backlog_does_not_wait_for_the_queue_and_flushes_sessions_first() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "Drain Backlog").await;
    let spool = tempfile::tempdir().unwrap();
    let state = crate::common::state(
        &db.pool,
        crate::common::config_with_ingest_dir(spool.path()),
    )
    .await;
    let app = crate::common::app_with(&state).await;

    let session_json = json!({
        "sid": Uuid::new_v4().to_string(),
        "init": true,
        "started": chrono::Utc::now().to_rfc3339(),
        "status": "ok",
        "attrs": {"release": "drain@1.0.0"}
    })
    .to_string();
    let envelope = format!(
        "{{}}\n{{\"type\":\"session\",\"length\":{}}}\n{}\n",
        session_json.len(),
        session_json
    );
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/api/{}/envelope/", project_id))
            .insert_header((
                "X-Sentry-Auth",
                format!("Sentry sentry_key={}, sentry_version=7", sentry_key),
            ))
            .set_payload(envelope.into_bytes())
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);

    // The running digests hold every slot; five more events queue behind.
    let slots = state.processors.processing_slot.available_permits() as u32;
    let running = state
        .processors
        .processing_slot
        .clone()
        .try_acquire_many_owned(slots)
        .unwrap();
    for _ in 0..5 {
        let resp = test::call_service(&app, event_request(project_id, &sentry_key)).await;
        assert_eq!(resp.status(), 200);
    }

    let drain = tokio::spawn({
        let state = state.clone();
        async move { state.drain(Duration::from_secs(5)).await }
    });
    let sessions = crate::common::eventually(|| async {
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM session_counts")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        (n > 0).then_some(n)
    })
    .await;
    assert_eq!(sessions, 1, "flushed while the drain still waits");
    assert!(!drain.is_finished(), "the running digests hold their slots");

    let started = std::time::Instant::now();
    drop(running);
    drain.await.unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "the queue is not waited for"
    );
    let spooled = rustrak::ingest::list_pending_event_metadata(spool.path())
        .await
        .unwrap();
    assert_eq!(spooled.len(), 5, "queued events stay in the spool");
}

/// With the digests free to run, shutdown waits for them before it closes
/// the pool, so the spool is drained rather than left for the next start.
#[actix_web::test]
async fn a_shutdown_waits_for_in_flight_digests() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "Shutdown Drain").await;
    let spool = tempfile::tempdir().unwrap();
    let state = crate::common::state(
        &db.pool,
        crate::common::config_with_ingest_dir(spool.path()),
    )
    .await;
    let app = crate::common::app_with(&state).await;
    for _ in 0..5 {
        let resp = test::call_service(&app, event_request(project_id, &sentry_key)).await;
        assert_eq!(resp.status(), 200);
    }

    state.drain(Duration::from_secs(10)).await;
    state.shutdown().await;

    assert!(db.pool.is_closed());
    let spooled = rustrak::ingest::list_pending_event_metadata(spool.path())
        .await
        .unwrap();
    assert!(
        spooled.is_empty(),
        "{} events left undigested",
        spooled.len()
    );
}

#[actix_web::test]
async fn test_ingest_event_is_digested_by_the_spawned_task() {
    // The production digest path: the route spawns a detached task that
    // takes the processing permit, reads the stored file and lands the
    // event + issue. Other tests drive the same processor directly via
    // process_error_event, so this one pins the spawn wiring itself.
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "Spawned Digest").await;
    let config = create_test_config();

    let app = crate::common::app_with_config(&db.pool, config.clone()).await;

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

    // The event appears only after permit → file read → parse → grouping →
    // insert all ran in the detached task.
    let token = create_test_token(&db.pool).await;
    let issues = eventually(|| async {
        let req = test::TestRequest::get()
            .uri(&format!("/api/projects/{project_id}/issues"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let body: Value = test::call_and_read_body_json(&app, req).await;
        let items = body["items"].as_array().unwrap().clone();
        (!items.is_empty()).then_some(items)
    })
    .await;
    assert_eq!(issues.len(), 1, "the digest must create the issue");
    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{project_id}/issues/{}/events",
            issues[0]["id"].as_str().unwrap()
        ))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let events: Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(
        events["items"][0]["event_id"]
            .as_str()
            .unwrap()
            .replace('-', ""),
        event_id
    );
}

#[actix_web::test]
async fn test_transaction_ingest_is_not_blocked_by_a_saturated_digest_queue() {
    // Transactions and spans are persisted inline during the request. The
    // commit gate pauses the error digest only: a burst of
    // errors being grouped must never make an SDK wait on its transaction.
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "Saturated Digest").await;
    let state = crate::common::state(&db.pool, create_test_config()).await;
    let processors = state.processors.clone();
    // Hold the commit gate for the lifetime of the test.
    let slots = processors.processing_slot.available_permits() as u32;
    processors
        .processing_slot
        .try_acquire_many(slots)
        .expect("all permits should be free")
        .forget();

    let app = crate::common::app_with(&state).await;

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
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "UTF-8 Encoding").await;
    let config = create_test_config();

    let app = crate::common::app_with_config(&db.pool, config.clone()).await;

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
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "Query Auth Project").await;
    let config = create_test_config();

    let app = crate::common::app_with_config(&db.pool, config.clone()).await;

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

#[actix_web::test]
async fn test_ingest_missing_auth() {
    let db = TestDb::new().await;
    let (project_id, _) = create_ingest_project(&db.pool, "No Auth Project").await;
    let config = create_test_config();

    let app = crate::common::app_with_config(&db.pool, config.clone()).await;

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
    let (project_id, _) = create_ingest_project(&db.pool, "Invalid Key Project").await;
    let config = create_test_config();

    let app = crate::common::app_with_config(&db.pool, config.clone()).await;

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
    let (_, sentry_key) = create_ingest_project(&db.pool, "Wrong ID Project").await;
    let config = create_test_config();

    let app = crate::common::app_with_config(&db.pool, config.clone()).await;

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

#[actix_web::test]
async fn test_ingest_missing_event_id() {
    // Relay behavior: if an event item is present but event_id is absent from headers,
    // the server must auto-generate a UUID (not reject with 400).
    // relay-server/src/envelope/mod.rs:300 — get_or_insert_with(EventId::new)
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "Missing Event ID").await;
    let config = create_test_config();

    let app = crate::common::app_with_config(&db.pool, config.clone()).await;

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
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "Invalid Event ID").await;
    let config = create_test_config();

    let app = crate::common::app_with_config(&db.pool, config.clone()).await;

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
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "Invalid JSON").await;
    let config = create_test_config();

    let app = crate::common::app_with_config(&db.pool, config.clone()).await;

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

#[actix_web::test]
async fn test_session_only_without_event_id_accepted() {
    // Sentry Node.js SDK sends session-only envelopes with empty headers: {}
    // Relay protocol: session items do NOT require event_id (requires_event() == false)
    // Expected: 200, response body {} (no id field — mirrors Relay StoreResponse)
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "Session Only No ID").await;
    let config = create_test_config();

    let app = crate::common::app_with_config(&db.pool, config.clone()).await;

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
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "Session With ID").await;
    let config = create_test_config();

    let app = crate::common::app_with_config(&db.pool, config.clone()).await;

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
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "Auto Generate ID").await;
    let config = create_test_config();

    let app = crate::common::app_with_config(&db.pool, config.clone()).await;

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
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "Mixed Envelope").await;
    let config = create_test_config();

    let app = crate::common::app_with_config(&db.pool, config.clone()).await;

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

#[actix_web::test]
async fn test_ingest_envelope_without_event_item() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "No Event Item").await;
    let config = create_test_config();

    let app = crate::common::app_with_config(&db.pool, config.clone()).await;

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
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "Empty Body").await;
    let config = create_test_config();

    let app = crate::common::app_with_config(&db.pool, config.clone()).await;

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

#[actix_web::test]
async fn test_ingest_cors_preflight() {
    let db = TestDb::new().await;
    let (project_id, _) = create_ingest_project(&db.pool, "CORS Project").await;
    let config = create_test_config();

    // CORS is handled by middleware, so we need to include it in the test

    let app = crate::common::app_with_config(&db.pool, config.clone()).await;

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::OPTIONS)
        .uri(&format!("/api/{}/envelope/", project_id))
        .insert_header(("Origin", "https://example.com"))
        .insert_header(("Access-Control-Request-Method", "POST"))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let headers = resp.headers();
    assert!(headers.contains_key("access-control-allow-origin"));
    assert!(headers.contains_key("access-control-allow-methods"));
}

#[actix_web::test]
async fn test_ingest_response_has_cors_headers() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "CORS Response Project").await;
    let config = create_test_config();

    // CORS is handled by middleware, so we need to include it in the test

    let app = crate::common::app_with_config(&db.pool, config.clone()).await;

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

#[actix_web::test]
async fn test_ingest_large_payload_above_256kb_is_accepted() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "Large Payload Project").await;
    let config = create_test_config();

    let app = crate::common::app_with_config(&db.pool, config.clone()).await;

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

#[actix_web::test]
async fn test_store_endpoint_ingests_a_json_event_like_relay() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "Store Project").await;
    let app = crate::common::app_with_config(&db.pool, create_test_config()).await;

    let event_id = Uuid::new_v4().simple().to_string();
    let event = json!({
        "event_id": event_id,
        "exception": {"values": [{"type": "StoreError", "value": "legacy"}]}
    });
    let req = test::TestRequest::post()
        .uri(&format!("/api/{project_id}/store/?sentry_key={sentry_key}"))
        .insert_header(("Content-Type", "application/json"))
        .insert_header(("Content-Encoding", "gzip"))
        .set_payload(encode("gzip", event.to_string().as_bytes()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["id"], event_id);

    let token = create_test_token(&db.pool).await;
    let issues = eventually(|| async {
        let req = test::TestRequest::get()
            .uri(&format!("/api/projects/{project_id}/issues"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let body: Value = test::call_and_read_body_json(&app, req).await;
        let items = body["items"].as_array().unwrap().clone();
        (!items.is_empty()).then_some(items)
    })
    .await;
    assert_eq!(issues[0]["title"], "StoreError: legacy");
}

#[actix_web::test]
async fn test_store_endpoint_without_event_id_answers_one() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "Store No Id").await;
    let app = crate::common::app_with_config(&db.pool, create_test_config()).await;

    let req = test::TestRequest::post()
        .uri(&format!("/api/{project_id}/store/"))
        .insert_header((
            "X-Sentry-Auth",
            format!("Sentry sentry_key={sentry_key}, sentry_version=7"),
        ))
        .set_payload(json!({"message": "no id"}).to_string())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["id"].as_str().unwrap().len(), 32);
}

#[actix_web::test]
async fn test_store_endpoint_rejects_a_body_that_is_not_json() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "Store Bad").await;
    let app = crate::common::app_with_config(&db.pool, create_test_config()).await;

    let req = test::TestRequest::post()
        .uri(&format!("/api/{project_id}/store/?sentry_key={sentry_key}"))
        .set_payload("not json")
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 400);
}

fn encode(coding: &str, data: &[u8]) -> Vec<u8> {
    use std::io::Write;
    match coding {
        "gzip" => {
            let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
            e.write_all(data).unwrap();
            e.finish().unwrap()
        }
        "deflate" => {
            let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
            e.write_all(data).unwrap();
            e.finish().unwrap()
        }
        "br" => {
            let mut out = Vec::new();
            {
                let mut e = brotli::CompressorWriter::new(&mut out, 4096, 5, 22);
                e.write_all(data).unwrap();
            }
            out
        }
        "zstd" => zstd::stream::encode_all(data, 1).unwrap(),
        "identity" => data.to_vec(),
        other => panic!("no encoder for {other}"),
    }
}

async fn post_envelope(
    pool: &rustrak::db::DbPool,
    encoding: &str,
    body: Vec<u8>,
) -> actix_web::http::StatusCode {
    let name = format!("Encodings {}", Uuid::new_v4());
    let (project_id, sentry_key) = create_ingest_project(pool, &name).await;
    let app = crate::common::app_with_config(pool, create_test_config()).await;
    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/{project_id}/envelope/?sentry_key={sentry_key}"
        ))
        .insert_header(("Content-Encoding", encoding))
        .set_payload(body)
        .to_request();
    test::call_service(&app, req).await.status()
}

fn small_envelope() -> Vec<u8> {
    let event_id = Uuid::new_v4().simple().to_string();
    create_envelope(&event_id, &json!({"event_id": event_id}).to_string())
}

#[actix_web::test]
async fn test_ingest_decodes_every_content_encoding_relay_accepts() {
    let db = TestDb::new().await;
    for coding in ["gzip", "deflate", "br", "zstd", "identity"] {
        let status = post_envelope(&db.pool, coding, encode(coding, &small_envelope())).await;
        assert_eq!(status, 200, "Content-Encoding {coding}");
    }
}

#[actix_web::test]
async fn test_ingest_decodes_the_body_exactly_once() {
    // A gzip of a gzip under one `gzip` header is gzip bytes after one
    // decode: not an envelope.
    let db = TestDb::new().await;
    let twice = encode("gzip", &encode("gzip", &small_envelope()));
    assert_eq!(post_envelope(&db.pool, "gzip", twice).await, 400);
}

#[actix_web::test]
async fn test_ingest_rejects_a_body_that_does_not_match_its_encoding() {
    let db = TestDb::new().await;
    for coding in ["gzip", "deflate", "br", "zstd"] {
        let status = post_envelope(&db.pool, coding, small_envelope()).await;
        assert_eq!(status, 400, "plain body sent as {coding}");
    }
}

#[actix_web::test]
async fn test_ingest_rejects_an_unsupported_content_encoding_like_relay() {
    // tower-http's RequestDecompression: 415 naming the codings it accepts.
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "Unsupported coding").await;
    let app = crate::common::app_with_config(&db.pool, create_test_config()).await;
    for coding in ["lz4", "gzip, br"] {
        let req = test::TestRequest::post()
            .uri(&format!(
                "/api/{project_id}/envelope/?sentry_key={sentry_key}"
            ))
            .insert_header(("Content-Encoding", coding))
            .set_payload(small_envelope())
            .to_request();
        let res = test::call_service(&app, req).await;
        assert_eq!(res.status(), 415, "Content-Encoding {coding}");
        assert_eq!(
            res.headers().get("Accept-Encoding").unwrap(),
            "zstd,gzip,deflate,br"
        );
    }
}

#[actix_web::test]
async fn test_ingest_caps_the_compressed_body() {
    let db = TestDb::new().await;
    let body = vec![b' '; rustrak::ingest::MAX_COMPRESSED_SIZE + 1];
    assert_eq!(post_envelope(&db.pool, "gzip", body).await, 413);
}

/// Bodies too large to decode on the worker thread are decoded off it, with
/// the same result.
#[actix_web::test]
async fn test_ingest_decodes_a_large_body_in_every_encoding() {
    use rustrak::ingest::decompression::INLINE_DECODE_LIMIT;
    let db = TestDb::new().await;
    // Random hex compresses about two to one, never below.
    let noise: String = (0..6_400)
        .map(|_| Uuid::new_v4().simple().to_string())
        .collect();
    let event_id = Uuid::new_v4().simple().to_string();
    let envelope = create_envelope(
        &event_id,
        &json!({"event_id": event_id, "extra": {"noise": noise}}).to_string(),
    );
    for coding in ["gzip", "deflate", "br", "zstd"] {
        let body = encode(coding, &envelope);
        assert!(body.len() > INLINE_DECODE_LIMIT, "{coding}: {}", body.len());
        assert_eq!(post_envelope(&db.pool, coding, body).await, 200, "{coding}");
    }
    // Corrupt past the limit: the same 400 as a small one.
    let mut corrupt = encode("gzip", &envelope);
    let middle = corrupt.len() / 2;
    corrupt[middle..middle + 64].fill(0xff);
    assert_eq!(post_envelope(&db.pool, "gzip", corrupt).await, 400);
}

#[actix_web::test]
async fn test_ingest_caps_the_decompressed_body() {
    let db = TestDb::new().await;
    for coding in ["gzip", "deflate", "br", "zstd"] {
        let bomb = encode(
            coding,
            &vec![0; rustrak::ingest::decompression::MAX_DECOMPRESSED_SIZE + 1],
        );
        assert_eq!(post_envelope(&db.pool, coding, bomb).await, 413, "{coding}");
    }
}

/// Batching must not add a wait: with nothing else to do, an event is in the
/// database a moment after its 200, every time.
#[actix_web::test]
async fn an_idle_event_is_visible_within_250_ms_of_its_ack() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "Idle Latency").await;
    let spool = tempfile::tempdir().unwrap();
    let state = crate::common::state(
        &db.pool,
        crate::common::config_with_ingest_dir(spool.path()),
    )
    .await;
    let app = crate::common::app_with(&state).await;

    for n in 1..=20i64 {
        let resp = test::call_service(&app, event_request(project_id, &sentry_key)).await;
        assert_eq!(resp.status(), 200);
        let acked = std::time::Instant::now();
        loop {
            let stored: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE project_id = $1")
                    .bind(project_id)
                    .fetch_one(&db.pool)
                    .await
                    .unwrap();
            if stored == n {
                break;
            }
            assert!(
                acked.elapsed() < Duration::from_millis(250),
                "event {n} not stored 250 ms after its ack"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
}

/// The digest's memory budget bounds the spool bytes it holds, and an event
/// larger than the whole budget still runs, alone, without starving the
/// small ones queued around it.
#[actix_web::test]
async fn a_large_event_and_a_burst_of_small_ones_make_progress_under_the_budget() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "Budget").await;
    let spool = tempfile::tempdir().unwrap();
    let state = crate::common::state(
        &db.pool,
        Config {
            digest_memory_bytes: 1024 * 1024,
            ..crate::common::config_with_ingest_dir(spool.path())
        },
    )
    .await;
    let app = crate::common::app_with(&state).await;

    let event_id = Uuid::new_v4().simple().to_string();
    let large = json!({
        "event_id": event_id,
        "message": "large",
        "extra": {"blob": "x".repeat(4 * 1024 * 1024 - 1024)}
    })
    .to_string();
    let req = test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", project_id))
        .insert_header((
            "X-Sentry-Auth",
            format!("Sentry sentry_key={}, sentry_version=7", sentry_key),
        ))
        .set_payload(create_envelope(&event_id, &large))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 200);
    for _ in 0..200 {
        let resp = test::call_service(&app, event_request(project_id, &sentry_key)).await;
        assert_eq!(resp.status(), 200);
    }

    let started = std::time::Instant::now();
    loop {
        let stored: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE project_id = $1")
            .bind(project_id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        if stored == 201 {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "{stored} of 201 events digested after 30 s"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
