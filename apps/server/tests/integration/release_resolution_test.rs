//! Release-aware resolutions end to end (GH #576): events create releases,
//! "resolve in next release" and "resolve in release" write a resolution,
//! and only an event from a release at or after the fix reopens the issue.
//!
//! Mirrors Sentry's `GroupResolution`, `clear_expired_resolutions` and
//! `handle_resolve_in_release` (`src/sentry/models/groupresolution.py`,
//! `src/sentry/tasks/clear_expired_resolutions.py`,
//! `src/sentry/api/helpers/group_index/update.py`).

use super::digest_test::digest_in_order;
use crate::common::{create_test_project, create_test_token, TestDb};
use actix_web::test;
use chrono::{DateTime, Duration, Utc};
use rustrak::db::DbPool;
use rustrak::models::{Issue, Project};
use rustrak::services::{IssueService, IssueSocialService};
use serde_json::{json, Value};
use tempfile::TempDir;
use uuid::Uuid;

/// One event of the issue every test works on, from `release`.
fn event(release: Option<&str>) -> Value {
    let mut e = json!({
        "platform": "rust",
        "message": "boom",
        "fingerprint": ["release-resolution"],
    });
    if let Some(r) = release {
        e["release"] = json!(r);
    }
    e
}

struct Env {
    db: TestDb,
    project: Project,
    token: String,
    dir: TempDir,
    start: DateTime<Utc>,
}

impl Env {
    async fn new(name: &str) -> Self {
        let db = TestDb::new().await;
        let project = create_test_project(&db.pool, name).await;
        let token = create_test_token(&db.pool).await;
        Self {
            db,
            project,
            token,
            dir: TempDir::new().unwrap(),
            // Whole seconds: an event's timestamp travels as epoch seconds.
            start: DateTime::from_timestamp(Utc::now().timestamp() - 7 * 86_400, 0).unwrap(),
        }
    }

    fn pool(&self) -> &DbPool {
        &self.db.pool
    }

    /// Digests one event per release, `minutes` after the start, in one batch.
    async fn events(&self, minutes: i64, releases: &[Option<&str>]) {
        let at = self.start + Duration::minutes(minutes);
        digest_in_order(
            self.pool(),
            self.dir.path(),
            self.project.id,
            releases.iter().map(|r| (at, event(*r))).collect(),
        )
        .await;
    }

    /// One event stamped `timestamp` by its SDK, received at `received`.
    async fn event_at(
        &self,
        release: Option<&str>,
        timestamp: DateTime<Utc>,
        received: DateTime<Utc>,
    ) {
        let mut e = event(release);
        let event_id = Uuid::new_v4();
        let raw = event_id.simple().to_string();
        e["event_id"] = json!(raw);
        e["timestamp"] = json!(timestamp.timestamp() as f64);
        rustrak::ingest::store_event_with_metadata(
            self.dir.path(),
            &raw,
            &serde_json::to_vec(&e).unwrap(),
            &rustrak::ingest::EventMetadata {
                event_id: raw.clone(),
                project_id: self.project.id,
                ingested_at: received,
                remote_addr: None,
            },
        )
        .await
        .unwrap();
        let processors =
            std::sync::Arc::new(crate::common::processors(self.pool(), self.dir.path()));
        let refs = vec![super::digest_test::spool_ref(
            &processors,
            self.project.id,
            event_id,
        )];
        for result in rustrak::ingest::pipeline::digest_refs(&processors, self.pool(), refs).await {
            result.unwrap();
        }
    }

    async fn issue(&self) -> Issue {
        // The first issue: some tests add another next to it.
        let id: Uuid = sqlx::query_scalar(
            "SELECT id FROM issues WHERE project_id = $1 ORDER BY digest_order LIMIT 1",
        )
        .bind(self.project.id)
        .fetch_one(self.pool())
        .await
        .expect("one issue");
        IssueService::get_by_id(self.pool(), id).await.unwrap()
    }

