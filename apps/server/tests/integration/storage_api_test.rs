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
// Retention: GET/PUT /api/storage/retention, its preview and its scheduled job
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

fn test_store() -> Arc<dyn SourceMapStore> {
    Arc::new(LocalSourceMapStore::new(
        std::env::temp_dir().join("rustrak-storage-test"),
    ))
}

/// An orphaned source map: a `source_file` row no upload references.
async fn insert_orphan_source_file(pool: &DbPool, size: i64) {
    // 40 characters, a SHA-1 like production's: Postgres pads `CHAR(40)`.
    let checksum = format!("{}00000000", Uuid::new_v4().simple());
    sqlx::query(
        "INSERT INTO source_file (id, checksum, size, storage_path) VALUES ($1, $2, $3, $4)",
    )
    .bind(Uuid::new_v4())
    .bind(&checksum)
    .bind(size)
    .bind(&checksum)
    .execute(pool)
    .await
    .unwrap();
}

async fn create_project(pool: &DbPool, name: &str) -> i32 {
    ProjectService::create(
        pool,
        CreateProject {
            name: name.to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap()
    .id
}

async fn project_log_count(pool: &DbPool, project_id: i32) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM logs WHERE project_id = $1")
        .bind(project_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Runs the scheduled retention job twice, a day apart: the first tick only
/// schedules it, the second runs it.
async fn run_retention_job(pool: &DbPool, cleanup: web::Data<CleanupJob>) {
    use rustrak::scheduler::engine::Scheduler;
    use rustrak::scheduler::jobs::StorageCleanupJob;

    let scheduler = Scheduler::new(vec![Arc::new(StorageCleanupJob::new(
        cleanup,
        test_store(),
    ))]);
    let now = Utc::now();
    for tick in [now, now + chrono::Duration::days(8)] {
        for handle in scheduler.tick(pool, tick).await.unwrap() {
            handle.await.unwrap();
        }
    }
}

async fn put_retention<S>(app: &S, token: &str, settings: &Value) -> actix_web::dev::ServiceResponse
where
    S: actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
{
    let req = test::TestRequest::put()
        .uri("/api/storage/retention")
        .insert_header(bearer(token))
        .set_json(settings)
        .to_request();
    test::call_service(app, req).await
}

#[actix_web::test]
async fn retention_keeps_everything_forever_by_default() {
    let db = TestDb::new().await;
    let admin = seed_user(&db.pool, "admin@x.com", UserRole::Admin).await;
    let token = token_for(&db.pool, admin.id).await;
    let app = build_app!(db.pool.clone(), create_test_config());

    let req = test::TestRequest::get()
        .uri("/api/storage/retention")
        .insert_header(bearer(&token))
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, req).await;

    assert_eq!(
        body["settings"],
        json!({
            "defaults": {"events_days": null, "transactions_days": null, "logs_days": null},
            "overrides": [],
            "frequency": "daily",
            "hour": 3,
            "weekday": 0,
        })
    );
    assert_eq!(body["next_run_at"], Value::Null);
    assert_eq!(body["last_run"], Value::Null);
}

#[actix_web::test]
async fn retention_settings_are_saved() {
    let db = TestDb::new().await;
    let admin = seed_user(&db.pool, "admin@x.com", UserRole::Admin).await;
    let token = token_for(&db.pool, admin.id).await;
    let project = create_project(&db.pool, "keep-longer").await;
    let app = build_app!(db.pool.clone(), create_test_config());

    let settings = json!({
        "defaults": {"events_days": 90, "transactions_days": 30, "logs_days": null},
        "overrides": [
            {"project_id": project, "events_days": 365, "transactions_days": null, "logs_days": 7}
        ],
        "frequency": "weekly",
        "hour": 4,
        "weekday": 6,
    });
    assert_eq!(put_retention(&app, &token, &settings).await.status(), 200);

    let req = test::TestRequest::get()
        .uri("/api/storage/retention")
        .insert_header(bearer(&token))
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(body["settings"], settings);
}

#[actix_web::test]
async fn retention_rejects_settings_that_cannot_run() {
    let db = TestDb::new().await;
    let admin = seed_user(&db.pool, "admin@x.com", UserRole::Admin).await;
    let token = token_for(&db.pool, admin.id).await;
    let project = create_project(&db.pool, "twice").await;
    let app = build_app!(db.pool.clone(), create_test_config());

    let base = json!({
        "defaults": {"events_days": 30, "transactions_days": 30, "logs_days": 30},
        "overrides": [],
        "frequency": "daily",
        "hour": 3,
        "weekday": 0,
    });
    let twice = json!([
        {"project_id": project, "events_days": 1, "transactions_days": 1, "logs_days": 1},
        {"project_id": project, "events_days": 2, "transactions_days": 2, "logs_days": 2},
    ]);
    let cases = [
        ("events_days", json!({"defaults": {"events_days": 0}})),
        ("logs_days", json!({"defaults": {"logs_days": i64::MAX}})),
        ("hour", json!({"hour": 24})),
        ("weekday", json!({"weekday": 7})),
        ("overrides", json!({"overrides": twice})),
    ];
    for (field, patch) in cases {
        let mut body = base.clone();
        for (k, v) in patch.as_object().unwrap() {
            if k == "defaults" {
                for (dk, dv) in v.as_object().unwrap() {
                    body["defaults"][dk] = dv.clone();
                }
            } else {
                body[k] = v.clone();
            }
        }
        let resp = put_retention(&app, &token, &body).await;
        assert_eq!(resp.status(), 400, "{field}");
        let err: Value = test::read_body_json(resp).await;
        assert_eq!(err["error"]["fields"][0]["field"], field);
    }
}

#[actix_web::test]
async fn retention_is_admin_only() {
    let db = TestDb::new().await;
    let member = seed_user(&db.pool, "member@x.com", UserRole::Member).await;
    let token = token_for(&db.pool, member.id).await;
    let app = build_app!(db.pool.clone(), create_test_config());

    for req in [
        test::TestRequest::get().uri("/api/storage/retention"),
        test::TestRequest::put()
            .uri("/api/storage/retention")
            .set_json(json!({})),
        test::TestRequest::post()
            .uri("/api/storage/retention/preview")
            .set_json(json!({})),
    ] {
        let req = req.insert_header(bearer(&token)).to_request();
        assert_eq!(test::call_service(&app, req).await.status(), 403);
    }
}

#[actix_web::test]
async fn retention_preview_counts_what_each_project_would_lose_without_deleting() {
    let db = TestDb::new().await;
    let admin = seed_user(&db.pool, "admin@x.com", UserRole::Admin).await;
    let token = token_for(&db.pool, admin.id).await;
    let default_project = create_project(&db.pool, "default").await;
    let custom = create_project(&db.pool, "custom").await;
    for project in [default_project, custom] {
        seed_old_log(&db.pool, project, 40).await;
        seed_old_log(&db.pool, project, 5).await;
    }
    let app = build_app!(db.pool.clone(), create_test_config());

    let req = test::TestRequest::post()
        .uri("/api/storage/retention/preview")
        .insert_header(bearer(&token))
        .set_json(json!({
            "defaults": {"events_days": null, "transactions_days": null, "logs_days": 30},
            "overrides": [
                {"project_id": custom, "events_days": null, "transactions_days": null, "logs_days": 1}
            ],
        }))
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, req).await;

    // The default project loses its 40-day-old log; the custom one both.
    assert_eq!(body["logs"], 3);
    assert_eq!(body["events"], 0);
    assert_eq!(log_count(&db.pool).await, 4, "a preview deletes nothing");
}

#[actix_web::test]
async fn the_retention_job_applies_each_projects_own_windows() {
    let db = TestDb::new().await;
    let default_project = create_project(&db.pool, "default").await;
    let custom = create_project(&db.pool, "custom").await;
    let forever = create_project(&db.pool, "forever").await;
    for project in [default_project, custom, forever] {
        seed_old_log(&db.pool, project, 40).await;
        seed_old_log(&db.pool, project, 5).await;
    }
    rustrak::scheduler::engine::Scheduler::update_settings(
        &db.pool,
        rustrak::scheduler::jobs::StorageCleanupJob::KEY,
        json!({
            "defaults": {"events_days": null, "transactions_days": null, "logs_days": 30},
            "overrides": [
                {"project_id": custom, "events_days": null, "transactions_days": null, "logs_days": 1},
                {"project_id": forever, "events_days": null, "transactions_days": null, "logs_days": null}
            ],
        }),
    )
    .await
    .unwrap();
    let cleanup = web::Data::new(CleanupJob::default());

    run_retention_job(&db.pool, cleanup.clone()).await;

    assert_eq!(project_log_count(&db.pool, default_project).await, 1);
    assert_eq!(project_log_count(&db.pool, custom).await, 0);
    assert_eq!(project_log_count(&db.pool, forever).await, 2);
    // It ran through the manual cleanup's job, so the two never overlap.
    let status = cleanup.status();
    assert_eq!(status.state, rustrak::models::CleanupState::Completed);
    assert_eq!(status.removed.logs, 3);
}

#[actix_web::test]
async fn keeping_everything_forever_never_runs_the_job() {
    let db = TestDb::new().await;
    let project = create_project(&db.pool, "forever").await;
    seed_old_log(&db.pool, project, 400).await;

    run_retention_job(&db.pool, web::Data::new(CleanupJob::default())).await;

    assert_eq!(log_count(&db.pool).await, 1);
    let state = rustrak::scheduler::engine::Scheduler::job(
        &db.pool,
        rustrak::scheduler::jobs::StorageCleanupJob::KEY,
    )
    .await
    .unwrap();
    assert_eq!(state.next_run_at, None);
}

#[actix_web::test]
async fn a_cleanup_cut_short_by_a_restart_is_not_shown_as_running() {
    use rustrak::scheduler::engine::Scheduler;
    use rustrak::scheduler::jobs::StorageCleanupJob;

    let db = TestDb::new().await;
    let admin = seed_user(&db.pool, "admin@x.com", UserRole::Admin).await;
    let token = token_for(&db.pool, admin.id).await;
    Scheduler::update_settings(
        &db.pool,
        StorageCleanupJob::KEY,
        json!({"defaults": {"logs_days": 30}}),
    )
    .await
    .unwrap();
    // Started before a restart, never finished, and no task survives it.
    sqlx::query(if cfg!(feature = "postgres") {
        "UPDATE scheduled_jobs SET last_started_at = $1 WHERE key = $2"
    } else {
        "UPDATE scheduled_jobs SET last_started_at = ? WHERE key = ?"
    })
    .bind(Utc::now())
    .bind(StorageCleanupJob::KEY)
    .execute(&db.pool)
    .await
    .unwrap();
    let app = build_app!(db.pool.clone(), create_test_config());

    let req = test::TestRequest::get()
        .uri("/api/storage/retention")
        .insert_header(bearer(&token))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, req).await).await;
    assert_eq!(body["last_run"]["status"], "error");
    assert!(body["last_run"]["error"]
        .as_str()
        .unwrap()
        .contains("restart"));
}

