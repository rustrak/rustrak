//! Check-ins arriving from SDKs: the `check_in` envelope item and Relay's
//! HTTP check-in endpoint (`/api/{project_id}/cron/{slug}/{sentry_key}/`),
//! through to the issue a failure opens and the recovery that resolves it.

use crate::common::TestDb;
use actix_web::dev::{Service, ServiceResponse};
use actix_web::{test, web, App};
use base64::Engine as _;
use rustrak::config::{Config, DashboardConfig, DatabaseConfig, RateLimitConfig};
use rustrak::db::DbPool;
use rustrak::digest::processors::Processors;
use rustrak::models::monitor::CheckInStatus;
use rustrak::routes;
use rustrak::services::monitor::MonitorService;
use rustrak::services::ProjectService;
use std::time::Duration;
use uuid::Uuid;

fn config(ingest_dir: &std::path::Path) -> Config {
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
        ingest_dir: Some(ingest_dir.to_string_lossy().to_string()),
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

struct Setup {
    db: TestDb,
    project_id: i32,
    key: String,
    _ingest: tempfile::TempDir,
}

async fn setup() -> Setup {
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        rustrak::models::CreateProject {
            name: "Crons".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();
    Setup {
        db,
        project_id: project.id,
        key: project.sentry_key.simple().to_string(),
        _ingest: tempfile::tempdir().unwrap(),
    }
}

async fn app(
    s: &Setup,
) -> impl Service<actix_http::Request, Response = ServiceResponse, Error = actix_web::Error> {
    let config = config(s._ingest.path());
    test::init_service(
        App::new()
            .app_data(web::Data::new(s.db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data(web::Data::new(Processors::new(
                s._ingest.path().to_path_buf(),
                config.rate_limit.clone(),
                crate::common::null_sourcemap_provider(),
                None,
            )))
            .configure(routes::ingest::configure),
    )
    .await
}

fn envelope(check_in: &str) -> Vec<u8> {
    format!(
        "{{}}\n{{\"type\":\"check_in\",\"length\":{}}}\n{}",
        check_in.len(),
        check_in
    )
    .into_bytes()
}

fn post_envelope(s: &Setup, body: Vec<u8>) -> actix_http::Request {
    test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", s.project_id))
        .insert_header((
            "X-Sentry-Auth",
            format!("Sentry sentry_key={}, sentry_version=7", s.key),
        ))
        .set_payload(body)
        .to_request()
}

const UPSERT: &str = r#"{"monitor_slug":"nightly","status":"ok","monitor_config":{"schedule":{"type":"crontab","value":"0 3 * * *"}}}"#;

async fn issue_status(pool: &DbPool, project_id: i32) -> Vec<String> {
    #[cfg(feature = "postgres")]
    const Q: &str = "SELECT status FROM issues WHERE project_id = $1";
    #[cfg(not(feature = "postgres"))]
    const Q: &str = "SELECT status FROM issues WHERE project_id = ?";
    sqlx::query_scalar(Q)
        .bind(project_id)
        .fetch_all(pool)
        .await
        .unwrap()
}

#[actix_web::test]
async fn check_in_envelope_upserts_the_monitor() {
    let s = setup().await;
    let app = app(&s).await;

    let resp = test::call_service(&app, post_envelope(&s, envelope(UPSERT))).await;
    assert_eq!(resp.status(), 200);

    let monitor = MonitorService::get_by_slug(&s.db.pool, s.project_id, "nightly")
        .await
        .unwrap();
    assert_eq!(
        monitor.environments[0].recent_checkins[0].status,
        CheckInStatus::Ok
    );
}

#[actix_web::test]
async fn malformed_check_in_is_dropped_without_failing_the_envelope() {
    let s = setup().await;
    let app = app(&s).await;

    // No status: Relay rejects the item as invalid JSON, not the envelope.
    let resp = test::call_service(
        &app,
        post_envelope(&s, envelope(r#"{"monitor_slug":"nightly"}"#)),
    )
    .await;
    assert_eq!(resp.status(), 200);
    assert!(MonitorService::list(&s.db.pool, s.project_id)
        .await
        .unwrap()
        .is_empty());
}

#[actix_web::test]
async fn failed_check_in_opens_an_issue_and_recovery_resolves_it() {
    let s = setup().await;
    let app = app(&s).await;
    test::call_service(&app, post_envelope(&s, envelope(UPSERT))).await;

    let failed = r#"{"monitor_slug":"nightly","status":"error"}"#;
    let resp = test::call_service(&app, post_envelope(&s, envelope(failed))).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(issue_status(&s.db.pool, s.project_id).await, ["unresolved"]);

    let title: (String, String) = sqlx::query_as(if cfg!(feature = "postgres") {
        "SELECT calculated_type, calculated_value FROM issues WHERE project_id = $1"
    } else {
        "SELECT calculated_type, calculated_value FROM issues WHERE project_id = ?"
    })
    .bind(s.project_id)
    .fetch_one(&s.db.pool)
    .await
    .unwrap();
    assert_eq!(title.0, "Cron failure: nightly");
    assert_eq!(title.1, "A failed check-in was detected");

    let ok = r#"{"monitor_slug":"nightly","status":"ok"}"#;
    test::call_service(&app, post_envelope(&s, envelope(ok))).await;
    assert_eq!(issue_status(&s.db.pool, s.project_id).await, ["resolved"]);
}

#[actix_web::test]
async fn http_get_check_in_with_key_in_path() {
    let s = setup().await;
    let app = app(&s).await;
    test::call_service(&app, post_envelope(&s, envelope(UPSERT))).await;

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/{}/cron/nightly/{}/?status=in_progress&environment=prod",
            s.project_id, s.key
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 202);

    let (rows, _) =
        MonitorService::list_checkins(&s.db.pool, s.project_id, "nightly", Some("prod"), 1, 10)
            .await
            .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, CheckInStatus::InProgress);
}

#[actix_web::test]
async fn http_post_json_upserts_with_slug_from_path() {
    let s = setup().await;
    let app = app(&s).await;

    let body = r#"{"monitor_slug":"ignored","status":"in_progress","monitor_config":{"schedule":{"type":"interval","value":2,"unit":"hour"},"checkin_margin":5}}"#;
    let req = test::TestRequest::post()
        .uri(&format!("/api/{}/cron/from-path/{}/", s.project_id, s.key))
        .insert_header(("Content-Type", "application/json"))
        .set_payload(body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 202);

    let monitor = MonitorService::get_by_slug(&s.db.pool, s.project_id, "from-path")
        .await
        .unwrap();
    assert_eq!(monitor.config.checkin_margin, 5);
}

#[actix_web::test]
async fn http_check_in_accepts_basic_auth_without_key_in_path() {
    let s = setup().await;
    let app = app(&s).await;
    test::call_service(&app, post_envelope(&s, envelope(UPSERT))).await;

    let basic = base64::engine::general_purpose::STANDARD.encode(format!("{}:", s.key));
    let req = test::TestRequest::get()
        .uri(&format!("/api/{}/cron/nightly?status=ok", s.project_id))
        .insert_header(("Authorization", format!("Basic {basic}")))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 202);
}

#[actix_web::test]
async fn http_check_in_with_wrong_key_is_unauthorized() {
    let s = setup().await;
    let app = app(&s).await;

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/{}/cron/nightly/{}/?status=ok",
            s.project_id,
            Uuid::new_v4().simple()
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn http_check_in_for_unknown_monitor_says_why() {
    let s = setup().await;
    let app = app(&s).await;

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/{}/cron/nobody/{}/?status=ok",
            s.project_id, s.key
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn sweep_failures_open_an_issue() {
    let s = setup().await;
    let app = app(&s).await;
    test::call_service(&app, post_envelope(&s, envelope(UPSERT))).await;

    let processors = Processors::new(
        s._ingest.path().to_path_buf(),
        config(s._ingest.path()).rate_limit,
        crate::common::null_sourcemap_provider(),
        None,
    );
    // Two days on, the 03:00 run has certainly been missed.
    let later = chrono::Utc::now() + chrono::Duration::days(2);
    rustrak::routes::ingest::run_monitor_sweep(&processors, &s.db.pool, later)
        .await
        .unwrap();

    assert_eq!(issue_status(&s.db.pool, s.project_id).await, ["unresolved"]);
}

#[actix_web::test]
async fn a_result_racing_ahead_of_the_upsert_on_the_first_run_is_kept() {
    // `Sentry.withMonitor` sends in_progress (with monitor_config) and the
    // result (without it) as two concurrent requests. On a job's first run
    // the result can reach the server before the monitor exists.
    let s = setup().await;
    let app = app(&s).await;
    let id = Uuid::new_v4().simple().to_string();
    let start = format!(
        r#"{{"check_in_id":"{id}","monitor_slug":"racy","status":"in_progress","monitor_config":{{"schedule":{{"type":"crontab","value":"0 * * * *"}}}}}}"#
    );
    let failed = format!(r#"{{"check_in_id":"{id}","monitor_slug":"racy","status":"error"}}"#);

    let late_upsert = async {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        test::call_service(&app, post_envelope(&s, envelope(&start))).await
    };
    let early_result = test::call_service(&app, post_envelope(&s, envelope(&failed)));
    let (a, b) = tokio::join!(late_upsert, early_result);
    assert_eq!(a.status(), 200);
    assert_eq!(b.status(), 200);

    let (rows, _) = MonitorService::list_checkins(&s.db.pool, s.project_id, "racy", None, 1, 10)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, CheckInStatus::Error);
}

#[actix_web::test]
async fn the_monitor_sweep_runs_as_a_scheduled_job() {
    use rustrak::scheduler::engine::Scheduler;
    use rustrak::scheduler::jobs::MonitorSweepJob;

    let s = setup().await;
    let app = app(&s).await;
    test::call_service(&app, post_envelope(&s, envelope(UPSERT))).await;

    let processors = web::Data::new(Processors::new(
        s._ingest.path().to_path_buf(),
        config(s._ingest.path()).rate_limit,
        crate::common::null_sourcemap_provider(),
        None,
    ));
    let scheduler = Scheduler::new(vec![std::sync::Arc::new(MonitorSweepJob::new(processors))]);
    let later = chrono::Utc::now() + chrono::Duration::days(2);
    for now in [later, later + chrono::Duration::minutes(1)] {
        for handle in scheduler.tick(&s.db.pool, now).await.unwrap() {
            handle.await.unwrap();
        }
    }

    let state = Scheduler::job(&s.db.pool, MonitorSweepJob::KEY)
        .await
        .unwrap();
    assert_eq!(state.last_result, Some(serde_json::json!({"failures": 1})));
    assert_eq!(issue_status(&s.db.pool, s.project_id).await, ["unresolved"]);
}
