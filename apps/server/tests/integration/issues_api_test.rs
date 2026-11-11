//! Integration tests for the Issues API
//!
//! Tests the complete Issues API with a real PostgreSQL database.

use crate::common::TestDb;
use crate::common::{create_test_project, create_test_token};
use actix_web::{test, web, App};
use chrono::Utc;
use rustrak::routes;
use rustrak::services::grouping::DenormalizedFields;
use rustrak::services::{EventService, IssueService, ProjectService};
use serde_json::{json, Value};
use uuid::Uuid;

fn create_denormalized_fields(
    calc_type: &str,
    calc_value: &str,
    transaction: &str,
) -> DenormalizedFields {
    DenormalizedFields {
        calculated_type: calc_type.to_string(),
        calculated_value: calc_value.to_string(),
        transaction: transaction.to_string(),
        last_frame_filename: "test.rs".to_string(),
        last_frame_module: "test_module".to_string(),
        last_frame_function: "test_function".to_string(),
        culprit: "test_function".to_string(),
        logger: String::new(),
        release: String::new(),
    }
}

async fn create_test_issue(
    pool: &rustrak::db::DbPool,
    project_id: i32,
    calc_type: &str,
    calc_value: &str,
) -> rustrak::models::Issue {
    create_test_issue_at(pool, project_id, calc_type, calc_value, Utc::now()).await
}

/// An issue first and last seen at `at`.
async fn create_test_issue_at(
    pool: &rustrak::db::DbPool,
    project_id: i32,
    calc_type: &str,
    calc_value: &str,
    at: chrono::DateTime<Utc>,
) -> rustrak::models::Issue {
    let denormalized = create_denormalized_fields(calc_type, calc_value, "/api/test");
    IssueService::create(
        pool,
        project_id,
        at,
        &denormalized,
        Some("error"),
        Some("rust"),
    )
    .await
    .expect("Failed to create test issue")
}

#[actix_web::test]
async fn test_list_issues_empty() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Empty Project").await;
    let config = crate::common::config();

    let app = crate::common::app_with_config(&db.pool, config).await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/issues", project.id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    // Offset pagination (GH #165), not the old cursor shape: items/total_count/
    // page/per_page/total_pages.
    let body: Value = test::read_body_json(resp).await;
    assert!(body["items"].as_array().unwrap().is_empty());
    assert_eq!(body["total_count"], 0);
    assert_eq!(body["total_pages"], 0);
}

#[actix_web::test]
async fn test_list_issues_with_data() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Issues Project").await;
    let config = crate::common::config();

    create_test_issue(&db.pool, project.id, "TypeError", "Cannot read property").await;
    create_test_issue(&db.pool, project.id, "ValueError", "Invalid value").await;

    let app = crate::common::app_with_config(&db.pool, config).await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/issues", project.id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body: Value = test::read_body_json(resp).await;
    let issues = body["items"].as_array().unwrap();
    assert_eq!(issues.len(), 2);
}

#[actix_web::test]
async fn test_list_issues_project_not_found() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let config = crate::common::config();

    let app = crate::common::app_with_config(&db.pool, config).await;

    let req = test::TestRequest::get()
        .uri("/api/projects/99999/issues")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn test_list_issues_filters_resolved_by_default() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Filter Project").await;
    let config = crate::common::config();

    let issue1 = create_test_issue(&db.pool, project.id, "TypeError", "Error 1").await;
    create_test_issue(&db.pool, project.id, "ValueError", "Error 2").await;

    // Resolve one issue
    IssueService::resolve(&db.pool, issue1.id).await.unwrap();

    let app = crate::common::app_with_config(&db.pool, config).await;

    // Default should filter out resolved
    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/issues", project.id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body: Value = test::read_body_json(resp).await;
    let issues = body["items"].as_array().unwrap();
    assert_eq!(issues.len(), 1);
}

#[actix_web::test]
async fn test_list_issues_include_resolved() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Include Resolved Project").await;
    let config = crate::common::config();

    let issue1 = create_test_issue(&db.pool, project.id, "TypeError", "Error 1").await;
    create_test_issue(&db.pool, project.id, "ValueError", "Error 2").await;

    IssueService::resolve(&db.pool, issue1.id).await.unwrap();

    let app = crate::common::app_with_config(&db.pool, config).await;

    // `include_resolved=true` is the old boolean param; the current API uses
    // `filter=all` (an `IssueFilter` enum: open/resolved/muted/all).
    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/issues?filter=all", project.id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body: Value = test::read_body_json(resp).await;
    let issues = body["items"].as_array().unwrap();
    assert_eq!(issues.len(), 2);
}

