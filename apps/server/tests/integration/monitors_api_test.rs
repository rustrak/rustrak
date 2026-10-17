//! The dashboard's monitor API: /api/projects/{id}/monitors.

use crate::common::TestDb;
use actix_web::{test, web, App};
use chrono::Utc;
use rustrak::config::{Config, DashboardConfig, DatabaseConfig, RateLimitConfig};
use rustrak::models::monitor::{CheckIn, CheckInStatus};
use rustrak::routes;
use rustrak::services::monitor::MonitorService;
use rustrak::services::{AuthTokenService, ProjectService};
use serde_json::{json, Value};
use std::time::Duration;
use uuid::Uuid;

fn config() -> Config {
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
        ingest_dir: None,
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

macro_rules! setup {
    ($db:ident, $app:ident, $token:ident, $pid:ident) => {
        let $db = TestDb::new().await;
        let $token = AuthTokenService::create(
            &$db.pool,
            rustrak::models::CreateAuthToken {
                description: Some("test".to_string()),
            },
        )
        .await
        .unwrap()
        .token;
        let $pid = ProjectService::create(
            &$db.pool,
            rustrak::models::CreateProject {
                name: "Monitors API".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap()
        .id;
        let $app = test::init_service(
            App::new()
                .app_data(web::Data::new($db.pool.clone()))
                .app_data(web::Data::new(config()))
                .configure(routes::monitors::configure),
        )
        .await;
    };
}

fn auth(token: &str) -> (&'static str, String) {
    ("Authorization", format!("Bearer {token}"))
}

fn hourly() -> Value {
    json!({"schedule": {"type": "crontab", "value": "0 * * * *"}})
}

#[actix_web::test]
async fn create_then_get_and_list() {
    setup!(db, app, token, pid);

    let req = test::TestRequest::post()
        .uri(&format!("/api/projects/{pid}/monitors"))
        .insert_header(auth(&token))
        .set_json(json!({"name": "Nightly Backup", "config": hourly()}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let created: Value = test::read_body_json(resp).await;
    assert_eq!(created["slug"], "nightly-backup");
    assert_eq!(created["config"]["checkin_margin"], 1);
    assert_eq!(created["environments"], json!([]));

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{pid}/monitors/nightly-backup"))
        .insert_header(auth(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{pid}/monitors"))
        .insert_header(auth(&token))
        .to_request();
    let list: Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    drop(db);
}

#[actix_web::test]
async fn create_rejects_an_invalid_schedule() {
    setup!(db, app, token, pid);

    let req = test::TestRequest::post()
        .uri(&format!("/api/projects/{pid}/monitors"))
        .insert_header(auth(&token))
        .set_json(
            json!({"name": "Bad", "config": {"schedule": {"type": "crontab", "value": "nope"}}}),
        )
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    drop(db);
}

#[actix_web::test]
async fn create_rejects_a_taken_slug() {
    setup!(db, app, token, pid);

    for expected in [201, 409] {
        let req = test::TestRequest::post()
            .uri(&format!("/api/projects/{pid}/monitors"))
            .insert_header(auth(&token))
            .set_json(json!({"name": "Same", "slug": "same", "config": hourly()}))
            .to_request();
        assert_eq!(test::call_service(&app, req).await.status(), expected);
    }
    drop(db);
}

#[actix_web::test]
async fn update_merges_config_renames_and_mutes() {
    setup!(db, app, token, pid);
    let req = test::TestRequest::post()
        .uri(&format!("/api/projects/{pid}/monitors"))
        .insert_header(auth(&token))
        .set_json(json!({"name": "Job", "slug": "job", "config": {"schedule": {"type": "crontab", "value": "0 * * * *"}, "max_runtime": 15}}))
        .to_request();
    test::call_service(&app, req).await;

    let req = test::TestRequest::put()
        .uri(&format!("/api/projects/{pid}/monitors/job"))
        .insert_header(auth(&token))
        .set_json(json!({"name": "Hourly job", "is_muted": true, "config": {"failure_issue_threshold": 3}}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let updated: Value = test::read_body_json(resp).await;
    assert_eq!(updated["name"], "Hourly job");
    assert_eq!(updated["is_muted"], true);
    assert_eq!(updated["config"]["failure_issue_threshold"], 3);
    assert_eq!(updated["config"]["max_runtime"], 15);
    drop(db);
}

#[actix_web::test]
async fn checkins_are_paginated_newest_first() {
    setup!(db, app, token, pid);
    for (i, status) in [CheckInStatus::Ok, CheckInStatus::Error]
        .into_iter()
        .enumerate()
    {
        let config = (i == 0).then(|| serde_json::from_value(hourly()).unwrap());
        MonitorService::process_check_in(
            &db.pool,
            pid,
            CheckIn::new(Uuid::new_v4(), "job", status, None, None, config, None).unwrap(),
            Utc::now() + chrono::Duration::seconds(i as i64),
        )
        .await
        .unwrap();
    }

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{pid}/monitors/job/checkins?per_page=1"
        ))
        .insert_header(auth(&token))
        .to_request();
    let page: Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(page["total_count"], 2);
    assert_eq!(page["items"][0]["status"], "error");
}

#[actix_web::test]
async fn delete_removes_the_monitor() {
    setup!(db, app, token, pid);
    let req = test::TestRequest::post()
        .uri(&format!("/api/projects/{pid}/monitors"))
        .insert_header(auth(&token))
        .set_json(json!({"name": "Gone", "slug": "gone", "config": hourly()}))
        .to_request();
    test::call_service(&app, req).await;

    let req = test::TestRequest::delete()
        .uri(&format!("/api/projects/{pid}/monitors/gone"))
        .insert_header(auth(&token))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 204);

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{pid}/monitors/gone"))
        .insert_header(auth(&token))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 404);
    drop(db);
}

#[actix_web::test]
async fn requires_authentication() {
    setup!(db, app, _token, pid);
    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{pid}/monitors"))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 401);
    drop(db);
}
