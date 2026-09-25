//! Integration tests for the Transactions API
//!
//! Tests GET /api/projects/{id}/transactions with a real PostgreSQL database.

use crate::common::TestDb;
use actix_web::{test, web, App};
use chrono::Utc;
use rustrak::config::{Config, DashboardConfig, DatabaseConfig, RateLimitConfig};
use rustrak::digest::processors::{Processor, ProcessorCtx, TransactionProcessor};
use rustrak::models::CreateProject;
use rustrak::routes;
use rustrak::services::{AuthTokenService, ProjectService};
use serde_json::{json, Value};
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

async fn create_test_project(pool: &rustrak::db::DbPool, name: &str) -> rustrak::models::Project {
    ProjectService::create(
        pool,
        CreateProject {
            name: name.to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .expect("Failed to create test project")
}

async fn store_test_transaction(pool: &rustrak::db::DbPool, project_id: i32, name: &str) {
    let ctx = ProcessorCtx {
        pool: pool.clone(),
        project_id,
        event_id: Uuid::new_v4(),
        ingested_at: Utc::now(),
        remote_addr: None,
    };
    let payload = serde_json::to_vec(&json!({
        "event_id": Uuid::new_v4().to_string(),
        "type": "transaction",
        "transaction": name,
        "timestamp": Utc::now().timestamp(),
        "start_timestamp": (Utc::now().timestamp() - 1),
        "platform": "javascript",
        "environment": "production",
        "release": "1.0.0",
    }))
    .unwrap();
    TransactionProcessor
        .process(bytes::Bytes::from(payload), &ctx)
        .await
        .expect("Failed to store transaction");
}

/// Stores a transaction carrying a full payload (spans + contexts.trace) and
/// returns the stored transactions row primary key (`id`) for detail lookups.
async fn store_rich_transaction(pool: &rustrak::db::DbPool, project_id: i32, name: &str) -> Uuid {
    let ctx = ProcessorCtx {
        pool: pool.clone(),
        project_id,
        event_id: Uuid::new_v4(),
        ingested_at: Utc::now(),
        remote_addr: None,
    };
    let payload = serde_json::to_vec(&json!({
        "event_id": Uuid::new_v4().to_string(),
        "type": "transaction",
        "transaction": name,
        "timestamp": Utc::now().timestamp(),
        "start_timestamp": (Utc::now().timestamp() - 1),
        "platform": "javascript",
        "environment": "production",
        "release": "1.0.0",
        "contexts": { "trace": { "trace_id": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "span_id": "bbbbbbbbbbbbbbbb", "op": "http.server" } },
        "spans": [
            { "span_id": "cccccccccccccccc", "parent_span_id": "bbbbbbbbbbbbbbbb", "op": "db", "description": "SELECT 1", "start_timestamp": 1.0, "timestamp": 1.5 }
        ],
        "measurements": { "lcp": { "value": 1200.0, "unit": "millisecond" } },
        "tags": { "browser": "Chrome" }
    }))
    .unwrap();
    TransactionProcessor
        .process(bytes::Bytes::from(payload), &ctx)
        .await
        .expect("Failed to store transaction");

    sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM transactions WHERE project_id = $1 ORDER BY ingested_at DESC LIMIT 1",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await
    .expect("stored transaction id")
}

#[actix_web::test]
async fn test_get_transaction_returns_full_detail_with_data() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let config = create_test_config();
    let token = create_test_token(&pool).await;
    let project = create_test_project(&pool, "Transaction Detail Test").await;

    let txn_id = store_rich_transaction(&pool, project.id, "/api/checkout").await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(routes::transactions::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{}/transactions/{}",
            project.id, txn_id
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["id"], txn_id.to_string());
    assert_eq!(body["transaction_name"], "/api/checkout");
    assert_eq!(body["platform"], "javascript");
    // The full Sentry payload must be returned under `data`.
    assert_eq!(body["data"]["spans"][0]["op"], "db");
    assert_eq!(
        body["data"]["contexts"]["trace"]["span_id"],
        "bbbbbbbbbbbbbbbb"
    );
    assert_eq!(body["data"]["measurements"]["lcp"]["value"], 1200.0);
}

#[actix_web::test]
async fn test_get_transaction_spans_returns_indexed_spans() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let config = create_test_config();
    let token = create_test_token(&pool).await;
    let project = create_test_project(&pool, "Transaction Spans Test").await;

    let txn_id = store_rich_transaction(&pool, project.id, "/api/checkout").await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(routes::transactions::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{}/transactions/{}/spans",
            project.id, txn_id
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert!(body.is_array(), "spans endpoint must return a JSON array");
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(body[0]["op"], "db");
    assert_eq!(body[0]["description"], "SELECT 1");
    assert_eq!(body[0]["span_id"], "cccccccccccccccc");
}

#[actix_web::test]
async fn test_transaction_stats_endpoint_returns_aggregates() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let config = create_test_config();
    let token = create_test_token(&pool).await;
    let project = create_test_project(&pool, "Transaction Stats Endpoint Test").await;

    // Two transactions sharing the same name+op so they form one group.
    for _ in 0..2 {
        store_rich_transaction(&pool, project.id, "/api/checkout").await;
    }

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(routes::transactions::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/transactions/stats", project.id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    // `/stats` must resolve to the stats handler, not be parsed as a transaction id.
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    // Paginated response shape, like the other list endpoints.
    assert_eq!(body["total_count"], 1, "one (name, op) group");
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
    assert_eq!(body["items"][0]["transaction_name"], "/api/checkout");
    assert_eq!(body["items"][0]["op"], "http.server");
    assert_eq!(body["items"][0]["count"], 2);
}

#[actix_web::test]
async fn test_transaction_stat_group_returns_single_group_or_404() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let config = create_test_config();
    let token = create_test_token(&pool).await;
    let project = create_test_project(&pool, "Transaction Stat Group Test").await;

    store_rich_transaction(&pool, project.id, "/api/checkout").await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(routes::transactions::configure),
    )
    .await;

    // Known group → 200 with its aggregate.
    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{}/transactions/stats/group?name=/api/checkout&op=http.server",
            project.id
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["transaction_name"], "/api/checkout");
    assert_eq!(body["op"], "http.server");
    assert_eq!(body["count"], 1);

    // Unknown group → 404 (not an empty body).
    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{}/transactions/stats/group?name=/nope",
            project.id
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn test_get_transaction_404_for_unknown_id() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let config = create_test_config();
    let token = create_test_token(&pool).await;
    let project = create_test_project(&pool, "Transaction Detail 404 Test").await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(routes::transactions::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{}/transactions/{}",
            project.id,
            Uuid::new_v4()
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn test_get_transaction_401_without_token() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let config = create_test_config();
    let project = create_test_project(&pool, "Transaction Detail Auth Test").await;
    let txn_id = store_rich_transaction(&pool, project.id, "/api/x").await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(routes::transactions::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{}/transactions/{}",
            project.id, txn_id
        ))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn test_list_transactions_returns_empty_when_none_exist() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let config = create_test_config();
    let token = create_test_token(&pool).await;
    let project = create_test_project(&pool, "Transactions Empty Test").await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(routes::transactions::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/transactions", project.id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["items"], json!([]));
    assert_eq!(body["total_count"], 0);
    assert_eq!(body["page"], 1);
    assert_eq!(body["total_pages"], 0);
}