#[actix_web::test]
async fn the_projects_breakdown_splits_its_size_by_kind() {
    let db = TestDb::new().await;
    let admin = seed_user(&db.pool, "admin@x.com", UserRole::Admin).await;
    let token = token_for(&db.pool, admin.id).await;
    let project = create_project(&db.pool, "logs-only").await;
    seed_old_log(&db.pool, project, 5).await;
    let app = build_app!(db.pool.clone(), create_test_config());

    let req = test::TestRequest::get()
        .uri("/api/storage/projects")
        .insert_header(bearer(&token))
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, req).await;
    let row = body
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["project_id"] == project)
        .unwrap();

    assert_eq!(row["events_bytes"], 0);
    assert_eq!(row["transactions_bytes"], 0);
    assert!(row["logs_bytes"].as_i64().unwrap() > 0);
    assert_eq!(row["estimated_bytes"], row["logs_bytes"]);
}

#[actix_web::test]
async fn the_age_breakdown_buckets_each_kind_by_how_long_ago_it_arrived() {
    let db = TestDb::new().await;
    let admin = seed_user(&db.pool, "admin@x.com", UserRole::Admin).await;
    let token = token_for(&db.pool, admin.id).await;
    let project = create_project(&db.pool, "aged").await;
    for days in [5, 40, 400] {
        seed_old_log(&db.pool, project, days).await;
    }
    let app = build_app!(db.pool.clone(), create_test_config());

    let req = test::TestRequest::get()
        .uri("/api/storage/age")
        .insert_header(bearer(&token))
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, req).await;

    assert_eq!(body["bucket_days"], json!([7, 14, 30, 90, 180, 365]));
    let row = body["projects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["project_id"] == project)
        .unwrap();
    // Under 7 days, 30–90 and over a year.
    assert_eq!(row["logs"]["counts"], json!([1, 0, 0, 1, 0, 0, 1]));
    assert_eq!(row["events"]["counts"], json!([0, 0, 0, 0, 0, 0, 0]));
    assert_eq!(row["events"]["oldest"], Value::Null);
    let oldest = chrono::DateTime::parse_from_rfc3339(row["logs"]["oldest"].as_str().unwrap())
        .unwrap()
        .with_timezone(&Utc);
    assert!(Utc::now() - oldest > chrono::Duration::days(399));
}

