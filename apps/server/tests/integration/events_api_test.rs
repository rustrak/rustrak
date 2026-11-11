//! The events API: listing an issue's events and reading one.

use crate::common::TestDb;
use crate::common::{create_test_project, create_test_token};
use actix_web::test;
use chrono::{DateTime, Utc};
use rustrak::models::Grouping;
use rustrak::services::grouping::DenormalizedFields;
use rustrak::services::{EventService, IssueService};
use serde_json::{json, Value};
use uuid::Uuid;

fn create_denormalized_fields(
    calc_type: &str,
    calc_value: &str,
    transaction: &str,
) -> DenormalizedFields {
    DenormalizedFields {
        calculated_type: calc_type.to_string(),
        calculated_value: calc_value.to_string(),
        transaction: transaction.to_string(),
        last_frame_filename: "test.rs".to_string(),
        last_frame_module: "test_module".to_string(),
        last_frame_function: "test_function".to_string(),
        culprit: "test_function".to_string(),
        logger: String::new(),
        release: String::new(),
    }
}

async fn create_test_issue(
    pool: &rustrak::db::DbPool,
    project_id: i32,
    calc_type: &str,
    calc_value: &str,
) -> rustrak::models::Issue {
    let denormalized = create_denormalized_fields(calc_type, calc_value, "/api/test");
    IssueService::create(
        pool,
        project_id,
        Utc::now(),
        &denormalized,
        Some("error"),
        Some("rust"),
    )
    .await
    .expect("Failed to create test issue")
}

async fn create_test_grouping(
    pool: &rustrak::db::DbPool,
    project_id: i32,
    issue_id: Uuid,
) -> Grouping {
    let grouping_key = format!("test_grouping_key_{}", Uuid::new_v4());
    let grouping_key_hash = format!("{:064x}", 0); // Simple hash for testing

    sqlx::query_as::<_, Grouping>(
        r#"
        INSERT INTO groupings (project_id, issue_id, grouping_key, grouping_key_hash)
        VALUES ($1, $2, $3, $4)
        RETURNING *
        "#,
    )
    .bind(project_id)
    .bind(issue_id)
    .bind(&grouping_key)
    .bind(&grouping_key_hash)
    .fetch_one(pool)
    .await
    .expect("Failed to create test grouping")
}

/// Creates a test event with an explicit `timestamp`, overriding whatever
/// `timestamp` field `event_data` carries (see `EventService::create`, which
/// prefers the payload's embedded `timestamp` over `ingested_at`). Events now
/// order within an issue by `(timestamp, id)`, not a `digest_order` counter,
/// so callers that care about ordering must give each event a distinct
/// timestamp.
async fn create_test_event(
    pool: &rustrak::db::DbPool,
    project_id: i32,
    issue_id: Uuid,
    grouping_id: i32,
    event_data: &Value,
    timestamp: DateTime<Utc>,
) -> rustrak::models::Event {
    let event_id = Uuid::new_v4();
    let denormalized = create_denormalized_fields("TypeError", "Test error", "/api/test");

    let mut data = event_data.clone();
    data["timestamp"] = json!(timestamp.timestamp() as f64);

    let id = EventService::create(
        pool,
        event_id,
        project_id,
        issue_id,
        grouping_id,
        &data,
        timestamp,
        &denormalized,
        None,
        None,
        None,
    )
    .await
    .expect("Failed to create test event");
    EventService::get_by_id(pool, id)
        .await
        .expect("Failed to load test event")
}

fn create_event_data() -> Value {
    json!({
        "event_id": Uuid::new_v4().to_string().replace("-", ""),
        "timestamp": Utc::now().timestamp() as f64,
        "platform": "rust",
        "level": "error",
        "transaction": "/api/test",
        "exception": {
            "values": [{
                "type": "TypeError",
                "value": "Test error message",
                "stacktrace": {
                    "frames": [{
                        "filename": "test.rs",
                        "function": "test_function",
                        "lineno": 42,
                        "in_app": true
                    }]
                }
            }]
        },
        "sdk": {
            "name": "sentry.rust",
            "version": "0.35.0"
        }
    })
}

#[actix_web::test]
async fn test_list_events_empty() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Empty Events Project").await;
    let issue = create_test_issue(&db.pool, project.id, "TypeError", "Error").await;
    let config = crate::common::config();

    let app = crate::common::app_with_config(&db.pool, config).await;

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{}/issues/{}/events",
            project.id, issue.id
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body: Value = test::read_body_json(resp).await;
    assert!(body["items"].as_array().unwrap().is_empty());
    assert_eq!(body["has_more"], false);
}

