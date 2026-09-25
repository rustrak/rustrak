//! Integration tests for Rate Limiting
//!
//! Tests that rate limiting is enforced during event ingestion.

use crate::common::TestDb;
use actix_web::{test, web, App};
use chrono::{Duration, SubsecRound, Utc};
use rustrak::config::{Config, DashboardConfig, DatabaseConfig, RateLimitConfig};
use rustrak::routes;
use rustrak::services::{
    DbSourceMapProvider, LocalSourceMapStore, ProjectService, RateLimitService, SourceMapProvider,
    SourceMapStore,
};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration as StdDuration;
use uuid::Uuid;

/// Creates a test config with given rate limits
fn create_test_config(rate_limit: RateLimitConfig) -> Config {
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
        rate_limit,
        security: rustrak::config::SecurityConfig {
            ssl_proxy: false,
            session_secret_key: None,
        },
        ingest_dir: Some("/tmp/rustrak_test_ratelimit".to_string()),
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

/// Default rate limit config (high limits for normal tests)
fn default_rate_limit_config() -> RateLimitConfig {
    RateLimitConfig {
        max_events_per_minute: 1000,
        max_events_per_hour: 10000,
        max_events_per_project_per_minute: 500,
        max_events_per_project_per_hour: 5000,
    }
}

/// Creates a test project and returns its sentry_key
async fn create_test_project(pool: &rustrak::db::DbPool, name: &str) -> (i32, String) {
    let project = ProjectService::create(
        pool,
        rustrak::models::CreateProject {
            name: name.to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .expect("Failed to create test project");
    (project.id, project.sentry_key.to_string())
}

/// Sets project quota exceeded until the given time
async fn set_project_quota_exceeded(
    pool: &rustrak::db::DbPool,
    project_id: i32,
    until: chrono::DateTime<Utc>,
) {
    sqlx::query(
        "UPDATE projects SET quota_exceeded_until = $1, next_quota_check = 1 WHERE id = $2",
    )
    .bind(until)
    .bind(project_id)
    .execute(pool)
    .await
    .expect("Failed to set project quota");
}

/// Sets installation quota exceeded until the given time
async fn set_installation_quota_exceeded(pool: &rustrak::db::DbPool, until: chrono::DateTime<Utc>) {
    sqlx::query(
        "UPDATE installation SET quota_exceeded_until = $1, next_quota_check = 1 WHERE id = 1",
    )
    .bind(until)
    .execute(pool)
    .await
    .expect("Failed to set installation quota");
}

#[actix_web::test]
async fn stale_quota_cache_is_refreshed_before_the_next_ingest() {
    let db = TestDb::new().await;
    let (project_id, _) = create_test_project(&db.pool, "Stale Quota Cache").await;
    let config = default_rate_limit_config();
    sqlx::query("UPDATE installation SET next_quota_check = 1 WHERE id = 1")
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE projects SET next_quota_check = 1 WHERE id = $1")
        .bind(project_id)
        .execute(&db.pool)
        .await
        .unwrap();
    // Whole seconds: Postgres keeps microseconds, so a nanosecond `Utc::now()`
    // would not compare equal once it comes back.
    let stale_until = (Utc::now() + Duration::minutes(10)).trunc_subsecs(0);
    set_project_quota_exceeded(&db.pool, project_id, stale_until).await;
    sqlx::query("UPDATE projects SET next_quota_check = 0 WHERE id = $1")
        .bind(project_id)
        .execute(&db.pool)
        .await
        .unwrap();
    // The row as the ingest extractor loads it at the start of the request:
    // `check_quota` reads the project's quota state from this, not from a
    // second lookup.
    let project = ProjectService::get_by_id(&db.pool, project_id)
        .await
        .unwrap();
    assert_eq!(project.quota_exceeded_until, Some(stale_until));

    assert!(RateLimitService::check_quota(&db.pool, &project, &config)
        .await
        .unwrap()
        .is_none());
    let refreshed = ProjectService::get_by_id(&db.pool, project_id)
        .await
        .unwrap();
    assert!(refreshed.quota_exceeded_until.is_none());
    assert!(refreshed.next_quota_check > i64::from(refreshed.digested_event_count));
}

/// Creates a minimal valid Sentry envelope
fn create_envelope(event_id: &str, event_json: &str) -> Vec<u8> {
    let envelope = format!(
        r#"{{"event_id":"{}"}}
{{"type":"event","length":{}}}
{}"#,
        event_id,
        event_json.len(),
        event_json
    );
    envelope.into_bytes()
}

// =============================================================================
// Project Rate Limit Tests
// =============================================================================

#[actix_web::test]
async fn test_rate_limit_project_exceeded_returns_429() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_test_project(&db.pool, "Rate Limited Project").await;
    let config = create_test_config(default_rate_limit_config());

    // Set project quota exceeded for 60 seconds from now
    let exceeded_until = Utc::now() + Duration::seconds(60);
    set_project_quota_exceeded(&db.pool, project_id, exceeded_until).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data({
                let store: Arc<dyn SourceMapStore> =
                    Arc::new(LocalSourceMapStore::new("/tmp/test_sourcemaps"));
                let provider: Arc<dyn SourceMapProvider> =
                    Arc::new(DbSourceMapProvider::new(db.pool.clone(), store));
                web::Data::new(provider)
            })
            .app_data(web::Data::new(
                rustrak::digest::processors::Processors::new(
                    rustrak::ingest::get_ingest_dir(config.ingest_dir.as_deref()),
                    config.rate_limit.clone(),
                    crate::common::null_sourcemap_provider(),
                    None,
                ),
            ))
            .configure(routes::ingest::configure),
    )
    .await;

    let event_id = Uuid::new_v4().to_string().replace("-", "");
    let event_json = json!({"event_id": event_id}).to_string();
    let envelope = create_envelope(&event_id, &event_json);

    let req = test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", project_id))
        .insert_header((
            "X-Sentry-Auth",
            format!("Sentry sentry_key={}, sentry_version=7", sentry_key),
        ))
        .set_payload(envelope)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 429);

    // Check Retry-After header
    let retry_after = resp.headers().get("retry-after");
    assert!(retry_after.is_some());
    let retry_after_value: u64 = retry_after
        .unwrap()
        .to_str()
        .unwrap()
        .parse()
        .expect("Retry-After should be a number");
    assert!(retry_after_value > 0);
    assert!(retry_after_value <= 60);
}

