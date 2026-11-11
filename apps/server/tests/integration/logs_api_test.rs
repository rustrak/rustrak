//! Integration tests for the Logs API
//!
//! Tests GET /api/projects/{id}/logs with a real database.

use crate::common::create_test_token;
use crate::common::TestDb;
use actix_web::test;
use chrono::Utc;
use rustrak::digest::processors::{LogsProcessor, ProcessorCtx};
use rustrak::models::CreateProject;
use rustrak::services::ProjectService;
use serde_json::Value;
use uuid::Uuid;

async fn store_sample_logs(pool: &rustrak::db::DbPool, project_id: i32) {
    let body = br#"{"items":[
        {"timestamp":1704801600.0,"trace_id":"aaaa","level":"error","body":"boom"},
        {"timestamp":1704801601.0,"trace_id":"bbbb","level":"info","body":"ok"}
    ]}"#
    .to_vec();
    let ctx = ProcessorCtx {
        pool: pool.clone(),
        project_id,
        event_id: Uuid::new_v4(),
        ingested_at: Utc::now(),
        remote_addr: None,
        keep_ai_spans: true,
    };
    LogsProcessor
        .process(bytes::Bytes::from(body), &ctx)
        .await
        .unwrap();
}

#[actix_web::test]
async fn test_list_logs_returns_stored_logs() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let config = crate::common::config();
    let token = create_test_token(&pool).await;
    let project = ProjectService::create(
        &pool,
        CreateProject {
            name: "Logs List Test".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    store_sample_logs(&pool, project.id).await;

    let app = crate::common::app_with_config(&pool, config.clone()).await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/logs", project.id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total_count"], 2);
    let items = body["items"].as_array().expect("items is array");
    assert_eq!(items.len(), 2);
    // Newest timestamp first.
    assert_eq!(items[0]["body"], "ok");
    assert_eq!(items[1]["body"], "boom");
}

#[actix_web::test]
async fn test_list_logs_filters_by_level() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let config = crate::common::config();
    let token = create_test_token(&pool).await;
    let project = ProjectService::create(
        &pool,
        CreateProject {
            name: "Logs Filter Test".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    store_sample_logs(&pool, project.id).await;

    let app = crate::common::app_with_config(&pool, config.clone()).await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/logs?level=error", project.id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total_count"], 1);
    let items = body["items"].as_array().expect("items is array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["level"], "error");
}

async fn store_logs(pool: &rustrak::db::DbPool, project_id: i32, items: Value) {
    let ctx = ProcessorCtx {
        pool: pool.clone(),
        project_id,
        event_id: Uuid::new_v4(),
        ingested_at: Utc::now(),
        remote_addr: None,
        keep_ai_spans: true,
    };
    let body = serde_json::to_vec(&serde_json::json!({ "items": items })).unwrap();
    LogsProcessor
        .process(bytes::Bytes::from(body), &ctx)
        .await
        .unwrap();
}

/// A project holding two logs from 2024 and three from the last hour.
async fn recent_and_old_logs() -> (TestDb, i32, String) {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "Logs Window Test".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();
    store_sample_logs(&db.pool, project.id).await;
    let now = Utc::now().timestamp() as f64;
    store_logs(
        &db.pool,
        project.id,
        serde_json::json!([
            {"timestamp": now - 60.0, "level": "error", "body": "Payment FAILED for order 7"},
            {"timestamp": now - 30.0, "level": "warn", "body": "retrying payment"},
            {"timestamp": now - 10.0, "level": "info", "body": "100% done_ok"},
        ]),
    )
    .await;
    (db, project.id, token)
}

async fn get_json(pool: &rustrak::db::DbPool, uri: &str, token: &str) -> Value {
    let app = crate::common::app_with_config(pool, crate::common::config()).await;
    let req = test::TestRequest::get()
        .uri(uri)
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    test::read_body_json(resp).await
}

#[actix_web::test]
async fn test_list_logs_searches_the_message_ignoring_case() {
    let (db, project, token) = recent_and_old_logs().await;

    let body = get_json(
        &db.pool,
        &format!("/api/projects/{project}/logs?query=payment"),
        &token,
    )
    .await;
    assert_eq!(body["total_count"], 2);

    // `%` and `_` are taken literally, not as LIKE wildcards.
    let body = get_json(
        &db.pool,
        &format!("/api/projects/{project}/logs?query=0%25%20done_"),
        &token,
    )
    .await;
    assert_eq!(body["total_count"], 1);
    let body = get_json(
        &db.pool,
        &format!("/api/projects/{project}/logs?query=_"),
        &token,
    )
    .await;
    assert_eq!(body["total_count"], 1);
}

