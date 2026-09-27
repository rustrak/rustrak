//! Integration tests for the Projects API
//!
//! Tests the complete Projects CRUD API with a real PostgreSQL database.
//! Uses session-based authentication via SessionMiddleware.

use crate::common::TestDb;
use actix_session::{storage::CookieSessionStore, SessionMiddleware};
use actix_web::{cookie::Key, test, web, App};
use rustrak::config::{Config, DashboardConfig, DatabaseConfig, RateLimitConfig};
use rustrak::models::{CreateAuthToken, CreateProject, UpdateProject};
use rustrak::routes;
use rustrak::services::{AuthTokenService, ProjectService};
use std::time::Duration;

/// Creates a test config
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

/// Session key for tests
fn test_session_key() -> Key {
    Key::from(&[0u8; 64])
}

// =============================================================================
// List Projects Tests
// =============================================================================

// =============================================================================
// NOTE: The test_list_projects_empty test has been removed because actix-web's
// test framework does not properly preserve session cookies between requests.
// The test_list_projects_unauthorized test below verifies the route exists
// and returns 401 for unauthenticated requests.
// =============================================================================

#[actix_web::test]
async fn test_list_projects_unauthorized() {
    let db = TestDb::new().await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config))
            .wrap(
                SessionMiddleware::builder(CookieSessionStore::default(), test_session_key())
                    .cookie_secure(false)
                    .build(),
            )
            .configure(routes::projects::configure),
    )
    .await;

    // No session cookie
    let req = test::TestRequest::get().uri("/api/projects").to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

// =============================================================================
// NOTE: The following tests are marked as ignored because actix-web's test
// framework does not properly preserve session cookies between requests.
// Session-based authentication tests should be done via E2E tests with a real
// HTTP client. See tests/e2e/ for end-to-end tests that properly test the
// full authentication flow.
// =============================================================================

#[actix_web::test]
#[ignore = "Session cookies not preserved in actix test framework - use E2E tests"]
async fn test_create_project() {
    // This test requires proper session cookie handling
}

#[actix_web::test]
#[ignore = "Session cookies not preserved in actix test framework - use E2E tests"]
async fn test_create_project_duplicate_name_fails() {
    // This test requires proper session cookie handling
}

#[actix_web::test]
#[ignore = "Session cookies not preserved in actix test framework - use E2E tests"]
async fn test_create_project_generates_unique_slug() {
    // This test requires proper session cookie handling
}

#[actix_web::test]
#[ignore = "Session cookies not preserved in actix test framework - use E2E tests"]
async fn test_create_project_empty_name() {
    // This test requires proper session cookie handling
}

#[actix_web::test]
#[ignore = "Session cookies not preserved in actix test framework - use E2E tests"]
async fn test_get_project() {
    // This test requires proper session cookie handling
}

#[actix_web::test]
#[ignore = "Session cookies not preserved in actix test framework - use E2E tests"]
async fn test_get_project_not_found() {
    // This test requires proper session cookie handling
}

#[actix_web::test]
#[ignore = "Session cookies not preserved in actix test framework - use E2E tests"]
async fn test_update_project() {
    // This test requires proper session cookie handling
}

#[actix_web::test]
#[ignore = "Session cookies not preserved in actix test framework - use E2E tests"]
async fn test_update_project_not_found() {
    // This test requires proper session cookie handling
}

#[actix_web::test]
#[ignore = "Session cookies not preserved in actix test framework - use E2E tests"]
async fn test_delete_project() {
    // This test requires proper session cookie handling
}

#[actix_web::test]
#[ignore = "Session cookies not preserved in actix test framework - use E2E tests"]
async fn test_delete_project_not_found() {
    // This test requires proper session cookie handling
}

#[actix_web::test]
#[ignore = "Session cookies not preserved in actix test framework - use E2E tests"]
async fn test_list_projects_with_data() {
    // This test requires proper session cookie handling
}

// =============================================================================
// Bearer Token Auth Tests
// =============================================================================