#[actix_web::test]
async fn test_list_issues_sort_by_last_seen() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Sort Project").await;
    let config = crate::common::config();

    let now = Utc::now();
    create_test_issue_at(
        &db.pool,
        project.id,
        "TypeError",
        "First",
        now - chrono::Duration::seconds(1),
    )
    .await;
    create_test_issue_at(&db.pool, project.id, "ValueError", "Second", now).await;

    let app = crate::common::app_with_config(&db.pool, config).await;

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{}/issues?sort=last_seen&order=desc",
            project.id
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body: Value = test::read_body_json(resp).await;
    let issues = body["items"].as_array().unwrap();
    assert_eq!(issues.len(), 2);
    // Most recent should be first
    assert!(issues[0]["title"].as_str().unwrap().contains("Second"));
}

#[actix_web::test]
async fn test_list_issues_sort_by_event_count() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Event Count Sort Project").await;
    let config = crate::common::config();

    let low = create_test_issue(&db.pool, project.id, "TypeError", "Low count").await;
    let high = create_test_issue(&db.pool, project.id, "ValueError", "High count").await;

    sqlx::query("UPDATE issues SET digested_event_count = 5 WHERE id = $1")
        .bind(low.id)
        .execute(&db.pool)
        .await
        .expect("failed to bump low issue event count");
    sqlx::query("UPDATE issues SET digested_event_count = 500 WHERE id = $1")
        .bind(high.id)
        .execute(&db.pool)
        .await
        .expect("failed to bump high issue event count");

    let app = crate::common::app_with_config(&db.pool, config).await;

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{}/issues?sort=event_count&order=desc",
            project.id
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body: Value = test::read_body_json(resp).await;
    let issues = body["items"].as_array().unwrap();
    assert_eq!(issues.len(), 2);
    assert!(issues[0]["title"].as_str().unwrap().contains("High count"));
    assert!(issues[1]["title"].as_str().unwrap().contains("Low count"));
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
async fn test_top_issues_for_release_empty_when_no_matching_release() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Empty Release Project").await;

    let issue = create_test_issue(&db.pool, project.id, "TypeError", "Some error").await;
    set_first_release(&db.pool, issue.id, "1.0.0").await;

    let issues = IssueService::top_issues_for_release(&db.pool, project.id, "2.0.0", 10)
        .await
        .expect("query failed");

    assert!(issues.is_empty());
}

#[actix_web::test]
async fn test_top_issues_for_release_filters_by_release() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Release Filter Project").await;

    let in_release = create_test_issue(&db.pool, project.id, "TypeError", "In release").await;
    set_first_release(&db.pool, in_release.id, "1.0.0").await;

    let other_release =
        create_test_issue(&db.pool, project.id, "ValueError", "Other release").await;
    set_first_release(&db.pool, other_release.id, "0.9.0").await;

    let issues = IssueService::top_issues_for_release(&db.pool, project.id, "1.0.0", 10)
        .await
        .expect("query failed");

    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].id, in_release.id);
}

#[actix_web::test]
async fn test_top_issues_for_release_filters_by_project() {
    let db = TestDb::new().await;
    let project_a = create_test_project(&db.pool, "Release Project A").await;
    let project_b = create_test_project(&db.pool, "Release Project B").await;

    let issue_a = create_test_issue(&db.pool, project_a.id, "TypeError", "Project A").await;
    set_first_release(&db.pool, issue_a.id, "1.0.0").await;

    let issue_b = create_test_issue(&db.pool, project_b.id, "TypeError", "Project B").await;
    set_first_release(&db.pool, issue_b.id, "1.0.0").await;

    let issues = IssueService::top_issues_for_release(&db.pool, project_a.id, "1.0.0", 10)
        .await
        .expect("query failed");

    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].id, issue_a.id);
}

#[actix_web::test]
async fn test_top_issues_for_release_orders_by_first_seen_desc() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Release Order Project").await;

    let now = Utc::now();
    let older = create_test_issue_at(
        &db.pool,
        project.id,
        "TypeError",
        "Older",
        now - chrono::Duration::seconds(1),
    )
    .await;
    set_first_release(&db.pool, older.id, "1.0.0").await;
    let newer = create_test_issue_at(&db.pool, project.id, "ValueError", "Newer", now).await;
    set_first_release(&db.pool, newer.id, "1.0.0").await;

    let issues = IssueService::top_issues_for_release(&db.pool, project.id, "1.0.0", 10)
        .await
        .expect("query failed");

    assert_eq!(issues.len(), 2);
    assert_eq!(issues[0].id, newer.id, "most recently introduced first");
    assert_eq!(issues[1].id, older.id);
}

