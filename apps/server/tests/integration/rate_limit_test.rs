//! Integration tests for Rate Limiting
//!
//! Tests that rate limiting is enforced during event ingestion.

use crate::common::create_ingest_project;
use crate::common::TestDb;
use actix_web::test;
use chrono::{Duration, SubsecRound, Utc};
use rustrak::config::{Config, RateLimitConfig};
use rustrak::services::{Pending, ProjectService, QuotaScope, RateLimitService};
use serde_json::json;
use uuid::Uuid;

fn create_test_config(rate_limit: RateLimitConfig) -> Config {
    Config {
        rate_limit,
        ..crate::common::config_with_ingest_dir(std::path::Path::new("/tmp/rustrak_test_ratelimit"))
    }
}

/// Fills the minute window that `until` falls in: the current one when
/// `until` is ahead, so the quota is full now; an earlier one when it has
/// passed, so the count belongs to a window that is over.
fn full_minute_window(until: chrono::DateTime<Utc>) -> i64 {
    let window = Utc::now().timestamp().div_euclid(60);
    if until > Utc::now() {
        window
    } else {
        window - 2
    }
}

/// Makes the project's quota full until the given time
async fn set_project_quota_exceeded(
    pool: &rustrak::db::DbPool,
    project_id: i32,
    until: chrono::DateTime<Utc>,
) {
    sqlx::query(
        "UPDATE projects SET quota_minute_window = $1, quota_minute_count = 1000000 WHERE id = $2",
    )
    .bind(full_minute_window(until))
    .bind(project_id)
    .execute(pool)
    .await
    .expect("Failed to set project quota");
}

/// Makes the installation's quota full until the given time
async fn set_installation_quota_exceeded(pool: &rustrak::db::DbPool, until: chrono::DateTime<Utc>) {
    sqlx::query(
        "UPDATE installation SET quota_minute_window = $1, quota_minute_count = 1000000 WHERE id = 1",
    )
    .bind(full_minute_window(until))
    .execute(pool)
    .await
    .expect("Failed to set installation quota");
}

#[actix_web::test]
async fn stale_quota_cache_is_refreshed_before_the_next_ingest() {
    let db = TestDb::new().await;
    let (project_id, _) = create_ingest_project(&db.pool, "Stale Quota Cache").await;
    let config = crate::common::rate_limit();
    // A closed-until left behind by an older server: the counters have room,
    // so it must not turn events away.
    let stale_until = (Utc::now() + Duration::minutes(10)).trunc_subsecs(0);
    sqlx::query("UPDATE projects SET quota_exceeded_until = $1 WHERE id = $2")
        .bind(stale_until)
        .bind(project_id)
        .execute(&db.pool)
        .await
        .unwrap();
    // The row as the ingest extractor loads it at the start of the request:
    // `check_quota` reads the project's quota state from this, not from a
    // second lookup.
    let (project, installation) = ProjectService::get_with_installation_quota(&db.pool, project_id)
        .await
        .unwrap();
    assert_eq!(project.quota_exceeded_until, Some(stale_until));

    assert!(
        RateLimitService::check_quota(&installation, &project, &config, Pending::default())
            .is_none()
    );
}