/// A valid Bearer token must be accepted by management endpoints.
/// Currently FAILS because handlers use AuthenticatedUser (session-only).
/// After the fix (ApiAuth composite extractor), this must return 200.
#[actix_web::test]
async fn test_list_projects_with_valid_bearer_token_returns_200() {
    let db = TestDb::new().await;
    let config = create_test_config();

    // Create a real token in the DB
    let token = AuthTokenService::create(&db.pool, CreateAuthToken { description: None })
        .await
        .expect("token creation must succeed");

    // No SessionMiddleware — Bearer auth must work standalone
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config))
            .configure(routes::projects::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/projects")
        .insert_header(("Authorization", format!("Bearer {}", token.token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

/// What the quota dropped is reported with the project, next to what it stored.
#[actix_web::test]
async fn test_get_project_reports_rate_limited_events() {
    let db = TestDb::new().await;
    let config = create_test_config();
    let token = AuthTokenService::create(&db.pool, CreateAuthToken { description: None })
        .await
        .unwrap();
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "Rate Limited".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();
    sqlx::query("UPDATE projects SET rate_limited_event_count = 7 WHERE id = $1")
        .bind(project.id)
        .execute(&db.pool)
        .await
        .unwrap();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config))
            .configure(routes::projects::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}", project.id))
        .insert_header(("Authorization", format!("Bearer {}", token.token)))
        .to_request();
    let body: serde_json::Value = test::call_and_read_body_json(&app, req).await;

    assert_eq!(body["rate_limited_event_count"], 7);
}

/// A project's own limit is set with a number, removed with `null`, and left
/// alone when the key is absent.
#[actix_web::test]
async fn test_update_project_rate_limit_set_keep_and_clear() {
    let db = TestDb::new().await;
    let token = AuthTokenService::create(&db.pool, CreateAuthToken { description: None })
        .await
        .unwrap();
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "Own Limit".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(create_test_config()))
            .configure(routes::projects::configure),
    )
    .await;
    let patch = |body: serde_json::Value| {
        test::TestRequest::patch()
            .uri(&format!("/api/projects/{}", project.id))
            .insert_header(("Authorization", format!("Bearer {}", token.token)))
            .set_json(body)
            .to_request()
    };

    let set: serde_json::Value = test::call_and_read_body_json(
        &app,
        patch(serde_json::json!({"rate_limit_per_minute": 100, "rate_limit_per_hour": 2000})),
    )
    .await;
    assert_eq!(
        (
            set["rate_limit_per_minute"].clone(),
            set["rate_limit_per_hour"].clone()
        ),
        (100.into(), 2000.into())
    );

    let kept: serde_json::Value =
        test::call_and_read_body_json(&app, patch(serde_json::json!({"name": "Renamed"}))).await;
    assert_eq!(kept["rate_limit_per_minute"], 100);

    let cleared: serde_json::Value = test::call_and_read_body_json(
        &app,
        patch(serde_json::json!({"rate_limit_per_minute": null})),
    )
    .await;
    assert!(cleared["rate_limit_per_minute"].is_null());
    assert_eq!(cleared["rate_limit_per_hour"], 2000);

    let zero = test::call_service(&app, patch(serde_json::json!({"rate_limit_per_hour": 0}))).await;
    assert_eq!(zero.status(), 400);
}

/// The server's own per-project limits, so the dashboard can show what a
/// project follows when it has no limit of its own.
#[actix_web::test]
async fn test_rate_limits_report_the_servers_project_limits() {
    let db = TestDb::new().await;
    let token = AuthTokenService::create(&db.pool, CreateAuthToken { description: None })
        .await
        .unwrap();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(create_test_config()))
            .configure(routes::projects::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/rate-limits")
        .insert_header(("Authorization", format!("Bearer {}", token.token)))
        .to_request();
    let body: serde_json::Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(body["project_per_minute"], 500);
    assert_eq!(body["project_per_hour"], 5000);

    let anonymous = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/rate-limits")
            .to_request(),
    )
    .await;
    assert_eq!(anonymous.status(), 401);
}

// =============================================================================
// Slug Collision Tests
// =============================================================================