#[actix_web::test]
async fn test_list_logs_keeps_the_window() {
    let (db, project, token) = recent_and_old_logs().await;

    let body = get_json(
        &db.pool,
        &format!("/api/projects/{project}/logs?period=24h"),
        &token,
    )
    .await;
    assert_eq!(body["total_count"], 3);
    assert_eq!(body["items"][0]["body"], "100% done_ok");
}

#[actix_web::test]
async fn test_log_volume_counts_each_level_per_bucket() {
    let (db, project, token) = recent_and_old_logs().await;

    let body = get_json(
        &db.pool,
        &format!("/api/projects/{project}/logs/volume?period=24h"),
        &token,
    )
    .await;
    let points = body.as_array().expect("points is array");
    // Hourly over a day, with no gaps: the empty hours are there as zeros.
    assert!(points.len() >= 24, "{} buckets", points.len());
    let sum = |level: &str| -> i64 { points.iter().map(|p| p[level].as_i64().unwrap()).sum() };
    assert_eq!(sum("error"), 1);
    assert_eq!(sum("warn"), 1);
    assert_eq!(sum("info"), 1);
    assert_eq!(sum("debug"), 0);

    // The same filters as the list.
    let body = get_json(
        &db.pool,
        &format!("/api/projects/{project}/logs/volume?period=24h&query=payment&level=error"),
        &token,
    )
    .await;
    let total: i64 = body
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            ["trace", "debug", "info", "warn", "error", "fatal"]
                .iter()
                .map(|l| p[*l].as_i64().unwrap())
                .sum::<i64>()
        })
        .sum();
    assert_eq!(total, 1);
}

#[actix_web::test]
async fn test_log_volume_without_a_window_starts_at_the_oldest_log() {
    let (db, project, token) = recent_and_old_logs().await;

    let body = get_json(
        &db.pool,
        &format!("/api/projects/{project}/logs/volume"),
        &token,
    )
    .await;
    let points = body.as_array().unwrap();
    // Back to January 2024 in about sixty whole-day buckets, all five logs in them.
    assert!(
        (50..=62).contains(&points.len()),
        "{} buckets",
        points.len()
    );
    let total: i64 = points
        .iter()
        .map(|p| {
            ["trace", "debug", "info", "warn", "error", "fatal"]
                .iter()
                .map(|l| p[*l].as_i64().unwrap())
                .sum::<i64>()
        })
        .sum();
    assert_eq!(total, 5);
}

fn volume_total(body: &Value) -> i64 {
    body.as_array()
        .unwrap()
        .iter()
        .map(|p| {
            ["trace", "debug", "info", "warn", "error", "fatal"]
                .iter()
                .map(|l| p[*l].as_i64().unwrap())
                .sum::<i64>()
        })
        .sum()
}

#[actix_web::test]
async fn test_log_volume_counts_a_log_from_a_clock_ahead() {
    let (db, project, token) = recent_and_old_logs().await;
    let ahead = Utc::now().timestamp() as f64 + 3600.0;
    store_logs(
        &db.pool,
        project,
        serde_json::json!([{"timestamp": ahead, "level": "info", "body": "from tomorrow"}]),
    )
    .await;

    let body = get_json(
        &db.pool,
        &format!("/api/projects/{project}/logs/volume?period=24h"),
        &token,
    )
    .await;
    // The three recent logs and the one ahead, as the list counts them.
    assert_eq!(volume_total(&body), 4);
}

#[actix_web::test]
async fn test_list_logs_finds_an_accented_message() {
    let (db, project, token) = recent_and_old_logs().await;
    store_logs(
        &db.pool,
        project,
        serde_json::json!([{"timestamp": Utc::now().timestamp() as f64, "level": "error", "body": "Échec du paiement"}]),
    )
    .await;

    let body = get_json(
        &db.pool,
        &format!("/api/projects/{project}/logs?query=%C3%89chec"),
        &token,
    )
    .await;
    assert_eq!(body["total_count"], 1);
}

#[actix_web::test]
async fn test_get_log_answers_for_its_project_only() {
    let (db, project, token) = recent_and_old_logs().await;
    let list = get_json(&db.pool, &format!("/api/projects/{project}/logs"), &token).await;
    let id = list["items"][0]["id"].as_str().unwrap().to_string();

    let log = get_json(
        &db.pool,
        &format!("/api/projects/{project}/logs/{id}"),
        &token,
    )
    .await;
    assert_eq!(log["id"], id.as_str());
    assert_eq!(log["body"], list["items"][0]["body"]);

    let other = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "Elsewhere".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();
    let app = crate::common::app_with_config(&db.pool, crate::common::config()).await;
    for uri in [
        format!("/api/projects/{}/logs/{id}", other.id),
        format!("/api/projects/{project}/logs/{}", Uuid::new_v4()),
    ] {
        let req = test::TestRequest::get()
            .uri(&uri)
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .to_request();
        assert_eq!(test::call_service(&app, req).await.status(), 404, "{uri}");
    }
}