#[actix_web::test]
async fn test_rate_limit_project_expired_allows_request() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_test_project(&db.pool, "Rate Limit Expired").await;
    let config = create_test_config(default_rate_limit_config());

    // Set quota exceeded to past (already expired)
    let exceeded_until = Utc::now() - Duration::seconds(10);
    set_project_quota_exceeded(&db.pool, project_id, exceeded_until).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data({
                let store: Arc<dyn SourceMapStore> =
                    Arc::new(LocalSourceMapStore::new("/tmp/test_sourcemaps"));
                let provider: Arc<dyn SourceMapProvider> =
                    Arc::new(DbSourceMapProvider::new(db.pool.clone(), store));
                web::Data::new(provider)
            })
            .app_data(web::Data::new(
                rustrak::digest::processors::Processors::new(
                    rustrak::ingest::get_ingest_dir(config.ingest_dir.as_deref()),
                    config.rate_limit.clone(),
                    crate::common::null_sourcemap_provider(),
                    None,
                ),
            ))
            .configure(routes::ingest::configure),
    )
    .await;

    let event_id = Uuid::new_v4().to_string().replace("-", "");
    let event_json = json!({"event_id": event_id}).to_string();
    let envelope = create_envelope(&event_id, &event_json);

    let req = test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", project_id))
        .insert_header((
            "X-Sentry-Auth",
            format!("Sentry sentry_key={}, sentry_version=7", sentry_key),
        ))
        .set_payload(envelope)
        .to_request();

    let resp = test::call_service(&app, req).await;
    // Should be allowed now that quota expired
    assert!(resp.status().is_success());
}

// =============================================================================
// Installation Rate Limit Tests
// =============================================================================

#[actix_web::test]
async fn test_rate_limit_installation_exceeded_returns_429() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_test_project(&db.pool, "Installation Rate Limited").await;
    let config = create_test_config(default_rate_limit_config());

    // Set installation (global) quota exceeded
    let exceeded_until = Utc::now() + Duration::seconds(30);
    set_installation_quota_exceeded(&db.pool, exceeded_until).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data({
                let store: Arc<dyn SourceMapStore> =
                    Arc::new(LocalSourceMapStore::new("/tmp/test_sourcemaps"));
                let provider: Arc<dyn SourceMapProvider> =
                    Arc::new(DbSourceMapProvider::new(db.pool.clone(), store));
                web::Data::new(provider)
            })
            .app_data(web::Data::new(
                rustrak::digest::processors::Processors::new(
                    rustrak::ingest::get_ingest_dir(config.ingest_dir.as_deref()),
                    config.rate_limit.clone(),
                    crate::common::null_sourcemap_provider(),
                    None,
                ),
            ))
            .configure(routes::ingest::configure),
    )
    .await;

    let event_id = Uuid::new_v4().to_string().replace("-", "");
    let event_json = json!({"event_id": event_id}).to_string();
    let envelope = create_envelope(&event_id, &event_json);

    let req = test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", project_id))
        .insert_header((
            "X-Sentry-Auth",
            format!("Sentry sentry_key={}, sentry_version=7", sentry_key),
        ))
        .set_payload(envelope)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 429);
}