#[actix_web::test]
async fn test_top_issues_for_release_respects_limit() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Release Limit Project").await;

    for i in 0..5 {
        let issue =
            create_test_issue(&db.pool, project.id, "TypeError", &format!("Error {i}")).await;
        set_first_release(&db.pool, issue.id, "1.0.0").await;
    }

    let issues = IssueService::top_issues_for_release(&db.pool, project.id, "1.0.0", 3)
        .await
        .expect("query failed");

    assert_eq!(issues.len(), 3);
}

#[actix_web::test]
async fn test_get_issue_success() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Get Issue Project").await;
    let config = crate::common::config();

    let issue = create_test_issue(&db.pool, project.id, "TypeError", "Test error message").await;

    let app = crate::common::app_with_config(&db.pool, config).await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/issues/{}", project.id, issue.id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["id"], issue.id.to_string());
    assert!(body["title"].as_str().unwrap().contains("TypeError"));
}

#[actix_web::test]
async fn test_get_issue_not_found() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Not Found Project").await;
    let config = crate::common::config();

    let app = crate::common::app_with_config(&db.pool, config).await;

    let fake_uuid = Uuid::new_v4();
    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{}/issues/{}",
            project.id, fake_uuid
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn test_get_issue_wrong_project() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project1 = create_test_project(&db.pool, "Project 1").await;
    let project2 = create_test_project(&db.pool, "Project 2").await;
    let config = crate::common::config();

    let issue = create_test_issue(&db.pool, project1.id, "TypeError", "Error").await;

    let app = crate::common::app_with_config(&db.pool, config).await;

    // Try to access issue via project2
    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{}/issues/{}",
            project2.id, issue.id
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

/// Sentry's API (and GitLab's client of it) addresses an issue by a numeric
/// id, unique across projects.
#[actix_web::test]
async fn test_issue_numeric_id_is_unique_across_projects_and_resolves() {
    let db = TestDb::new().await;
    let project1 = create_test_project(&db.pool, "Numeric 1").await;
    let project2 = create_test_project(&db.pool, "Numeric 2").await;

    let first = create_test_issue(&db.pool, project1.id, "TypeError", "a").await;
    let second = create_test_issue(&db.pool, project2.id, "TypeError", "a").await;

    assert!(second.numeric_id > first.numeric_id);

    let found = IssueService::get_by_numeric_id(&db.pool, second.numeric_id)
        .await
        .expect("issue by numeric id");
    assert_eq!(found.id, second.id);

    let missing = IssueService::get_by_numeric_id(&db.pool, second.numeric_id + 1).await;
    assert!(matches!(
        missing,
        Err(rustrak::error::AppError::NotFound(_))
    ));
}

/// A deleted issue's numeric id is never handed to a new issue: a client that
/// cached it (GitLab links) must not land on a different issue.
#[actix_web::test]
async fn test_issue_numeric_id_is_not_reused_after_delete() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Numeric Reuse").await;

    let deleted = create_test_issue(&db.pool, project.id, "TypeError", "a").await;
    sqlx::query("DELETE FROM issues WHERE id = $1")
        .bind(deleted.id)
        .execute(&db.pool)
        .await
        .expect("delete issue");

    let next = create_test_issue(&db.pool, project.id, "TypeError", "b").await;
    assert!(next.numeric_id > deleted.numeric_id);
}

#[actix_web::test]
async fn test_resolve_issue() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Resolve Project").await;
    let config = crate::common::config();

    let issue = create_test_issue(&db.pool, project.id, "TypeError", "Error").await;
    assert!(!issue.is_resolved());

    let app = crate::common::app_with_config(&db.pool, config).await;

    let req = test::TestRequest::patch()
        .uri(&format!("/api/projects/{}/issues/{}", project.id, issue.id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .insert_header(("Content-Type", "application/json"))
        .set_json(json!({"status": "resolved"}))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["status"], "resolved");
}

#[actix_web::test]
async fn test_unresolve_issue() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Unresolve Project").await;
    let config = crate::common::config();

    let issue = create_test_issue(&db.pool, project.id, "TypeError", "Error").await;
    IssueService::resolve(&db.pool, issue.id).await.unwrap();

    let app = crate::common::app_with_config(&db.pool, config).await;

    let req = test::TestRequest::patch()
        .uri(&format!("/api/projects/{}/issues/{}", project.id, issue.id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .insert_header(("Content-Type", "application/json"))
        .set_json(json!({"status": "unresolved"}))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["status"], "unresolved");
}

