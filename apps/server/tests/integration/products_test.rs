//! A project's products: which kinds of data it keeps.
//!
//! A disabled product is not hidden after the fact, it is never stored. Its
//! items are dropped at ingest with a 200, as Relay drops filtered data, so the
//! SDK does not retry them, and their siblings in the same envelope are kept.

use crate::common::TestDb;
use actix_web::test;
use rustrak::config::Config;
use rustrak::error::AppError;
use rustrak::models::{CreateProject, Product, UpdateProject};
use rustrak::services::ProjectService;
use serde_json::json;
use tempfile::TempDir;
use uuid::Uuid;

fn test_config(ingest_dir: &TempDir) -> Config {
    Config {
        sourcemap_storage_path: "/tmp/test_sourcemaps_products".to_string(),
        ..crate::common::config_with_ingest_dir(ingest_dir.path())
    }
}

/// A project with `disabled` turned off, and its sentry key.
async fn project_without(db: &TestDb, name: &str, disabled: Vec<Product>) -> (i32, String) {
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: name.to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();
    ProjectService::update(
        &db.pool,
        project.id,
        UpdateProject {
            disabled_products: Some(disabled),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    (project.id, project.sentry_key.to_string())
}

/// Posts an envelope of `(header, payload)` items and asserts the 200. The
/// ingest directory comes back: dropping it would delete the stored event
/// before the digest reads it.
#[must_use]
async fn send(
    db: &TestDb,
    project_id: i32,
    key: &str,
    items: &[(serde_json::Value, String)],
) -> TempDir {
    let dir = TempDir::new().unwrap();
    let config = test_config(&dir);
    let app = crate::common::app_with_config(&db.pool, config.clone()).await;

    let event_id = Uuid::new_v4().simple().to_string();
    let mut envelope = format!("{}\n", json!({ "event_id": event_id }));
    for (header, payload) in items {
        envelope.push_str(&format!("{header}\n{payload}\n"));
    }

    let req = test::TestRequest::post()
        .uri(&format!("/api/{project_id}/envelope/"))
        .insert_header(("X-Sentry-Auth", format!("Sentry sentry_key={key}")))
        .insert_header(("Content-Type", "application/x-sentry-envelope"))
        .set_payload(envelope)
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        200,
        "a disabled product is dropped with a 200, never a retryable error"
    );
    dir
}

async fn count(db: &TestDb, table: &str, project_id: i32) -> i64 {
    let sql = format!("SELECT COUNT(*) FROM {table} WHERE project_id = $1");
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(project_id)
        .fetch_one(&db.pool)
        .await
        .unwrap()
}

/// Waits for the digest to store `n` error events.
async fn wait_for_events(db: &TestDb, project_id: i32, n: i64) {
    crate::common::eventually(|| async {
        (count(db, "events", project_id).await >= n).then_some(())
    })
    .await;
}

fn error_item() -> (serde_json::Value, String) {
    let event = json!({
        "event_id": Uuid::new_v4().simple().to_string(),
        "timestamp": 1704801600.0_f64,
        "platform": "python",
        "level": "error",
        "exception": { "values": [{ "type": "ValueError", "value": "kept" }] }
    });
    (json!({ "type": "event" }), event.to_string())
}

fn log_item() -> (serde_json::Value, String) {
    let logs = json!({ "items": [{
        "timestamp": 1704801600.0_f64,
        "level": "info",
        "body": "a log line",
        "trace_id": "5b8efff798038103d269b633813fc60c"
    }]});
    (
        json!({
            "type": "log",
            "item_count": 1,
            "content_type": "application/vnd.sentry.items.log+json"
        }),
        logs.to_string(),
    )
}

fn transaction_item(spans: serde_json::Value) -> (serde_json::Value, String) {
    let txn = json!({
        "event_id": Uuid::new_v4().simple().to_string(),
        "type": "transaction",
        "transaction": "/checkout",
        "start_timestamp": 1704801590.0_f64,
        "timestamp": 1704801600.0_f64,
        "contexts": { "trace": {
            "trace_id": "5b8efff798038103d269b633813fc60c",
            "span_id": "b0e6f15b45c36b12",
            "op": "http.server"
        }},
        "spans": spans,
    });
    (json!({ "type": "transaction" }), txn.to_string())
}

fn span(span_id: &str, op: &str) -> serde_json::Value {
    json!({
        "span_id": span_id,
        "parent_span_id": "b0e6f15b45c36b12",
        "trace_id": "5b8efff798038103d269b633813fc60c",
        "op": op,
        "description": op,
        "data": if op.starts_with("gen_ai.") {
            json!({ "gen_ai.operation.name": "chat" })
        } else {
            json!({})
        },
        "start_timestamp": 1704801591.0_f64,
        "timestamp": 1704801592.0_f64
    })
}

#[actix_web::test]
async fn a_new_project_has_every_product_on() {
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "Defaults".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    assert!(project.disabled_products.is_empty());
    for product in [
        Product::Issues,
        Product::Feedback,
        Product::Releases,
        Product::Performance,
        Product::Agents,
        Product::Logs,
        Product::Crons,
    ] {
        assert!(project.keeps(product), "{product:?} is on by default");
    }
}

#[actix_web::test]
async fn issues_cannot_be_turned_off() {
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "Always errors".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    let result = ProjectService::update(
        &db.pool,
        project.id,
        UpdateProject {
            disabled_products: Some(vec![Product::Issues]),
            ..Default::default()
        },
    )
    .await;

    assert!(matches!(result, Err(AppError::Validation(_))));
}

#[actix_web::test]
async fn the_sidebar_order_is_kept_as_sent() {
    let db = TestDb::new().await;
    let (project_id, _) = project_without(&db, "Ordered", vec![]).await;
    let order = vec![Product::Logs, Product::Issues, Product::Crons];

    let project = ProjectService::update(
        &db.pool,
        project_id,
        UpdateProject {
            product_order: Some(order.clone()),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    assert_eq!(project.product_order.0, order);
    // An update that does not mention the products leaves them alone.
    let renamed = ProjectService::update(
        &db.pool,
        project_id,
        UpdateProject {
            name: Some("Still ordered".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(renamed.product_order.0, order);
}

#[actix_web::test]
async fn logs_off_drops_the_log_and_keeps_the_error() {
    let db = TestDb::new().await;
    let (project_id, key) = project_without(&db, "No logs", vec![Product::Logs]).await;

    let _dir = send(&db, project_id, &key, &[log_item(), error_item()]).await;

    wait_for_events(&db, project_id, 1).await;
    assert_eq!(count(&db, "logs", project_id).await, 0);
}

#[actix_web::test]
async fn logs_on_stores_the_log() {
    let db = TestDb::new().await;
    let (project_id, key) = project_without(&db, "With logs", vec![]).await;

    let _dir = send(&db, project_id, &key, &[log_item()]).await;

    assert_eq!(count(&db, "logs", project_id).await, 1);
}

#[actix_web::test]
async fn performance_off_drops_transactions_and_their_spans() {
    let db = TestDb::new().await;
    let (project_id, key) = project_without(&db, "No perf", vec![Product::Performance]).await;

    let _dir = send(
        &db,
        project_id,
        &key,
        &[
            transaction_item(json!([span("a1b2c3d4e5f60718", "db")])),
            error_item(),
        ],
    )
    .await;

    wait_for_events(&db, project_id, 1).await;
    assert_eq!(count(&db, "transactions", project_id).await, 0);
    assert_eq!(count(&db, "spans", project_id).await, 0);
}

#[actix_web::test]
async fn agents_off_drops_only_the_ai_spans() {
    let db = TestDb::new().await;
    let (project_id, key) = project_without(&db, "No agents", vec![Product::Agents]).await;

    let _dir = send(
        &db,
        project_id,
        &key,
        &[transaction_item(json!([
            span("a1b2c3d4e5f60718", "db"),
            span("b1b2c3d4e5f60718", "gen_ai.chat"),
        ]))],
    )
    .await;

    assert_eq!(count(&db, "transactions", project_id).await, 1);
    let ops: Vec<String> = sqlx::query_scalar("SELECT op FROM spans WHERE project_id = $1")
        .bind(project_id)
        .fetch_all(&db.pool)
        .await
        .unwrap();
    assert_eq!(ops, vec!["db".to_string()]);
}

#[actix_web::test]
async fn agents_off_keeps_ai_data_out_of_the_stored_transaction() {
    let db = TestDb::new().await;
    let (project_id, key) = project_without(&db, "No agents", vec![Product::Agents]).await;
    let (header, payload) = transaction_item(json!([
        span("a1b2c3d4e5f60718", "db"),
        span("b1b2c3d4e5f60718", "gen_ai.chat"),
    ]));
    // An AI trace root: some SDKs send it inline on contexts.trace.
    let mut txn: serde_json::Value = serde_json::from_str(&payload).unwrap();
    txn["contexts"]["trace"]["op"] = json!("gen_ai.invoke_agent");
    txn["contexts"]["trace"]["data"] = json!({ "gen_ai.operation.name": "invoke_agent" });

    let _dir = send(&db, project_id, &key, &[(header, txn.to_string())]).await;

    let data: sqlx::types::Json<serde_json::Value> =
        sqlx::query_scalar("SELECT data FROM transactions WHERE project_id = $1")
            .bind(project_id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let ops: Vec<&str> = data.0["spans"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| s["op"].as_str())
        .collect();
    assert_eq!(ops, vec!["db"], "the stored payload is not a second copy");
    assert!(data.0["contexts"]["trace"].get("data").is_none());
}

#[actix_web::test]
async fn feedback_off_drops_a_feedback_sent_as_an_event() {
    let db = TestDb::new().await;
    let (project_id, key) = project_without(&db, "No feedback", vec![Product::Feedback]).await;
    let feedback = json!({
        "type": "feedback",
        "event_id": Uuid::new_v4().simple().to_string(),
        "timestamp": 1704801600.0_f64,
        "platform": "javascript",
        "contexts": { "feedback": { "message": "the button does nothing" } }
    });

    // An envelope carries one event item, so the feedback goes alone.
    let dir = send(
        &db,
        project_id,
        &key,
        &[(json!({ "type": "event" }), feedback.to_string())],
    )
    .await;

    // The digest removes the event's file whether it stores or drops it, so
    // an empty ingest directory means it has run: only then is "no issue" an
    // answer rather than a digest that has not happened yet.
    crate::common::eventually(|| async { (!crate::common::has_files(dir.path())).then_some(()) })
        .await;
    assert_eq!(count(&db, "issues", project_id).await, 0);
}

#[actix_web::test]
async fn performance_off_turns_agents_off_with_it() {
    let db = TestDb::new().await;
    let (project_id, _) = project_without(&db, "No perf", vec![Product::Performance]).await;
    let project = ProjectService::get_by_id(&db.pool, project_id)
        .await
        .unwrap();

    assert!(!project.keeps(Product::Agents));
}

#[actix_web::test]
async fn feedback_off_drops_the_feedback() {
    let db = TestDb::new().await;
    let (project_id, key) = project_without(&db, "No feedback", vec![Product::Feedback]).await;
    let feedback = json!({
        "type": "feedback",
        "event_id": Uuid::new_v4().simple().to_string(),
        "timestamp": 1704801600.0_f64,
        "platform": "javascript",
        "contexts": { "feedback": { "message": "the button does nothing" } }
    });

    let dir = send(
        &db,
        project_id,
        &key,
        &[(json!({ "type": "feedback" }), feedback.to_string())],
    )
    .await;

    // Nothing is queued for the digest, so it can never store an issue.
    assert!(
        !crate::common::has_files(dir.path()),
        "nothing queued for the digest"
    );
    assert_eq!(count(&db, "issues", project_id).await, 0);
}

#[actix_web::test]
async fn crons_off_accepts_the_http_check_in_without_storing_it() {
    let db = TestDb::new().await;
    let (project_id, key) = project_without(&db, "No crons", vec![Product::Crons]).await;
    let dir = TempDir::new().unwrap();
    let config = test_config(&dir);
    let app = crate::common::app_with_config(&db.pool, config.clone()).await;

    let body = r#"{"status":"in_progress","monitor_config":{"schedule":{"type":"interval","value":2,"unit":"hour"}}}"#;
    let req = test::TestRequest::post()
        .uri(&format!("/api/{project_id}/cron/nightly/{key}/"))
        .insert_header(("Content-Type", "application/json"))
        .set_payload(body)
        .to_request();

    assert_eq!(test::call_service(&app, req).await.status(), 202);
    assert_eq!(count(&db, "monitors", project_id).await, 0);
}
