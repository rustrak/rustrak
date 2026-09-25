//! Integration tests for the Releases API
//!
//! Covers both the "new issues introduced in this release" endpoint (release
//! detail page) and the Sentry-compatible CI endpoints (GH #191):
//! `POST/PUT /api/0/projects/{org_slug}/{project_slug}/releases/...` used by
//! sentry-cli and JS bundler plugins on every build.

use crate::common::TestDb;
use actix_web::{test, web, App};
use chrono::{Duration, Utc};
use rustrak::config::{Config, DashboardConfig, DatabaseConfig, RateLimitConfig};
use rustrak::models::CreateProject;
use rustrak::routes;
use rustrak::services::grouping::DenormalizedFields;
use rustrak::services::{AuthTokenService, IssueService, ProjectService};
use serde_json::{json, Value};
use std::time::Duration as StdDuration;

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

async fn create_test_issue(
    pool: &rustrak::db::DbPool,
    project_id: i32,
    calc_type: &str,
    calc_value: &str,
) -> rustrak::models::Issue {
    let denormalized = DenormalizedFields {
        calculated_type: calc_type.to_string(),
        calculated_value: calc_value.to_string(),
        transaction: "/api/test".to_string(),
        last_frame_filename: "test.rs".to_string(),
        last_frame_module: "test_module".to_string(),
        last_frame_function: "test_function".to_string(),
        culprit: "test_function".to_string(),
        logger: String::new(),
        release: String::new(),
    };
    IssueService::create(
        pool,
        project_id,
        Utc::now(),
        &denormalized,
        Some("error"),
        Some("rust"),
    )
    .await
    .expect("Failed to create test issue")
}

async fn set_first_release(pool: &rustrak::db::DbPool, issue_id: uuid::Uuid, release: &str) {
    sqlx::query("UPDATE issues SET first_release = $1 WHERE id = $2")
        .bind(release)
        .bind(issue_id)
        .execute(pool)
        .await
        .expect("failed to set first_release");
}

#[actix_web::test]
async fn test_new_issues_for_release_returns_matching_issues() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "New Issues Project").await;
    let config = create_test_config();

    let matching = create_test_issue(&db.pool, project.id, "TypeError", "In this release").await;
    set_first_release(&db.pool, matching.id, "1.0.0").await;

    let other = create_test_issue(&db.pool, project.id, "ValueError", "Other release").await;
    set_first_release(&db.pool, other.id, "0.9.0").await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config))
            .configure(routes::releases::configure)
            .configure(routes::projects::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{}/releases/1.0.0/new-issues",
            project.id
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body: Value = test::read_body_json(resp).await;
    let issues = body.as_array().unwrap();
    assert_eq!(issues.len(), 1);
    assert!(issues[0]["title"]
        .as_str()
        .unwrap()
        .contains("In this release"));
}

#[actix_web::test]
async fn test_new_issues_for_release_respects_limit() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Limit Project").await;
    let config = create_test_config();

    for i in 0..3 {
        let issue = create_test_issue(&db.pool, project.id, "TypeError", &format!("Err {i}")).await;
        set_first_release(&db.pool, issue.id, "1.0.0").await;
    }

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config))
            .configure(routes::releases::configure)
            .configure(routes::projects::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{}/releases/1.0.0/new-issues?limit=2",
            project.id
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body: Value = test::read_body_json(resp).await;
    let issues = body.as_array().unwrap();
    assert_eq!(issues.len(), 2);
}

#[actix_web::test]
async fn test_new_issues_for_release_unauthorized_without_token() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Unauthorized Release Project").await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config))
            .configure(routes::releases::configure)
            .configure(routes::projects::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{}/releases/1.0.0/new-issues",
            project.id
        ))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

// =============================================================================
// Sentry-compatible CI endpoints (GH #191): POST/PUT .../releases/...
// =============================================================================

async fn set_last_release(pool: &rustrak::db::DbPool, issue_id: uuid::Uuid, release: &str) {
    sqlx::query("UPDATE issues SET last_release = $1 WHERE id = $2")
        .bind(release)
        .bind(issue_id)
        .execute(pool)
        .await
        .expect("failed to set last_release");
}