#[actix_web::test]
async fn test_rate_limit_response_body() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_test_project(&db.pool, "Rate Limit Response Body").await;
    let config = create_test_config(default_rate_limit_config());

    // Set quota exceeded
    let exceeded_until = Utc::now() + Duration::seconds(45);
    set_project_quota_exceeded(&db.pool, project_id, exceeded_until).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data({
                let store: Arc<dyn SourceMapStore> =
                    Arc::new(LocalSourceMapStore::new("/tmp/test_sourcemaps"));
                let provider: Arc<dyn SourceMapProvider> =
                    Arc::new(DbSourceMapProvider::new(db.pool.clone(), store));
                web::Data::new(provider)
            })
            .app_data(web::Data::new(
                rustrak::digest::processors::Processors::new(
                    rustrak::ingest::get_ingest_dir(config.ingest_dir.as_deref()),
                    config.rate_limit.clone(),
                    crate::common::null_sourcemap_provider(),
                    None,
                ),
            ))
            .configure(routes::ingest::configure),
    )
    .await;

    let event_id = Uuid::new_v4().to_string().replace("-", "");
    let event_json = json!({"event_id": event_id}).to_string();
    let envelope = create_envelope(&event_id, &event_json);

    let req = test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", project_id))
        .insert_header((
            "X-Sentry-Auth",
            format!("Sentry sentry_key={}, sentry_version=7", sentry_key),
        ))
        .set_payload(envelope)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 429);

    // Check response body
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["error"], "rate_limit_exceeded");
    assert!(body["retry_after"].as_u64().is_some());
}

// =============================================================================
// CORS with Rate Limiting Tests
// =============================================================================

#[actix_web::test]
async fn test_rate_limit_429_has_cors_headers() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_test_project(&db.pool, "Rate Limit CORS").await;
    let config = create_test_config(default_rate_limit_config());

    // Set quota exceeded
    let exceeded_until = Utc::now() + Duration::seconds(60);
    set_project_quota_exceeded(&db.pool, project_id, exceeded_until).await;

    // CORS is handled by middleware, so we need to include it in the test
    let cors = actix_cors::Cors::default()
        .allow_any_origin()
        .allowed_methods(vec!["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"])
        .allowed_headers(vec![
            actix_web::http::header::AUTHORIZATION,
            actix_web::http::header::ACCEPT,
            actix_web::http::header::CONTENT_TYPE,
            actix_web::http::header::CONTENT_ENCODING,
            actix_web::http::header::HeaderName::from_static("x-sentry-auth"),
        ])
        .max_age(3600);

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data({
                let store: Arc<dyn SourceMapStore> =
                    Arc::new(LocalSourceMapStore::new("/tmp/test_sourcemaps"));
                let provider: Arc<dyn SourceMapProvider> =
                    Arc::new(DbSourceMapProvider::new(db.pool.clone(), store));
                web::Data::new(provider)
            })
            .wrap(cors)
            .app_data(web::Data::new(
                rustrak::digest::processors::Processors::new(
                    rustrak::ingest::get_ingest_dir(config.ingest_dir.as_deref()),
                    config.rate_limit.clone(),
                    crate::common::null_sourcemap_provider(),
                    None,
                ),
            ))
            .configure(routes::ingest::configure),
    )
    .await;

    let event_id = Uuid::new_v4().to_string().replace("-", "");
    let event_json = json!({"event_id": event_id}).to_string();
    let envelope = create_envelope(&event_id, &event_json);

    let req = test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", project_id))
        .insert_header((
            "X-Sentry-Auth",
            format!("Sentry sentry_key={}, sentry_version=7", sentry_key),
        ))
        .insert_header(("Origin", "https://example.com"))
        .set_payload(envelope)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 429);

    // Even 429 responses need CORS headers for browser clients
    let headers = resp.headers();
    // CORS reflects the Origin header back
    assert_eq!(
        headers.get("access-control-allow-origin").unwrap(),
        "https://example.com"
    );
}

// =============================================================================
// No Rate Limit (Normal Operation) Tests
// =============================================================================

#[actix_web::test]
async fn test_no_rate_limit_allows_request() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_test_project(&db.pool, "No Rate Limit").await;
    let config = create_test_config(default_rate_limit_config());

    // Don't set any quota limits

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data({
                let store: Arc<dyn SourceMapStore> =
                    Arc::new(LocalSourceMapStore::new("/tmp/test_sourcemaps"));
                let provider: Arc<dyn SourceMapProvider> =
                    Arc::new(DbSourceMapProvider::new(db.pool.clone(), store));
                web::Data::new(provider)
            })
            .app_data(web::Data::new(
                rustrak::digest::processors::Processors::new(
                    rustrak::ingest::get_ingest_dir(config.ingest_dir.as_deref()),
                    config.rate_limit.clone(),
                    crate::common::null_sourcemap_provider(),
                    None,
                ),
            ))
            .configure(routes::ingest::configure),
    )
    .await;

    let event_id = Uuid::new_v4().to_string().replace("-", "");
    let event_json = json!({"event_id": event_id}).to_string();
    let envelope = create_envelope(&event_id, &event_json);

    let req = test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", project_id))
        .insert_header((
            "X-Sentry-Auth",
            format!("Sentry sentry_key={}, sentry_version=7", sentry_key),
        ))
        .set_payload(envelope)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
}