/// Simulates the TOCTOU race: "My Project" already owns "my-project".
/// A second request for "My-Project" reads a stale view (no slug taken yet)
/// and tries to INSERT with slug "my-project" — the same slug.
/// The expected behavior is a transparent retry yielding "my-project-1",
/// NOT a 409 Conflict visible to the caller.
#[actix_web::test]
async fn test_slug_toctou_retries_with_next_candidate() {
    let db = TestDb::new().await;

    // Establish "my-project" as taken
    ProjectService::create(
        &db.pool,
        CreateProject {
            name: "My Project".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .expect("first create must succeed");

    // Simulate what happens when generate_unique_slug returned "my-project"
    // from a stale read (TOCTOU race): the INSERT should retry, not 409.
    let project =
        ProjectService::create_with_stale_slug(&db.pool, "My-Project", "my-project", None)
            .await
            .expect("stale-slug create must succeed via retry, not 409");

    assert_eq!(project.slug, "my-project-1");
    assert_eq!(project.name, "My-Project");
}

// =============================================================================
// Manual Platform Override Tests
//
// HTTP-level update_project tests are marked #[ignore] above (session cookies
// aren't preserved in actix's test framework), so these exercise
// ProjectService::update() directly — same approach as the slug TOCTOU test.
// =============================================================================

#[actix_web::test]
async fn test_update_project_platform_sets_valid_platform() {
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "Platform Update Project".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .expect("create must succeed");
    assert_eq!(project.platform, None);

    let updated = ProjectService::update(
        &db.pool,
        project.id,
        UpdateProject {
            name: None,
            slug: None,
            platform: Some("python".to_string()),
            ..Default::default()
        },
    )
    .await
    .expect("manual platform update must succeed");

    assert_eq!(updated.platform, Some("python".to_string()));
}

/// A manual override may name a framework-specific platform such as
/// `javascript-nextjs`. Those ids never appear in an event's `platform`
/// field (Relay normalizes anything outside `VALID_PLATFORMS` away), so
/// they are only ever chosen by a human in project settings — exactly
/// what real Sentry's `is_valid_platform` allows.
#[actix_web::test]
async fn test_update_project_platform_accepts_framework_specific_id() {
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "Framework Platform Project".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .expect("create must succeed");

    let updated = ProjectService::update(
        &db.pool,
        project.id,
        UpdateProject {
            name: None,
            slug: None,
            platform: Some("javascript-nextjs".to_string()),
            ..Default::default()
        },
    )
    .await
    .expect("framework-specific platform must be accepted");

    assert_eq!(updated.platform, Some("javascript-nextjs".to_string()));
}

#[actix_web::test]
async fn test_update_project_platform_rejects_invalid_value() {
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "Invalid Platform Project".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .expect("create must succeed");

    let result = ProjectService::update(
        &db.pool,
        project.id,
        UpdateProject {
            name: None,
            slug: None,
            platform: Some("not-a-real-platform".to_string()),
            ..Default::default()
        },
    )
    .await;

    assert!(
        matches!(result, Err(rustrak::error::AppError::Validation(_))),
        "expected a Validation error, got: {result:?}"
    );

    // Rejected update must not have persisted anything.
    let unchanged = ProjectService::get_by_id(&db.pool, project.id)
        .await
        .expect("get must succeed");
    assert_eq!(unchanged.platform, None);
}

#[actix_web::test]
async fn test_update_project_sets_name_and_platform_together() {
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "Old Name".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .expect("create must succeed");

    let updated = ProjectService::update(
        &db.pool,
        project.id,
        UpdateProject {
            name: Some("New Name".to_string()),
            slug: None,
            platform: Some("go".to_string()),
            ..Default::default()
        },
    )
    .await
    .expect("combined update must succeed");

    assert_eq!(updated.name, "New Name");
    assert_eq!(updated.platform, Some("go".to_string()));
}

// =============================================================================
// Editable Slug Tests
//
// Real Sentry exposes the slug as the first field of General Settings. Its
// warning that renaming "can break your build scripts" does not apply here:
// Sentry puts the slug in every API URL, whereas Rustrak's DSN and routes key
// off the numeric project id, so nothing downstream depends on it.
//
// Note the deliberate asymmetry with create: `ProjectService::create` silently
// de-duplicates a taken slug ("api" -> "api-1") because there the slug is
// usually derived from the name. On update the user typed it, so storing
// something other than what they typed would be wrong. Update conflicts.
// =============================================================================

#[actix_web::test]
async fn test_update_project_slug() {
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "Slug Update Project".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .expect("create must succeed");
    assert_eq!(project.slug, "slug-update-project");

    let updated = ProjectService::update(
        &db.pool,
        project.id,
        UpdateProject {
            name: None,
            platform: None,
            slug: Some("renamed-slug".to_string()),
            ..Default::default()
        },
    )
    .await
    .expect("slug update must succeed");

    assert_eq!(updated.slug, "renamed-slug");

    let fetched = ProjectService::get_by_id(&db.pool, project.id)
        .await
        .expect("get must succeed");
    assert_eq!(fetched.slug, "renamed-slug");
}

/// A slug the user typed that is already taken must surface as a Conflict
/// (409), not a raw database error (500), and must leave the target project
/// untouched. Note the existing unique-violation mapping only produced a
/// Conflict when a `name` was being set, so a slug-only update fell through
/// to `AppError::Database`.
#[actix_web::test]
async fn test_update_project_slug_conflict_is_reported_as_conflict() {
    let db = TestDb::new().await;

    ProjectService::create(
        &db.pool,
        CreateProject {
            name: "Taken Slug Project".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .expect("create must succeed");

    let target = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "Target Slug Project".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .expect("create must succeed");

    let result = ProjectService::update(
        &db.pool,
        target.id,
        UpdateProject {
            name: None,
            platform: None,
            slug: Some("taken-slug-project".to_string()),
            ..Default::default()
        },
    )
    .await;

    // `kind()`, not the error itself: this Conflict names the field it
    // blames, so it arrives wrapped in `AppError::WithFields`.
    assert!(
        matches!(
            result.as_ref().err().map(rustrak::error::AppError::kind),
            Some(rustrak::error::AppError::Conflict(_))
        ),
        "expected a Conflict error, got: {result:?}"
    );

    let unchanged = ProjectService::get_by_id(&db.pool, target.id)
        .await
        .expect("get must succeed");
    assert_eq!(unchanged.slug, "target-slug-project");
}

/// Input that slugifies to nothing must be a 400, not a silent no-op and not
/// an empty slug in the database. `create` already rejects this case; update
/// must match.
#[actix_web::test]
async fn test_update_project_slug_rejects_unslugifiable_input() {
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "Unslugifiable Project".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .expect("create must succeed");

    let result = ProjectService::update(
        &db.pool,
        project.id,
        UpdateProject {
            name: None,
            platform: None,
            slug: Some("!!!".to_string()),
            ..Default::default()
        },
    )
    .await;

    assert!(
        matches!(result, Err(rustrak::error::AppError::Validation(_))),
        "expected a Validation error, got: {result:?}"
    );

    let unchanged = ProjectService::get_by_id(&db.pool, project.id)
        .await
        .expect("get must succeed");
    assert_eq!(unchanged.slug, "unslugifiable-project");
}

// =============================================================================
// Platform-at-creation Tests
//
// Real Sentry accepts `platform` on the create-project request itself
// (ProjectPostSerializer, src/sentry/core/endpoints/team_projects.py), so a
// user who picks a platform in the create form does not need a follow-up
// PATCH. Validation is the same SELECTABLE_PLATFORMS list the manual
// override above uses, deliberately wider than the VALID_PLATFORMS list
// auto-detection filters on.
// =============================================================================

#[actix_web::test]
async fn test_create_project_sets_platform() {
    let db = TestDb::new().await;

    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "Created With Platform".to_string(),
            slug: None,
            platform: Some("javascript-nextjs".to_string()),
        },
    )
    .await
    .expect("create with platform must succeed");

    assert_eq!(project.platform, Some("javascript-nextjs".to_string()));

    // Re-read: the returned struct must reflect what was persisted, not just
    // the input echoed back.
    let fetched = ProjectService::get_by_id(&db.pool, project.id)
        .await
        .expect("get must succeed");
    assert_eq!(fetched.platform, Some("javascript-nextjs".to_string()));
}

/// The TOCTOU retry re-issues the INSERT with a freshly generated slug. That
/// second statement is a separate set of binds, so a platform threaded only
/// through the first one is silently dropped for exactly the concurrent-create
/// case. Nothing else covers the retry path's column list.
#[actix_web::test]
async fn test_create_preserves_platform_across_slug_retry() {
    let db = TestDb::new().await;

    // Establish "retry-platform" as taken so the next INSERT collides.
    ProjectService::create(
        &db.pool,
        CreateProject {
            name: "Retry Platform".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .expect("first create must succeed");

    let project = ProjectService::create_with_stale_slug(
        &db.pool,
        "Retry-Platform",
        "retry-platform",
        Some("python-django"),
    )
    .await
    .expect("stale-slug create must succeed via retry");

    assert_eq!(project.slug, "retry-platform-1");
    assert_eq!(
        project.platform,
        Some("python-django".to_string()),
        "platform must survive the retry INSERT, not just the first one"
    );
}

/// A bad platform must be rejected *before* the INSERT, not stored and not
/// silently dropped. The row must not exist afterwards: a project created
/// with a silently-discarded platform looks successful to the user while
/// losing their choice.
#[actix_web::test]
async fn test_create_project_rejects_invalid_platform() {
    let db = TestDb::new().await;

    let result = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "Invalid Platform At Creation".to_string(),
            slug: None,
            platform: Some("not-a-real-platform".to_string()),
        },
    )
    .await;

    assert!(
        matches!(result, Err(rustrak::error::AppError::Validation(_))),
        "expected a Validation error, got: {result:?}"
    );

    let projects = ProjectService::list(&db.pool)
        .await
        .expect("list must succeed");
    assert!(
        !projects
            .iter()
            .any(|p| p.name == "Invalid Platform At Creation"),
        "rejected create must not have persisted a row"
    );
}

#[actix_web::test]
async fn test_update_project_platform_overwrites_existing_value() {
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "Overwrite Platform Project".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .expect("create must succeed");

    // Simulate an already auto-detected platform (infer_platform_from_event
    // only writes when NULL, so this seeds the "already set" state directly).
    ProjectService::update(
        &db.pool,
        project.id,
        UpdateProject {
            name: None,
            slug: None,
            platform: Some("javascript".to_string()),
            ..Default::default()
        },
    )
    .await
    .expect("seed update must succeed");

    // A manual update must be able to overwrite it — unlike
    // infer_platform_from_event, which would no-op here.
    let updated = ProjectService::update(
        &db.pool,
        project.id,
        UpdateProject {
            name: None,
            slug: None,
            platform: Some("ruby".to_string()),
            ..Default::default()
        },
    )
    .await
    .expect("manual overwrite must succeed");

    assert_eq!(updated.platform, Some("ruby".to_string()));
}

// =============================================================================
// User-Chosen Slug on Create
//
// A slug the caller supplied and a slug derived from the name get opposite
// treatment on collision. These pin both halves: silently de-duplicating a
// slug the user typed would store something they never asked for, and
// conflicting on a derived one would break the ordinary create path.
// =============================================================================

#[actix_web::test]
async fn test_create_project_with_taken_user_slug_conflicts() {
    let db = TestDb::new().await;

    ProjectService::create(
        &db.pool,
        CreateProject {
            name: "First Project".to_string(),
            slug: Some("shared-slug".to_string()),
            platform: None,
        },
    )
    .await
    .expect("first create must succeed");

    let result = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "Second Project".to_string(),
            slug: Some("shared-slug".to_string()),
            platform: None,
        },
    )
    .await;

    assert!(
        matches!(
            result.as_ref().err().map(rustrak::error::AppError::kind),
            Some(rustrak::error::AppError::Conflict(_))
        ),
        "a slug the user typed must conflict, not be de-duplicated, got: {result:?}"
    );

    // And nothing was written under a renamed slug.
    let projects = ProjectService::list(&db.pool)
        .await
        .expect("list must succeed");
    assert_eq!(
        projects
            .iter()
            .filter(|p| p.slug.starts_with("shared-slug"))
            .count(),
        1,
        "the rejected create must not have inserted a de-duplicated row"
    );
}