#[actix_web::test]
async fn test_mute_issue() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Mute Project").await;
    let config = crate::common::config();

    let issue = create_test_issue(&db.pool, project.id, "TypeError", "Error").await;

    let app = crate::common::app_with_config(&db.pool, config).await;

    let req = test::TestRequest::patch()
        .uri(&format!("/api/projects/{}/issues/{}", project.id, issue.id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .insert_header(("Content-Type", "application/json"))
        .set_json(json!({"status": "ignored"}))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["status"], "ignored");
}

#[actix_web::test]
async fn test_update_issue_not_found() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Update Not Found Project").await;
    let config = crate::common::config();

    let app = crate::common::app_with_config(&db.pool, config).await;

    let fake_uuid = Uuid::new_v4();
    let req = test::TestRequest::patch()
        .uri(&format!(
            "/api/projects/{}/issues/{}",
            project.id, fake_uuid
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .insert_header(("Content-Type", "application/json"))
        .set_json(json!({"status": "resolved"}))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn test_update_issue_empty_body() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Empty Update Project").await;
    let config = crate::common::config();

    let issue = create_test_issue(&db.pool, project.id, "TypeError", "Error").await;

    let app = crate::common::app_with_config(&db.pool, config).await;

    // Empty update should succeed but not change anything
    let req = test::TestRequest::patch()
        .uri(&format!("/api/projects/{}/issues/{}", project.id, issue.id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .insert_header(("Content-Type", "application/json"))
        .set_json(json!({}))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
}

#[actix_web::test]
async fn test_delete_issue_success() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Delete Project").await;
    let config = crate::common::config();

    let issue = create_test_issue(&db.pool, project.id, "TypeError", "Error").await;

    let app = crate::common::app_with_config(&db.pool, config).await;

    let req = test::TestRequest::delete()
        .uri(&format!("/api/projects/{}/issues/{}", project.id, issue.id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 204);

    let row: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM issues WHERE id = $1")
        .bind(issue.id)
        .fetch_optional(&db.pool)
        .await
        .expect("DB query failed");
    assert!(row.is_none(), "issue row should be gone after hard delete");

    let get_req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/issues/{}", project.id, issue.id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();
    let get_resp = test::call_service(&app, get_req).await;
    assert_eq!(get_resp.status(), 404);
}

#[actix_web::test]
async fn test_delete_issue_not_found() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Delete Not Found Project").await;
    let config = crate::common::config();

    let app = crate::common::app_with_config(&db.pool, config).await;

    let fake_uuid = Uuid::new_v4();
    let req = test::TestRequest::delete()
        .uri(&format!(
            "/api/projects/{}/issues/{}",
            project.id, fake_uuid
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn test_delete_issue_wrong_project() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project1 = create_test_project(&db.pool, "Delete Project 1").await;
    let project2 = create_test_project(&db.pool, "Delete Project 2").await;
    let config = crate::common::config();

    let issue = create_test_issue(&db.pool, project1.id, "TypeError", "Error").await;

    let app = crate::common::app_with_config(&db.pool, config).await;

    // Try to delete via wrong project
    let req = test::TestRequest::delete()
        .uri(&format!(
            "/api/projects/{}/issues/{}",
            project2.id, issue.id
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);

    // Verify the issue row was NOT deleted (cross-project isolation)
    let row: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM issues WHERE id = $1")
        .bind(issue.id)
        .fetch_optional(&db.pool)
        .await
        .expect("DB query failed");
    assert!(
        row.is_some(),
        "issue row should still exist after wrong-project delete attempt"
    );
}

#[actix_web::test]
async fn test_list_issues_pagination() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Pagination Project").await;
    let config = crate::common::config();

    // Create 25 issues — more than the default page size (PAGE_SIZE = 20,
    // pagination/mod.rs:8), so pagination structure is actually exercised.
    for i in 0..25 {
        create_test_issue(
            &db.pool,
            project.id,
            &format!("Error{}", i),
            &format!("Message {}", i),
        )
        .await;
    }

    let app = crate::common::app_with_config(&db.pool, config).await;

    // First page: 20 items, 25 total, 2 pages.
    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/issues", project.id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body: Value = test::read_body_json(resp).await;
    let issues = body["items"].as_array().unwrap();
    assert_eq!(issues.len(), 20);
    assert_eq!(body["total_count"], 25);
    assert_eq!(body["page"], 1);
    assert_eq!(body["per_page"], 20);
    assert_eq!(body["total_pages"], 2);

    // Second page: the remaining 5 items.
    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/issues?page=2", project.id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body: Value = test::read_body_json(resp).await;
    let issues = body["items"].as_array().unwrap();
    assert_eq!(issues.len(), 5);
    assert_eq!(body["page"], 2);
}

#[actix_web::test]
async fn test_list_issues_invalid_page_param() {
    // `cursor` doesn't exist as a query param on the offset-paginated
    // endpoint anymore (`?page=` replaced it); a non-numeric `page` is the
    // current equivalent of a malformed pagination param and must 400 via
    // actix's `Query<ListIssuesQuery>` extraction failure.
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Invalid Page Project").await;
    let config = crate::common::config();

    let app = crate::common::app_with_config(&db.pool, config).await;

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{}/issues?page=not_a_number",
            project.id
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn test_list_issues_page_zero_rejected_not_negative_offset() {
    // page=0 used to silently produce a negative SQL OFFSET instead of an
    // error (SQLite tolerated it and returned zero rows; Postgres would
    // likely 500). Must be a clean 400 now.
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Page Zero Project").await;
    let config = crate::common::config();

    let app = crate::common::app_with_config(&db.pool, config).await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/issues?page=0", project.id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn test_issue_response_format() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Format Project").await;
    let config = crate::common::config();

    let issue = create_test_issue(
        &db.pool,
        project.id,
        "TypeError",
        "Cannot read property 'x' of null",
    )
    .await;

    let app = crate::common::app_with_config(&db.pool, config).await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/issues/{}", project.id, issue.id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body: Value = test::read_body_json(resp).await;

    assert!(body["id"].is_string());
    assert!(body["project_id"].is_number());
    assert!(body["short_id"].is_string());
    assert!(body["title"].is_string());
    assert!(body["first_seen"].is_string());
    assert!(body["last_seen"].is_string());
    assert!(body["event_count"].is_number());
    assert!(body.get("level").is_some());
    assert!(body.get("platform").is_some());
    assert!(body.get("status").is_some());
    assert!(body.get("is_resolved").is_none(), "Sentry has only status");
    assert!(body.get("is_muted").is_none(), "Sentry has only status");

    // Verify short_id format (PROJECT-N)
    let short_id = body["short_id"].as_str().unwrap();
    assert!(short_id.starts_with(&project.slug.to_uppercase()));
    assert!(short_id.contains("-"));
}

// =============================================================================
// Bulk Operations Tests (through the real HTTP handlers, not IssueService
// directly — closes the coverage gap where digest_test.rs's bulk tests only
// exercised the service layer and never touched access::require, the
// resolvedInNextRelease branch, or the size-cap/IDOR checks in routes/issues.rs)
// =============================================================================

#[actix_web::test]
async fn test_bulk_update_issues_via_http_sets_status_and_priority() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Bulk HTTP Project").await;
    let config = crate::common::config();

    let i1 = create_test_issue(&db.pool, project.id, "TypeError", "a").await;
    let i2 = create_test_issue(&db.pool, project.id, "ValueError", "b").await;

    let app = crate::common::app_with_config(&db.pool, config).await;

    let req = test::TestRequest::put()
        .uri(&format!("/api/projects/{}/issues", project.id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({
            "ids": [i1.id, i2.id],
            "status": "resolved",
            "priority": "high",
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["updated"], 2);

    let updated1 = IssueService::get_by_id(&db.pool, i1.id).await.unwrap();
    assert_eq!(updated1.status, "resolved");
    assert_eq!(updated1.priority.as_deref(), Some("high"));
    let updated2 = IssueService::get_by_id(&db.pool, i2.id).await.unwrap();
    assert_eq!(updated2.status, "resolved");
    assert_eq!(updated2.priority.as_deref(), Some("high"));
}

#[actix_web::test]
async fn test_bulk_update_issues_rejects_oversized_batch() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Bulk Oversized Project").await;
    let config = crate::common::config();

    let app = crate::common::app_with_config(&db.pool, config).await;

    // 1001 random ids (over MAX_BULK_IDS = 1000, matching Sentry's own
    // BULK_MUTATION_LIMIT) — none need to exist, the size cap must reject
    // the request before any DB lookup happens.
    let ids: Vec<Uuid> = (0..1001).map(|_| Uuid::new_v4()).collect();
    let req = test::TestRequest::put()
        .uri(&format!("/api/projects/{}/issues", project.id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({ "ids": ids, "status": "resolved" }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn test_bulk_delete_issues_via_http_scoped_to_project() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Bulk Delete HTTP Project").await;
    let other_project = create_test_project(&db.pool, "Other HTTP Project").await;
    let config = crate::common::config();

    let mine = create_test_issue(&db.pool, project.id, "TypeError", "a").await;
    let theirs = create_test_issue(&db.pool, other_project.id, "TypeError", "b").await;

    let app = crate::common::app_with_config(&db.pool, config).await;

    // A bulk-delete request scoped to `project` must not delete an issue
    // belonging to `other_project`, even though its id is included (IDOR
    // check on the bulk path, not just single-issue delete).
    let req = test::TestRequest::delete()
        .uri(&format!("/api/projects/{}/issues", project.id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({ "ids": [mine.id, theirs.id] }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["deleted"], 1);

    assert!(IssueService::get_by_id(&db.pool, mine.id).await.is_err());
    assert!(IssueService::get_by_id(&db.pool, theirs.id).await.is_ok());
}

#[actix_web::test]
async fn test_bulk_delete_issues_by_filter_updates_project_counts() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Bulk Delete Filter Project").await;
    let other_project = create_test_project(&db.pool, "Other Filter Project").await;
    let config = crate::common::config();

    let open = create_test_issue(&db.pool, project.id, "TypeError", "open").await;
    let resolved = create_test_issue(&db.pool, project.id, "TypeError", "resolved").await;
    let other_resolved = create_test_issue(&db.pool, other_project.id, "TypeError", "b").await;
    let mut event_ids = Vec::new();
    for (issue, events) in [(&open, 2), (&resolved, 3), (&other_resolved, 3)] {
        let grouping_id: i32 = sqlx::query_scalar(
            "INSERT INTO groupings (project_id, issue_id, grouping_key, grouping_key_hash)
             VALUES ($1, $2, $3, $4) RETURNING id",
        )
        .bind(issue.project_id)
        .bind(issue.id)
        .bind(issue.id.to_string())
        .bind(issue.id.to_string())
        .fetch_one(&db.pool)
        .await
        .unwrap();
        for _ in 0..events {
            let event_id = EventService::create(
                &db.pool,
                Uuid::new_v4(),
                issue.project_id,
                issue.id,
                grouping_id,
                &json!({ "level": "error" }),
                Utc::now(),
                &create_denormalized_fields("TypeError", "bulk delete", "/api/test"),
                None,
                None,
                None,
            )
            .await
            .unwrap();
            event_ids.push((issue.id, event_id));
        }
        sqlx::query(
            "UPDATE issues SET stored_event_count = $1, digested_event_count = $1 + 1 WHERE id = $2",
        )
        .bind(events)
        .bind(issue.id)
        .execute(&db.pool)
        .await
        .unwrap();
    }
    sqlx::query("UPDATE issues SET status = 'resolved' WHERE id IN ($1, $2)")
        .bind(resolved.id)
        .bind(other_resolved.id)
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE projects SET stored_event_count = 5, digested_event_count = 7 WHERE id = $1",
    )
    .bind(project.id)
    .execute(&db.pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE projects SET stored_event_count = 3, digested_event_count = 4 WHERE id = $1",
    )
    .bind(other_project.id)
    .execute(&db.pool)
    .await
    .unwrap();

    let app = crate::common::app_with_config(&db.pool, config).await;
    let delete = |body: Value| {
        test::TestRequest::delete()
            .uri(&format!("/api/projects/{}/issues", project.id))
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .set_json(body)
            .to_request()
    };

    // Invalid selections must preserve the JSON error contract and delete nothing.
    for selection in [
        json!({}),
        json!({ "ids": [], "filter": "all" }),
        json!({ "filter": "everything" }),
        json!({ "ids": [], "filter": "everything" }),
        json!({ "ids": ["invalid-uuid"] }),
        json!({ "filter": null }),
        json!({ "filter": "resolved", "extra": true }),
    ] {
        let resp = test::call_service(&app, delete(selection)).await;
        assert_eq!(resp.status(), 400);
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "application/json"
        );
        let body: Value = test::read_body_json(resp).await;
        assert_eq!(body["error"]["type"], "ValidationError");
        assert!(body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("ids or filter"));
    }

    let resp = test::call_service(&app, delete(json!({ "filter": "resolved" }))).await;
    assert!(resp.status().is_success());
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["deleted"], 1);

    assert!(IssueService::get_by_id(&db.pool, resolved.id)
        .await
        .is_err());
    assert!(IssueService::get_by_id(&db.pool, open.id).await.is_ok());
    assert!(IssueService::get_by_id(&db.pool, other_resolved.id)
        .await
        .is_ok());
    let project_after = ProjectService::get_by_id(&db.pool, project.id)
        .await
        .unwrap();
    assert_eq!(project_after.stored_event_count, 2);
    assert_eq!(project_after.digested_event_count, 3);
    for (issue_id, event_id) in &event_ids {
        let exists = EventService::get_by_id(&db.pool, *event_id).await.is_ok();
        assert_eq!(exists, *issue_id != resolved.id);
    }

    let resp = test::call_service(&app, delete(json!({ "filter": "all" }))).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["deleted"], 1);
    let project_after = ProjectService::get_by_id(&db.pool, project.id)
        .await
        .unwrap();
    assert_eq!(project_after.stored_event_count, 0);
    assert_eq!(project_after.digested_event_count, 0);
    for (issue_id, event_id) in &event_ids {
        let exists = EventService::get_by_id(&db.pool, *event_id).await.is_ok();
        assert_eq!(exists, *issue_id == other_resolved.id);
    }
    let other_after = ProjectService::get_by_id(&db.pool, other_project.id)
        .await
        .unwrap();
    assert_eq!(other_after.stored_event_count, 3);
    assert_eq!(other_after.digested_event_count, 4);

    let resp = test::call_service(&app, delete(json!({ "filter": "all" }))).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["deleted"], 0);

    // An admin token skips the project check, so a missing project reaches the
    // service. Nothing matches; that is 0 deleted, as with unknown ids.
    let req = test::TestRequest::delete()
        .uri("/api/projects/999999/issues")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({ "filter": "all" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["deleted"], 0);
}

#[actix_web::test]
async fn test_bulk_delete_issues_open_and_muted_filters() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Open and Muted").await;
    let open = create_test_issue(&db.pool, project.id, "TypeError", "open").await;
    let muted = create_test_issue(&db.pool, project.id, "TypeError", "muted").await;
    let resolved = create_test_issue(&db.pool, project.id, "TypeError", "resolved").await;
    for (issue_id, status) in [(muted.id, "ignored"), (resolved.id, "resolved")] {
        sqlx::query("UPDATE issues SET status = $1 WHERE id = $2")
            .bind(status)
            .bind(issue_id)
            .execute(&db.pool)
            .await
            .unwrap();
    }
    let app = crate::common::app(&db.pool).await;

    for (filter, deleted_id, kept_id) in [
        ("open", open.id, muted.id),
        ("muted", muted.id, resolved.id),
    ] {
        let req = test::TestRequest::delete()
            .uri(&format!("/api/projects/{}/issues", project.id))
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .set_json(json!({ "filter": filter }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        let body: Value = test::read_body_json(resp).await;
        assert_eq!(body["deleted"], 1);
        assert!(IssueService::get_by_id(&db.pool, deleted_id).await.is_err());
        assert!(IssueService::get_by_id(&db.pool, kept_id).await.is_ok());
        assert!(IssueService::get_by_id(&db.pool, resolved.id).await.is_ok());
    }
}

#[actix_web::test]
async fn test_bulk_delete_issues_preserves_payload_limit() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Payload Limit").await;
    let issue = create_test_issue(&db.pool, project.id, "TypeError", "preserved").await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::JsonConfig::default().limit(32))
            .service(
                web::scope("/api/projects/{project_id}")
                    .configure(routes::issues::configure_project),
            ),
    )
    .await;
    let req = test::TestRequest::delete()
        .uri(&format!("/api/projects/{}/issues", project.id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(json!({ "ids": [issue.id] }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 413);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["error"]["type"], "PayloadTooLarge");
    assert!(IssueService::get_by_id(&db.pool, issue.id).await.is_ok());
}

#[actix_web::test]
async fn test_get_issue_tag_values_returns_bare_list_not_wrapped() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Tag Values HTTP Project").await;
    let config = crate::common::config();

    let issue = create_test_issue(&db.pool, project.id, "TypeError", "Error").await;

    let app = crate::common::app_with_config(&db.pool, config).await;

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{}/issues/{}/tags/browser",
            project.id, issue.id
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    // Real Sentry returns a bare list per value (`{key, name, value, count,
    // firstSeen/lastSeen}` each), not a `{key, values: [...]}` wrapper —
    // no events with this tag exist yet, so the list is empty, but the
    // response itself must be a JSON array, not an object.
    let body: Value = test::read_body_json(resp).await;
    assert!(
        body.is_array(),
        "expected a bare list response, got: {}",
        body
    );
    assert_eq!(body.as_array().unwrap().len(), 0);
}

/// Lists `project`'s issues with `query` appended, as the dashboard does, and
/// returns the titles in the order they came back.
async fn list_titles(pool: &rustrak::db::DbPool, project_id: i32, query: &str) -> Vec<String> {
    let token = create_test_token(pool).await;
    let app = crate::common::app_with_config(pool, crate::common::config()).await;

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{project_id}/issues?filter=all&{query}"
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success(), "{query}: {}", resp.status());

    let body: Value = test::read_body_json(resp).await;
    body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["title"].as_str().unwrap().to_string())
        .collect()
}

async fn set_issue(pool: &rustrak::db::DbPool, id: Uuid, assignment: &str) {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "UPDATE issues SET {assignment} WHERE id = $1"
    )))
    .bind(id)
    .execute(pool)
    .await
    .expect("update issue failed");
}