/// The storage page reads summary, projects and age at once. They come from
/// one read of the tables, kept until a cleanup changes what is stored:
/// reading each table once per endpoint and per load took over 30s on a
/// large instance.
#[actix_web::test]
async fn the_storage_figures_are_read_once_and_kept_until_a_cleanup() {
    let db = TestDb::new().await;
    let admin = seed_user(&db.pool, "admin@x.com", UserRole::Admin).await;
    let token = token_for(&db.pool, admin.id).await;
    let project = create_project(&db.pool, "kept").await;
    seed_old_log(&db.pool, project, 40).await;
    let app = build_app!(db.pool.clone(), create_test_config());
    let get = |uri: &'static str| {
        test::TestRequest::get()
            .uri(uri)
            .insert_header(bearer(&token))
            .to_request()
    };
    let logs_in = |projects: &Value| {
        projects
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["project_id"] == project)
            .unwrap()["logs_count"]
            .clone()
    };

    let summary: Value = test::call_and_read_body_json(&app, get("/api/storage/summary")).await;
    assert_eq!(summary["logs_count"], 1);

    seed_old_log(&db.pool, project, 5).await;
    let summary: Value = test::call_and_read_body_json(&app, get("/api/storage/summary")).await;
    let projects: Value = test::call_and_read_body_json(&app, get("/api/storage/projects")).await;
    let age: Value = test::call_and_read_body_json(&app, get("/api/storage/age")).await;
    assert_eq!(summary["logs_count"], 1, "the figures read a moment ago");
    assert_eq!(logs_in(&projects), 1, "the same read as the summary");
    assert_eq!(
        age["projects"][0]["logs"]["counts"],
        json!([0, 0, 0, 1, 0, 0, 0])
    );

    // A cleanup changes what is stored, even one that deletes nothing.
    let req = test::TestRequest::post()
        .uri("/api/storage/cleanup")
        .insert_header(bearer(&token))
        .set_json(json!({ "older_than_days": 365 }))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 202);
    wait_for_cleanup(&app, &token).await;
    let summary: Value = test::call_and_read_body_json(&app, get("/api/storage/summary")).await;
    let projects: Value = test::call_and_read_body_json(&app, get("/api/storage/projects")).await;
    let age: Value = test::call_and_read_body_json(&app, get("/api/storage/age")).await;
    assert_eq!(summary["logs_count"], 2);
    assert_eq!(logs_in(&projects), 2);
    assert_eq!(
        age["projects"][0]["logs"]["counts"],
        json!([1, 0, 0, 1, 0, 0, 0])
    );
}