    async fn patch(&self, body: Value) -> (u16, Value) {
        let issue = self.issue().await;
        let app = crate::common::app(self.pool()).await;
        let req = test::TestRequest::patch()
            .uri(&format!(
                "/api/projects/{}/issues/{}",
                self.project.id, issue.id
            ))
            .insert_header(("Authorization", format!("Bearer {}", self.token)))
            .set_json(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        let status = resp.status().as_u16();
        let body: Value =
            serde_json::from_slice(&test::read_body(resp).await).unwrap_or(Value::Null);
        (status, body)
    }

    async fn create_release(&self, version: &str) -> u16 {
        let app = crate::common::app(self.pool()).await;
        let req = test::TestRequest::post()
            .uri(&format!(
                "/api/0/projects/org/{}/releases/",
                self.project.slug
            ))
            .insert_header(("Authorization", format!("Bearer {}", self.token)))
            .set_json(json!({ "version": version }))
            .to_request();
        test::call_service(&app, req).await.status().as_u16()
    }

    /// A release row, registered `minutes` after the start.
    async fn insert_release(&self, version: &str, minutes: i64) {
        sqlx::query("INSERT INTO releases (project_id, version, date_created) VALUES ($1, $2, $3)")
            .bind(self.project.id)
            .bind(version)
            .bind(self.start + Duration::minutes(minutes))
            .execute(self.pool())
            .await
            .unwrap();
    }

    async fn releases(&self) -> Vec<(String, DateTime<Utc>)> {
        sqlx::query_as(
            "SELECT version, date_created FROM releases WHERE project_id = $1 ORDER BY date_created, version",
        )
        .bind(self.project.id)
        .fetch_all(self.pool())
        .await
        .unwrap()
    }

    /// The issue's resolution: type, status, release, current version.
    async fn resolution(&self) -> Option<(Option<i16>, i16, String, Option<String>)> {
        let issue = self.issue().await;
        sqlx::query_as(
            "SELECT r.type, r.status, rel.version, r.current_release_version \
             FROM issue_resolutions r JOIN releases rel ON rel.id = r.release_id \
             WHERE r.issue_id = $1",
        )
        .bind(issue.id)
        .fetch_optional(self.pool())
        .await
        .unwrap()
    }

    async fn activity(&self, kind: &str) -> Vec<Value> {
        let issue = self.issue().await;
        IssueSocialService::list_activity(self.pool(), issue.id)
            .await
            .unwrap()
            .into_iter()
            .filter(|a| a.activity_type == kind)
            .map(|a| serde_json::from_str(&a.data).unwrap())
            .collect()
    }

    async fn status(&self) -> (String, Option<String>, Value) {
        let issue = self.issue().await;
        (
            issue.status,
            issue.substatus,
            serde_json::from_str(&issue.status_details).unwrap(),
        )
    }
}

const IN_RELEASE: Option<i16> = Some(0);
const IN_NEXT_RELEASE: Option<i16> = Some(1);
const PENDING: i16 = 0;
const RESOLVED: i16 = 1;

// ---------------------------------------------------------------------------
// Releases from events (`_get_or_create_release_many`)
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn an_event_creates_its_release_dated_by_the_first_event() {
    let env = Env::new("Release From Event").await;
    env.events(10, &[Some("app@1.0.0"), Some("app@1.0.0")])
        .await;
    env.events(20, &[Some("app@1.0.0"), Some("app@1.1.0"), None])
        .await;

    let start = env.start;
    assert_eq!(
        env.releases().await,
        [
            ("app@1.0.0".to_string(), start + Duration::minutes(10)),
            ("app@1.1.0".to_string(), start + Duration::minutes(20)),
        ]
    );
}

/// A client clock ahead of the server dates the release when it arrived,
/// as Relay normalizes such a timestamp.
#[actix_web::test]
async fn a_future_timestamp_dates_the_release_on_arrival() {
    let env = Env::new("Future Clock").await;
    let at = env.start;
    env.event_at(Some("app@9.9.9"), at + Duration::days(3), at)
        .await;
    assert_eq!(env.releases().await, [("app@9.9.9".to_string(), at)]);
}

/// A release CI registered first keeps its own date.
#[actix_web::test]
async fn an_event_does_not_redate_a_registered_release() {
    let env = Env::new("Registered Release").await;
    env.insert_release("app@1.0.0", 0).await;
    env.events(30, &[Some("app@1.0.0")]).await;
    assert_eq!(env.releases().await, [("app@1.0.0".to_string(), env.start)]);
}

/// Versions Sentry's `is_valid_version` refuses get no release row.
#[actix_web::test]
async fn an_invalid_release_creates_no_row() {
    let env = Env::new("Invalid Release").await;
    env.events(0, &[Some("latest"), Some("a/b"), Some("..")])
        .await;
    assert!(env.releases().await.is_empty());
}

// ---------------------------------------------------------------------------
// GH #576: staggered rollouts
// ---------------------------------------------------------------------------

/// Semver releases: resolved in next release while `1.0.4118` was the
/// greatest, older builds keep sending and the issue stays resolved; the
/// first event from `1.0.4120` reopens it, and the activity says why.
#[actix_web::test]
async fn staggered_semver_rollout_older_builds_do_not_reopen() {
    let env = Env::new("Staggered Semver").await;
    env.events(0, &[Some("app@1.0.4117")]).await;
    env.events(1, &[Some("app@1.0.4118")]).await;

    let (status, body) = env
        .patch(json!({ "status": "resolvedInNextRelease" }))
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["status"], "resolved");
    assert_eq!(
        body["status_details"],
        json!({ "inRelease": "app@1.0.4118" })
    );
    assert_eq!(
        env.resolution().await,
        Some((
            IN_RELEASE,
            RESOLVED,
            "app@1.0.4118".into(),
            Some("app@1.0.4118".into())
        ))
    );
    assert_eq!(
        env.activity("set_resolved_in_release").await,
        [json!({ "version": "", "current_release_version": "app@1.0.4118" })]
    );

    env.events(5, &[Some("app@1.0.4117"), Some("app@1.0.4118"), None])
        .await;
    assert_eq!(env.status().await.0, "resolved");