#[actix_web::test]
async fn test_create_project_derived_slug_still_dedupes() {
    let db = TestDb::new().await;

    let first = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "Derived Project".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .expect("first create must succeed");
    assert_eq!(first.slug, "derived-project");

    // A different name (so the UNIQUE on name passes) that slugifies to the
    // same base. Nobody chose this slug, so appending a counter is correct.
    let second = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "Derived  Project".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .expect("a derived slug must de-duplicate, not conflict");

    assert_eq!(second.slug, "derived-project-1");
}

#[actix_web::test]
async fn test_create_project_rejects_unslugifiable_user_slug() {
    let db = TestDb::new().await;

    let result = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "Punctuation Project".to_string(),
            slug: Some("!!!".to_string()),
            platform: None,
        },
    )
    .await;

    assert!(
        matches!(result, Err(rustrak::error::AppError::Validation(_))),
        "a slug that slugifies to nothing must be a Validation error, got: {result:?}"
    );
}

// =============================================================================
// Per-row Stats Tests (?stats_period=)
// =============================================================================

/// Lists projects as an admin-equivalent Bearer caller and returns the parsed
/// `items` array.
async fn list_projects_json(db: &TestDb, uri: &str) -> serde_json::Value {
    let token = AuthTokenService::create(&db.pool, CreateAuthToken { description: None })
        .await
        .expect("token creation must succeed");

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(create_test_config()))
            .configure(routes::projects::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(uri)
        .insert_header(("Authorization", format!("Bearer {}", token.token)))
        .to_request();

    test::call_and_read_body_json(&app, req).await
}

