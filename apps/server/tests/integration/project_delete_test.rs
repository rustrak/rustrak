//! Deleting a project (#556). The request only marks it, as Sentry's does
//! (`PENDING_DELETION`): it is gone for every reader at once, and its rows
//! are removed afterwards, in batches, by a background run that a restart
//! picks up again.

use crate::common::{create_test_project, create_test_token, eventually, TestDb};
use actix_web::test;
use chrono::Utc;
use rustrak::db::DbPool;
use rustrak::digest::processors::{LogsProcessor, ProcessorCtx, TransactionProcessor};
use rustrak::services::{ProjectDeletions, ProjectService};
use serde_json::{json, Value};
use uuid::Uuid;

/// A transaction with two spans, two log lines and one issue with an event.
async fn fill(pool: &DbPool, project_id: i32) {
    let ctx = ProcessorCtx {
        pool: pool.clone(),
        project_id,
        event_id: Uuid::new_v4(),
        ingested_at: Utc::now(),
        remote_addr: None,
        keep_ai_spans: true,
    };
    let now = Utc::now().timestamp() as f64;
    let span = |span_id: &str| {
        json!({
            "span_id": span_id, "parent_span_id": "aaaaaaaaaaaaaaaa", "op": "db.query",
            "start_timestamp": now, "timestamp": now + 0.1,
        })
    };
    let transaction = json!({
        "type": "transaction", "transaction": "GET /items",
        "start_timestamp": now, "timestamp": now + 1.0,
        "contexts": { "trace": {
            "trace_id": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "span_id": "aaaaaaaaaaaaaaaa", "op": "http.server",
        }},
        "spans": [span("bbbbbbbbbbbbbbbb"), span("cccccccccccccccc")],
    });
    TransactionProcessor
        .process(serde_json::to_vec(&transaction).unwrap().into(), &ctx)
        .await
        .unwrap();
    let logs = json!({ "items": [
        { "timestamp": now, "level": "info", "body": "one" },
        { "timestamp": now, "level": "error", "body": "two" },
    ]});
    LogsProcessor
        .process(serde_json::to_vec(&logs).unwrap().into(), &ctx)
        .await
        .unwrap();
    // One error event, digested into an issue.
    let dir = tempfile::tempdir().unwrap();
    let event_id = Uuid::new_v4().simple().to_string();
    let event = json!({
        "event_id": event_id, "level": "error", "platform": "other", "timestamp": now,
        "exception": { "values": [{ "type": "Error", "value": "boom" }] },
    });
    crate::common::store_event(
        dir.path(),
        project_id,
        &event_id,
        &serde_json::to_vec(&event).unwrap(),
    )
    .await
    .unwrap();
    crate::common::process_error_event(
        pool,
        &rustrak::ingest::EventMetadata {
            event_id,
            project_id,
            ingested_at: Utc::now(),
            remote_addr: None,
        },
        dir.path(),
        &crate::common::rate_limit(),
        crate::common::null_sourcemap_provider(),
    )
    .await
    .unwrap();
}

/// Rows of the project left in the tables that hold its data, and its own.
async fn rows(pool: &DbPool, project_id: i32) -> Vec<(&'static str, i64)> {
    let mut left = Vec::new();
    for table in ["transactions", "spans", "logs", "events", "issues"] {
        let n: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT COUNT(*) FROM {table} WHERE project_id = $1"
        )))
        .bind(project_id)
        .fetch_one(pool)
        .await
        .unwrap();
        left.push((table, n));
    }
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM projects WHERE id = $1")
        .bind(project_id)
        .fetch_one(pool)
        .await
        .unwrap();
    left.push(("projects", n));
    left
}

const GONE: [(&str, i64); 6] = [
    ("transactions", 0),
    ("spans", 0),
    ("logs", 0),
    ("events", 0),
    ("issues", 0),
    ("projects", 0),
];
const KEPT: [(&str, i64); 6] = [
    ("transactions", 1),
    ("spans", 2),
    ("logs", 2),
    ("events", 1),
    ("issues", 1),
    ("projects", 1),
];

fn request(method: test::TestRequest, token: &str, uri: &str) -> actix_http::Request {
    method
        .uri(uri)
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request()
}