    env.events(10, &[Some("app@1.0.4120"), Some("app@1.0.4117")])
        .await;
    let (status, substatus, details) = env.status().await;
    assert_eq!(
        (status.as_str(), substatus.as_deref()),
        ("unresolved", Some("regressed"))
    );
    assert_eq!(details, json!({}));
    assert_eq!(env.resolution().await, None);
    let regression = env.activity("set_regression").await;
    assert_eq!(regression.len(), 1);
    assert_eq!(regression[0]["version"], "app@1.0.4120");
    assert_eq!(regression[0]["resolved_in_version"], "app@1.0.4120");
    assert_eq!(regression[0]["follows_semver"], true);
    assert_eq!(regression[0]["status"], "unresolved");
    assert_eq!(regression[0]["event_id"].as_str().map(str::len), Some(32));
    // The "resolved in next release" activity now names where it broke.
    assert_eq!(
        env.activity("set_resolved_in_release").await,
        [json!({ "version": "app@1.0.4120", "current_release_version": "app@1.0.4118" })]
    );
}

/// The CI step that made it worse in #576: the first deployment registering
/// the new release cleared the marker for everyone. Now it resolves the
/// issue in that release, and older builds still do not reopen it.
#[actix_web::test]
async fn a_release_created_by_ci_resolves_pending_issues_in_it() {
    let env = Env::new("Release From CI").await;
    env.events(0, &[Some("1.0.4117")]).await;
    env.events(1, &[Some("1.0.4118")]).await;

    // Bare versions are not semver: ordered by date, and with no release
    // after 1.0.4118 yet, the resolution waits.
    let (status, body) = env
        .patch(json!({ "status": "resolvedInNextRelease" }))
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["status_details"], json!({ "inNextRelease": true }));
    assert_eq!(
        env.resolution().await,
        Some((
            IN_NEXT_RELEASE,
            PENDING,
            "1.0.4118".into(),
            Some("1.0.4118".into())
        ))
    );

    assert_eq!(env.create_release("1.0.4120").await, 201);
    assert_eq!(
        env.resolution().await,
        Some((
            IN_RELEASE,
            RESOLVED,
            "1.0.4120".into(),
            Some("1.0.4118".into())
        ))
    );
    assert_eq!(env.status().await.2, json!({ "inRelease": "1.0.4120" }));
    assert_eq!(
        env.activity("set_resolved_in_release").await,
        [json!({ "version": "1.0.4120" })]
    );

    env.events(5, &[Some("1.0.4117"), Some("1.0.4118")]).await;
    assert_eq!(env.status().await.0, "resolved");

    env.events(10, &[Some("1.0.4120")]).await;
    assert_eq!(env.status().await.0, "unresolved");
}

/// Repeating the release creation (sentry-cli does on every build) does
/// not touch resolutions again.
#[actix_web::test]
async fn an_existing_release_resolves_nothing() {
    let env = Env::new("Existing Release").await;
    env.events(0, &[Some("b1")]).await;
    env.patch(json!({ "status": "resolvedInNextRelease" }))
        .await;
    assert_eq!(
        env.resolution().await.map(|r| (r.0, r.1)),
        Some((IN_NEXT_RELEASE, PENDING))
    );
    // Registered behind the API's back, newer than b1.
    env.insert_release("b2", 60).await;

    assert_eq!(env.create_release("b2").await, 208);
    assert_eq!(
        env.resolution().await.map(|r| (r.0, r.1)),
        Some((IN_NEXT_RELEASE, PENDING))
    );
}

/// A new build seen by the digest before any CI step: its event reopens the
/// pending issue (the release is newer than the one it was resolved in).
#[actix_web::test]
async fn an_event_from_a_new_release_reopens_a_pending_issue() {
    let env = Env::new("New Build Event").await;
    env.events(0, &[Some("1.0.4118")]).await;
    env.patch(json!({ "status": "resolvedInNextRelease" }))
        .await;
    assert_eq!(env.resolution().await.map(|r| r.1), Some(PENDING));

    env.events(10, &[Some("1.0.4120")]).await;
    assert_eq!(env.status().await.0, "unresolved");
    assert_eq!(env.resolution().await, None);
    let regression = env.activity("set_regression").await;
    assert_eq!(regression[0]["resolved_in_version"], "1.0.4120");
    assert_eq!(regression[0]["follows_semver"], false);
    assert_eq!(
        env.activity("set_resolved_in_release").await,
        [json!({ "version": "1.0.4120" })]
    );
}

/// A new release first seen by the digest resolves the other issues
/// pending on an older one, as Sentry's `post_save` does for any release.
#[actix_web::test]
async fn a_release_created_by_an_event_resolves_other_pending_issues() {
    let env = Env::new("Release From Other Issue").await;
    env.events(0, &[Some("r1")]).await;
    env.patch(json!({ "status": "resolvedInNextRelease" }))
        .await;

    let other = json!({ "platform": "rust", "message": "other", "fingerprint": ["other"], "release": "r2" });
    digest_in_order(
        env.pool(),
        env.dir.path(),
        env.project.id,
        vec![(env.start + Duration::minutes(10), other)],
    )
    .await;

    assert_eq!(
        env.resolution().await,
        Some((IN_RELEASE, RESOLVED, "r2".into(), Some("r1".into())))
    );
    assert_eq!(env.status().await.2, json!({ "inRelease": "r2" }));
}

