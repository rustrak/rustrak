//! Level 3 integration tests for typed envelope dispatch (spec: spec-transaction-processing.md)
//!
//! Full HTTP → dispatch → DB path. Tests the transaction spawn path in routes/ingest.rs.

use crate::common::create_ingest_project;
use crate::common::TestDb;
use actix_web::test;
use rustrak::config::Config;
use rustrak::ingest::{store_event_with_metadata, EventMetadata};
use serde_json::json;
use tempfile::TempDir;
use uuid::Uuid;

fn create_test_config() -> Config {
    Config {
        sourcemap_storage_path: "/tmp/test_sourcemaps_v2".to_string(),
        ..crate::common::config_with_ingest_dir(std::path::Path::new("/tmp/rustrak_test_ingest_v2"))
    }
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
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "Error Regression Test").await;
    let config = create_test_config();

    let app = crate::common::app_with_config(&db.pool, config.clone()).await;

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
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "Transaction Ingest Test").await;
    let config = create_test_config();

    let app = crate::common::app_with_config(&db.pool, config.clone()).await;

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

    let stored: i64 = crate::common::eventually(|| async {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM transactions WHERE project_id = $1")
                .bind(project_id)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        (count > 0).then_some(count)
    })
    .await;

    assert_eq!(
        stored, 1,
        "transaction must be stored in the dedicated transactions table"
    );
}

#[actix_web::test]
async fn same_event_id_isolated_between_projects_over_http() {
    let db = TestDb::new().await;
    let (first_project, first_key) = create_ingest_project(&db.pool, "Scoped Event One").await;
    let (second_project, second_key) = create_ingest_project(&db.pool, "Scoped Event Two").await;
    let temp_dir = TempDir::new().unwrap();
    let mut config = create_test_config();
    config.ingest_dir = temp_dir.path().to_path_buf();

    let app = crate::common::app_with_config(&db.pool, config.clone()).await;

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

    crate::common::eventually(|| async {
        let mut both = true;
        for project in [first_project, second_project] {
            let count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE project_id = $1")
                    .bind(project)
                    .fetch_one(&db.pool)
                    .await
                    .unwrap();
            both &= count == 1;
        }
        both.then_some(())
    })
    .await;
}