/// Ingest and digest read the project and the installation's quota in one
/// round trip: both rows come back as they are stored.
#[actix_web::test]
async fn the_project_comes_with_the_installation_quota() {
    let db = TestDb::new().await;
    let (project_id, _) = create_ingest_project(&db.pool, "One Round Trip").await;
    sqlx::query(
        "UPDATE installation SET quota_minute_window = 7, quota_minute_count = 3, \
         quota_hour_window = 11, quota_hour_count = 5 WHERE id = 1",
    )
    .execute(&db.pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE projects SET quota_minute_window = 2, quota_minute_count = 1 WHERE id = $1",
    )
    .bind(project_id)
    .execute(&db.pool)
    .await
    .unwrap();

    let (project, installation) = ProjectService::get_with_installation_quota(&db.pool, project_id)
        .await
        .unwrap();

    assert_eq!(project.id, project_id);
    assert_eq!(
        (
            project.quota.quota_minute_window,
            project.quota.quota_minute_count
        ),
        (2, 1)
    );
    assert_eq!(
        (
            installation.quota_minute_window,
            installation.quota_minute_count,
            installation.quota_hour_window,
            installation.quota_hour_count
        ),
        (7, 3, 11, 5)
    );
    assert!(matches!(
        ProjectService::get_with_installation_quota(&db.pool, project_id + 1000).await,
        Err(rustrak::error::AppError::NotFound(_))
    ));
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

#[actix_web::test]
async fn test_rate_limit_project_exceeded_returns_429() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "Rate Limited Project").await;
    let config = create_test_config(crate::common::rate_limit());

    // Set project quota exceeded for 60 seconds from now
    let exceeded_until = Utc::now() + Duration::seconds(60);
    set_project_quota_exceeded(&db.pool, project_id, exceeded_until).await;

    let app = crate::common::app_with_config(&db.pool, config.clone()).await;

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

    // Relay's own header, so SDKs back off exactly as they do against Sentry:
    // `retry_after:categories:scope`, no categories meaning all of them.
    let rate_limits = resp.headers().get("x-sentry-rate-limits").unwrap();
    assert_eq!(
        rate_limits.to_str().unwrap(),
        format!("{retry_after_value}::project")
    );
}

#[actix_web::test]
async fn test_rate_limit_project_expired_allows_request() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "Rate Limit Expired").await;
    let config = create_test_config(crate::common::rate_limit());

    // Set quota exceeded to past (already expired)
    let exceeded_until = Utc::now() - Duration::seconds(10);
    set_project_quota_exceeded(&db.pool, project_id, exceeded_until).await;

    let app = crate::common::app_with_config(&db.pool, config.clone()).await;

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

#[actix_web::test]
async fn test_rate_limit_installation_exceeded_returns_429() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) =
        create_ingest_project(&db.pool, "Installation Rate Limited").await;
    let config = create_test_config(crate::common::rate_limit());

    // Set installation (global) quota exceeded
    let exceeded_until = Utc::now() + Duration::seconds(30);
    set_installation_quota_exceeded(&db.pool, exceeded_until).await;

    let app = crate::common::app_with_config(&db.pool, config.clone()).await;

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

    // The installation is Relay's organization scope: one per server.
    let retry_after = resp.headers().get("retry-after").unwrap().to_str().unwrap();
    let rate_limits = resp.headers().get("x-sentry-rate-limits").unwrap();
    assert_eq!(
        rate_limits.to_str().unwrap(),
        format!("{retry_after}::organization")
    );
}

#[actix_web::test]
async fn test_rate_limit_response_body() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) =
        create_ingest_project(&db.pool, "Rate Limit Response Body").await;
    let config = create_test_config(crate::common::rate_limit());

    // Set quota exceeded
    let exceeded_until = Utc::now() + Duration::seconds(45);
    set_project_quota_exceeded(&db.pool, project_id, exceeded_until).await;

    let app = crate::common::app_with_config(&db.pool, config.clone()).await;

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

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["error"], "rate_limit_exceeded");
    assert!(body["retry_after"].as_u64().is_some());
}

#[actix_web::test]
async fn test_rate_limit_429_has_cors_headers() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "Rate Limit CORS").await;
    let config = create_test_config(crate::common::rate_limit());

    // Set quota exceeded
    let exceeded_until = Utc::now() + Duration::seconds(60);
    set_project_quota_exceeded(&db.pool, project_id, exceeded_until).await;

    // The server's own CORS policy, not a copy of it.

    let app = crate::common::app_with_config(&db.pool, config.clone()).await;

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

    // A browser SDK only sees the headers the response exposes; Relay exposes
    // these three (`relay-server/src/middlewares/cors.rs`).
    let exposed = headers
        .get("access-control-expose-headers")
        .unwrap()
        .to_str()
        .unwrap()
        .to_ascii_lowercase();
    for header in ["x-sentry-rate-limits", "retry-after", "x-sentry-error"] {
        assert!(
            exposed.contains(header),
            "{header} must be exposed: {exposed}"
        );
    }
}