#[actix_web::test]
async fn test_list_transactions_returns_stored_transactions() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let config = create_test_config();
    let token = create_test_token(&pool).await;
    let project = create_test_project(&pool, "Transactions List Test").await;

    store_test_transaction(&pool, project.id, "/api/checkout").await;
    store_test_transaction(&pool, project.id, "/api/payment").await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(routes::transactions::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/transactions", project.id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total_count"], 2);
    assert_eq!(body["total_pages"], 1);
    let items = body["items"].as_array().expect("items is array");
    assert_eq!(items.len(), 2);
    // Newest first
    assert_eq!(items[0]["transaction_name"], "/api/payment");
    assert_eq!(items[1]["transaction_name"], "/api/checkout");
}

#[actix_web::test]
async fn test_list_transactions_does_not_return_error_events() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let config = create_test_config();
    let token = create_test_token(&pool).await;
    let project = create_test_project(&pool, "Transactions Isolation Test").await;

    // Only store a transaction (no error events manually inserted)
    store_test_transaction(&pool, project.id, "/api/users").await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(routes::transactions::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/transactions", project.id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    let items = body["items"].as_array().expect("items is array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["transaction_name"], "/api/users");
}

#[actix_web::test]
async fn test_list_transactions_returns_401_without_token() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let config = create_test_config();
    let project = create_test_project(&pool, "Transactions Auth Test").await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(routes::transactions::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/transactions", project.id))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}