/// The current release is the one of the issue's newest event by its
/// timestamp (`GroupRelease.last_seen`), not of the last one to arrive: a
/// delayed event from an older build does not move it back.
#[actix_web::test]
async fn a_late_event_from_an_older_release_is_not_the_current_one() {
    let env = Env::new("Late Event").await;
    let t = |m| env.start + Duration::minutes(m);
    env.event_at(Some("1.0"), t(0), t(0)).await;
    env.event_at(Some("2.0"), t(10), t(10)).await;
    // Buffered on a device, sent after 2.0's.
    env.event_at(Some("1.0"), t(5), t(20)).await;
    assert_eq!(env.issue().await.last_release, "1.0");

    env.patch(json!({ "status": "resolvedInNextRelease" }))
        .await;
    assert_eq!(
        env.resolution().await,
        Some((IN_NEXT_RELEASE, PENDING, "2.0".into(), Some("2.0".into())))
    );
    env.event_at(Some("2.0"), t(30), t(30)).await;
    assert_eq!(env.status().await.0, "resolved");
}

/// Non-semver, with a release after the issue's already registered:
/// "next release" is that one straight away
/// (`test_in_non_semver_projects_resolved_in_next_release_is_equated_to_in_release`).
#[actix_web::test]
async fn in_next_release_with_a_later_release_resolves_in_it() {
    let env = Env::new("Known Next").await;
    env.events(0, &[Some("foobar 1")]).await;
    env.insert_release("foobar 2", 30).await;
    env.insert_release("foobar 3", 60).await;

    let (_, body) = env
        .patch(json!({ "status": "resolvedInNextRelease" }))
        .await;
    assert_eq!(body["status_details"], json!({ "inRelease": "foobar 2" }));
    assert_eq!(
        env.activity("set_resolved_in_release").await,
        [json!({ "version": "foobar 2" })]
    );

    env.events(90, &[Some("foobar 1")]).await;
    assert_eq!(env.status().await.0, "resolved");
    env.events(91, &[Some("foobar 3")]).await;
    assert_eq!(env.status().await.0, "unresolved");
}

// ---------------------------------------------------------------------------
// Resolve in a release (`statusDetails.inRelease`)
// ---------------------------------------------------------------------------

/// The release it is resolved in has the fix: its events reopen the issue,
/// older ones do not.
#[actix_web::test]
async fn resolved_in_an_explicit_release() {
    let env = Env::new("Explicit Release").await;
    env.events(
        0,
        &[Some("app@1.0.0"), Some("app@2.0.0"), Some("app@3.0.0")],
    )
    .await;

    let (status, body) = env
        .patch(json!({ "status": "resolved", "statusDetails": { "inRelease": "app@2.0.0" } }))
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["status_details"], json!({ "inRelease": "app@2.0.0" }));
    assert_eq!(
        env.resolution().await,
        Some((IN_RELEASE, RESOLVED, "app@2.0.0".into(), None))
    );
    assert_eq!(
        env.activity("set_resolved_in_release").await,
        [json!({ "version": "app@2.0.0" })]
    );

    env.events(1, &[Some("app@1.0.0")]).await;
    assert_eq!(env.status().await.0, "resolved");
    env.events(2, &[Some("app@2.0.0")]).await;
    assert_eq!(env.status().await.0, "unresolved");
}

#[actix_web::test]
async fn resolved_in_the_latest_release() {
    let env = Env::new("Latest Release").await;
    env.events(0, &[Some("a")]).await;
    env.events(1, &[Some("b")]).await;
    let (_, body) = env
        .patch(json!({ "status": "resolved", "statusDetails": { "inRelease": "latest" } }))
        .await;
    assert_eq!(body["status_details"], json!({ "inRelease": "b" }));
}

/// A refused release changes nothing.
#[actix_web::test]
async fn an_unknown_release_is_refused() {
    let env = Env::new("Unknown Release").await;
    env.events(0, &[Some("a")]).await;
    let (status, body) = env
        .patch(json!({ "status": "resolved", "statusDetails": { "inRelease": "nope" } }))
        .await;
    assert_eq!(status, 400, "{body}");
    assert!(
        body.to_string().contains("Unable to find a release"),
        "{body}"
    );
    assert_eq!(env.status().await.0, "unresolved");
    assert!(env.activity("set_resolved_in_release").await.is_empty());
}

/// `statusDetails.inNextRelease` needs a release; the bare status does not.
#[actix_web::test]
async fn in_next_release_without_releases() {
    let env = Env::new("No Releases").await;
    env.events(0, &[None]).await;

    let (status, body) = env
        .patch(json!({ "status": "resolved", "statusDetails": { "inNextRelease": true } }))
        .await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(env.status().await.0, "unresolved");

    let (status, body) = env
        .patch(json!({ "status": "resolvedInNextRelease" }))
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["status"], "resolved");
    assert_eq!(body["status_details"], json!({}));
    assert_eq!(env.resolution().await, None);
    assert_eq!(
        env.activity("set_resolved_in_release").await,
        [json!({ "version": "" })]
    );
    // Nothing to compare against: the next event reopens it.
    env.events(1, &[None]).await;
    assert_eq!(env.status().await.0, "unresolved");
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