/// Collecting unused source maps changes what is stored too, without a
/// cleanup job: the next read of the figures must not be the one from before.
#[actix_web::test]
async fn collecting_source_maps_drops_the_kept_figures() {
    let db = TestDb::new().await;
    let admin = seed_user(&db.pool, "admin@x.com", UserRole::Admin).await;
    let token = token_for(&db.pool, admin.id).await;
    insert_orphan_source_file(&db.pool, 2_048).await;
    let app = build_app!(db.pool.clone(), create_test_config());
    let files = || async {
        let req = test::TestRequest::get()
            .uri("/api/storage/summary")
            .insert_header(bearer(&token))
            .to_request();
        let body: Value = test::call_and_read_body_json(&app, req).await;
        body["source_maps"]["file_count"].clone()
    };

    assert_eq!(files().await, 1);
    let req = test::TestRequest::post()
        .uri("/api/storage/source-maps/gc")
        .insert_header(bearer(&token))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 200);
    assert_eq!(files().await, 0);
}

#[actix_web::test]
async fn the_age_breakdown_is_admin_only() {
    let db = TestDb::new().await;
    let member = seed_user(&db.pool, "member@x.com", UserRole::Member).await;
    let token = token_for(&db.pool, member.id).await;
    let app = build_app!(db.pool.clone(), create_test_config());

    let req = test::TestRequest::get()
        .uri("/api/storage/age")
        .insert_header(bearer(&token))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 403);
}

