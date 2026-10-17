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
use rustrak::models::{
    CleanupFilter, CreateAuthToken, CreateProject, CreateUserRequest, User, UserRole,
};
use rustrak::routes;
use rustrak::services::sourcemap_store::{LocalSourceMapStore, SourceMapStore};
use rustrak::services::{AuthTokenService, CleanupJob, ProjectService, UsersService};
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
        build_app!($pool, $config, web::Data::new(CleanupJob::default()))
    };
    ($pool:expr, $config:expr, $job:expr) => {
        test::init_service(
            App::new()
                .app_data(web::Data::new($pool))
                .app_data(web::Data::new($config))
                .app_data(web::Data::new(Arc::new(LocalSourceMapStore::new(
                    std::env::temp_dir().join("rustrak-storage-test"),
                )) as Arc<dyn SourceMapStore>))
                .app_data($job)
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
    assert_eq!(resp.status(), 202, "admin can start a cleanup");
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["state"], "running", "the response is the started job");

    // Its progress is admin-only as well.
    let (k, v) = bearer(&member_token);
    let req = test::TestRequest::get()
        .uri("/api/storage/cleanup/status")
        .insert_header((k, v))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403, "non-admin cannot read cleanup status");
}

/// Polls `GET /api/storage/cleanup/status` until the job leaves `running`.
async fn wait_for_cleanup<S>(app: &S, token: &str) -> Value
where
    S: actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
{
    for _ in 0..200 {
        let (k, v) = bearer(token);
        let req = test::TestRequest::get()
            .uri("/api/storage/cleanup/status")
            .insert_header((k, v))
            .to_request();
        let resp = test::call_service(app, req).await;
        assert_eq!(resp.status(), 200);
        let body: Value = test::read_body_json(resp).await;
        if body["state"] != "running" {
            return body;
        }
        actix_web::rt::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("cleanup did not finish");
}

#[actix_web::test]
async fn execute_cleanup_refuses_a_second_run_while_one_is_running() {
    let db = TestDb::new().await;
    let admin = seed_user(&db.pool, "admin@x.com", UserRole::Admin).await;
    let admin_token = token_for(&db.pool, admin.id).await;

    // The running job gets a pool whose only connection the test holds, so
    // it cannot finish before the second request arrives, however fast the
    // database is.
    let blocked: DbPool = sqlx::pool::PoolOptions::new()
        .max_connections(1)
        .connect_with((*db.pool.connect_options()).clone())
        .await
        .unwrap();
    let held = blocked.acquire().await.unwrap();
    let job = web::Data::new(CleanupJob::default());
    job.start(blocked.clone(), 30, None, CleanupFilter::all())
        .unwrap();
    let app = build_app!(db.pool.clone(), create_test_config(), job.clone());

    let (k, v) = bearer(&admin_token);
    let second = test::TestRequest::post()
        .uri("/api/storage/cleanup")
        .insert_header((k, v))
        .set_json(json!({ "older_than_days": 30 }))
        .to_request();
    let second = test::call_service(&app, second).await;
    assert_eq!(second.status(), 409, "one cleanup at a time");

    drop(held);
    let done = wait_for_cleanup(&app, &admin_token).await;
    assert_eq!(done["state"], "completed");
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
    assert_eq!(resp.status(), 202);
    let body = wait_for_cleanup(&app, &admin_token).await;
    assert_eq!(body["state"], "completed");
    let body = &body["removed"];
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

// =============================================================================
// Automatic cleanup: GET/PUT /api/storage/auto-cleanup and its scheduled job
// =============================================================================

async fn seed_old_log(pool: &DbPool, project_id: i32, days_ago: i64) {
    let old = Utc::now() - chrono::Duration::days(days_ago);
    sqlx::query(
        "INSERT INTO logs (id, project_id, trace_id, span_id, level, severity_number, \
         body, attributes, timestamp, ingested_at) \
         VALUES ($1, $2, $3, NULL, $4, $5, $6, $7, $8, $9)",
    )
    .bind(Uuid::new_v4())
    .bind(project_id)
    .bind("trace")
    .bind("info")
    .bind(9_i16)
    .bind("hello")
    .bind(json!({}))
    .bind(old)
    .bind(old)
    .execute(pool)
    .await
    .expect("seed log");
}

async fn log_count(pool: &DbPool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM logs")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[actix_web::test]
async fn auto_cleanup_is_off_by_default_with_sensible_settings() {
    let db = TestDb::new().await;
    let admin = seed_user(&db.pool, "admin@x.com", UserRole::Admin).await;
    let token = token_for(&db.pool, admin.id).await;
    let app = build_app!(db.pool.clone(), create_test_config());

    let req = test::TestRequest::get()
        .uri("/api/storage/auto-cleanup")
        .insert_header(bearer(&token))
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, req).await;

    assert_eq!(body["settings"]["enabled"], false);
    assert_eq!(
        body["settings"]["schedule"],
        json!({"type": "crontab", "value": "0 3 * * *"})
    );
    assert_eq!(body["settings"]["timezone"], "UTC");
    assert_eq!(body["settings"]["older_than_days"], 90);
    assert_eq!(body["settings"]["include_events"], true);
    assert_eq!(body["settings"]["include_transactions"], true);
    assert_eq!(body["settings"]["include_logs"], true);
    assert_eq!(body["next_run_at"], Value::Null);
    assert_eq!(body["last_run"], Value::Null);
}

#[actix_web::test]
async fn auto_cleanup_settings_are_saved() {
    let db = TestDb::new().await;
    let admin = seed_user(&db.pool, "admin@x.com", UserRole::Admin).await;
    let token = token_for(&db.pool, admin.id).await;
    let app = build_app!(db.pool.clone(), create_test_config());

    let settings = json!({
        "enabled": true,
        "schedule": {"type": "crontab", "value": "30 4 * * 0"},
        "timezone": "Europe/Madrid",
        "older_than_days": 30,
        "include_events": false,
        "include_transactions": true,
        "include_logs": true,
    });
    let req = test::TestRequest::put()
        .uri("/api/storage/auto-cleanup")
        .insert_header(bearer(&token))
        .set_json(&settings)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/api/storage/auto-cleanup")
        .insert_header(bearer(&token))
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(body["settings"], settings);
}

#[actix_web::test]
async fn auto_cleanup_rejects_settings_that_cannot_run() {
    let db = TestDb::new().await;
    let admin = seed_user(&db.pool, "admin@x.com", UserRole::Admin).await;
    let token = token_for(&db.pool, admin.id).await;
    let app = build_app!(db.pool.clone(), create_test_config());

    let base = json!({
        "enabled": true,
        "schedule": {"type": "crontab", "value": "0 3 * * *"},
        "timezone": "UTC",
        "older_than_days": 30,
        "include_events": true,
        "include_transactions": true,
        "include_logs": true,
    });
    let cases = [
        (
            "schedule",
            json!({"schedule": {"type": "crontab", "value": "nope"}}),
        ),
        ("timezone", json!({"timezone": "Mars/Olympus"})),
        ("older_than_days", json!({"older_than_days": 0})),
        (
            "include_events",
            json!({"include_events": false, "include_transactions": false, "include_logs": false}),
        ),
    ];
    for (field, patch) in cases {
        let mut body = base.clone();
        for (k, v) in patch.as_object().unwrap() {
            body[k] = v.clone();
        }
        let req = test::TestRequest::put()
            .uri("/api/storage/auto-cleanup")
            .insert_header(bearer(&token))
            .set_json(&body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400, "{field}");
        let err: Value = test::read_body_json(resp).await;
        assert_eq!(err["error"]["fields"][0]["field"], field);
    }
}

#[actix_web::test]
async fn auto_cleanup_is_admin_only() {
    let db = TestDb::new().await;
    let member = seed_user(&db.pool, "member@x.com", UserRole::Member).await;
    let token = token_for(&db.pool, member.id).await;
    let app = build_app!(db.pool.clone(), create_test_config());

    let req = test::TestRequest::get()
        .uri("/api/storage/auto-cleanup")
        .insert_header(bearer(&token))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 403);
    let req = test::TestRequest::put()
        .uri("/api/storage/auto-cleanup")
        .insert_header(bearer(&token))
        .set_json(json!({"enabled": true}))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 403);
}