/// An event without a release cannot be shown to be newer.
#[actix_web::test]
async fn an_event_without_a_release_keeps_a_release_resolution() {
    let env = Env::new("No Release Event").await;
    env.events(0, &[Some("a")]).await;
    env.patch(json!({ "status": "resolved", "statusDetails": { "inRelease": "a" } }))
        .await;
    env.events(1, &[None]).await;
    assert_eq!(env.status().await.0, "resolved");
    // Nor one whose release Sentry would drop as invalid.
    env.events(2, &[Some("latest")]).await;
    assert_eq!(env.status().await.0, "resolved");
}

/// A regression's activity has no version for an event without a valid one.
#[actix_web::test]
async fn a_regression_from_an_invalid_release_names_none() {
    let env = Env::new("Invalid Regression").await;
    env.events(0, &[Some("a")]).await;
    env.patch(json!({ "status": "resolved" })).await;
    env.events(1, &[Some("a/b")]).await;
    assert_eq!(env.activity("set_regression").await[0]["version"], "");
}

/// Plain "resolve" has no release to compare: any event reopens it.
#[actix_web::test]
async fn a_plain_resolve_reopens_on_any_event() {
    let env = Env::new("Plain Resolve").await;
    env.events(0, &[Some("a")]).await;
    let (_, body) = env.patch(json!({ "status": "resolved" })).await;
    assert_eq!(body["status_details"], json!({}));
    assert_eq!(env.resolution().await, None);
    env.events(1, &[Some("a")]).await;
    assert_eq!(env.status().await.0, "unresolved");
}

/// Resolving an issue that is already resolved is skipped, as in Sentry:
/// its resolution, status details and activity stay as they were.
#[actix_web::test]
async fn resolving_a_resolved_issue_changes_nothing() {
    let env = Env::new("Already Resolved").await;
    env.events(0, &[Some("a")]).await;
    env.events(1, &[Some("b")]).await;
    env.patch(json!({ "status": "resolved", "statusDetails": { "inRelease": "a" } }))
        .await;
    let activity = IssueSocialService::list_activity(env.pool(), env.issue().await.id)
        .await
        .unwrap()
        .len();

    for body in [
        json!({ "status": "resolved" }),
        json!({ "status": "resolvedInNextRelease" }),
        json!({ "status": "resolved", "statusDetails": { "inRelease": "b" } }),
    ] {
        let (status, response) = env.patch(body).await;
        assert_eq!(status, 200);
        assert_eq!(response["status_details"], json!({ "inRelease": "a" }));
    }
    assert_eq!(env.resolution().await.map(|r| r.2), Some("a".to_string()));
    assert_eq!(
        IssueSocialService::list_activity(env.pool(), env.issue().await.id)
            .await
            .unwrap()
            .len(),
        activity
    );
}

/// Unresolving or ignoring drops the resolution (`handle_other_status_updates`).
#[actix_web::test]
async fn leaving_resolved_drops_the_resolution() {
    for status in ["unresolved", "ignored"] {
        let env = Env::new(&format!("Leave {status}")).await;
        env.events(0, &[Some("a")]).await;
        let (code, body) = env
            .patch(json!({ "status": "resolved", "statusDetails": { "inRelease": "a" } }))
            .await;
        assert_eq!(code, 200, "{body}");
        assert!(
            env.resolution().await.is_some(),
            "{status}: no resolution to drop"
        );
        let (code, body) = env.patch(json!({ "status": status })).await;
        assert_eq!(code, 200);
        assert_eq!(body["status"], status);
        assert_eq!(env.status().await.0, status);
        assert_eq!(body["status_details"], json!({}), "{status}");
        assert_eq!(env.resolution().await, None, "{status}");
    }
}

/// A resolution the digest has to read in the same batch as the event that
/// breaks it: one regression, at the first newer event.
#[actix_web::test]
async fn one_batch_regresses_once_at_the_first_newer_event() {
    let env = Env::new("One Batch").await;
    env.events(0, &[Some("app@1.0.0")]).await;
    env.patch(json!({ "status": "resolvedInNextRelease" }))
        .await;
    env.events(
        1,
        &[
            Some("app@1.0.0"),
            Some("app@2.0.0"),
            Some("app@1.0.0"),
            Some("app@3.0.0"),
        ],
    )
    .await;
    assert_eq!(env.status().await.0, "unresolved");
    let regression = env.activity("set_regression").await;
    assert_eq!(regression.len(), 1);
    assert_eq!(regression[0]["version"], "app@2.0.0");
}