#[actix_web::test]
async fn a_deleted_project_is_gone_at_once_and_its_rows_follow() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let doomed = create_test_project(&db.pool, "Doomed").await;
    let other = create_test_project(&db.pool, "Other").await;
    fill(&db.pool, doomed.id).await;
    fill(&db.pool, other.id).await;
    let app = crate::common::app(&db.pool).await;
    let project = format!("/api/projects/{}", doomed.id);
    // The storage page has been open: its figures are kept for a minute.
    let before: Value = test::call_and_read_body_json(
        &app,
        request(test::TestRequest::get(), &token, "/api/storage/projects"),
    )
    .await;
    assert_eq!(before.as_array().unwrap().len(), 2);

    let resp =
        test::call_service(&app, request(test::TestRequest::delete(), &token, &project)).await;
    assert_eq!(resp.status(), 204);

    // Gone for every reader, before any of its rows are.
    let resp = test::call_service(&app, request(test::TestRequest::get(), &token, &project)).await;
    assert_eq!(resp.status(), 404, "the project itself");
    let list: Value = test::call_and_read_body_json(
        &app,
        request(test::TestRequest::get(), &token, "/api/projects"),
    )
    .await;
    let listed: Vec<i64> = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_i64().unwrap())
        .collect();
    assert_eq!(listed, [i64::from(other.id)], "the project list");
    let storage: Value = test::call_and_read_body_json(
        &app,
        request(test::TestRequest::get(), &token, "/api/storage/projects"),
    )
    .await;
    let stored: Vec<i64> = storage
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["project_id"].as_i64().unwrap())
        .collect();
    assert_eq!(stored, [i64::from(other.id)], "the storage page");

    let age: Value = test::call_and_read_body_json(
        &app,
        request(test::TestRequest::get(), &token, "/api/storage/age"),
    )
    .await;
    let aged: Vec<i64> = age["projects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["project_id"].as_i64().unwrap())
        .collect();
    assert_eq!(aged, [i64::from(other.id)], "the storage page's ages");
    // The list of issues across projects, which an owner reads unscoped.
    let issues: Value = test::call_and_read_body_json(
        &app,
        request(test::TestRequest::get(), &token, "/api/issues"),
    )
    .await;
    let of: Vec<i64> = issues["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|issue| issue["project_id"].as_i64().unwrap())
        .collect();
    assert_eq!(of, [i64::from(other.id)], "issues across projects");

    // Its DSN stops taking events.
    let envelope = "{}\n{\"type\":\"event\",\"length\":2}\n{}\n";
    let ingest = test::TestRequest::post()
        .uri(&format!(
            "/api/{}/envelope/?sentry_key={}",
            doomed.id, doomed.sentry_key
        ))
        .insert_header(("Content-Type", "application/x-sentry-envelope"))
        .set_payload(envelope)
        .to_request();
    assert_eq!(
        test::call_service(&app, ingest).await.status(),
        404,
        "ingest"
    );

    // A second delete finds nothing, as Sentry's does.
    let resp =
        test::call_service(&app, request(test::TestRequest::delete(), &token, &project)).await;
    assert_eq!(resp.status(), 404, "a second delete");

    // Its name and slug are free at once.
    let create = test::TestRequest::post()
        .uri("/api/projects")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(json!({ "name": "Doomed", "slug": doomed.slug }))
        .to_request();
    let resp = test::call_service(&app, create).await;
    assert_eq!(resp.status(), 201, "a project of the same name and slug");

    // The rows follow, and nobody else's go with them.
    eventually(|| async { (rows(&db.pool, doomed.id).await == GONE).then_some(()) }).await;
    assert_eq!(rows(&db.pool, other.id).await, KEPT);
}

#[actix_web::test]
async fn a_deletion_cut_short_is_finished_by_the_next_run() {
    // What a restart finds: the project marked, its rows still there.
    let db = TestDb::new().await;
    let doomed = create_test_project(&db.pool, "Interrupted").await;
    let other = create_test_project(&db.pool, "Bystander").await;
    fill(&db.pool, doomed.id).await;
    fill(&db.pool, other.id).await;
    ProjectService::delete(&db.pool, doomed.id).await.unwrap();
    assert_eq!(rows(&db.pool, doomed.id).await, KEPT, "only marked");

    let removed = ProjectDeletions::default().drain(&db.pool).await.unwrap();

    assert_eq!(removed, 1);
    assert_eq!(rows(&db.pool, doomed.id).await, GONE);
    assert_eq!(rows(&db.pool, other.id).await, KEPT);
    // Nothing left to do.
    assert_eq!(
        ProjectDeletions::default().drain(&db.pool).await.unwrap(),
        0
    );
}

#[actix_web::test]
async fn deleting_a_project_twice_is_not_found() {
    let db = TestDb::new().await;
    let doomed = create_test_project(&db.pool, "Twice").await;
    ProjectService::delete(&db.pool, doomed.id).await.unwrap();
    let again = ProjectService::delete(&db.pool, doomed.id).await;
    assert!(
        matches!(again, Err(rustrak::error::AppError::NotFound(_))),
        "{again:?}"
    );
    assert!(ProjectService::get_by_id(&db.pool, doomed.id)
        .await
        .is_err());
}

#[actix_web::test]
async fn release_health_rows_go_in_the_batches_too() {
    // Sessions are counted per minute and per user and day, so they grow
    // with traffic like the rest: they are not left to the project row's
    // cascade, which is one statement however many there are.
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Sessions").await;
    let other = create_test_project(&db.pool, "Other sessions").await;
    for id in [project.id, other.id] {
        for minute in 0..3 {
            sqlx::query(
                "INSERT INTO session_counts (project_id, release, environment, bucket, total) \
                 VALUES ($1, 'app@1', 'production', $2, 1)",
            )
            .bind(id)
            .bind(Utc::now() - chrono::Duration::minutes(minute))
            .execute(&db.pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO session_users (project_id, release, environment, day, did) \
                 VALUES ($1, 'app@1', 'production', $2, $3)",
            )
            .bind(id)
            .bind(Utc::now().date_naive() - chrono::Duration::days(minute))
            .bind(format!("user-{minute}"))
            .execute(&db.pool)
            .await
            .unwrap();
        }
    }
    let sessions = |id: i32| {
        let pool = db.pool.clone();
        async move {
            let mut left = Vec::new();
            for table in ["session_counts", "session_users"] {
                let n: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
                    "SELECT COUNT(*) FROM {table} WHERE project_id = $1"
                )))
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
                left.push(n);
            }
            left
        }
    };

    rustrak::services::StorageService::purge_project(&db.pool, project.id)
        .await
        .unwrap();

    // Gone while the project row, and so its cascade, is still to come.
    assert_eq!(rows(&db.pool, project.id).await[5], ("projects", 1));
    assert_eq!(sessions(project.id).await, [0, 0]);
    assert_eq!(sessions(other.id).await, [3, 3]);
}