#[actix_web::test]
async fn cleaning_up_now_enforces_the_saved_retention_in_the_background() {
    let db = TestDb::new().await;
    let admin = seed_user(&db.pool, "admin@x.com", UserRole::Admin).await;
    let token = token_for(&db.pool, admin.id).await;
    let project = create_project(&db.pool, "now").await;
    seed_old_log(&db.pool, project, 40).await;
    seed_old_log(&db.pool, project, 5).await;
    insert_orphan_source_file(&db.pool, 2_048).await;
    let app = build_app!(db.pool.clone(), create_test_config());
    let settings =
        json!({"defaults": {"events_days": null, "transactions_days": null, "logs_days": 30}});
    assert_eq!(put_retention(&app, &token, &settings).await.status(), 200);

    let req = test::TestRequest::post()
        .uri("/api/storage/retention/run")
        .insert_header(bearer(&token))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 202);
    let status = wait_for_cleanup(&app, &token).await;

    assert_eq!(status["state"], "completed");
    assert_eq!(status["removed"]["logs"], 1);
    // Unused source maps go with every cleanup.
    assert_eq!(status["removed"]["source_maps"], 1);
    assert_eq!(project_log_count(&db.pool, project).await, 1);

    // It is the last cleanup, as a scheduled one would be. Recorded just
    // after the job finishes, so wait for it.
    let mut last_run = Value::Null;
    for _ in 0..100 {
        let req = test::TestRequest::get()
            .uri("/api/storage/retention")
            .insert_header(bearer(&token))
            .to_request();
        let body: Value = test::call_and_read_body_json(&app, req).await;
        last_run = body["last_run"].clone();
        if last_run["status"] != "running" && !last_run.is_null() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(last_run["status"], "ok");
    assert_eq!(last_run["removed"]["logs"], 1);
}

#[actix_web::test]
async fn cleaning_up_now_refuses_while_everything_is_kept_forever() {
    let db = TestDb::new().await;
    let admin = seed_user(&db.pool, "admin@x.com", UserRole::Admin).await;
    let token = token_for(&db.pool, admin.id).await;
    let app = build_app!(db.pool.clone(), create_test_config());

    let req = test::TestRequest::post()
        .uri("/api/storage/retention/run")
        .insert_header(bearer(&token))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 400);
}

#[actix_web::test]
async fn the_retention_preview_counts_the_unused_source_maps_it_would_delete() {
    let db = TestDb::new().await;
    let admin = seed_user(&db.pool, "admin@x.com", UserRole::Admin).await;
    let token = token_for(&db.pool, admin.id).await;
    insert_orphan_source_file(&db.pool, 2_048).await;
    let app = build_app!(db.pool.clone(), create_test_config());

    let req = test::TestRequest::post()
        .uri("/api/storage/retention/preview")
        .insert_header(bearer(&token))
        .set_json(json!({"defaults": {"logs_days": 30}}))
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, req).await;

    assert_eq!(body["source_maps"], 1);
}

#[actix_web::test]
async fn keeping_everything_forever_previews_no_source_maps_since_no_cleanup_runs() {
    let db = TestDb::new().await;
    let admin = seed_user(&db.pool, "admin@x.com", UserRole::Admin).await;
    let token = token_for(&db.pool, admin.id).await;
    insert_orphan_source_file(&db.pool, 2_048).await;
    let app = build_app!(db.pool.clone(), create_test_config());

    let req = test::TestRequest::post()
        .uri("/api/storage/retention/preview")
        .insert_header(bearer(&token))
        .set_json(json!({}))
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, req).await;

    assert_eq!(body["source_maps"], 0);
}