// =============================================================================
// Multiple Projects Rate Limit Tests
// =============================================================================

#[actix_web::test]
async fn test_rate_limit_affects_only_specific_project() {
    let db = TestDb::new().await;
    let (project_a_id, sentry_key_a) =
        create_test_project(&db.pool, "Project A Rate Limited").await;
    let (project_b_id, sentry_key_b) = create_test_project(&db.pool, "Project B Not Limited").await;
    let config = create_test_config(default_rate_limit_config());

    // Only rate limit Project A
    let exceeded_until = Utc::now() + Duration::seconds(60);
    set_project_quota_exceeded(&db.pool, project_a_id, exceeded_until).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data({
                let store: Arc<dyn SourceMapStore> =
                    Arc::new(LocalSourceMapStore::new("/tmp/test_sourcemaps"));
                let provider: Arc<dyn SourceMapProvider> =
                    Arc::new(DbSourceMapProvider::new(db.pool.clone(), store));
                web::Data::new(provider)
            })
            .app_data(web::Data::new(
                rustrak::digest::processors::Processors::new(
                    rustrak::ingest::get_ingest_dir(config.ingest_dir.as_deref()),
                    config.rate_limit.clone(),
                    crate::common::null_sourcemap_provider(),
                    None,
                ),
            ))
            .configure(routes::ingest::configure),
    )
    .await;

    // Project A should be rate limited
    let event_id_a = Uuid::new_v4().to_string().replace("-", "");
    let event_json_a = json!({"event_id": event_id_a}).to_string();
    let envelope_a = create_envelope(&event_id_a, &event_json_a);

    let req_a = test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", project_a_id))
        .insert_header((
            "X-Sentry-Auth",
            format!("Sentry sentry_key={}, sentry_version=7", sentry_key_a),
        ))
        .set_payload(envelope_a)
        .to_request();

    let resp_a = test::call_service(&app, req_a).await;
    assert_eq!(resp_a.status(), 429);

    // Project B should NOT be rate limited
    let event_id_b = Uuid::new_v4().to_string().replace("-", "");
    let event_json_b = json!({"event_id": event_id_b}).to_string();
    let envelope_b = create_envelope(&event_id_b, &event_json_b);

    let req_b = test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", project_b_id))
        .insert_header((
            "X-Sentry-Auth",
            format!("Sentry sentry_key={}, sentry_version=7", sentry_key_b),
        ))
        .set_payload(envelope_b)
        .to_request();

    let resp_b = test::call_service(&app, req_b).await;
    assert!(resp_b.status().is_success());
}

#[tokio::test]
async fn test_check_quota_degrades_to_stale_state_when_refresh_fails() {
    // Quota refresh is derived bookkeeping. When it cannot run (e.g. SQLite
    // busy under write load), ingestion must proceed on the stale quota state
    // instead of failing the request: the digest side already treats the same
    // refresh as best-effort, and a 5xx here would reject an event that
    // nothing else objects to.
    let db = TestDb::new().await;
    let (project_id, _key) = create_test_project(&db.pool, "Stale Quota Project").await;
    let project = ProjectService::get_by_id(&db.pool, project_id)
        .await
        .expect("project lookup must succeed");
    let config = RateLimitConfig {
        max_events_per_minute: 1000,
        max_events_per_hour: 10000,
        max_events_per_project_per_minute: 500,
        max_events_per_project_per_hour: 5000,
    };

    // Make the derived state stale so check_quota attempts a refresh...
    sqlx::query(
        "UPDATE installation SET next_quota_check = 1, digested_event_count = 5 WHERE id = 1",
    )
    .execute(&db.pool)
    .await
    .expect("staleness setup must succeed");
    // ...and make that refresh fail deterministically.
    sqlx::query(
        "CREATE TRIGGER fail_quota_refresh BEFORE UPDATE OF next_quota_check ON installation \
         BEGIN SELECT RAISE(ABORT, 'injected: quota refresh unavailable'); END",
    )
    .execute(&db.pool)
    .await
    .expect("trigger creation must succeed");

    let verdict = RateLimitService::check_quota(&db.pool, &project, &config)
        .await
        .expect("a failed quota refresh must not fail the ingest request");
    assert!(
        verdict.is_none(),
        "stale non-exceeded quota state must admit the event"
    );
}
