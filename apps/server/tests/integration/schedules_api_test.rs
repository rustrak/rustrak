//! The shared schedule preview: POST /api/schedules/preview.

use crate::common::TestDb;
use actix_web::test;
use rustrak::services::AuthTokenService;
use serde_json::{json, Value};

async fn token(db: &TestDb) -> String {
    AuthTokenService::create(
        &db.pool,
        rustrak::models::CreateAuthToken {
            description: Some("test".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .token
}

async fn preview(body: Value, with_token: bool) -> (u16, Value) {
    let db = TestDb::new().await;
    let token = token(&db).await;
    let app = crate::common::app_with_config(&db.pool, crate::common::config()).await;
    let mut req = test::TestRequest::post()
        .uri("/api/schedules/preview")
        .set_json(body);
    if with_token {
        req = req.insert_header(("Authorization", format!("Bearer {token}")));
    }
    let resp = test::call_service(&app, req.to_request()).await;
    let status = resp.status().as_u16();
    let body: Value = test::read_body_json(resp).await;
    (status, body)
}

#[actix_web::test]
async fn lists_the_next_five_runs_in_the_zone() {
    let (status, body) = preview(
        json!({"schedule": {"type": "crontab", "value": "0 9 * * *"}, "timezone": "Europe/Madrid"}),
        true,
    )
    .await;
    assert_eq!(status, 200);
    let runs = body["next_runs"].as_array().unwrap();
    assert_eq!(runs.len(), 5);
    // 09:00 in Madrid is 07:00Z or 08:00Z depending on DST, never 09:00Z.
    let first = runs[0].as_str().unwrap();
    assert!(
        first.contains("T07:00:00") || first.contains("T08:00:00"),
        "{first}"
    );
}

#[actix_web::test]
async fn the_timezone_defaults_to_utc() {
    let (status, body) = preview(
        json!({"schedule": {"type": "interval", "value": 2, "unit": "hour"}}),
        true,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["next_runs"].as_array().unwrap().len(), 5);
}

#[actix_web::test]
async fn an_invalid_schedule_names_the_field() {
    let (status, body) = preview(
        json!({"schedule": {"type": "crontab", "value": "0 0 * * * *"}}),
        true,
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(body["error"]["fields"][0]["field"], "schedule");

    let (status, body) = preview(
        json!({"schedule": {"type": "crontab", "value": "0 * * * *"}, "timezone": "Mars/Olympus"}),
        true,
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(body["error"]["fields"][0]["field"], "timezone");
}

#[actix_web::test]
async fn requires_authentication() {
    let (status, _) = preview(
        json!({"schedule": {"type": "crontab", "value": "0 * * * *"}}),
        false,
    )
    .await;
    assert_eq!(status, 401);
}
