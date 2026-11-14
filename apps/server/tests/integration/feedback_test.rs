//! User Feedback: the `feedback` envelope item (an event carrying
//! `contexts.feedback`) and the legacy `user_report` item, through to the
//! feedback issue, the link to the error it was about, and its alert.
//!
//! Mirrors Sentry: every feedback is its own issue (`FeedbackGroup`, category
//! `feedback`), kept out of the default issue list, and a legacy user report
//! is shimmed into one (`shim_to_feedback.py`).

use crate::common::{process_error_event, TestDb};
use actix_web::dev::{Service, ServiceResponse};
use actix_web::test;
use chrono::Utc;
use rustrak::config::Config;
use rustrak::db::DbPool;
use rustrak::ingest::EventMetadata;
use rustrak::models::{
    AlertRuleChannelInput, AlertType, CreateAlertIntegration, CreateAlertRule, CreateProject,
    ProviderType,
};
use rustrak::pagination::{IssueCategory, IssueFilter, IssueSort, SortOrder};
use rustrak::services::scope::Scope;
use rustrak::services::{AlertService, IssueService, IssueSocialService, ProjectService};
use serde_json::{json, Value};
use std::path::Path;
use uuid::Uuid;

fn config(ingest_dir: &Path) -> Config {
    crate::common::config_with_ingest_dir(ingest_dir)
}

struct Setup {
    db: TestDb,
    project: rustrak::models::Project,
    ingest: tempfile::TempDir,
}

async fn setup() -> Setup {
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: format!("Feedback {}", Uuid::new_v4()),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();
    Setup {
        db,
        project,
        ingest: tempfile::tempdir().unwrap(),
    }
}

fn post_envelope(s: &Setup, body: Vec<u8>) -> actix_http::Request {
    test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", s.project.id))
        .insert_header((
            "X-Sentry-Auth",
            format!(
                "Sentry sentry_key={}, sentry_version=7",
                s.project.sentry_key.simple()
            ),
        ))
        .set_payload(body)
        .to_request()
}

/// An envelope of one item of `item_type`.
fn envelope(event_id: &str, item_type: &str, payload: &Value) -> Vec<u8> {
    let body = payload.to_string();
    format!(
        "{{\"event_id\":\"{event_id}\"}}\n{{\"type\":\"{item_type}\",\"length\":{}}}\n{body}",
        body.len()
    )
    .into_bytes()
}

fn new_id() -> String {
    Uuid::new_v4().simple().to_string()
}

/// A feedback event as the SDKs send it (develop.sentry.dev/sdk/telemetry/feedbacks/).
fn feedback_event(event_id: &str, feedback: Value) -> Value {
    json!({
        "type": "feedback",
        "event_id": event_id,
        "timestamp": Utc::now().timestamp() as f64,
        "platform": "javascript",
        "environment": "production",
        "contexts": { "feedback": feedback },
    })
}

fn error_event(event_id: &str) -> Value {
    json!({
        "event_id": event_id,
        "timestamp": Utc::now().timestamp() as f64,
        "platform": "javascript",
        "level": "error",
        "exception": { "values": [{ "type": "TypeError", "value": "x is undefined" }] },
    })
}

/// Digests `event` the way the ingest path does, synchronously.
async fn digest(s: &Setup, event: &Value) -> rustrak::error::AppResult<()> {
    let event_id = event["event_id"].as_str().unwrap();
    crate::common::store_event(
        s.ingest.path(),
        s.project.id,
        event_id,
        event.to_string().as_bytes(),
    )
    .await
    .unwrap();
    let metadata = EventMetadata {
        event_id: event_id.to_string(),
        project_id: s.project.id,
        ingested_at: Utc::now(),
        remote_addr: None,
    };
    process_error_event(
        &s.db.pool,
        &metadata,
        s.ingest.path(),
        &crate::common::rate_limit(),
        crate::common::null_sourcemap_provider(),
    )
    .await
}

#[derive(sqlx::FromRow, Debug)]
struct IssueRow {
    id: Uuid,
    issue_type: String,
    issue_category: String,
    calculated_type: String,
    calculated_value: String,
    level: Option<String>,
}

async fn issues(pool: &DbPool, project_id: i32) -> Vec<IssueRow> {
    sqlx::query_as(
        "SELECT id, issue_type, issue_category, calculated_type, calculated_value, level \
         FROM issues WHERE project_id = $1 ORDER BY digest_order",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// The project's issues once the spawned digest has stored `count`.
async fn wait_for_issues(pool: &DbPool, project_id: i32, count: usize) -> Vec<IssueRow> {
    crate::common::eventually(|| async {
        let rows = issues(pool, project_id).await;
        (rows.len() >= count).then_some(rows)
    })
    .await
}

#[actix_web::test]
async fn feedback_envelope_creates_a_feedback_issue() {
    let s = setup().await;
    let app = crate::common::app_with_config(&s.db.pool, config(s.ingest.path())).await;
    let id = new_id();
    let event = feedback_event(
        &id,
        json!({ "message": "The checkout button does nothing", "contact_email": "jane@example.com" }),
    );

    let resp = test::call_service(&app, post_envelope(&s, envelope(&id, "feedback", &event))).await;
    assert_eq!(resp.status(), 200);

    let rows = wait_for_issues(&s.db.pool, s.project.id, 1).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].issue_type, "feedback");
    assert_eq!(rows[0].issue_category, "feedback");
    assert_eq!(rows[0].calculated_type, "User Feedback");
    assert_eq!(rows[0].calculated_value, "The checkout button does nothing");
    // Sentry's feedback occurrence defaults to `info`.
    assert_eq!(rows[0].level.as_deref(), Some("info"));
}