#[actix_web::test]
async fn the_cleanup_job_purges_only_the_chosen_categories_on_schedule() {
    use rustrak::scheduler::engine::Scheduler;
    use rustrak::scheduler::jobs::StorageCleanupJob;

    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "auto-cleanup".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();
    seed_old_log(&db.pool, project.id, 40).await;
    seed_old_log(&db.pool, project.id, 5).await;

    Scheduler::update_settings(
        &db.pool,
        StorageCleanupJob::KEY,
        json!({
            "enabled": true,
            "schedule": {"type": "interval", "value": 1, "unit": "day"},
            "timezone": "UTC",
            "older_than_days": 30,
            "include_events": false,
            "include_transactions": false,
            "include_logs": true,
        }),
    )
    .await
    .unwrap();
    let scheduler = Scheduler::new(vec![Arc::new(StorageCleanupJob)]);
    let now = Utc::now();
    for tick in [now, now + chrono::Duration::days(1)] {
        for handle in scheduler.tick(&db.pool, tick).await.unwrap() {
            handle.await.unwrap();
        }
    }

    assert_eq!(log_count(&db.pool).await, 1, "only the 40-day-old log goes");
    let state = Scheduler::job(&db.pool, StorageCleanupJob::KEY)
        .await
        .unwrap();
    assert_eq!(state.last_result.unwrap()["logs"], 1);
}

#[actix_web::test]
async fn a_disabled_cleanup_job_deletes_nothing() {
    use rustrak::scheduler::engine::Scheduler;
    use rustrak::scheduler::jobs::StorageCleanupJob;

    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "auto-cleanup-off".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();
    seed_old_log(&db.pool, project.id, 400).await;

    let scheduler = Scheduler::new(vec![Arc::new(StorageCleanupJob)]);
    let now = Utc::now();
    for tick in [now, now + chrono::Duration::days(30)] {
        for handle in scheduler.tick(&db.pool, tick).await.unwrap() {
            handle.await.unwrap();
        }
    }

    assert_eq!(log_count(&db.pool).await, 1);
}
