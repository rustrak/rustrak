//! Integration tests for the Logs API
//!
//! Tests GET /api/projects/{id}/logs with a real database.

use crate::common::TestDb;
use actix_web::{test, web, App};
use chrono::Utc;
use rustrak::config::{Config, DashboardConfig, DatabaseConfig, RateLimitConfig};
use rustrak::digest::processors::{LogsProcessor, Processor, ProcessorCtx};
use rustrak::models::CreateProject;
use rustrak::routes;
use rustrak::services::{AuthTokenService, ProjectService};
use serde_json::Value;
use std::time::Duration as StdDuration;
use uuid::Uuid;

fn create_test_config() -> Config {
    Config {
        host: "127.0.0.1".to_string(),
        port: 0,
        database: DatabaseConfig {
            url: "postgres://test:test@localhost/test".to_string(),
            max_connections: 5,
            min_connections: 1,
            acquire_timeout: StdDuration::from_secs(5),
            idle_timeout: StdDuration::from_secs(60),
            max_lifetime: StdDuration::from_secs(300),
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

async fn create_test_token(pool: &rustrak::db::DbPool) -> String {
    AuthTokenService::create(
        pool,
        rustrak::models::CreateAuthToken {
            description: Some("Test token".to_string()),
        },
    )
    .await
    .expect("Failed to create test token")
    .token
}

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
    let config = create_test_config();
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

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(routes::logs::configure),
    )
    .await;

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
    let config = create_test_config();
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

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(routes::logs::configure),
    )
    .await;

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

#[actix_web::test]
async fn test_list_logs_filters_environment_and_excludes_missing_values() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let token = create_test_token(&pool).await;
    let project = ProjectService::create(
        &pool,
        CreateProject {
            name: "Logs Environment Test".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();
    let body = br#"{"items":[
        {"timestamp":1704801600.0,"body":"prod","attributes":{"sentry.environment":{"type":"string","value":"production"}}},
        {"timestamp":1704801601.0,"body":"stage","attributes":{"sentry.environment":{"type":"string","value":"staging"}}},
        {"timestamp":1704801602.0,"body":"missing"}
    ]}"#;
    LogsProcessor
        .process(
            bytes::Bytes::copy_from_slice(body),
            &ProcessorCtx {
                pool: pool.clone(),
                project_id: project.id,
                event_id: Uuid::new_v4(),
                ingested_at: Utc::now(),
                remote_addr: None,
            },
        )
        .await
        .unwrap();

    // Model a pre-migration row and verify the backfill expression.
    sqlx::query("UPDATE logs SET environment = NULL WHERE project_id = $1 AND body = 'prod'")
        .bind(project.id)
        .execute(&pool)
        .await
        .unwrap();
    #[cfg(feature = "postgres")]
    sqlx::query("UPDATE logs SET environment = attributes -> 'sentry.environment' ->> 'value' WHERE project_id = $1 AND attributes -> 'sentry.environment' ->> 'type' = 'string'")
        .bind(project.id).execute(&pool).await.unwrap();
    #[cfg(not(feature = "postgres"))]
    sqlx::query("UPDATE logs SET environment = json_extract(attributes, '$.\"sentry.environment\".value') WHERE project_id = $1 AND json_extract(attributes, '$.\"sentry.environment\".type') = 'string'")
        .bind(project.id).execute(&pool).await.unwrap();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(create_test_config()))
            .configure(routes::logs::configure)
            .configure(routes::projects::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/environments", project.id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let environments: Value = test::read_body_json(resp).await;
    assert_eq!(environments, serde_json::json!(["production", "staging"]));
    for (query, count, expected) in [
        ("", 3, "missing"),
        ("?environment=", 3, "missing"),
        ("?environment=production", 1, "prod"),
        ("?environment=staging", 1, "stage"),
        ("?environment=unknown", 0, ""),
    ] {
        let req = test::TestRequest::get()
            .uri(&format!("/api/projects/{}/logs{}", project.id, query))
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: Value = test::read_body_json(resp).await;
        assert_eq!(body["total_count"], count, "query {query}");
        if count > 0 {
            assert_eq!(body["items"][0]["body"], expected);
        }
    }
}

#[actix_web::test]
async fn test_long_log_environment_survives_ingestion_and_backfill() {
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "Long Log Environment".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();
    let environment = "staging-".repeat(10);
    let body = serde_json::to_vec(&serde_json::json!({
        "items": [{
            "timestamp": 1704801600.0,
            "body": "long environment",
            "attributes": {"sentry.environment": {"type": "string", "value": environment}}
        }]
    }))
    .unwrap();
    LogsProcessor
        .process(
            bytes::Bytes::from(body),
            &ProcessorCtx {
                pool: db.pool.clone(),
                project_id: project.id,
                event_id: Uuid::new_v4(),
                ingested_at: Utc::now(),
                remote_addr: None,
            },
        )
        .await
        .unwrap();

    sqlx::query("UPDATE logs SET environment = NULL WHERE project_id = $1")
        .bind(project.id)
        .execute(&db.pool)
        .await
        .unwrap();
    #[cfg(feature = "postgres")]
    sqlx::query("UPDATE logs SET environment = attributes -> 'sentry.environment' ->> 'value' WHERE project_id = $1")
        .bind(project.id)
        .execute(&db.pool)
        .await
        .unwrap();
    #[cfg(not(feature = "postgres"))]
    sqlx::query("UPDATE logs SET environment = json_extract(attributes, '$.\"sentry.environment\".value') WHERE project_id = $1")
        .bind(project.id)
        .execute(&db.pool)
        .await
        .unwrap();

    let stored: (String,) = sqlx::query_as("SELECT environment FROM logs WHERE project_id = $1")
        .bind(project.id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(stored.0, environment);
}

#[actix_web::test]
async fn test_list_logs_returns_401_without_token() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let config = create_test_config();
    let project = ProjectService::create(
        &pool,
        CreateProject {
            name: "Logs Auth Test".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(routes::logs::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/logs", project.id))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}