// =============================================================================
// SQLite: a cleanup gives the space it freed back to the filesystem
// =============================================================================

/// A database built the way the server builds one (`db::create_pool`), so it
/// has the `auto_vacuum` mode a new deployment gets.
#[cfg(feature = "sqlite")]
async fn production_sqlite(path: &std::path::Path) -> DbPool {
    let pool = rustrak::db::create_pool(&DatabaseConfig {
        url: format!("sqlite://{}?mode=rwc", path.display()),
        max_connections: 1,
        min_connections: 1,
        acquire_timeout: Duration::from_secs(5),
        idle_timeout: Duration::from_secs(60),
        max_lifetime: Duration::from_secs(300),
    })
    .await
    .expect("create the SQLite pool");
    rustrak::db::run_migrations(&pool)
        .await
        .expect("run migrations");
    pool
}

/// `count` logs ingested `days_ago` days back, each with a `body_bytes` body.
#[cfg(feature = "sqlite")]
async fn seed_old_logs(
    pool: &DbPool,
    project_id: i32,
    days_ago: i64,
    count: usize,
    body_bytes: usize,
) {
    let old = Utc::now() - chrono::Duration::days(days_ago);
    let body = "x".repeat(body_bytes);
    let mut tx = pool.begin().await.unwrap();
    for _ in 0..count {
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
        .bind(&body)
        .bind(json!({}))
        .bind(old)
        .bind(old)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
}

/// `(page_count, freelist_count)`.
#[cfg(feature = "sqlite")]
async fn pages(pool: &DbPool) -> (i64, i64) {
    let total: i64 = sqlx::query_scalar("PRAGMA page_count")
        .fetch_one(pool)
        .await
        .unwrap();
    let free: i64 = sqlx::query_scalar("PRAGMA freelist_count")
        .fetch_one(pool)
        .await
        .unwrap();
    (total, free)
}

#[cfg(feature = "sqlite")]
fn logs_retention(days: i64) -> rustrak::models::RetentionSettings {
    serde_json::from_value(json!({"defaults": {"logs_days": days}})).unwrap()
}

#[cfg(feature = "sqlite")]
#[actix_web::test]
async fn a_new_sqlite_database_can_give_space_back() {
    let dir = tempfile::tempdir().unwrap();
    let pool = production_sqlite(&dir.path().join("rustrak.db")).await;

    let auto_vacuum: i64 = sqlx::query_scalar("PRAGMA auto_vacuum")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        auto_vacuum, 2,
        "new databases are created with auto_vacuum = INCREMENTAL"
    );
}

#[cfg(feature = "sqlite")]
#[actix_web::test]
async fn a_cleanup_gives_the_space_it_freed_back_to_the_filesystem() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rustrak.db");
    let pool = production_sqlite(&path).await;
    let project = create_project(&pool, "reclaim").await;
    // About 8 MiB of logs, all older than the window.
    seed_old_logs(&pool, project, 10, 2_000, 4_096).await;
    let (before, _) = pages(&pool).await;
    let file_before = std::fs::metadata(&path).unwrap().len();

    let cleanup = CleanupJob::default();
    cleanup
        .run_retention(pool.clone(), logs_retention(1), test_store())
        .await
        .unwrap();

    assert_eq!(log_count(&pool).await, 0);
    let (after, free) = pages(&pool).await;
    assert_eq!(free, 0, "every freed page went back to the filesystem");
    assert!(
        after * 4 < before,
        "the database shrank: {before} pages before, {after} after"
    );
    let file_after = std::fs::metadata(&path).unwrap().len();
    assert!(
        file_after * 4 < file_before,
        "the file shrank: {file_before} bytes before, {file_after} after"
    );
}