// ---------------------------------------------------------------------------
// Bulk and the Sentry API
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn bulk_resolve_in_a_release() {
    let env = Env::new("Bulk Release").await;
    env.events(0, &[Some("a")]).await;
    let other =
        json!({ "platform": "rust", "message": "two", "fingerprint": ["two"], "release": "a" });
    digest_in_order(
        env.pool(),
        env.dir.path(),
        env.project.id,
        vec![(env.start, other)],
    )
    .await;
    let ids: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM issues WHERE project_id = $1")
        .bind(env.project.id)
        .fetch_all(env.pool())
        .await
        .unwrap();
    let app = crate::common::app(env.pool()).await;
    let bulk = |body: Value| {
        test::TestRequest::put()
            .uri(&format!("/api/projects/{}/issues", env.project.id))
            .insert_header(("Authorization", format!("Bearer {}", env.token)))
            .set_json(body)
            .to_request()
    };

    let resp = test::call_service(
        &app,
        bulk(json!({ "ids": ids, "status": "resolved", "statusDetails": { "inRelease": "nope" } })),
    )
    .await;
    assert_eq!(resp.status(), 400);
    let resolved: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM issues WHERE project_id = $1 AND status = 'resolved'",
    )
    .bind(env.project.id)
    .fetch_one(env.pool())
    .await
    .unwrap();
    assert_eq!(resolved, 0);

    let resp = test::call_service(
        &app,
        bulk(json!({ "ids": ids, "status": "resolved", "statusDetails": { "inRelease": "a" } })),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["updated"], 2);
    let activity = IssueSocialService::list_activity(env.pool(), ids[0])
        .await
        .unwrap();
    let resolved = activity
        .iter()
        .find(|a| a.activity_type == "set_resolved_in_release")
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&resolved.data).unwrap(),
        json!({ "version": "a", "bulk": true })
    );
    let rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM issue_resolutions WHERE project_id = $1")
            .bind(env.project.id)
            .fetch_one(env.pool())
            .await
            .unwrap();
    assert_eq!(rows, 2);

    // Already resolved: skipped, not counted.
    let resp = test::call_service(&app, bulk(json!({ "ids": ids, "status": "resolved" }))).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["updated"], 0);
    // Unless the same request changes something else about them.
    let resp = test::call_service(
        &app,
        bulk(json!({ "ids": ids, "status": "resolved", "priority": "low" })),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["updated"], 2);
}