/// The plain list is on the hot path for SDK tooling and the project picker,
/// which must not start paying for aggregate queries they never asked for.
#[actix_web::test]
async fn test_list_projects_omits_stats_without_the_param() {
    let db = TestDb::new().await;
    ProjectService::create(
        &db.pool,
        CreateProject {
            name: "No Stats".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .expect("create failed");

    let body = list_projects_json(&db, "/api/projects").await;

    let project = &body["items"][0];
    assert_eq!(project["name"], "No Stats");
    assert!(
        project.get("stats").is_none(),
        "stats must be absent, not null: {project}"
    );
}

#[actix_web::test]
async fn test_list_projects_attaches_stats_with_the_param() {
    let db = TestDb::new().await;
    ProjectService::create(
        &db.pool,
        CreateProject {
            name: "With Stats".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .expect("create failed");

    let body = list_projects_json(&db, "/api/projects?stats_period=24h").await;

    let stats = &body["items"][0]["stats"];
    assert_eq!(
        stats["trend"]
            .as_array()
            .expect("trend must be an array")
            .len(),
        24
    );
    assert_eq!(stats["events"]["current"], 0);
    assert_eq!(stats["events"]["previous"], 0);
    assert_eq!(stats["new_issues"]["current"], 0);
    assert_eq!(stats["new_issues"]["previous"], 0);
    assert_eq!(stats["open_issues"], 0);
    assert_eq!(stats["fatal_issues"], 0);
}

/// `parse_period_hours` returns `None` for junk, which here must mean "no
/// stats" rather than a 500 or a silently wrong window.
#[actix_web::test]
async fn test_list_projects_ignores_an_unparseable_stats_period() {
    let db = TestDb::new().await;
    ProjectService::create(
        &db.pool,
        CreateProject {
            name: "Junk Period".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .expect("create failed");

    let body = list_projects_json(&db, "/api/projects?stats_period=last%20tuesday").await;

    assert_eq!(body["items"][0]["name"], "Junk Period");
    assert!(body["items"][0].get("stats").is_none());
}