/// Inserts a release row directly with an explicit `date_created`, bypassing
/// `ReleaseService::create` (which always stamps "now"). Used to deterministically
/// control chronological ordering in regression-clearing tests without relying
/// on real clock advancement between two service calls in the same test.
async fn insert_release_with_date_created(
    pool: &rustrak::db::DbPool,
    project_id: i32,
    version: &str,
    date_created: chrono::DateTime<Utc>,
) {
    sqlx::query("INSERT INTO releases (project_id, version, date_created) VALUES ($1, $2, $3)")
        .bind(project_id)
        .bind(version)
        .bind(date_created)
        .execute(pool)
        .await
        .expect("failed to insert release");
}

async fn count_releases(pool: &rustrak::db::DbPool, project_id: i32, version: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM releases WHERE project_id = $1 AND version = $2",
    )
    .bind(project_id)
    .bind(version)
    .fetch_one(pool)
    .await
    .expect("count query must succeed")
}

#[actix_web::test]
async fn test_create_release_new_returns_201_with_null_date_released() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Create Release Project").await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(create_test_config()))
            .configure(routes::releases::configure)
            .configure(routes::projects::configure),
    )
    .await;

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/0/projects/my-org/{}/releases/",
            project.slug
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({ "version": "1.2.1" }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["version"], "1.2.1");
    assert!(body["dateReleased"].is_null());

    assert_eq!(count_releases(&db.pool, project.id, "1.2.1").await, 1);
}

#[actix_web::test]
async fn test_create_release_duplicate_returns_208_without_duplicate_row() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Duplicate Release Project").await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(create_test_config()))
            .configure(routes::releases::configure)
            .configure(routes::projects::configure),
    )
    .await;

    let make_req = || {
        test::TestRequest::post()
            .uri(&format!(
                "/api/0/projects/my-org/{}/releases/",
                project.slug
            ))
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .set_json(json!({ "version": "2.0.0" }))
            .to_request()
    };

    let first = test::call_service(&app, make_req()).await;
    assert_eq!(first.status(), 201);

    let second = test::call_service(&app, make_req()).await;
    assert_eq!(second.status(), 208);

    let body: Value = test::read_body_json(second).await;
    assert_eq!(body["version"], "2.0.0");

    assert_eq!(count_releases(&db.pool, project.id, "2.0.0").await, 1);
}

#[actix_web::test]
async fn test_create_release_invalid_version_returns_400_and_no_row() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Invalid Version Project").await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(create_test_config()))
            .configure(routes::releases::configure)
            .configure(routes::projects::configure),
    )
    .await;

    for bad_version in ["../etc", "latest", "LATEST", "."] {
        let req = test::TestRequest::post()
            .uri(&format!(
                "/api/0/projects/my-org/{}/releases/",
                project.slug
            ))
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .set_json(json!({ "version": bad_version }))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            400,
            "expected 400 for invalid version {:?}",
            bad_version
        );
        assert_eq!(count_releases(&db.pool, project.id, bad_version).await, 0);
    }
}