#[cfg(feature = "sqlite")]
#[actix_web::test]
async fn a_reader_blocking_the_checkpoint_means_nothing_has_left_the_file_yet() {
    use sqlx::Connection;
    let dir = tempfile::tempdir().unwrap();
    // Incremental, WAL and the shipped pool's busy timeout: a checkpoint that
    // waited on the reader would hold every writer back for those 5 seconds.
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(dir.path().join("rustrak.db"))
        .create_if_missing(true)
        .auto_vacuum(sqlx::sqlite::SqliteAutoVacuum::Incremental)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5));
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options.clone())
        .await
        .unwrap();
    rustrak::db::run_migrations(&pool).await.unwrap();
    let project = create_project(&pool, "busy").await;
    seed_old_logs(&pool, project, 10, 500, 4_096).await;

    // A reader on the snapshot before the delete, open across the reclaim.
    let mut reader = sqlx::SqliteConnection::connect_with(&options)
        .await
        .unwrap();
    let mut read = reader.begin().await.unwrap();
    let _: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM logs")
        .fetch_one(&mut *read)
        .await
        .unwrap();
    sqlx::query("DELETE FROM logs")
        .execute(&pool)
        .await
        .unwrap();

    let started = std::time::Instant::now();
    let released = rustrak::services::StorageService::reclaim_free_space(&pool)
        .await
        .unwrap();

    assert!(
        started.elapsed() < Duration::from_secs(2),
        "the checkpoint does not wait on the reader: {:?}",
        started.elapsed()
    );
    assert_eq!(released, 0, "the checkpoint could not shrink the file yet");
    let (_, free) = pages(&pool).await;
    assert_eq!(free, 0, "the pages were freed all the same");
    read.rollback().await.unwrap();
}

#[cfg(feature = "sqlite")]
#[actix_web::test]
async fn a_database_created_before_incremental_mode_keeps_its_freed_pages() {
    // TestDb's file is created without auto_vacuum, like a database a server
    // created before this mode existed: freed pages stay on the freelist for
    // reuse, and releasing them needs a one-off VACUUM.
    let db = TestDb::new().await;
    let project = create_project(&db.pool, "legacy").await;
    seed_old_logs(&db.pool, project, 10, 500, 4_096).await;

    let cleanup = CleanupJob::default();
    cleanup
        .run_retention(db.pool.clone(), logs_retention(1), test_store())
        .await
        .unwrap();

    assert_eq!(log_count(&db.pool).await, 0);
    assert_eq!(
        rustrak::services::StorageService::reclaim_free_space(&db.pool)
            .await
            .unwrap(),
        0
    );
    let (_, free) = pages(&db.pool).await;
    assert!(free > 0, "the freed pages stay on the freelist");
}

// =============================================================================
// SQLite: compacting a database created before incremental mode
// =============================================================================

/// `GET /api/storage/compaction` as `token`.
async fn get_compaction<S>(app: &S, token: &str) -> actix_web::dev::ServiceResponse
where
    S: actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
{
    let (k, v) = bearer(token);
    let req = test::TestRequest::get()
        .uri("/api/storage/compaction")
        .insert_header((k, v))
        .to_request();
    test::call_service(app, req).await
}

/// `POST /api/storage/compaction` as `token`.
async fn post_compaction<S>(app: &S, token: &str) -> actix_web::dev::ServiceResponse
where
    S: actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
{
    let (k, v) = bearer(token);
    let req = test::TestRequest::post()
        .uri("/api/storage/compaction")
        .insert_header((k, v))
        .to_request();
    test::call_service(app, req).await
}

#[cfg(feature = "sqlite")]
#[actix_web::test]
async fn an_old_database_reports_the_space_a_compaction_gives_back() {
    // TestDb's file has SQLite's default, auto_vacuum = NONE.
    let db = TestDb::new().await;
    let admin = seed_user(&db.pool, "admin@x.com", UserRole::Admin).await;
    let token = token_for(&db.pool, admin.id).await;
    let project = create_project(&db.pool, "legacy").await;
    seed_old_logs(&db.pool, project, 10, 500, 4_096).await;
    CleanupJob::default()
        .run_retention(db.pool.clone(), logs_retention(1), test_store())
        .await
        .unwrap();
    let app = build_app!(db.pool.clone(), create_test_config());

    let resp = get_compaction(&app, &token).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["needed"], true, "the file never shrinks on its own");
    let reclaimable = body["reclaimable_bytes"].as_i64().unwrap();
    let live = body["live_bytes"].as_i64().unwrap();
    assert!(reclaimable > 1024 * 1024, "the deleted logs: {reclaimable}");
    assert_eq!(body["file_bytes"].as_i64().unwrap(), live + reclaimable);
    assert_eq!(body["required_disk_bytes"].as_i64().unwrap(), 2 * live);
    assert!(body["free_disk_bytes"].as_i64().unwrap() > 0);
}