#[actix_web::test]
async fn every_feedback_is_its_own_issue() {
    let s = setup().await;
    for _ in 0..2 {
        digest(
            &s,
            &feedback_event(&new_id(), json!({ "message": "Same words" })),
        )
        .await
        .unwrap();
    }
    assert_eq!(issues(&s.db.pool, s.project.id).await.len(), 2);
}

#[actix_web::test]
async fn invalid_feedback_is_dropped() {
    let s = setup().await;
    let too_long = "x".repeat(4097);
    for feedback in [
        json!({}),
        json!({ "message": "   " }),
        json!({ "message": too_long }),
        json!({ "message": "ok", "associated_event_id": "not-a-uuid" }),
    ] {
        assert!(digest(&s, &feedback_event(&new_id(), feedback))
            .await
            .is_err());
    }
    assert!(issues(&s.db.pool, s.project.id).await.is_empty());
}

#[actix_web::test]
async fn a_message_of_exactly_the_limit_is_kept() {
    let s = setup().await;
    // The limit counts characters, not bytes.
    let message = "é".repeat(4096);
    digest(
        &s,
        &feedback_event(&new_id(), json!({ "message": message })),
    )
    .await
    .unwrap();
    assert_eq!(issues(&s.db.pool, s.project.id).await.len(), 1);
}

#[actix_web::test]
async fn feedback_is_linked_to_the_issue_of_its_event() {
    let s = setup().await;
    let error_id = new_id();
    digest(&s, &error_event(&error_id)).await.unwrap();
    let error_issue = issues(&s.db.pool, s.project.id).await.remove(0);

    digest(
        &s,
        &feedback_event(
            &new_id(),
            json!({
                "message": "It crashed when I paid",
                "contact_email": "jane@example.com",
                "name": "Jane",
                "associated_event_id": error_id,
            }),
        ),
    )
    .await
    .unwrap();

    let reports = IssueSocialService::list_user_reports(&s.db.pool, error_issue.id)
        .await
        .unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].comments, "It crashed when I paid");
    assert_eq!(reports[0].email, "jane@example.com");
    assert_eq!(reports[0].name, "Jane");
}

#[actix_web::test]
async fn feedback_before_its_event_is_linked_when_the_event_arrives() {
    let s = setup().await;
    let error_id = new_id();
    digest(
        &s,
        &feedback_event(
            &new_id(),
            json!({ "message": "Broken", "associated_event_id": error_id }),
        ),
    )
    .await
    .unwrap();
    digest(&s, &error_event(&error_id)).await.unwrap();

    let error_issue = issues(&s.db.pool, s.project.id)
        .await
        .into_iter()
        .find(|i| i.issue_category == "error")
        .unwrap();
    let reports = IssueSocialService::list_user_reports(&s.db.pool, error_issue.id)
        .await
        .unwrap();
    assert_eq!(reports.len(), 1);
}

#[actix_web::test]
async fn legacy_user_report_becomes_a_feedback_issue() {
    let s = setup().await;
    let app = crate::common::app_with_config(&s.db.pool, config(s.ingest.path())).await;
    let error_id = new_id();
    digest(&s, &error_event(&error_id)).await.unwrap();

    let report = json!({
        "event_id": error_id,
        "name": "Josh",
        "email": "josh@example.com",
        "comments": "I'm having fun",
    });
    let resp = test::call_service(
        &app,
        post_envelope(&s, envelope(&error_id, "user_report", &report)),
    )
    .await;
    assert_eq!(resp.status(), 200);

    let rows = wait_for_issues(&s.db.pool, s.project.id, 2).await;
    let feedback: Vec<_> = rows
        .iter()
        .filter(|i| i.issue_category == "feedback")
        .collect();
    assert_eq!(feedback.len(), 1);
    assert_eq!(feedback[0].calculated_value, "I'm having fun");

    let error_issue = rows.iter().find(|i| i.issue_category == "error").unwrap();
    let reports = IssueSocialService::list_user_reports(&s.db.pool, error_issue.id)
        .await
        .unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].name, "Josh");
    assert_eq!(reports[0].email, "josh@example.com");
}

#[actix_web::test]
async fn a_retried_user_report_is_one_feedback() {
    let s = setup().await;
    let app = crate::common::app_with_config(&s.db.pool, config(s.ingest.path())).await;
    let error_id = new_id();
    let report = json!({ "event_id": error_id, "comments": "Twice" });
    for _ in 0..2 {
        let resp = test::call_service(
            &app,
            post_envelope(&s, envelope(&error_id, "user_report", &report)),
        )
        .await;
        assert_eq!(resp.status(), 200);
    }
    wait_for_issues(&s.db.pool, s.project.id, 1).await;
    crate::common::eventually(|| async {
        (!crate::common::has_files(s.ingest.path())).then_some(())
    })
    .await;
    assert_eq!(issues(&s.db.pool, s.project.id).await.len(), 1);
}

