//! Auto resolve (GH #621): an unresolved issue that goes `resolve_age` hours
//! without an event is resolved by the scheduler.
//!
//! Mirrors Sentry's `auto_resolve_project_issues`
//! (`src/sentry/tasks/auto_resolve_issues.py`).

use super::digest_test::digest_in_order;
use crate::common::{create_test_project, create_test_token, TestDb};
use chrono::{DateTime, Duration, Utc};
use rustrak::db::DbPool;
use rustrak::models::{Issue, Project, UpdateProject};
use rustrak::scheduler::engine::{JobContext, ScheduledJob};
use rustrak::scheduler::jobs::AutoResolveJob;
use rustrak::services::{IssueService, ProjectService};
use serde_json::{json, Value};
use tempfile::TempDir;
use uuid::Uuid;

struct Env {
    db: TestDb,
    project: Project,
    dir: TempDir,
    /// The clock the job runs at; events are placed relative to it.
    now: DateTime<Utc>,
}

impl Env {
    /// A project with auto resolve after `resolve_age` hours, if any.
    async fn new(resolve_age: Option<i64>) -> Self {
        let db = TestDb::new().await;
        let project = create_test_project(&db.pool, "Auto Resolve").await;
        let project = ProjectService::update(
            &db.pool,
            project.id,
            UpdateProject {
                resolve_age: Some(resolve_age),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        Self {
            db,
            project,
            dir: TempDir::new().unwrap(),
            // Whole seconds: an event's timestamp travels as epoch seconds.
            now: DateTime::from_timestamp(Utc::now().timestamp(), 0).unwrap(),
        }
    }

    fn pool(&self) -> &DbPool {
        &self.db.pool
    }

    /// One event of the issue `name`, `hours_ago` before the job's clock.
    /// Returns that issue.
    async fn event(&self, name: &str, hours_ago: i64) -> Issue {
        let event = json!({
            "platform": "rust",
            "message": name,
            "fingerprint": [name],
        });
        digest_in_order(
            self.pool(),
            self.dir.path(),
            self.project.id,
            vec![(self.now - Duration::hours(hours_ago), event)],
        )
        .await;
        self.issue(name).await
    }

    async fn issue(&self, name: &str) -> Issue {
        let id: Uuid = sqlx::query_scalar(
            "SELECT id FROM issues WHERE project_id = $1 AND calculated_value = $2",
        )
        .bind(self.project.id)
        .bind(name)
        .fetch_one(self.pool())
        .await
        .expect("issue");
        IssueService::get_by_id(self.pool(), id).await.unwrap()
    }

    /// Sets an issue's status through the API, as a person does.
    async fn set_status(&self, issue: &Issue, status: &str) {
        let token = create_test_token(self.pool()).await;
        let app = crate::common::app(self.pool()).await;
        let req = actix_web::test::TestRequest::patch()
            .uri(&format!(
                "/api/projects/{}/issues/{}",
                self.project.id, issue.id
            ))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({ "status": status }))
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    /// One run of the job at the test's clock.
    async fn run(&self) -> Value {
        AutoResolveJob
            .run(JobContext {
                pool: self.pool(),
                settings: &json!({}),
                scheduled_for: self.now,
                now: self.now,
            })
            .await
            .expect("auto resolve run")
    }
}

#[actix_web::test]
async fn resolves_an_issue_unseen_for_longer_than_the_age_and_keeps_a_recent_one() {
    let env = Env::new(Some(24)).await;
    env.event("stale", 25).await;
    env.event("recent", 1).await;

    env.run().await;

    assert_eq!(env.issue("stale").await.status, "resolved");
    assert_eq!(env.issue("recent").await.status, "unresolved");
}

/// Off by default: a project without an age keeps every issue open.
#[actix_web::test]
async fn a_project_without_an_age_keeps_its_issues() {
    let env = Env::new(None).await;
    env.event("stale", 24 * 30).await;

    env.run().await;

    assert_eq!(env.issue("stale").await.status, "unresolved");
}

/// User feedback is never auto resolved, as Sentry's `FeedbackGroup`
/// (`enable_auto_resolve = False`).
#[actix_web::test]
async fn user_feedback_is_not_auto_resolved() {
    let env = Env::new(Some(24)).await;
    let feedback = env.event("feedback", 25).await;
    sqlx::query(
        "UPDATE issues SET issue_category = 'feedback', issue_type = 'feedback' WHERE id = $1",
    )
    .bind(feedback.id)
    .execute(env.pool())
    .await
    .unwrap();

    env.run().await;

    assert_eq!(env.issue("feedback").await.status, "unresolved");
}

/// Reopening an issue by hand grants it another full age before it is
/// resolved again (Sentry's `get_recent_unresolved_activities`).
#[actix_web::test]
async fn an_issue_reopened_within_the_age_is_kept() {
    let env = Env::new(Some(24)).await;
    let issue = env.event("reopened", 25).await;
    env.set_status(&issue, "resolved").await;
    env.set_status(&issue, "unresolved").await;

    env.run().await;

    assert_eq!(env.issue("reopened").await.status, "unresolved");
}

/// The issue's activity says it was resolved by age, and after how long, as
/// Sentry's `SET_RESOLVED_BY_AGE` with `{"age": hours}`.
#[actix_web::test]
async fn an_auto_resolved_issue_records_why() {
    let env = Env::new(Some(24)).await;
    let issue = env.event("stale", 25).await;

    env.run().await;

    let activity = rustrak::services::IssueSocialService::list_activity(env.pool(), issue.id)
        .await
        .unwrap();
    let by_age: Vec<Value> = activity
        .iter()
        .filter(|a| a.activity_type == "set_resolved_by_age")
        .map(|a| serde_json::from_str(&a.data).unwrap())
        .collect();
    assert_eq!(by_age, vec![json!({ "age": 24 })]);
    assert!(activity.iter().all(|a| a.user_id.is_none()));
}

/// Resolved in batches so one run never holds the write lock for long
/// (Sentry's `chunk_size`), and a run keeps going until none are left.
#[actix_web::test]
async fn every_stale_issue_is_resolved_across_batches() {
    let env = Env::new(Some(24)).await;
    for name in ["one", "two", "three"] {
        env.event(name, 25).await;
    }

    let resolved = IssueService::resolve_by_age(env.pool(), env.now, 2)
        .await
        .unwrap();

    assert_eq!(resolved, 3);
    for name in ["one", "two", "three"] {
        assert_eq!(env.issue(name).await.status, "resolved", "{name}");
    }
}

/// A plain resolve, tied to no release: the next event, from any release,
/// reopens the issue as regressed.
#[actix_web::test]
async fn an_event_after_an_auto_resolve_regresses_the_issue() {
    let env = Env::new(Some(24)).await;
    env.event("stale", 25).await;
    env.run().await;

    let issue = env.event("stale", 0).await;

    assert_eq!(issue.status, "unresolved");
    assert_eq!(issue.substatus.as_deref(), Some("regressed"));
}