#[cfg(feature = "sqlite")]
#[actix_web::test]
async fn compacting_an_old_database_lets_it_give_space_back_from_then_on() {
    let db = TestDb::new().await;
    let admin = seed_user(&db.pool, "admin@x.com", UserRole::Admin).await;
    let token = token_for(&db.pool, admin.id).await;
    let project = create_project(&db.pool, "legacy").await;
    seed_old_logs(&db.pool, project, 10, 500, 4_096).await;
    CleanupJob::default()
        .run_retention(db.pool.clone(), logs_retention(1), test_store())
        .await
        .unwrap();
    let (before, _) = pages(&db.pool).await;
    let app = build_app!(db.pool.clone(), create_test_config());

    let resp = post_compaction(&app, &token).await;
    assert_eq!(
        resp.status(),
        202,
        "the compaction starts in the background"
    );
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["state"], "running");
    assert_eq!(body["kind"], "compaction");

    let done = wait_for_cleanup(&app, &token).await;
    assert_eq!(done["state"], "completed", "{done}");
    assert_eq!(done["kind"], "compaction");
    let auto_vacuum: i64 = sqlx::query_scalar("PRAGMA auto_vacuum")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(auto_vacuum, 2, "now in incremental mode");
    let (after, free) = pages(&db.pool).await;
    assert_eq!(free, 0, "the freed pages left the file");
    assert!(after * 4 < before, "{before} pages before, {after} after");

    let body: Value = test::read_body_json(get_compaction(&app, &token).await).await;
    assert_eq!(
        body["needed"], false,
        "a second compaction is never offered"
    );
}

#[cfg(feature = "sqlite")]
#[actix_web::test]
async fn a_new_database_is_never_compacted() {
    let dir = tempfile::tempdir().unwrap();
    let pool = production_sqlite(&dir.path().join("rustrak.db")).await;
    let admin = seed_user(&pool, "admin@x.com", UserRole::Admin).await;
    let token = token_for(&pool, admin.id).await;
    let app = build_app!(pool.clone(), create_test_config());

    let body: Value = test::read_body_json(get_compaction(&app, &token).await).await;
    assert_eq!(body["needed"], false, "it already gives space back");
    assert_eq!(post_compaction(&app, &token).await.status(), 400);
}

#[cfg(feature = "postgres")]
#[actix_web::test]
async fn postgres_never_needs_a_compaction() {
    let db = TestDb::new().await;
    let admin = seed_user(&db.pool, "admin@x.com", UserRole::Admin).await;
    let token = token_for(&db.pool, admin.id).await;
    let app = build_app!(db.pool.clone(), create_test_config());

    let body: Value = test::read_body_json(get_compaction(&app, &token).await).await;
    assert_eq!(body["needed"], false, "autovacuum gives the space back");
    assert_eq!(post_compaction(&app, &token).await.status(), 400);
}

#[actix_web::test]
async fn compaction_is_admin_only() {
    let db = TestDb::new().await;
    let member = seed_user(&db.pool, "member@x.com", UserRole::Member).await;
    let token = token_for(&db.pool, member.id).await;
    let app = build_app!(db.pool.clone(), create_test_config());

    assert_eq!(get_compaction(&app, &token).await.status(), 403);
    assert_eq!(post_compaction(&app, &token).await.status(), 403);
}

#[actix_web::test]
async fn a_compaction_needs_twice_the_live_data_free() {
    let compaction = |free| rustrak::models::Compaction {
        needed: true,
        file_bytes: 300,
        live_bytes: 100,
        reclaimable_bytes: 200,
        required_disk_bytes: 200,
        free_disk_bytes: free,
    };
    assert!(compaction(Some(200)).has_room());
    assert!(
        !compaction(Some(199)).has_room(),
        "VACUUM would fill the disk"
    );
    assert!(compaction(None).has_room(), "unmeasurable: SQLite decides");
}