#[actix_web::test]
async fn sentry_api_resolves_in_a_release() {
    let env = Env::new("Sentry API Release").await;
    env.events(0, &[Some("app@1.0.0")]).await;
    let issue = env.issue().await;
    let app = crate::common::app(env.pool()).await;
    let req = test::TestRequest::put()
        .uri(&format!("/api/0/issues/{}/", issue.numeric_id))
        .insert_header(("Authorization", format!("Bearer {}", env.token)))
        .set_json(json!({ "status": "resolved", "statusDetails": { "inRelease": "latest" } }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["status"], "resolved");
    assert_eq!(body["statusDetails"], json!({ "inRelease": "app@1.0.0" }));
}

// ---------------------------------------------------------------------------
// `clear_expired_resolutions`, from `tests/sentry/tasks/test_clear_expired_resolutions.py`
// (the legacy, `date_added` path).
// ---------------------------------------------------------------------------

mod clear_expired {
    use super::*;
    use rustrak::models::resolution::{ReleaseRef, ResolutionPlan, ResolutionType};
    use rustrak::services::grouping::get_denormalized_fields;
    use rustrak::services::ResolutionService;

    async fn release(env: &Env, version: &str, minutes: i64) -> ReleaseRef {
        env.insert_release(version, minutes).await;
        sqlx::query_as(
            "SELECT id, version, date_created, date_released FROM releases \
             WHERE project_id = $1 AND version = $2",
        )
        .bind(env.project.id)
        .bind(version)
        .fetch_one(env.pool())
        .await
        .unwrap()
    }

    /// An issue with a resolution on `anchor` and its activity, `{"version": ""}`.
    async fn resolved_on(
        env: &Env,
        project_id: i32,
        name: &str,
        anchor: &ReleaseRef,
        kind: ResolutionType,
        status: i16,
    ) -> Uuid {
        let fields = get_denormalized_fields(&json!({ "message": name }));
        let issue = IssueService::create(env.pool(), project_id, env.start, &fields, None, None)
            .await
            .unwrap();
        let mut conn = env.pool().acquire().await.unwrap();
        let plan = ResolutionPlan {
            kind,
            status,
            release: anchor.clone(),
            current_release_version: None,
        };
        ResolutionService::write(&mut conn, issue.id, project_id, &plan, None)
            .await
            .unwrap();
        ResolutionService::record_activity(&mut conn, issue.id, None, &json!({ "version": "" }))
            .await
            .unwrap();
        issue.id
    }

    async fn row(env: &Env, issue: Uuid) -> (i16, i16, String, Value) {
        let (kind, status, version): (i16, i16, String) = sqlx::query_as(
            "SELECT r.type, r.status, rel.version FROM issue_resolutions r \
             JOIN releases rel ON rel.id = r.release_id WHERE r.issue_id = $1",
        )
        .bind(issue)
        .fetch_one(env.pool())
        .await
        .unwrap();
        let activity = IssueSocialService::list_activity(env.pool(), issue)
            .await
            .unwrap()
            .into_iter()
            .find(|a| a.activity_type == "set_resolved_in_release")
            .unwrap();
        (
            kind,
            status,
            version,
            serde_json::from_str(&activity.data).unwrap(),
        )
    }

    async fn clear(env: &Env, project_id: i32, release: &ReleaseRef) -> u64 {
        let mut conn = env.pool().acquire().await.unwrap();
        ResolutionService::clear_expired(&mut conn, project_id, release)
            .await
            .unwrap()
    }

    /// `test_simple`: a pending resolution on an older release is now resolved
    /// in the new one, and its activity names it; one anchored on the new
    /// release itself waits on.
    #[actix_web::test]
    async fn simple() {
        let env = Env::new("Clear Simple").await;
        let old = release(&env, "a", 0).await;
        let new = release(&env, "b", 1).await;
        let older = resolved_on(
            &env,
            env.project.id,
            "one",
            &old,
            ResolutionType::InNextRelease,
            0,
        )
        .await;
        let same = resolved_on(
            &env,
            env.project.id,
            "two",
            &new,
            ResolutionType::InNextRelease,
            0,
        )
        .await;

        assert_eq!(clear(&env, env.project.id, &new).await, 1);

        assert_eq!(
            row(&env, older).await,
            (0, 1, "b".into(), json!({ "version": "b" }))
        );
        assert_eq!(
            row(&env, same).await,
            (1, 0, "b".into(), json!({ "version": "" }))
        );
    }

    /// An older release created later in time is not a successor.
    #[actix_web::test]
    async fn a_release_dated_before_the_anchor_resolves_nothing() {
        let env = Env::new("Clear Older").await;
        let anchor = release(&env, "anchor", 10).await;
        let earlier = release(&env, "earlier", 5).await;
        let issue = resolved_on(
            &env,
            env.project.id,
            "x",
            &anchor,
            ResolutionType::InNextRelease,
            0,
        )
        .await;
        assert_eq!(clear(&env, env.project.id, &earlier).await, 0);
        assert_eq!(row(&env, issue).await.1, 0);
    }

    /// The legacy path compares `date_added` strictly.
    #[actix_web::test]
    async fn a_release_at_the_same_instant_resolves_nothing() {
        let env = Env::new("Clear Same Date").await;
        let anchor = release(&env, "anchor", 10).await;
        let twin = release(&env, "twin", 10).await;
        let issue = resolved_on(
            &env,
            env.project.id,
            "x",
            &anchor,
            ResolutionType::InNextRelease,
            0,
        )
        .await;
        assert_eq!(clear(&env, env.project.id, &twin).await, 0);
        assert_eq!(row(&env, issue).await.2, "anchor");
    }

    /// `test_existing_resolved_assignment_is_not_rewritten`.
    #[actix_web::test]
    async fn a_resolution_in_a_known_release_is_not_rewritten() {
        let env = Env::new("Clear Resolved").await;
        let anchor = release(&env, "anchor", 0).await;
        let next = release(&env, "next", 10).await;
        let issue = resolved_on(
            &env,
            env.project.id,
            "x",
            &anchor,
            ResolutionType::InRelease,
            1,
        )
        .await;
        assert_eq!(clear(&env, env.project.id, &next).await, 0);
        assert_eq!(row(&env, issue).await.2, "anchor");
    }

    /// `test_shared_release_does_not_resolve_other_projects`.
    #[actix_web::test]
    async fn another_projects_release_resolves_nothing() {
        let env = Env::new("Clear Project A").await;
        let other = create_test_project(env.pool(), "Clear Project B").await;
        let anchor = release(&env, "anchor", 0).await;
        let issue = resolved_on(
            &env,
            env.project.id,
            "x",
            &anchor,
            ResolutionType::InNextRelease,
            0,
        )
        .await;
        sqlx::query(
            "INSERT INTO releases (project_id, version, date_created) VALUES ($1, 'next', $2)",
        )
        .bind(other.id)
        .bind(env.start + Duration::minutes(10))
        .execute(env.pool())
        .await
        .unwrap();
        let next: ReleaseRef = sqlx::query_as(
            "SELECT id, version, date_created, date_released FROM releases WHERE project_id = $1",
        )
        .bind(other.id)
        .fetch_one(env.pool())
        .await
        .unwrap();
        assert_eq!(clear(&env, other.id, &next).await, 0);
        assert_eq!(row(&env, issue).await.1, 0);
    }
}

/// A release created while someone reopens a pending issue: the reopen
/// locks the issue, then deletes its resolution; completing pending
/// resolutions must lock in that same order, or Postgres aborts one of the
/// two as a deadlock.
#[cfg(feature = "postgres")]
#[actix_web::test]
async fn completing_resolutions_and_reopening_do_not_deadlock() {
    use rustrak::models::resolution::ReleaseRef;
    use rustrak::services::ResolutionService;

    let env = Env::new("Deadlock").await;
    env.events(0, &[Some("r1")]).await;
    env.patch(json!({ "status": "resolvedInNextRelease" }))
        .await;
    assert_eq!(env.resolution().await.map(|r| r.1), Some(PENDING));
    env.insert_release("r2", 60).await;
    let r2: ReleaseRef = sqlx::query_as(
        "SELECT id, version, date_created, date_released FROM releases \
         WHERE project_id = $1 AND version = 'r2'",
    )
    .bind(env.project.id)
    .fetch_one(env.pool())
    .await
    .unwrap();
    let issue = env.issue().await.id;

    // The reopen, half done: the issue row is locked.
    let mut reopen = env.pool().begin().await.unwrap();
    sqlx::query("UPDATE issues SET status = 'unresolved', status_details = '{}' WHERE id = $1")
        .bind(issue)
        .execute(&mut *reopen)
        .await
        .unwrap();

    let pool = env.pool().clone();
    let project = env.project.id;
    let release = tokio::spawn(async move {
        let mut tx = pool.begin().await.unwrap();
        let n = ResolutionService::clear_expired(&mut tx, project, &r2).await?;
        tx.commit().await?;
        Ok::<_, rustrak::error::AppError>(n)
    });
    blocked_on_a_lock(env.pool()).await;

    sqlx::query("DELETE FROM issue_resolutions WHERE issue_id = $1")
        .bind(issue)
        .execute(&mut *reopen)
        .await
        .expect("the reopen finishes");
    reopen.commit().await.unwrap();
    assert_eq!(release.await.unwrap().expect("the release finishes"), 0);
    assert_eq!(env.status().await.2, json!({}));
}

/// A bulk status change named `[b, a]` while a release completes the pending
/// `a` and `b`: both lock issues in id order, or Postgres aborts one of them.
#[cfg(feature = "postgres")]
#[actix_web::test]
async fn completing_resolutions_and_a_bulk_change_do_not_deadlock() {
    let env = Env::new("Bulk Deadlock").await;
    for name in ["one", "two"] {
        let e =
            json!({ "platform": "rust", "message": name, "fingerprint": [name], "release": "r1" });
        digest_in_order(
            env.pool(),
            env.dir.path(),
            env.project.id,
            vec![(env.start, e)],
        )
        .await;
    }
    let mut ids: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM issues WHERE project_id = $1")
        .bind(env.project.id)
        .fetch_all(env.pool())
        .await
        .unwrap();
    ids.sort();
    let (a, b) = (ids[0], ids[1]);

    // The release's side, holding `a` as `clear_expired` does first.
    let mut release = env.pool().begin().await.unwrap();
    sqlx::query("SELECT id FROM issues WHERE id = $1 FOR NO KEY UPDATE")
        .bind(a)
        .execute(&mut *release)
        .await
        .unwrap();

    let pool = env.pool().clone();
    let project = env.project.id;
    let bulk = tokio::spawn(async move {
        IssueService::bulk_update(
            &pool,
            project,
            &[b, a],
            Some(rustrak::models::StatusChange::Set("ignored", None)),
            None,
            None,
        )
        .await
    });
    blocked_on_a_lock(env.pool()).await;

    sqlx::query("SELECT id FROM issues WHERE id = $1 FOR NO KEY UPDATE")
        .bind(b)
        .execute(&mut *release)
        .await
        .expect("the release reaches its second issue");
    release.commit().await.unwrap();
    assert_eq!(bulk.await.unwrap().expect("the bulk change finishes"), 2);
}

/// Waits until some connection of this database is waiting on a row lock,
/// so the test goes on only once the other transaction is blocked where it
/// has to be. Fails rather than proving nothing if that never happens.
#[cfg(feature = "postgres")]
async fn blocked_on_a_lock(pool: &DbPool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let waiting: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pg_stat_activity \
             WHERE datname = current_database() AND wait_event_type = 'Lock'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        if waiting > 0 {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the other transaction never waited on a lock"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

/// A release the digest registers while someone resolves in next release:
/// the resolve waits for the digest's project lock, so it sees the release
/// and resolves in it, instead of anchoring on the older one.
#[cfg(feature = "postgres")]
#[actix_web::test]
async fn resolving_waits_for_the_releases_the_digest_is_registering() {
    let env = Env::new("Release Race").await;
    env.events(0, &[Some("r1")]).await;
    let issue = env.issue().await.id;

    // The digest's side: the project lock, then a release.
    let mut digest = env.pool().begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(env.project.id as i64)
        .execute(&mut *digest)
        .await
        .unwrap();
    sqlx::query("INSERT INTO releases (project_id, version, date_created) VALUES ($1, 'r2', $2)")
        .bind(env.project.id)
        .bind(env.start + Duration::minutes(10))
        .execute(&mut *digest)
        .await
        .unwrap();

    let pool = env.pool().clone();
    let resolve = tokio::spawn(async move {
        IssueService::resolve_in(
            &pool,
            issue,
            rustrak::models::resolution::ResolveTarget::NextRelease { explicit: false },
            None,
        )
        .await
    });
    blocked_on_a_lock(env.pool()).await;
    digest.commit().await.unwrap();
    resolve.await.unwrap().unwrap();

    assert_eq!(
        env.resolution().await,
        Some((IN_RELEASE, RESOLVED, "r2".into(), Some("r1".into())))
    );
}