#[actix_web::test]
async fn two_reports_about_one_event_are_two_feedbacks() {
    let s = setup().await;
    let app = crate::common::app_with_config(&s.db.pool, config(s.ingest.path())).await;
    let error_id = new_id();
    for comments in ["First", "Second"] {
        let report = json!({ "event_id": error_id, "comments": comments });
        let resp = test::call_service(
            &app,
            post_envelope(&s, envelope(&error_id, "user_report", &report)),
        )
        .await;
        assert_eq!(resp.status(), 200);
    }
    assert_eq!(wait_for_issues(&s.db.pool, s.project.id, 2).await.len(), 2);
}

#[actix_web::test]
async fn the_issue_list_shows_one_category_at_a_time() {
    let s = setup().await;
    digest(&s, &error_event(&new_id())).await.unwrap();
    digest(&s, &feedback_event(&new_id(), json!({ "message": "Hi" })))
        .await
        .unwrap();

    let no_filters = rustrak::services::issue::IssueFilters::default();
    let project = rustrak::models::rbac::ProjectScope::Only(vec![s.project.id]);
    let list = |category| {
        IssueService::list_offset(
            &s.db.pool,
            &project,
            IssueSort::LastSeen,
            SortOrder::Desc,
            IssueFilter::All,
            category,
            1,
            50,
            None,
            Scope::default(),
            &no_filters,
        )
    };
    let (errors, total) = list(IssueCategory::Error).await.unwrap();
    assert_eq!((errors.len(), total), (1, 1));
    assert_eq!(errors[0].issue_category, "error");

    let (feedback, total) = list(IssueCategory::Feedback).await.unwrap();
    assert_eq!((feedback.len(), total), (1, 1));
    assert_eq!(feedback[0].issue_category, "feedback");
}

async fn rule(s: &Setup, alert_type: AlertType, conditions: Value) {
    let channel = AlertService::create_channel(
        &s.db.pool,
        &Default::default(),
        CreateAlertIntegration {
            name: format!("sink {alert_type}"),
            provider_type: ProviderType::Webhook,
            credentials: json!({ "url": "http://127.0.0.1:9/unused" }),
            is_enabled: true,
        },
    )
    .await
    .unwrap();
    AlertService::create_rule(
        &s.db.pool,
        s.project.id,
        CreateAlertRule {
            name: alert_type.to_string(),
            alert_type,
            conditions,
            cooldown_minutes: 0,
            channels: vec![AlertRuleChannelInput {
                integration_id: channel.id,
                routing_override: json!({}),
            }],
        },
    )
    .await
    .unwrap();
}

async fn fired(pool: &DbPool, project_id: i32) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT alert_type FROM alert_history WHERE project_id = $1 ORDER BY alert_type",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

#[actix_web::test]
async fn feedback_fires_new_feedback_rules_and_errors_fire_new_issue_rules() {
    let s = setup().await;
    rule(&s, AlertType::NewIssue, json!({})).await;
    rule(&s, AlertType::NewFeedback, json!({})).await;

    digest(&s, &feedback_event(&new_id(), json!({ "message": "Hi" })))
        .await
        .unwrap();
    assert_eq!(fired(&s.db.pool, s.project.id).await, ["new_feedback"]);

    digest(&s, &error_event(&new_id())).await.unwrap();
    assert_eq!(
        fired(&s.db.pool, s.project.id).await,
        ["new_feedback", "new_issue"]
    );
}

#[actix_web::test]
async fn a_feedback_without_a_level_is_info_everywhere() {
    let s = setup().await;
    rule(&s, AlertType::NewFeedback, json!({ "min_level": "error" })).await;

    digest(&s, &feedback_event(&new_id(), json!({ "message": "Hi" })))
        .await
        .unwrap();
    assert!(fired(&s.db.pool, s.project.id).await.is_empty());
    let level: String = sqlx::query_scalar("SELECT level FROM events WHERE project_id = $1")
        .bind(s.project.id)
        .fetch_one(&s.db.pool)
        .await
        .unwrap();
    assert_eq!(level, "info");
}

#[actix_web::test]
async fn feedback_is_not_counted_as_an_issue() {
    use rustrak::services::StatsService;
    let s = setup().await;
    digest(&s, &error_event(&new_id())).await.unwrap();
    digest(&s, &feedback_event(&new_id(), json!({ "message": "Hi" })))
        .await
        .unwrap();

    let summary =
        StatsService::project_summary(&s.db.pool, s.project.id, Some(24), Scope::default())
            .await
            .unwrap();
    assert_eq!(summary.open_issues, 1);
    assert_eq!(summary.new_issues.current, 1);
    let all_time = StatsService::project_summary(&s.db.pool, s.project.id, None, Scope::default())
        .await
        .unwrap();
    assert_eq!(all_time.new_issues.current, 1);

    let list = StatsService::list_stats(&s.db.pool, &[s.project.id], 24)
        .await
        .unwrap();
    assert_eq!(list[&s.project.id].open_issues, 1);
    assert_eq!(list[&s.project.id].new_issues.current, 1);
}