#[actix_web::test]
async fn test_list_issues_filters_by_level_and_priority() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Level Filter Project").await;
    let fatal = create_test_issue(&db.pool, project.id, "Fatal", "a").await;
    let warning = create_test_issue(&db.pool, project.id, "Warning", "b").await;
    create_test_issue(&db.pool, project.id, "Error", "c").await;
    set_issue(&db.pool, fatal.id, "level = 'fatal', priority = 'high'").await;
    set_issue(&db.pool, warning.id, "level = 'warning', priority = 'low'").await;

    let mut by_level = list_titles(&db.pool, project.id, "level=fatal,warning").await;
    by_level.sort();
    assert_eq!(by_level, vec!["Fatal: a", "Warning: b"]);

    let by_priority = list_titles(&db.pool, project.id, "priority=low").await;
    assert_eq!(by_priority, vec!["Warning: b"]);
}

#[actix_web::test]
async fn test_an_issue_without_a_level_is_filtered_as_error() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Levelless Project").await;
    let levelless = create_test_issue(&db.pool, project.id, "Levelless", "a").await;
    set_issue(&db.pool, levelless.id, "level = NULL").await;

    // The list shows it as an error, so the Error filter has to find it.
    let by_level = list_titles(&db.pool, project.id, "level=error").await;
    assert_eq!(by_level, vec!["Levelless: a"]);
}