#[actix_web::test]
async fn pending_event_recovery_replays_a_scoped_event() {
    let db = TestDb::new().await;
    let (project, _) = create_ingest_project(&db.pool, "Recovery Replay Test").await;
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

    let processors = std::sync::Arc::new(crate::common::processors_with_config(
        &db.pool,
        &rustrak::config::Config {
            ingest_dir: temp_dir.path().to_path_buf(),
            rate_limit: config.rate_limit,
            ..crate::common::config()
        },
    ));
    rustrak::ingest::pipeline::recover_pending_events_once(
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
    let (project, _) = create_ingest_project(&db.pool, "Recovery Skips Owned").await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config();
    let metadata = store_scoped_event(temp_dir.path(), project).await;

    let processors = std::sync::Arc::new(crate::common::processors_with_config(
        &db.pool,
        &rustrak::config::Config {
            ingest_dir: temp_dir.path().to_path_buf(),
            rate_limit: config.rate_limit,
            ..crate::common::config()
        },
    ));
    let owner = processors
        .errors
        .in_flight()
        .register(project, Uuid::parse_str(&metadata.event_id).unwrap());

    rustrak::ingest::pipeline::recover_pending_events_once(
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
    rustrak::ingest::pipeline::recover_pending_events_once(
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

/// A scoped file is deleted once the commit that stored its event is
/// durable (the digest's own `synchronous=FULL` connection), and the event
/// stays registered as in flight until then.
#[cfg(feature = "sqlite")]
#[actix_web::test]
async fn scoped_event_file_is_deleted_after_its_commit() {
    let db = TestDb::new().await;
    let (project, _) = create_ingest_project(&db.pool, "Durability Queue").await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config();
    let processors = crate::common::processors_with_config(
        &db.pool,
        &rustrak::config::Config {
            ingest_dir: temp_dir.path().to_path_buf(),
            rate_limit: config.rate_limit,
            ..crate::common::config()
        },
    );

    let first = store_scoped_event(temp_dir.path(), project).await;
    let second = store_scoped_event(temp_dir.path(), project).await;
    for metadata in [&first, &second] {
        processors
            .errors
            .digest(&db.pool, metadata)
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
    crate::common::eventually(|| async {
        (pending_files(temp_dir.path()) == 0 && processors.errors.in_flight().is_empty())
            .then_some(())
    })
    .await;
}

/// A process killed after a digest committed but before its file was deleted
/// leaves the file behind. The next process replays it into the event
/// already stored, not a second one, and then removes the file.
#[actix_web::test]
async fn an_event_committed_before_a_crash_replays_into_the_same_row() {
    let db = TestDb::new().await;
    let (project, _) = create_ingest_project(&db.pool, "Crash Replay").await;
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config();
    let processors = || {
        std::sync::Arc::new(crate::common::processors_with_config(
            &db.pool,
            &rustrak::config::Config {
                ingest_dir: temp_dir.path().to_path_buf(),
                rate_limit: config.rate_limit.clone(),
                ..crate::common::config()
            },
        ))
    };

    // The first process commits the digest; the copy of the file below is
    // what the kill left behind.
    let metadata = store_scoped_event(temp_dir.path(), project).await;
    let path = std::fs::read_dir(temp_dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.to_string_lossy().ends_with(".pending.json"))
        .unwrap();
    let left_behind = std::fs::read(&path).unwrap();
    processors()
        .errors
        .digest(&db.pool, &metadata)
        .await
        .expect("digest must succeed");
    crate::common::eventually(|| async { (!path.exists()).then_some(()) }).await;
    std::fs::write(&path, left_behind).unwrap();

    // The restarted process.
    let restarted = processors();
    rustrak::ingest::pipeline::recover_pending_events_once(
        db.pool.clone(),
        restarted.clone(),
        temp_dir.path().to_path_buf(),
    )
    .await;

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE project_id = $1")
        .bind(project)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "the replay must not store the event twice");
    crate::common::eventually(|| async {
        (pending_files(temp_dir.path()) == 0 && restarted.errors.in_flight().is_empty())
            .then_some(())
    })
    .await;
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
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "Malformed Log Sibling").await;
    let temp_dir = TempDir::new().unwrap();
    let mut config = create_test_config();
    config.ingest_dir = temp_dir.path().to_path_buf();

    let app = crate::common::app_with_config(&db.pool, config.clone()).await;

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

    crate::common::eventually(|| async {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE project_id = $1")
            .bind(project_id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        (count == 1).then_some(())
    })
    .await;
}

/// A new issue's alert links into the dashboard at the address the processors
/// were given, which `main` resolves from `DASHBOARD_URL` or `PUBLIC_URL`.
/// Before 0.15 the digest read `DASHBOARD_URL` itself and fell back to
/// `http://localhost:3000`, the port of a dashboard container that no longer
/// exists.
#[actix_web::test]
async fn new_issue_alert_links_to_the_dashboard_the_processors_were_given() {
    use rustrak::models::{
        AlertRuleChannelInput, AlertType, CreateAlertIntegration, CreateAlertRule, ProviderType,
    };
    use rustrak::services::AlertService;

    let db = TestDb::new().await;
    let (project, _) = create_ingest_project(&db.pool, "Alert Links").await;
    let channel = AlertService::create_channel(
        &db.pool,
        &Default::default(),
        CreateAlertIntegration {
            name: "Links Webhook".to_string(),
            provider_type: ProviderType::Webhook,
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
    let processors = crate::common::processors_with_config(
        &db.pool,
        &rustrak::config::Config {
            ingest_dir: temp_dir.path().to_path_buf(),
            rate_limit: config.rate_limit,
            dashboard: rustrak::config::DashboardConfig {
                url: Some("https://rustrak.example.com".to_string()),
                ..crate::common::config().dashboard
            },
            ..crate::common::config()
        },
    );

    let metadata = store_scoped_event(temp_dir.path(), project).await;
    processors
        .errors
        .digest(&db.pool, &metadata)
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

/// A replayed event fires every rule that has not recorded it yet. Rules
/// commit one by one, so a crash between two leaves the first recorded and
/// the second not; the replay must not read the first's history as the
/// whole trigger's.
#[actix_web::test]
async fn replayed_event_fires_the_rules_that_had_not_recorded_it() {
    use rustrak::models::{
        AlertRuleChannelInput, AlertType, CreateAlertIntegration, CreateAlertRule, ProviderType,
    };
    use rustrak::services::AlertService;

    let db = TestDb::new().await;
    let (project, _) = create_ingest_project(&db.pool, "Alert Replay").await;
    let channel = AlertService::create_channel(
        &db.pool,
        &Default::default(),
        CreateAlertIntegration {
            name: "Replay Webhook".to_string(),
            provider_type: ProviderType::Webhook,
            credentials: json!({ "url": "https://example.com/webhook" }),
            is_enabled: true,
        },
    )
    .await
    .unwrap();
    // In cooldown, so rows are recorded and nothing is delivered.
    let rule_in_cooldown = |name: &str| CreateAlertRule {
        name: name.to_string(),
        alert_type: AlertType::NewIssue,
        channels: vec![AlertRuleChannelInput {
            integration_id: channel.id,
            routing_override: json!({}),
        }],
        conditions: json!({}),
        cooldown_minutes: 60,
    };
    let cool = |id: i32| {
        sqlx::query("UPDATE alert_rules SET last_triggered_at = $1 WHERE id = $2")
            .bind(chrono::Utc::now())
            .bind(id)
            .execute(&db.pool)
    };
    let first = AlertService::create_rule(&db.pool, project, rule_in_cooldown("First"))
        .await
        .unwrap();
    cool(first.id).await.unwrap();

    let temp_dir = TempDir::new().unwrap();
    let processors = crate::common::processors_with_config(
        &db.pool,
        &rustrak::config::Config {
            ingest_dir: temp_dir.path().to_path_buf(),
            rate_limit: create_test_config().rate_limit,
            ..crate::common::config()
        },
    );
    let metadata = store_scoped_event(temp_dir.path(), project).await;
    processors.errors.digest(&db.pool, &metadata).await.unwrap();

    // The second rule is the one the crash cut off.
    let second = AlertService::create_rule(&db.pool, project, rule_in_cooldown("Second"))
        .await
        .unwrap();
    cool(second.id).await.unwrap();
    let event_data = serde_json::json!({
        "event_id": metadata.event_id,
        "timestamp": 1704801600.0_f64,
        "platform": "rust",
        "level": "error",
        "exception": {"values": [{"type": "Owned", "value": "by a task"}]}
    });
    // The replay reads its own copy: the first run's file is deleted from a
    // queue, which could take a copy written to the same place with it.
    let replay_dir = TempDir::new().unwrap();
    let replay = crate::common::processors_with_config(
        &db.pool,
        &rustrak::config::Config {
            ingest_dir: replay_dir.path().to_path_buf(),
            rate_limit: create_test_config().rate_limit,
            ..crate::common::config()
        },
    );
    store_event_with_metadata(
        replay_dir.path(),
        &metadata.event_id,
        &serde_json::to_vec(&event_data).unwrap(),
        &metadata,
    )
    .await
    .unwrap();
    replay.errors.digest(&db.pool, &metadata).await.unwrap();

    let rows: Vec<i32> = sqlx::query_scalar(
        "SELECT alert_rule_id FROM alert_history WHERE project_id = $1 ORDER BY alert_rule_id",
    )
    .bind(project)
    .fetch_all(&db.pool)
    .await
    .unwrap();
    assert_eq!(rows, vec![first.id, second.id]);
}