#[actix_web::test]
async fn feedback_is_not_counted_as_an_error_event() {
    use rustrak::services::StatsService;
    let s = setup().await;
    digest(&s, &error_event(&new_id())).await.unwrap();
    digest(&s, &feedback_event(&new_id(), json!({ "message": "Hi" })))
        .await
        .unwrap();

    let summary =
        StatsService::project_summary(&s.db.pool, s.project.id, Some(24), Scope::default())
            .await
            .unwrap();
    assert_eq!(summary.events.current, 1);
    let list = StatsService::list_stats(&s.db.pool, &[s.project.id], 24)
        .await
        .unwrap();
    assert_eq!(list[&s.project.id].events.current, 1);
    let chart = StatsService::event_timeseries(&s.db.pool, s.project.id, None, 1, Scope::default())
        .await
        .unwrap();
    assert_eq!(chart.iter().map(|p| p.total).sum::<i64>(), 1);
}

/// Sentry's `POST /api/0/projects/{org}/{project}/user-feedback/`
/// (`ProjectUserReportsEndpoint`), with DSN or token auth.
fn post_user_feedback(s: &Setup, auth: String, body: &Value) -> actix_http::Request {
    test::TestRequest::post()
        .uri(&format!(
            "/api/0/projects/rustrak/{}/user-feedback/",
            s.project.slug
        ))
        .insert_header(("Authorization", auth))
        .set_json(body)
        .to_request()
}

fn dsn(s: &Setup, key: &str) -> String {
    format!("DSN http://{key}@localhost:8080/{}", s.project.id)
}

#[actix_web::test]
async fn the_user_feedback_endpoint_takes_a_report_with_dsn_auth() {
    let s = setup().await;
    let app = crate::common::app_with_config(&s.db.pool, config(s.ingest.path())).await;
    let error_id = new_id();
    digest(&s, &error_event(&error_id)).await.unwrap();

    let report = json!({ "event_id": error_id, "name": "Ana", "email": "ana@example.com", "comments": "Blank page" });
    let key = s.project.sentry_key.simple().to_string();
    let resp = test::call_service(&app, post_user_feedback(&s, dsn(&s, &key), &report)).await;
    assert_eq!(resp.status(), 200);

    let rows = wait_for_issues(&s.db.pool, s.project.id, 2).await;
    let error_issue = rows.iter().find(|i| i.issue_category == "error").unwrap();
    assert!(rows.iter().any(|i| i.calculated_value == "Blank page"));
    let reports = IssueSocialService::list_user_reports(&s.db.pool, error_issue.id)
        .await
        .unwrap();
    assert_eq!(reports[0].name, "Ana");
}

