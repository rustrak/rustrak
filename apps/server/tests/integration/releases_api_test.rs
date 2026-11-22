//! Integration tests for the Releases API
//!
//! Covers both the "new issues introduced in this release" endpoint (release
//! detail page) and the Sentry-compatible CI endpoints (GH #191):
//! `POST/PUT /api/0/projects/{org_slug}/{project_slug}/releases/...` used by
//! sentry-cli and JS bundler plugins on every build.

use crate::common::TestDb;
use crate::common::{create_test_project, create_test_token};
use actix_web::test;
use chrono::Utc;
use rustrak::services::grouping::DenormalizedFields;
use rustrak::services::IssueService;
use serde_json::{json, Value};

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
    let config = crate::common::config();

    let matching = create_test_issue(&db.pool, project.id, "TypeError", "In this release").await;
    set_first_release(&db.pool, matching.id, "1.0.0").await;

    let other = create_test_issue(&db.pool, project.id, "ValueError", "Other release").await;
    set_first_release(&db.pool, other.id, "0.9.0").await;

    let app = crate::common::app_with_config(&db.pool, config).await;

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
    let config = crate::common::config();

    for i in 0..3 {
        let issue = create_test_issue(&db.pool, project.id, "TypeError", &format!("Err {i}")).await;
        set_first_release(&db.pool, issue.id, "1.0.0").await;
    }

    let app = crate::common::app_with_config(&db.pool, config).await;

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
    let app = crate::common::app_with_config(&db.pool, crate::common::config()).await;

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
    let app = crate::common::app_with_config(&db.pool, crate::common::config()).await;

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
    let app = crate::common::app_with_config(&db.pool, crate::common::config()).await;

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
async fn test_create_release_unknown_project_returns_404() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let app = crate::common::app_with_config(&db.pool, crate::common::config()).await;

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
    let app = crate::common::app_with_config(&db.pool, crate::common::config()).await;

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
    let app = crate::common::app_with_config(&db.pool, crate::common::config()).await;

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
