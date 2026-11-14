//! The instance-wide issue stream (`/api/issues`): every project the reader
//! may read issues on, narrowed by `?project=`, as Sentry's organization
//! issues endpoint.

use crate::common::{create_test_project, create_test_token, grant_project_role, seed_user};
use crate::common::{token_for, TestDb};
use actix_web::test;
use chrono::Utc;
use rustrak::db::DbPool;
use rustrak::models::{Issue, UserRole};
use rustrak::services::grouping::DenormalizedFields;
use rustrak::services::IssueService;
use serde_json::Value;

async fn seed_issue(pool: &DbPool, project_id: i32, calc_type: &str) -> Issue {
    let denormalized = DenormalizedFields {
        calculated_type: calc_type.to_string(),
        calculated_value: "boom".to_string(),
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
    .expect("seed issue")
}

async fn seed_feedback(pool: &DbPool, project_id: i32, message: &str) -> Issue {
    let mut conn = pool.acquire().await.unwrap();
    let denormalized = DenormalizedFields {
        calculated_type: "User Feedback".to_string(),
        calculated_value: message.to_string(),
        transaction: String::new(),
        last_frame_filename: String::new(),
        last_frame_module: String::new(),
        last_frame_function: String::new(),
        culprit: String::new(),
        logger: String::new(),
        release: String::new(),
    };
    IssueService::create_in(
        &mut conn,
        project_id,
        Utc::now(),
        &denormalized,
        Some("info"),
        None,
        "feedback",
    )
    .await
    .expect("seed feedback")
}

async fn get(pool: &DbPool, token: &str, uri: &str) -> (u16, Value) {
    let app = crate::common::app(pool).await;
    let req = test::TestRequest::get()
        .uri(uri)
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status().as_u16();
    (status, test::read_body_json(resp).await)
}

/// The `calculated_type`s of a page's issues, sorted.
fn titles(body: &Value) -> Vec<String> {
    let mut t: Vec<String> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| {
            i["title"]
                .as_str()
                .unwrap()
                .split(':')
                .next()
                .unwrap()
                .to_string()
        })
        .collect();
    t.sort();
    t
}

#[actix_web::test]
async fn lists_issues_from_every_project() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let web = create_test_project(&db.pool, "Web").await;
    let api = create_test_project(&db.pool, "Api").await;
    seed_issue(&db.pool, web.id, "WebError").await;
    seed_issue(&db.pool, api.id, "ApiError").await;

    let (status, body) = get(&db.pool, &token, "/api/issues").await;

    assert_eq!(status, 200);
    assert_eq!(body["total_count"], 2);
    assert_eq!(titles(&body), ["ApiError", "WebError"]);
    let projects: Vec<i64> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["project_id"].as_i64().unwrap())
        .collect();
    assert!(projects.contains(&(web.id as i64)) && projects.contains(&(api.id as i64)));
}

#[actix_web::test]
async fn the_project_filter_narrows_the_stream() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let web = create_test_project(&db.pool, "Web").await;
    let api = create_test_project(&db.pool, "Api").await;
    let worker = create_test_project(&db.pool, "Worker").await;
    seed_issue(&db.pool, web.id, "WebError").await;
    seed_issue(&db.pool, api.id, "ApiError").await;
    seed_issue(&db.pool, worker.id, "WorkerError").await;

    let uri = format!("/api/issues?project={},{}", web.id, worker.id);
    let (status, body) = get(&db.pool, &token, &uri).await;

    assert_eq!(status, 200);
    assert_eq!(titles(&body), ["WebError", "WorkerError"]);
}

/// A member only ever sees the projects they may read issues on, whatever
/// `?project=` names: another project is left out, not a 403, so its
/// existence does not leak.
#[actix_web::test]
async fn a_member_only_sees_projects_they_can_read() {
    let db = TestDb::new().await;
    let web = create_test_project(&db.pool, "Web").await;
    let api = create_test_project(&db.pool, "Api").await;
    seed_issue(&db.pool, web.id, "WebError").await;
    seed_issue(&db.pool, api.id, "ApiError").await;
    let member = seed_user(&db.pool, "m@x.com", UserRole::Member).await;
    grant_project_role(&db.pool, web.id, member.id, "Viewer")
        .await
        .unwrap();
    let token = token_for(&db.pool, member.id).await;

    let (_, all) = get(&db.pool, &token, "/api/issues").await;
    assert_eq!(titles(&all), ["WebError"]);

    let (status, named) = get(&db.pool, &token, &format!("/api/issues?project={}", api.id)).await;
    assert_eq!(status, 200);
    assert_eq!(named["total_count"], 0);
}

#[actix_web::test]
async fn someone_who_reads_no_issues_gets_an_empty_stream() {
    let db = TestDb::new().await;
    let web = create_test_project(&db.pool, "Web").await;
    seed_issue(&db.pool, web.id, "WebError").await;
    let member = seed_user(&db.pool, "m@x.com", UserRole::Member).await;
    let token = token_for(&db.pool, member.id).await;

    let (status, body) = get(&db.pool, &token, "/api/issues").await;

    assert_eq!(status, 200);
    assert_eq!(body["total_count"], 0);
}

#[actix_web::test]
async fn feedback_from_every_project_in_one_inbox() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let web = create_test_project(&db.pool, "Web").await;
    let api = create_test_project(&db.pool, "Api").await;
    seed_feedback(&db.pool, web.id, "love it").await;
    seed_feedback(&db.pool, api.id, "broken").await;
    seed_issue(&db.pool, web.id, "WebError").await;

    let (status, all) = get(&db.pool, &token, "/api/feedback").await;
    assert_eq!(status, 200);
    assert_eq!(all["total_count"], 2);

    let (_, one) = get(
        &db.pool,
        &token,
        &format!("/api/feedback?project={}", api.id),
    )
    .await;
    assert_eq!(one["total_count"], 1);
    assert_eq!(one["items"][0]["project_id"], api.id);
}