#[actix_web::test]
async fn test_list_events_with_data() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Events Project").await;
    let issue = create_test_issue(&db.pool, project.id, "TypeError", "Error").await;
    let grouping = create_test_grouping(&db.pool, project.id, issue.id).await;
    let config = crate::common::config();

    let event_data = create_event_data();
    create_test_event(
        &db.pool,
        project.id,
        issue.id,
        grouping.id,
        &event_data,
        Utc::now(),
    )
    .await;
    create_test_event(
        &db.pool,
        project.id,
        issue.id,
        grouping.id,
        &event_data,
        Utc::now(),
    )
    .await;

    let app = crate::common::app_with_config(&db.pool, config).await;

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{}/issues/{}/events",
            project.id, issue.id
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body: Value = test::read_body_json(resp).await;
    let events = body["items"].as_array().unwrap();
    assert_eq!(events.len(), 2);
}

#[actix_web::test]
async fn test_list_events_issue_not_found() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Issue Not Found Project").await;
    let config = crate::common::config();

    let app = crate::common::app_with_config(&db.pool, config).await;

    let fake_uuid = Uuid::new_v4();
    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{}/issues/{}/events",
            project.id, fake_uuid
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn test_list_events_wrong_project() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project1 = create_test_project(&db.pool, "Events Project 1").await;
    let project2 = create_test_project(&db.pool, "Events Project 2").await;
    let issue = create_test_issue(&db.pool, project1.id, "TypeError", "Error").await;
    let config = crate::common::config();

    let app = crate::common::app_with_config(&db.pool, config).await;

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{}/issues/{}/events",
            project2.id, issue.id
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn test_list_events_order_desc() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Events Order Project").await;
    let issue = create_test_issue(&db.pool, project.id, "TypeError", "Error").await;
    let grouping = create_test_grouping(&db.pool, project.id, issue.id).await;
    let config = crate::common::config();

    let event_data = create_event_data();
    let event1 = create_test_event(
        &db.pool,
        project.id,
        issue.id,
        grouping.id,
        &event_data,
        Utc::now() - chrono::Duration::seconds(2),
    )
    .await;
    let event2 = create_test_event(
        &db.pool,
        project.id,
        issue.id,
        grouping.id,
        &event_data,
        Utc::now() - chrono::Duration::seconds(1),
    )
    .await;
    let event3 = create_test_event(
        &db.pool,
        project.id,
        issue.id,
        grouping.id,
        &event_data,
        Utc::now(),
    )
    .await;

    let app = crate::common::app_with_config(&db.pool, config).await;

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{}/issues/{}/events?order=desc",
            project.id, issue.id
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body: Value = test::read_body_json(resp).await;
    let events = body["items"].as_array().unwrap();
    assert_eq!(events.len(), 3);

    let event_ids: Vec<String> = events
        .iter()
        .map(|e| e["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        event_ids,
        vec![
            event3.id.to_string(),
            event2.id.to_string(),
            event1.id.to_string()
        ]
    );
}

#[actix_web::test]
async fn test_list_events_order_asc() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Events ASC Project").await;
    let issue = create_test_issue(&db.pool, project.id, "TypeError", "Error").await;
    let grouping = create_test_grouping(&db.pool, project.id, issue.id).await;
    let config = crate::common::config();

    let event_data = create_event_data();
    let event1 = create_test_event(
        &db.pool,
        project.id,
        issue.id,
        grouping.id,
        &event_data,
        Utc::now() - chrono::Duration::seconds(2),
    )
    .await;
    let event2 = create_test_event(
        &db.pool,
        project.id,
        issue.id,
        grouping.id,
        &event_data,
        Utc::now() - chrono::Duration::seconds(1),
    )
    .await;
    let event3 = create_test_event(
        &db.pool,
        project.id,
        issue.id,
        grouping.id,
        &event_data,
        Utc::now(),
    )
    .await;

    let app = crate::common::app_with_config(&db.pool, config).await;

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{}/issues/{}/events?order=asc",
            project.id, issue.id
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body: Value = test::read_body_json(resp).await;
    let events = body["items"].as_array().unwrap();
    assert_eq!(events.len(), 3);

    let event_ids: Vec<String> = events
        .iter()
        .map(|e| e["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        event_ids,
        vec![
            event1.id.to_string(),
            event2.id.to_string(),
            event3.id.to_string()
        ]
    );
}