#[actix_web::test]
async fn test_list_issues_filters_by_assignee() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Assignee Filter Project").await;
    let mine = create_test_issue(&db.pool, project.id, "Mine", "a").await;
    create_test_issue(&db.pool, project.id, "Nobodys", "b").await;

    sqlx::query(
        "INSERT INTO users (email, password_hash, is_active, role) VALUES ('assignee@example.com', 'x', true, 'member')",
    )
    .execute(&db.pool)
    .await
    .expect("insert user failed");
    let (user_id,): (i32,) =
        sqlx::query_as("SELECT id FROM users WHERE email = 'assignee@example.com'")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    set_issue(&db.pool, mine.id, &format!("assigned_to = {user_id}")).await;

    let assigned = list_titles(&db.pool, project.id, &format!("assigned={user_id}")).await;
    assert_eq!(assigned, vec!["Mine: a"]);

    let unassigned = list_titles(&db.pool, project.id, "assigned=none").await;
    assert_eq!(unassigned, vec!["Nobodys: b"]);
}

#[actix_web::test]
async fn test_list_issues_filters_by_seen_window_and_event_count() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Seen Filter Project").await;
    let ten_days_ago = Utc::now() - chrono::Duration::days(10);
    create_test_issue_at(&db.pool, project.id, "Old", "a", ten_days_ago).await;
    let loud = create_test_issue(&db.pool, project.id, "Loud", "b").await;
    set_issue(&db.pool, loud.id, "digested_event_count = 500").await;

    assert_eq!(
        list_titles(&db.pool, project.id, "last_seen=7d").await,
        vec!["Loud: b"]
    );
    assert_eq!(
        list_titles(&db.pool, project.id, "first_seen=24h").await,
        vec!["Loud: b"]
    );
    assert_eq!(
        list_titles(&db.pool, project.id, "events_min=100").await,
        vec!["Loud: b"]
    );
    assert_eq!(
        list_titles(&db.pool, project.id, "events_max=100").await,
        vec!["Old: a"]
    );
}

#[actix_web::test]
async fn test_list_issues_sorts_by_first_seen_and_priority() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "First Seen Sort Project").await;
    let ten_days_ago = Utc::now() - chrono::Duration::days(10);
    let old = create_test_issue_at(&db.pool, project.id, "Old", "a", ten_days_ago).await;
    let urgent = create_test_issue(&db.pool, project.id, "Urgent", "b").await;

    set_issue(&db.pool, old.id, "priority = 'low'").await;
    set_issue(&db.pool, urgent.id, "priority = 'high'").await;

    assert_eq!(
        list_titles(&db.pool, project.id, "sort=first_seen&order=asc").await,
        vec!["Old: a", "Urgent: b"]
    );
    assert_eq!(
        list_titles(&db.pool, project.id, "sort=priority&order=desc").await,
        vec!["Urgent: b", "Old: a"]
    );
}