#[actix_web::test]
async fn the_user_feedback_endpoint_rejects_another_projects_dsn() {
    let s = setup().await;
    let app = crate::common::app_with_config(&s.db.pool, config(s.ingest.path())).await;
    let report = json!({ "event_id": new_id(), "comments": "x" });
    let wrong = Uuid::new_v4().simple().to_string();
    let resp = test::call_service(&app, post_user_feedback(&s, dsn(&s, &wrong), &report)).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn the_user_feedback_endpoint_takes_a_token_and_returns_the_report() {
    let s = setup().await;
    let app = crate::common::app_with_config(&s.db.pool, config(s.ingest.path())).await;
    let token = rustrak::services::AuthTokenService::create(
        &s.db.pool,
        rustrak::models::CreateAuthToken::default(),
    )
    .await
    .unwrap()
    .token;
    let error_id = new_id();
    let report = json!({ "event_id": error_id, "name": "Ana", "email": "ana@example.com", "comments": "Blank page" });
    let resp = test::call_service(
        &app,
        post_user_feedback(&s, format!("Bearer {token}"), &report),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["eventID"], error_id);
    assert_eq!(body["name"], "Ana");
    assert_eq!(body["email"], "ana@example.com");
    assert_eq!(body["comments"], "Blank page");

    // An invalid report is a 400, not a silent drop: nothing else tells the caller.
    for bad in [
        json!({ "comments": "no event" }),
        json!({ "event_id": new_id(), "comments": "   " }),
        json!({ "event_id": new_id(), "comments": "x".repeat(4097) }),
    ] {
        let resp = test::call_service(
            &app,
            post_user_feedback(&s, format!("Bearer {token}"), &bad),
        )
        .await;
        assert_eq!(resp.status(), 400);
    }
}

async fn admin_token(pool: &DbPool) -> String {
    rustrak::services::AuthTokenService::create(pool, rustrak::models::CreateAuthToken::default())
        .await
        .unwrap()
        .token
}

async fn get_feedback(
    app: &impl Service<actix_http::Request, Response = ServiceResponse, Error = actix_web::Error>,
    s: &Setup,
    token: &str,
    query: &str,
) -> Value {
    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/feedback{query}", s.project.id))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let resp = test::call_service(app, req).await;
    assert_eq!(resp.status(), 200);
    test::read_body_json(resp).await
}

#[actix_web::test]
async fn the_feedback_list_carries_what_the_user_submitted() {
    let s = setup().await;
    let app = crate::common::app_with_config(&s.db.pool, config(s.ingest.path())).await;
    let token = admin_token(&s.db.pool).await;
    let error_id = new_id();
    digest(&s, &error_event(&error_id)).await.unwrap();
    digest(
        &s,
        &feedback_event(
            &new_id(),
            json!({
                "message": "Checkout froze",
                "name": "Jane",
                "contact_email": "jane@example.com",
                "url": "https://shop.example/checkout",
                "associated_event_id": error_id,
            }),
        ),
    )
    .await
    .unwrap();
    digest(
        &s,
        &feedback_event(&new_id(), json!({ "message": "Spam spam" })),
    )
    .await
    .unwrap();
    let spam = issues(&s.db.pool, s.project.id)
        .await
        .into_iter()
        .find(|i| i.calculated_value == "Spam spam")
        .unwrap();
    IssueService::set_status(&s.db.pool, spam.id, "ignored", None)
        .await
        .unwrap();

    let body = get_feedback(&app, &s, &token, "").await;
    assert_eq!(body["total_count"], 1, "the inbox holds open feedback only");
    let item = &body["items"][0];
    assert_eq!(item["message"], "Checkout froze");
    assert_eq!(item["name"], "Jane");
    assert_eq!(item["contact_email"], "jane@example.com");
    assert_eq!(item["url"], "https://shop.example/checkout");
    assert_eq!(item["associated_event_id"], error_id);
    assert_eq!(item["status"], "unresolved");

    let body = get_feedback(&app, &s, &token, "?filter=muted").await;
    assert_eq!(body["items"][0]["message"], "Spam spam");
    assert!(body["items"][0]["associated_event_id"].is_null());
}

// --- Attachments (screenshots) ---------------------------------------------

/// A PNG signature, then bytes a JSON scrub or a text decode would mangle.
const SCREENSHOT: &[u8] = b"\x89PNG\r\n\x1a\n\x00\\u0000\n\xff\xfe binary";

/// An envelope of one item, plus an `attachment` item after it.
fn envelope_with_attachment(
    event_id: &str,
    item_type: &str,
    payload: &Value,
    filename: &str,
    content_type: &str,
    data: &[u8],
) -> Vec<u8> {
    let mut body = envelope(event_id, item_type, payload);
    body.extend_from_slice(
        format!(
            "\n{{\"type\":\"attachment\",\"length\":{},\"filename\":\"{filename}\",\"content_type\":\"{content_type}\"}}\n",
            data.len()
        )
        .as_bytes(),
    );
    body.extend_from_slice(data);
    body
}

async fn attachment_count(pool: &DbPool, project_id: i32) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM event_attachments WHERE project_id = $1")
        .bind(project_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[actix_web::test]
async fn the_reports_of_an_issue_come_a_page_at_a_time() {
    let s = setup().await;
    let app = crate::common::app_with_config(&s.db.pool, config(s.ingest.path())).await;
    let token = admin_token(&s.db.pool).await;
    digest(&s, &error_event(&new_id())).await.unwrap();
    let issue = issues(&s.db.pool, s.project.id).await.remove(0);
    for comments in ["First", "Second", "Third"] {
        IssueSocialService::create_user_report(
            &s.db.pool,
            s.project.id,
            Some(issue.id),
            None,
            "",
            "",
            comments,
        )
        .await
        .unwrap();
    }

    let uri = |query: &str| {
        format!(
            "/api/projects/{}/issues/{}/user-reports{query}",
            s.project.id, issue.id
        )
    };
    let resp = get_raw(&app, &uri("?per_page=2"), &token).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["items"].as_array().unwrap().len(), 2);
    assert_eq!(body["total_count"], 3);
    assert_eq!(body["total_pages"], 2);

    let body: Value =
        test::read_body_json(get_raw(&app, &uri("?per_page=2&page=2"), &token).await).await;
    assert_eq!(body["items"].as_array().unwrap().len(), 1);

    // A page past any offset is an empty page, not an overflow.
    let resp = get_raw(
        &app,
        &uri(&format!("?per_page=100&page={}", i64::MAX)),
        &token,
    )
    .await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert!(body["items"].as_array().unwrap().is_empty());
}

async fn get_raw(
    app: &impl Service<actix_http::Request, Response = ServiceResponse, Error = actix_web::Error>,
    uri: &str,
    token: &str,
) -> ServiceResponse {
    let req = test::TestRequest::get()
        .uri(uri)
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    test::call_service(app, req).await
}

/// Sends a feedback with one attachment and waits for its issue.
async fn feedback_with_attachment(
    s: &Setup,
    app: &impl Service<actix_http::Request, Response = ServiceResponse, Error = actix_web::Error>,
    filename: &str,
    content_type: &str,
    data: &[u8],
) {
    let id = new_id();
    let event = feedback_event(&id, json!({ "message": "See the screenshot" }));
    let resp = test::call_service(
        app,
        post_envelope(
            s,
            envelope_with_attachment(&id, "feedback", &event, filename, content_type, data),
        ),
    )
    .await;
    assert_eq!(resp.status(), 200);
    wait_for_issues(&s.db.pool, s.project.id, 1).await;
}

#[actix_web::test]
async fn the_storage_page_counts_attachment_bytes() {
    use rustrak::services::StorageService;
    let s = setup().await;
    let app = crate::common::app_with_config(&s.db.pool, config(s.ingest.path())).await;
    let screenshot = vec![0u8; 100_000];
    feedback_with_attachment(&s, &app, "screenshot.png", "image/png", &screenshot).await;

    let projects = StorageService::by_project(&s.db.pool).await.unwrap();
    let project = projects
        .iter()
        .find(|p| p.project_id == s.project.id)
        .unwrap();
    assert!(project.events_bytes > screenshot.len() as i64);
    assert_eq!(
        project.estimated_bytes,
        project.events_bytes + project.transactions_bytes + project.logs_bytes
    );
}

#[actix_web::test]
async fn a_feedback_screenshot_is_kept_and_served_byte_for_byte() {
    let s = setup().await;
    let app = crate::common::app_with_config(&s.db.pool, config(s.ingest.path())).await;
    let token = admin_token(&s.db.pool).await;
    feedback_with_attachment(&s, &app, "screenshot.png", "image/png", SCREENSHOT).await;

    let body = get_feedback(&app, &s, &token, "").await;
    let attachment = &body["items"][0]["attachments"][0];
    assert_eq!(attachment["filename"], "screenshot.png");
    assert_eq!(attachment["content_type"], "image/png");
    assert_eq!(attachment["size"], SCREENSHOT.len());

    let resp = get_raw(
        &app,
        &format!(
            "/api/projects/{}/attachments/{}",
            s.project.id,
            attachment["id"].as_str().unwrap()
        ),
        &token,
    )
    .await;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers().get("content-type").unwrap(), "image/png");
    assert_eq!(
        resp.headers().get("x-content-type-options").unwrap(),
        "nosniff"
    );
    assert_eq!(
        resp.headers().get("content-security-policy").unwrap(),
        "sandbox"
    );
    assert_eq!(test::read_body(resp).await.as_ref(), SCREENSHOT);
}

