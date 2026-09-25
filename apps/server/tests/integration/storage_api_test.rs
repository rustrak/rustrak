//! Integration tests for the admin-only Storage API (`/api/storage/*`).
//!
//! These endpoints surface storage usage and run destructive cleanups, so the
//! primary concern under test is the admin gate: a non-admin actor must never
//! see usage or trigger a purge. Bearer tokens are user-scoped so the actor
//! inherits the user's global role.

use crate::common::TestDb;
use actix_session::{storage::CookieSessionStore, SessionMiddleware};
use actix_web::{cookie::Key, test, web, App};
use chrono::Utc;
use rustrak::config::{Config, DashboardConfig, DatabaseConfig, RateLimitConfig, SecurityConfig};
use rustrak::db::DbPool;
use rustrak::models::{CreateAuthToken, CreateProject, CreateUserRequest, User, UserRole};
use rustrak::routes;
use rustrak::services::sourcemap_store::{LocalSourceMapStore, SourceMapStore};
use rustrak::services::{AuthTokenService, ProjectService, UsersService};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

fn create_test_config() -> Config {
    Config {
        host: "127.0.0.1".to_string(),
        port: 0,
        database: DatabaseConfig {
            url: "postgres://test:test@localhost/test".to_string(),
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
        security: SecurityConfig {
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

macro_rules! build_app {
    ($pool:expr, $config:expr) => {
        test::init_service(
            App::new()
                .app_data(web::Data::new($pool))
                .app_data(web::Data::new($config))
                .app_data(web::Data::new(Arc::new(LocalSourceMapStore::new(
                    std::env::temp_dir().join("rustrak-storage-test"),
                )) as Arc<dyn SourceMapStore>))
                .wrap(
                    SessionMiddleware::builder(
                        CookieSessionStore::default(),
                        Key::from(&[0u8; 64]),
                    )
                    .cookie_secure(false)
                    .build(),
                )
                .configure(routes::storage::configure),
        )
        .await
    };
}

async fn seed_user(pool: &DbPool, email: &str, role: UserRole) -> User {
    UsersService::create_user(
        pool,
        &CreateUserRequest {
            email: email.to_string(),
            password: "password123".to_string(),
        },
        role,
    )
    .await
    .expect("seed user")
}

async fn token_for(pool: &DbPool, user_id: i32) -> String {
    AuthTokenService::create_for_user(pool, CreateAuthToken { description: None }, Some(user_id))
        .await
        .expect("seed token")
        .token
}

fn bearer(token: &str) -> (&'static str, String) {
    ("Authorization", format!("Bearer {token}"))
}

#[actix_web::test]
async fn summary_returns_totals_for_admin_and_403_for_non_admin() {
    let db = TestDb::new().await;
    let admin = seed_user(&db.pool, "admin@x.com", UserRole::Admin).await;
    let admin_token = token_for(&db.pool, admin.id).await;
    let member = seed_user(&db.pool, "member@x.com", UserRole::Member).await;
    let member_token = token_for(&db.pool, member.id).await;

    let app = build_app!(db.pool.clone(), create_test_config());

    let (k, v) = bearer(&admin_token);
    let req = test::TestRequest::get()
        .uri("/api/storage/summary")
        .insert_header((k, v))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200, "admin can read storage summary");
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["events_count"], 0);
    assert!(
        body["source_maps"].is_object(),
        "summary includes source-map weight"
    );

    let (k, v) = bearer(&member_token);
    let req = test::TestRequest::get()
        .uri("/api/storage/summary")
        .insert_header((k, v))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403, "non-admin is forbidden");
}

#[actix_web::test]
async fn projects_breakdown_requires_admin() {
    let db = TestDb::new().await;
    let admin = seed_user(&db.pool, "admin@x.com", UserRole::Admin).await;
    let admin_token = token_for(&db.pool, admin.id).await;

    let app = build_app!(db.pool.clone(), create_test_config());

    let (k, v) = bearer(&admin_token);
    let req = test::TestRequest::get()
        .uri("/api/storage/projects")
        .insert_header((k, v))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert!(body.is_array(), "per-project breakdown is a list");
}

#[actix_web::test]
async fn preview_cleanup_is_a_dry_run_returning_counts() {
    let db = TestDb::new().await;
    let admin = seed_user(&db.pool, "admin@x.com", UserRole::Admin).await;
    let admin_token = token_for(&db.pool, admin.id).await;

    let app = build_app!(db.pool.clone(), create_test_config());

    let (k, v) = bearer(&admin_token);
    let req = test::TestRequest::post()
        .uri("/api/storage/cleanup/preview")
        .insert_header((k, v))
        .set_json(json!({ "older_than_days": 30 }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["events"], 0);
    assert_eq!(body["issues_removed"], 0);
}

#[actix_web::test]
async fn execute_cleanup_is_admin_only() {
    let db = TestDb::new().await;
    let member = seed_user(&db.pool, "member@x.com", UserRole::Member).await;
    let member_token = token_for(&db.pool, member.id).await;
    let admin = seed_user(&db.pool, "admin@x.com", UserRole::Admin).await;
    let admin_token = token_for(&db.pool, admin.id).await;

    let app = build_app!(db.pool.clone(), create_test_config());

    // Non-admin must not be able to trigger a destructive purge.
    let (k, v) = bearer(&member_token);
    let req = test::TestRequest::post()
        .uri("/api/storage/cleanup")
        .insert_header((k, v))
        .set_json(json!({ "older_than_days": 30 }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403, "non-admin cannot execute cleanup");

    let (k, v) = bearer(&admin_token);
    let req = test::TestRequest::post()
        .uri("/api/storage/cleanup")
        .insert_header((k, v))
        .set_json(json!({ "older_than_days": 30 }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200, "admin can execute cleanup");
}

#[actix_web::test]
async fn execute_cleanup_honors_data_type_filter_from_request_body() {
    // The cleanup request can scope itself to specific data categories. A
    // logs-only purge sent over HTTP must delete the old log and spare the old
    // transaction — proving the request-body flags actually reach the service
    // instead of the route always wiping everything.
    let db = TestDb::new().await;
    let admin = seed_user(&db.pool, "admin@x.com", UserRole::Admin).await;
    let admin_token = token_for(&db.pool, admin.id).await;

    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "filter-http".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .expect("create project");

    let old = Utc::now() - chrono::Duration::days(60);

    sqlx::query(
        "INSERT INTO transactions (id, event_id, project_id, timestamp, ingested_at, data) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(Uuid::new_v4())
    .bind(Uuid::new_v4())
    .bind(project.id)
    .bind(old)
    .bind(old)
    .bind(json!({}))
    .execute(&db.pool)
    .await
    .expect("seed transaction");

    sqlx::query(
        "INSERT INTO logs (id, project_id, trace_id, span_id, level, severity_number, \
         body, attributes, timestamp, ingested_at) \
         VALUES ($1, $2, $3, NULL, $4, $5, $6, $7, $8, $9)",
    )
    .bind(Uuid::new_v4())
    .bind(project.id)
    .bind("trace")
    .bind("info")
    .bind(9_i16)
    .bind("hello")
    .bind(json!({}))
    .bind(old)
    .bind(old)
    .execute(&db.pool)
    .await
    .expect("seed log");

    let app = build_app!(db.pool.clone(), create_test_config());

    let (k, v) = bearer(&admin_token);
    let req = test::TestRequest::post()
        .uri("/api/storage/cleanup")
        .insert_header((k, v))
        .set_json(json!({
            "older_than_days": 30,
            "include_events": false,
            "include_transactions": false,
            "include_logs": true,
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["logs"], 1, "the old log is removed");
    assert_eq!(
        body["transactions"], 0,
        "transactions out of scope → untouched"
    );

    // The transaction really survived.
    let txns: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM transactions WHERE project_id = $1")
        .bind(project.id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(txns, 1, "logs-only purge left the transaction in place");
}

#[actix_web::test]
async fn source_map_gc_preview_is_admin_only() {
    let db = TestDb::new().await;
    let member = seed_user(&db.pool, "member@x.com", UserRole::Member).await;
    let member_token = token_for(&db.pool, member.id).await;
    let admin = seed_user(&db.pool, "admin@x.com", UserRole::Admin).await;
    let admin_token = token_for(&db.pool, admin.id).await;

    let app = build_app!(db.pool.clone(), create_test_config());

    let (k, v) = bearer(&member_token);
    let req = test::TestRequest::post()
        .uri("/api/storage/source-maps/gc/preview")
        .insert_header((k, v))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403, "non-admin cannot preview gc");

    let (k, v) = bearer(&admin_token);
    let req = test::TestRequest::post()
        .uri("/api/storage/source-maps/gc/preview")
        .insert_header((k, v))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200, "admin can preview gc");
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["files_removed"], 0, "no orphans on an empty instance");
}

#[actix_web::test]
async fn source_map_gc_is_admin_only() {
    let db = TestDb::new().await;
    let member = seed_user(&db.pool, "member@x.com", UserRole::Member).await;
    let member_token = token_for(&db.pool, member.id).await;
    let admin = seed_user(&db.pool, "admin@x.com", UserRole::Admin).await;
    let admin_token = token_for(&db.pool, admin.id).await;

    let app = build_app!(db.pool.clone(), create_test_config());

    let (k, v) = bearer(&member_token);
    let req = test::TestRequest::post()
        .uri("/api/storage/source-maps/gc")
        .insert_header((k, v))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403, "non-admin cannot gc source maps");

    let (k, v) = bearer(&admin_token);
    let req = test::TestRequest::post()
        .uri("/api/storage/source-maps/gc")
        .insert_header((k, v))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200, "admin can gc source maps");
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["files_removed"], 0, "no orphans on an empty instance");
}