#[actix_web::test]
async fn test_no_rate_limit_allows_request() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "No Rate Limit").await;
    let config = create_test_config(crate::common::rate_limit());

    // Don't set any quota limits

    let app = crate::common::app_with_config(&db.pool, config.clone()).await;

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

#[actix_web::test]
async fn test_rate_limit_affects_only_specific_project() {
    let db = TestDb::new().await;
    let (project_a_id, sentry_key_a) =
        create_ingest_project(&db.pool, "Project A Rate Limited").await;
    let (project_b_id, sentry_key_b) =
        create_ingest_project(&db.pool, "Project B Not Limited").await;
    let config = create_test_config(crate::common::rate_limit());

    // Only rate limit Project A
    let exceeded_until = Utc::now() + Duration::seconds(60);
    set_project_quota_exceeded(&db.pool, project_a_id, exceeded_until).await;

    let app = crate::common::app_with_config(&db.pool, config.clone()).await;

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

/// Events accepted and not digested yet count against the window they will
/// be digested in, which is the current one whatever window the stored
/// counters belong to: a minute that rolled over clears the counters, not
/// the backlog.
#[actix_web::test]
async fn pending_events_count_against_the_current_window_after_a_rollover() {
    let db = TestDb::new().await;
    let (project_id, _) = create_ingest_project(&db.pool, "Rollover").await;
    let config = crate::common::rate_limit();
    let limit = config.max_events_per_project_per_minute;
    // Last window full: it is over, so it no longer counts.
    set_project_quota_exceeded(&db.pool, project_id, Utc::now() - Duration::seconds(1)).await;
    let (project, installation) = ProjectService::get_with_installation_quota(&db.pool, project_id)
        .await
        .unwrap();
    let pending = |count| Pending {
        installation: count,
        project: count,
    };

    assert!(
        RateLimitService::check_quota(&installation, &project, &config, pending(limit - 1))
            .is_none(),
        "a rolled-over window only holds what is pending"
    );
    let exceeded =
        RateLimitService::check_quota(&installation, &project, &config, pending(limit)).unwrap();
    assert!(matches!(exceeded.scope, QuotaScope::Project));
    assert!((1..=60).contains(&exceeded.retry_after));
}

/// The hour windows count pending events as the minute windows do.
#[actix_web::test]
async fn pending_events_count_against_the_hour_windows() {
    let db = TestDb::new().await;
    let (project_id, _) = create_ingest_project(&db.pool, "Hour").await;
    let config = RateLimitConfig {
        max_events_per_minute: 100,
        max_events_per_hour: 100,
        max_events_per_project_per_minute: 100,
        max_events_per_project_per_hour: 5,
    };
    let (project, installation) = ProjectService::get_with_installation_quota(&db.pool, project_id)
        .await
        .unwrap();

    let exceeded = RateLimitService::check_quota(
        &installation,
        &project,
        &config,
        Pending {
            installation: 5,
            project: 5,
        },
    )
    .unwrap();
    assert!(matches!(exceeded.scope, QuotaScope::Project));
    let to_hour_end = 3600 - Utc::now().timestamp().rem_euclid(3600) as u64;
    assert!(
        exceeded.retry_after.abs_diff(to_hour_end) <= 1,
        "the hour is what is full"
    );

    let exceeded = RateLimitService::check_quota(
        &installation,
        &project,
        &config,
        Pending {
            installation: 100,
            project: 0,
        },
    )
    .unwrap();
    assert!(matches!(exceeded.scope, QuotaScope::Installation));
}