#[actix_web::test]
async fn an_event_lists_its_attachments() {
    let s = setup().await;
    let app = crate::common::app_with_config(&s.db.pool, config(s.ingest.path())).await;
    let token = admin_token(&s.db.pool).await;
    feedback_with_attachment(&s, &app, "screenshot.png", "image/png", SCREENSHOT).await;
    let issue = issues(&s.db.pool, s.project.id).await.remove(0);
    let event_id: Uuid = sqlx::query_scalar("SELECT id FROM events WHERE issue_id = $1")
        .bind(issue.id)
        .fetch_one(&s.db.pool)
        .await
        .unwrap();

    let resp = get_raw(
        &app,
        &format!(
            "/api/projects/{}/issues/{}/events/{event_id}/attachments",
            s.project.id, issue.id
        ),
        &token,
    )
    .await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body[0]["filename"], "screenshot.png");
}

#[actix_web::test]
async fn an_attachment_that_is_not_an_image_is_only_ever_downloaded() {
    let s = setup().await;
    let app = crate::common::app_with_config(&s.db.pool, config(s.ingest.path())).await;
    let token = admin_token(&s.db.pool).await;
    // Claims to be an image; an SVG served inline would run its script.
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>"#;
    feedback_with_attachment(&s, &app, "shot.svg", "image/svg+xml", svg).await;

    let body = get_feedback(&app, &s, &token, "").await;
    let id = body["items"][0]["attachments"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let resp = get_raw(
        &app,
        &format!("/api/projects/{}/attachments/{id}", s.project.id),
        &token,
    )
    .await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/octet-stream"
    );
    assert!(resp
        .headers()
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("attachment"));
}