#[actix_web::test]
async fn test_create_release_unauthorized_without_token() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Unauthorized Create Project").await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(create_test_config()))
            .configure(routes::releases::configure)
            .configure(routes::projects::configure),
    )
    .await;

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/0/projects/my-org/{}/releases/",
            project.slug
        ))
        .set_json(json!({ "version": "1.0.0" }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn test_create_release_unknown_project_returns_404() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(create_test_config()))
            .configure(routes::releases::configure)
            .configure(routes::projects::configure),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/0/projects/my-org/does-not-exist/releases/")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({ "version": "1.0.0" }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn test_finalize_release_sets_date_released() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Finalize Release Project").await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(create_test_config()))
            .configure(routes::releases::configure)
            .configure(routes::projects::configure),
    )
    .await;

    let create_req = test::TestRequest::post()
        .uri(&format!(
            "/api/0/projects/my-org/{}/releases/",
            project.slug
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({ "version": "3.0.0" }))
        .to_request();
    assert_eq!(test::call_service(&app, create_req).await.status(), 201);

    let put_req = test::TestRequest::put()
        .uri(&format!(
            "/api/0/projects/my-org/{}/releases/3.0.0/",
            project.slug
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({ "dateReleased": "2026-07-18T12:00:00Z", "ref": "abc123", "url": "https://example.com" }))
        .to_request();

    let resp = test::call_service(&app, put_req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["version"], "3.0.0");
    assert_eq!(body["dateReleased"], "2026-07-18T12:00:00Z");
    assert_eq!(body["ref"], "abc123");
    assert_eq!(body["url"], "https://example.com");
}

#[actix_web::test]
async fn test_finalize_release_not_found_returns_404() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Missing Release Project").await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(create_test_config()))
            .configure(routes::releases::configure)
            .configure(routes::projects::configure),
    )
    .await;

    let req = test::TestRequest::put()
        .uri(&format!(
            "/api/0/projects/my-org/{}/releases/does-not-exist/",
            project.slug
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({ "dateReleased": "2026-07-18T12:00:00Z" }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn test_create_release_clears_in_next_release_marker_for_older_release() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Regression Clear Project").await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(create_test_config()))
            .configure(routes::releases::configure)
            .configure(routes::projects::configure),
    )
    .await;

    // Issue resolved "in next release", denormalized against 1.0.0.
    let issue = create_test_issue(&db.pool, project.id, "TypeError", "Regressed later").await;
    set_last_release(&db.pool, issue.id, "1.0.0").await;
    IssueService::resolve_in_next_release(&db.pool, issue.id)
        .await
        .expect("resolve_in_next_release must succeed");

    // 1.0.0 was created well in the past — deterministic anchor for the
    // "older than the new release" comparison, no reliance on clock skew.
    insert_release_with_date_created(
        &db.pool,
        project.id,
        "1.0.0",
        Utc::now() - Duration::hours(1),
    )
    .await;

    // Creating a newer release (default "now" date_created) must clear the marker.
    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/0/projects/my-org/{}/releases/",
            project.slug
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({ "version": "2.0.0" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let updated = IssueService::get_by_id(&db.pool, issue.id)
        .await
        .expect("issue must still exist");
    assert!(
        !updated.status_details.contains("in_next_release"),
        "marker should be cleared, got status_details={}",
        updated.status_details
    );
    assert_eq!(updated.status, "resolved");
}

#[actix_web::test]
async fn test_create_release_does_not_clear_marker_for_release_newer_than_the_comparison() {
    // Negative case, deliberately using ONLY the real POST -> ReleaseService::create
    // path on both sides (unlike the test above, which anchors the "older" release
    // via a raw bind). This is the scenario that catches the SQLite date_created
    // format bug that a *positive* clearing test cannot: relying on the column's
    // `DEFAULT (datetime('now'))` stores a space-separated "YYYY-MM-DD HH:MM:SS"
    // TEXT value, while a sqlx-bound `DateTime<Utc>` parameter encodes as
    // 'T'-separated RFC3339 — `' ' < 'T'` in ASCII, so a DEFAULT-stored row
    // *always* sorts as "older" than any bound comparison value on the same
    // calendar day, regardless of real chronology. A test that only asserts
    // "clearing happens when it should" can't distinguish correct clearing from
    // this bug's false positive; this test asserts clearing does NOT happen when
    // it shouldn't, which the bug gets wrong.
    //
    // Sequence: create an early release, then a later one; anchor the issue to
    // the *later* release; re-touch the *early* release (duplicate POST, 208
    // branch, which still runs finalize_release per its self-healing design).
    // The early release's date_created must not be "less than" the later
    // release's — the marker must survive.
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "No False Clear Project").await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(create_test_config()))
            .configure(routes::releases::configure)
            .configure(routes::projects::configure),
    )
    .await;

    let post_release = |version: &'static str| {
        test::TestRequest::post()
            .uri(&format!(
                "/api/0/projects/my-org/{}/releases/",
                project.slug
            ))
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .set_json(json!({ "version": version }))
            .to_request()
    };

    let early = test::call_service(&app, post_release("1.0.0")).await;
    assert_eq!(early.status(), 201);

    let late = test::call_service(&app, post_release("2.0.0")).await;
    assert_eq!(late.status(), 201);

    // The issue's last known release is the LATER one — it must not be treated
    // as "older than" the early release just because we re-touch the early one.
    let issue = create_test_issue(&db.pool, project.id, "TypeError", "Newer than anchor").await;
    set_last_release(&db.pool, issue.id, "2.0.0").await;
    IssueService::resolve_in_next_release(&db.pool, issue.id)
        .await
        .expect("resolve_in_next_release must succeed");

    // Re-touch the EARLY release (duplicate POST -> 208, still runs
    // finalize_release). Correct behavior: no-op, since "2.0.0" (the issue's
    // last_release) is not older than "1.0.0".
    let dup = test::call_service(&app, post_release("1.0.0")).await;
    assert_eq!(dup.status(), 208);

    let updated = IssueService::get_by_id(&db.pool, issue.id)
        .await
        .expect("issue must still exist");
    assert!(
        updated.status_details.contains("in_next_release"),
        "marker must survive: \"2.0.0\" is not older than \"1.0.0\", got status_details={}",
        updated.status_details
    );
}