#[actix_web::test]
async fn test_get_event_not_found() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Event Not Found Project").await;
    let issue = create_test_issue(&db.pool, project.id, "TypeError", "Error").await;
    let config = crate::common::config();

    let app = crate::common::app_with_config(&db.pool, config).await;

    let fake_uuid = Uuid::new_v4();
    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{}/issues/{}/events/{}",
            project.id, issue.id, fake_uuid
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn test_get_event_wrong_issue() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Event Wrong Issue Project").await;
    let issue1 = create_test_issue(&db.pool, project.id, "TypeError", "Error 1").await;
    let issue2 = create_test_issue(&db.pool, project.id, "ValueError", "Error 2").await;
    let grouping = create_test_grouping(&db.pool, project.id, issue1.id).await;
    let config = crate::common::config();

    let event_data = create_event_data();
    let event = create_test_event(
        &db.pool,
        project.id,
        issue1.id,
        grouping.id,
        &event_data,
        Utc::now(),
    )
    .await;

    let app = crate::common::app_with_config(&db.pool, config).await;

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{}/issues/{}/events/{}",
            project.id, issue2.id, event.id
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn test_get_event_includes_full_data() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Event Full Data Project").await;
    let issue = create_test_issue(&db.pool, project.id, "TypeError", "Error").await;
    let grouping = create_test_grouping(&db.pool, project.id, issue.id).await;
    let config = crate::common::config();

    let event_data = json!({
        "event_id": Uuid::new_v4().to_string().replace("-", ""),
        "timestamp": Utc::now().timestamp() as f64,
        "platform": "rust",
        "level": "error",
        "message": "This is a detailed error message",
        "exception": {
            "values": [{
                "type": "CustomError",
                "value": "Something went wrong",
                "stacktrace": {
                    "frames": [
                        {"filename": "main.rs", "function": "main", "lineno": 10},
                        {"filename": "lib.rs", "function": "process", "lineno": 50}
                    ]
                }
            }]
        },
        "breadcrumbs": {
            "values": [
                {"timestamp": 1234567890.0, "message": "User clicked button"}
            ]
        },
        "tags": {
            "environment": "production",
            "version": "1.0.0"
        }
    });

    let event = create_test_event(
        &db.pool,
        project.id,
        issue.id,
        grouping.id,
        &event_data,
        Utc::now(),
    )
    .await;

    let app = crate::common::app_with_config(&db.pool, config).await;

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{}/issues/{}/events/{}",
            project.id, issue.id, event.id
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body: Value = test::read_body_json(resp).await;

    assert_eq!(body["id"], event.id.to_string());
    assert!(body["event_id"].is_string());
    assert!(body["timestamp"].is_string());
    let data = &body["data"];
    assert!(data["exception"].is_object());
    assert!(data["breadcrumbs"].is_object());
    assert!(data["tags"].is_object());
}

#[actix_web::test]
async fn test_list_events_pagination() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Events Pagination Project").await;
    let issue = create_test_issue(&db.pool, project.id, "TypeError", "Error").await;
    let grouping = create_test_grouping(&db.pool, project.id, issue.id).await;
    let config = crate::common::config();

    let event_data = create_event_data();
    for i in 1..=25 {
        create_test_event(
            &db.pool,
            project.id,
            issue.id,
            grouping.id,
            &event_data,
            Utc::now() + chrono::Duration::seconds(i as i64),
        )
        .await;
    }

    let app = crate::common::app_with_config(&db.pool, config).await;

    let page = |cursor: Option<&str>| {
        let mut uri = format!("/api/projects/{}/issues/{}/events", project.id, issue.id);
        if let Some(cursor) = cursor {
            uri = format!("{uri}?cursor={cursor}");
        }
        test::TestRequest::get()
            .uri(&uri)
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .to_request()
    };

    let first: Value = test::call_and_read_body_json(&app, page(None)).await;
    assert_eq!(first["items"].as_array().unwrap().len(), 20);
    assert_eq!(first["has_more"], true);
    let cursor = first["next_cursor"].as_str().unwrap();

    let second: Value = test::call_and_read_body_json(&app, page(Some(cursor))).await;
    assert_eq!(second["items"].as_array().unwrap().len(), 5);
    assert_eq!(second["has_more"], false);
    assert!(second["next_cursor"].is_null());

    let mut ids: Vec<&str> = first["items"]
        .as_array()
        .unwrap()
        .iter()
        .chain(second["items"].as_array().unwrap())
        .map(|e| e["id"].as_str().unwrap())
        .collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 25);
}

#[actix_web::test]
async fn test_list_events_invalid_cursor() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Invalid Cursor Events Project").await;
    let issue = create_test_issue(&db.pool, project.id, "TypeError", "Error").await;
    let config = crate::common::config();

    let app = crate::common::app_with_config(&db.pool, config).await;

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{}/issues/{}/events?cursor=invalid_cursor",
            project.id, issue.id
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn test_event_list_response_format() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Event List Format Project").await;
    let issue = create_test_issue(&db.pool, project.id, "TypeError", "Error").await;
    let grouping = create_test_grouping(&db.pool, project.id, issue.id).await;
    let config = crate::common::config();

    let event_data = create_event_data();
    create_test_event(
        &db.pool,
        project.id,
        issue.id,
        grouping.id,
        &event_data,
        Utc::now(),
    )
    .await;

    let app = crate::common::app_with_config(&db.pool, config).await;

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{}/issues/{}/events",
            project.id, issue.id
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body: Value = test::read_body_json(resp).await;
    let event = &body["items"][0];

    assert!(event["id"].is_string());
    assert!(event["event_id"].is_string());
    assert!(event["timestamp"].is_string());
    assert!(event.get("level").is_some());
    assert!(event.get("platform").is_some());
}