#[actix_web::test]
async fn an_attachment_keeps_its_unicode_filename() {
    let s = setup().await;
    let app = crate::common::app_with_config(&s.db.pool, config(s.ingest.path())).await;
    let token = admin_token(&s.db.pool).await;
    feedback_with_attachment(&s, &app, "café.png", "image/png", SCREENSHOT).await;

    let body = get_feedback(&app, &s, &token, "").await;
    let id = body["items"][0]["attachments"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let resp = get_raw(
        &app,
        &format!("/api/projects/{}/attachments/{id}", s.project.id),
        &token,
    )
    .await;
    assert_eq!(
        resp.headers().get("content-disposition").unwrap(),
        "inline; filename=\"caf_.png\"; filename*=UTF-8''caf%C3%A9.png"
    );
}

#[actix_web::test]
async fn an_attachment_is_private_to_its_project() {
    let s = setup().await;
    let app = crate::common::app_with_config(&s.db.pool, config(s.ingest.path())).await;
    let token = admin_token(&s.db.pool).await;
    feedback_with_attachment(&s, &app, "screenshot.png", "image/png", SCREENSHOT).await;
    let body = get_feedback(&app, &s, &token, "").await;
    let id = body["items"][0]["attachments"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let other = ProjectService::create(
        &s.db.pool,
        CreateProject {
            name: format!("Other {}", Uuid::new_v4()),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();
    let resp = get_raw(
        &app,
        &format!("/api/projects/{}/attachments/{id}", other.id),
        &token,
    )
    .await;
    assert_eq!(resp.status(), 404);
}

/// Sentry keeps every attachment an error event is sent with, whatever its
/// `attachment_type` (a plain file, a minidump, a view hierarchy...).
#[actix_web::test]
async fn attachments_of_an_error_event_are_kept_listed_and_downloadable() {
    let s = setup().await;
    let app = crate::common::app_with_config(&s.db.pool, config(s.ingest.path())).await;
    let token = admin_token(&s.db.pool).await;
    let id = new_id();
    let mut body = envelope_with_attachment(
        &id,
        "event",
        &error_event(&id),
        "log.txt",
        "text/plain",
        b"log",
    );
    let hierarchy = br#"{"rendering_system":"UIKIT","windows":[]}"#;
    body.extend_from_slice(
        format!(
            "\n{{\"type\":\"attachment\",\"length\":{},\"filename\":\"view-hierarchy.json\",\"content_type\":\"application/json\",\"attachment_type\":\"event.view_hierarchy\"}}\n",
            hierarchy.len()
        )
        .as_bytes(),
    );
    body.extend_from_slice(hierarchy);
    let resp = test::call_service(&app, post_envelope(&s, body)).await;
    assert_eq!(resp.status(), 200);
    wait_for_issues(&s.db.pool, s.project.id, 1).await;

    let issue = issues(&s.db.pool, s.project.id).await.remove(0);
    let event_id: Uuid = sqlx::query_scalar("SELECT id FROM events WHERE issue_id = $1")
        .bind(issue.id)
        .fetch_one(&s.db.pool)
        .await
        .unwrap();
    let resp = get_raw(
        &app,
        &format!(
            "/api/projects/{}/issues/{}/events/{event_id}/attachments",
            s.project.id, issue.id
        ),
        &token,
    )
    .await;
    assert_eq!(resp.status(), 200);
    let listed: Value = test::read_body_json(resp).await;
    let mut names: Vec<&str> = listed
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["filename"].as_str().unwrap())
        .collect();
    names.sort();
    assert_eq!(names, ["log.txt", "view-hierarchy.json"]);

    let log = listed
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["filename"] == "log.txt")
        .unwrap();
    let resp = get_raw(
        &app,
        &format!(
            "/api/projects/{}/attachments/{}",
            s.project.id,
            log["id"].as_str().unwrap()
        ),
        &token,
    )
    .await;
    assert_eq!(resp.status(), 200);
    assert_eq!(test::read_body(resp).await.as_ref(), b"log");
    // Never in the event's own data.
    let data: String = sqlx::query_scalar("SELECT CAST(data AS TEXT) FROM events WHERE id = $1")
        .bind(event_id)
        .fetch_one(&s.db.pool)
        .await
        .unwrap();
    assert!(!data.contains("_rustrak_attachments"));
}

/// Relay trims an event on the event's own size: a small event carrying a
/// large attachment keeps its breadcrumb data whole.
#[actix_web::test]
async fn an_attachment_does_not_make_its_event_trimmed() {
    let s = setup().await;
    let app = crate::common::app_with_config(&s.db.pool, config(s.ingest.path())).await;
    let id = new_id();
    let crumb = "c".repeat(4096);
    let mut event = error_event(&id);
    event["breadcrumbs"] = json!({ "values": [{ "message": "m", "data": { "payload": crumb } }] });
    let body = envelope_with_attachment(
        &id,
        "event",
        &event,
        "big.bin",
        "application/octet-stream",
        &vec![b'a'; 900 * 1024],
    );
    let resp = test::call_service(&app, post_envelope(&s, body)).await;
    assert_eq!(resp.status(), 200);
    wait_for_issues(&s.db.pool, s.project.id, 1).await;

    let data: String = sqlx::query_scalar("SELECT CAST(data AS TEXT) FROM events")
        .fetch_one(&s.db.pool)
        .await
        .unwrap();
    assert!(data.contains(&crumb), "breadcrumb data was trimmed");
}

#[actix_web::test]
async fn deleting_a_feedback_deletes_its_screenshot() {
    let s = setup().await;
    let app = crate::common::app_with_config(&s.db.pool, config(s.ingest.path())).await;
    feedback_with_attachment(&s, &app, "screenshot.png", "image/png", SCREENSHOT).await;
    assert_eq!(attachment_count(&s.db.pool, s.project.id).await, 1);

    let issue = issues(&s.db.pool, s.project.id).await.remove(0);
    IssueService::delete(&s.db.pool, issue.id, &errors(&s.db.pool))
        .await
        .unwrap();
    assert_eq!(attachment_count(&s.db.pool, s.project.id).await, 0);
}

#[actix_web::test]
async fn an_oversized_attachment_is_dropped_and_the_feedback_kept() {
    let s = setup().await;
    let app = crate::common::app_with_config(&s.db.pool, config(s.ingest.path())).await;
    let huge = vec![0u8; rustrak::ingest::MAX_ATTACHMENT_SIZE + 1];
    feedback_with_attachment(&s, &app, "huge.png", "image/png", &huge).await;
    assert_eq!(issues(&s.db.pool, s.project.id).await.len(), 1);
    assert_eq!(attachment_count(&s.db.pool, s.project.id).await, 0);
}

#[actix_web::test]
async fn the_storage_cleanup_removes_old_feedback_and_its_screenshot() {
    use rustrak::models::CleanupFilter;
    use rustrak::services::StorageService;
    let s = setup().await;
    let app = crate::common::app_with_config(&s.db.pool, config(s.ingest.path())).await;
    feedback_with_attachment(&s, &app, "screenshot.png", "image/png", SCREENSHOT).await;
    sqlx::query("UPDATE events SET ingested_at = $1 WHERE project_id = $2")
        .bind(Utc::now() - chrono::Duration::days(40))
        .bind(s.project.id)
        .execute(&s.db.pool)
        .await
        .unwrap();

    StorageService::execute_cleanup(&s.db.pool, 30, None, CleanupFilter::all())
        .await
        .unwrap();
    assert!(issues(&s.db.pool, s.project.id).await.is_empty());
    assert_eq!(attachment_count(&s.db.pool, s.project.id).await, 0);
}

async fn user_report_count(pool: &DbPool, project_id: i32) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM user_reports WHERE project_id = $1")
        .bind(project_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// An error and a feedback about it; returns (error issue, feedback issue).
async fn error_with_feedback(s: &Setup) -> (Uuid, Uuid) {
    let error_id = new_id();
    digest(s, &error_event(&error_id)).await.unwrap();
    digest(
        s,
        &feedback_event(
            &new_id(),
            json!({ "message": "Broken", "associated_event_id": error_id }),
        ),
    )
    .await
    .unwrap();
    let rows = issues(&s.db.pool, s.project.id).await;
    let id = |category: &str| {
        rows.iter()
            .find(|i| i.issue_category == category)
            .unwrap()
            .id
    };
    (id("error"), id("feedback"))
}

/// A digest processor whose write turn a delete takes.
fn errors(pool: &DbPool) -> rustrak::digest::processors::ErrorProcessor {
    crate::common::processors_with_config(pool, &Config::default()).errors
}

#[actix_web::test]
async fn deleting_a_feedback_removes_it_from_its_error() {
    let s = setup().await;
    let (error_issue, feedback_issue) = error_with_feedback(&s).await;
    IssueService::delete(&s.db.pool, feedback_issue, &errors(&s.db.pool))
        .await
        .unwrap();

    let reports = IssueSocialService::list_user_reports(&s.db.pool, error_issue)
        .await
        .unwrap();
    assert!(reports.is_empty());
    assert_eq!(user_report_count(&s.db.pool, s.project.id).await, 0);
}

#[actix_web::test]
async fn deleting_an_error_deletes_the_reports_about_it() {
    let s = setup().await;
    let (error_issue, _) = error_with_feedback(&s).await;
    IssueService::delete(&s.db.pool, error_issue, &errors(&s.db.pool))
        .await
        .unwrap();
    // Not left behind with no issue, where nothing would ever show or clean it.
    assert_eq!(user_report_count(&s.db.pool, s.project.id).await, 0);
}

// --- Attachments over MCP ---------------------------------------------------

/// The first attachment of the project's newest feedback, read over MCP.
async fn get_attachment_over_mcp(
    s: &Setup,
    app: &impl Service<actix_http::Request, Response = ServiceResponse, Error = actix_web::Error>,
    token: &str,
) -> Value {
    use crate::common::{call_tool, set_mcp};
    test::call_service(app, set_mcp(token, true).to_request()).await;
    let body = get_feedback(app, s, token, "").await;
    let id = body["items"][0]["attachments"][0]["id"].clone();
    test::call_and_read_body_json(
        app,
        call_tool(
            Some(token),
            "get_attachment",
            json!({ "project_id": s.project.id, "attachment_id": id }),
        )
        .to_request(),
    )
    .await
}

#[actix_web::test]
async fn an_image_attachment_is_an_image_over_mcp() {
    use base64::Engine as _;
    let s = setup().await;
    let app = crate::common::app_with_config(&s.db.pool, config(s.ingest.path())).await;
    let token = admin_token(&s.db.pool).await;
    let png = b"\x89PNG\r\n\x1a\nnot really the rest of a png";
    feedback_with_attachment(&s, &app, "screenshot.png", "image/png", png).await;

    let body = get_attachment_over_mcp(&s, &app, &token).await;
    let content = &body["result"]["content"][0];
    assert_eq!(content["type"], "image", "{body}");
    assert_eq!(content["mimeType"], "image/png");
    assert_eq!(
        content["data"],
        base64::engine::general_purpose::STANDARD.encode(png)
    );
}

#[actix_web::test]
async fn an_attachment_that_is_not_an_image_is_described_over_mcp() {
    let s = setup().await;
    let app = crate::common::app_with_config(&s.db.pool, config(s.ingest.path())).await;
    let token = admin_token(&s.db.pool).await;
    // Claims to be an image; its bytes are not one.
    feedback_with_attachment(&s, &app, "shot.svg", "image/svg+xml", b"<svg/>").await;

    let body = get_attachment_over_mcp(&s, &app, &token).await;
    let content = &body["result"]["content"][0];
    assert_eq!(content["type"], "text", "{body}");
    assert!(content["text"].as_str().unwrap().contains("not an image"));
    assert_eq!(body["result"]["isError"], false);
}