#[actix_web::test]
async fn test_create_release_duplicate_does_not_reclear_marker() {
    // create_release runs finalize_release on every call (new row AND the
    // idempotent 208 branch — see its doc comment for why: self-healing a
    // prior call that created the row but failed before clearing ran). This
    // test isn't about the 208 branch skipping finalize_release — it doesn't
    // — it's about finalize_release itself being scoped: a marker whose
    // last_release was never registered as a release has nothing to compare
    // against, so no amount of re-running touches it.
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "No Reclear Project").await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(create_test_config()))
            .configure(routes::releases::configure)
            .configure(routes::projects::configure),
    )
    .await;

    let first_req = test::TestRequest::post()
        .uri(&format!(
            "/api/0/projects/my-org/{}/releases/",
            project.slug
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({ "version": "1.0.0" }))
        .to_request();
    assert_eq!(test::call_service(&app, first_req).await.status(), 201);

    let issue = create_test_issue(&db.pool, project.id, "TypeError", "Still pending").await;
    set_last_release(&db.pool, issue.id, "0.9.0").await;
    IssueService::resolve_in_next_release(&db.pool, issue.id)
        .await
        .expect("resolve_in_next_release must succeed");
    // No releases row for 0.9.0 exists, so nothing could clear the marker yet.

    let dup_req = test::TestRequest::post()
        .uri(&format!(
            "/api/0/projects/my-org/{}/releases/",
            project.slug
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({ "version": "1.0.0" }))
        .to_request();
    let resp = test::call_service(&app, dup_req).await;
    assert_eq!(resp.status(), 208);

    let updated = IssueService::get_by_id(&db.pool, issue.id)
        .await
        .expect("issue must still exist");
    assert!(
        updated.status_details.contains("in_next_release"),
        "duplicate create must not touch unrelated markers"
    );
}

#[actix_web::test]
async fn test_create_release_cross_project_isolation() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project_x = create_test_project(&db.pool, "Project X").await;
    let project_y = create_test_project(&db.pool, "Project Y").await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(create_test_config()))
            .configure(routes::releases::configure)
            .configure(routes::projects::configure),
    )
    .await;

    // Issue in project Y, same version string as what will be created in X.
    let issue_y = create_test_issue(&db.pool, project_y.id, "TypeError", "Y issue").await;
    set_last_release(&db.pool, issue_y.id, "1.0.0").await;
    IssueService::resolve_in_next_release(&db.pool, issue_y.id)
        .await
        .expect("resolve_in_next_release must succeed");
    insert_release_with_date_created(
        &db.pool,
        project_y.id,
        "1.0.0",
        Utc::now() - Duration::hours(1),
    )
    .await;

    // New release created in X only.
    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/0/projects/my-org/{}/releases/",
            project_x.slug
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({ "version": "2.0.0" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    // Project Y's issue must be untouched.
    let updated_y = IssueService::get_by_id(&db.pool, issue_y.id)
        .await
        .expect("issue must still exist");
    assert!(
        updated_y.status_details.contains("in_next_release"),
        "release created in another project must not clear this project's markers"
    );

    // No release row leaked into project Y either.
    assert_eq!(count_releases(&db.pool, project_y.id, "2.0.0").await, 0);
}
