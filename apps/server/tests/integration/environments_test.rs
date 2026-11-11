//! Issues by environment, as Sentry's project issue list reads them
//! (`ProjectGroupIndexEndpoint`): the issues seen in it (`GroupEnvironment`),
//! with that environment's numbers (`group.py`, `__seen_stats_impl`).

use crate::common::{null_sourcemap_provider, process_error_event, TestDb};
use actix_web::test;
use chrono::{DateTime, Duration, Utc};
use rustrak::config::RateLimitConfig;
use rustrak::db::DbPool;
use rustrak::ingest::EventMetadata;
use rustrak::models::{CreateAuthToken, CreateProject};
use rustrak::services::{AuthTokenService, EventService, ProjectService};
use serde_json::{json, Value};
use uuid::Uuid;

/// Digests an error event of type `exc_type` at `at` by `user`, in
/// `environment` when there is one. Returns its issue id.
async fn ingest(
    pool: &DbPool,
    project_id: i32,
    exc_type: &str,
    environment: Option<&str>,
    user: &str,
    at: DateTime<Utc>,
) -> Uuid {
    ingest_release(pool, project_id, exc_type, environment, None, user, at).await
}

/// [`ingest`], with the event in `release` when there is one.
async fn ingest_release(
    pool: &DbPool,
    project_id: i32,
    exc_type: &str,
    environment: Option<&str>,
    release: Option<&str>,
    user: &str,
    at: DateTime<Utc>,
) -> Uuid {
    let dir = tempfile::TempDir::new().unwrap();
    let event_id = Uuid::new_v4();
    let mut event = json!({
        "event_id": event_id.simple().to_string(),
        "timestamp": at.timestamp() as f64,
        "platform": "python",
        "level": "error",
        "user": {"id": user},
        "exception": {"values": [{"type": exc_type, "value": "boom"}]}
    });
    if let Some(env) = environment {
        event["environment"] = json!(env);
    }
    if let Some(release) = release {
        event["release"] = json!(release);
    }
    crate::common::store_event(
        dir.path(),
        project_id,
        &event_id.simple().to_string(),
        &serde_json::to_vec(&event).unwrap(),
    )
    .await
    .unwrap();
    let limits = RateLimitConfig {
        max_events_per_minute: 1000,
        max_events_per_hour: 10000,
        max_events_per_project_per_minute: 1000,
        max_events_per_project_per_hour: 10000,
    };
    process_error_event(
        pool,
        &EventMetadata {
            event_id: event_id.simple().to_string(),
            project_id,
            ingested_at: at,
            remote_addr: None,
        },
        dir.path(),
        &limits,
        null_sourcemap_provider(),
    )
    .await
    .expect("digest");
    EventService::issue_id_for_event(pool, project_id, event_id)
        .await
        .unwrap()
        .expect("event has an issue")
}

async fn setup() -> (TestDb, i32, String) {
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "Tenants".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();
    let token = AuthTokenService::create(&db.pool, CreateAuthToken::default())
        .await
        .unwrap()
        .token;
    (db, project.id, token)
}

async fn get(pool: &DbPool, token: &str, uri: &str) -> Value {
    let app = crate::common::app(pool).await;
    let req = test::TestRequest::get()
        .uri(uri)
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success(), "{uri}: {}", resp.status());
    test::read_body_json(resp).await
}

fn ids(body: &Value) -> Vec<String> {
    body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap().to_string())
        .collect()
}

fn ts(v: &Value) -> i64 {
    v.as_str()
        .unwrap()
        .parse::<DateTime<Utc>>()
        .unwrap()
        .timestamp()
}

#[actix_web::test]
async fn issue_list_keeps_the_issues_seen_in_the_environment() {
    let (db, project_id, token) = setup().await;
    let now = Utc::now();
    let shared = ingest(&db.pool, project_id, "Shared", Some("acme"), "u1", now).await;
    ingest(&db.pool, project_id, "Shared", Some("lanza"), "u2", now).await;
    let acme_only = ingest(&db.pool, project_id, "AcmeOnly", Some("acme"), "u1", now).await;
    ingest(&db.pool, project_id, "Bare", None, "u1", now).await;
    let base = format!("/api/projects/{project_id}/issues");

    let body = get(&db.pool, &token, &format!("{base}?environment=lanza")).await;
    assert_eq!(ids(&body), vec![shared.to_string()]);
    assert_eq!(body["total_count"], 1);

    let body = get(&db.pool, &token, &format!("{base}?environment=acme")).await;
    let mut acme = ids(&body);
    acme.sort();
    let mut expected = vec![shared.to_string(), acme_only.to_string()];
    expected.sort();
    assert_eq!(acme, expected);

    // An environment nothing was seen in is an empty list (`prep_search`).
    let body = get(&db.pool, &token, &format!("{base}?environment=nowhere")).await;
    assert_eq!(body["total_count"], 0);

    let body = get(&db.pool, &token, &base).await;
    assert_eq!(body["total_count"], 3);
}

#[actix_web::test]
async fn issue_numbers_are_the_environments() {
    let (db, project_id, token) = setup().await;
    let t0 = Utc::now() - Duration::hours(3);
    let t1 = Utc::now() - Duration::hours(2);
    let t2 = Utc::now() - Duration::hours(1);
    ingest(&db.pool, project_id, "Shared", Some("lanza"), "u1", t0).await;
    ingest(&db.pool, project_id, "Shared", Some("acme"), "u2", t1).await;
    ingest(&db.pool, project_id, "Shared", Some("acme"), "u3", t2).await;
    let base = format!("/api/projects/{project_id}/issues");

    let body = get(&db.pool, &token, &format!("{base}?environment=acme")).await;
    let issue = &body["items"][0];
    assert_eq!(issue["event_count"], 2);
    assert_eq!(ts(&issue["first_seen"]), t1.timestamp());
    assert_eq!(ts(&issue["last_seen"]), t2.timestamp());
    assert_eq!(issue["user_count"], 2);
    let trend: i64 = issue["trend"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_i64().unwrap())
        .sum();
    assert_eq!(trend, 2);

    let body = get(&db.pool, &token, &format!("{base}?environment=lanza")).await;
    let issue = &body["items"][0];
    assert_eq!(issue["event_count"], 1);
    assert_eq!(ts(&issue["last_seen"]), t0.timestamp());
    assert_eq!(issue["user_count"], 1);
}

#[actix_web::test]
async fn a_late_older_event_keeps_the_environments_dates() {
    let (db, project_id, token) = setup().await;
    let older = Utc::now() - Duration::hours(2);
    let newer = Utc::now() - Duration::hours(1);
    ingest(&db.pool, project_id, "Shared", Some("acme"), "u", newer).await;
    ingest(&db.pool, project_id, "Shared", Some("acme"), "u", older).await;

    let body = get(
        &db.pool,
        &token,
        &format!("/api/projects/{project_id}/issues?environment=acme"),
    )
    .await;
    let issue = &body["items"][0];
    assert_eq!(ts(&issue["first_seen"]), older.timestamp());
    assert_eq!(ts(&issue["last_seen"]), newer.timestamp());
}

#[actix_web::test]
async fn environments_of_a_missing_project_are_not_found() {
    let (db, _, token) = setup().await;
    let app = crate::common::app(&db.pool).await;
    let req = test::TestRequest::get()
        .uri("/api/projects/999999/environments")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 404);
}

#[actix_web::test]
async fn environment_combines_with_search_status_and_sort() {
    let (db, project_id, token) = setup().await;
    let now = Utc::now();
    ingest(&db.pool, project_id, "Alpha", Some("acme"), "u", now).await;
    let beta = ingest(&db.pool, project_id, "Beta", Some("acme"), "u", now).await;
    ingest(&db.pool, project_id, "Beta", Some("acme"), "u", now).await;
    ingest(&db.pool, project_id, "Alpha", Some("lanza"), "u", now).await;
    ingest(&db.pool, project_id, "Alpha", Some("lanza"), "u", now).await;
    ingest(&db.pool, project_id, "Alpha", Some("lanza"), "u", now).await;
    let base = format!("/api/projects/{project_id}/issues");

    let body = get(
        &db.pool,
        &token,
        &format!("{base}?environment=acme&q=beta&filter=open"),
    )
    .await;
    assert_eq!(ids(&body), vec![beta.to_string()]);

    // Alpha has more events overall, but fewer in acme.
    let body = get(
        &db.pool,
        &token,
        &format!("{base}?environment=acme&sort=event_count&per_page=1"),
    )
    .await;
    assert_eq!(ids(&body), vec![beta.to_string()]);
    assert_eq!(body["total_count"], 2);

    let body = get(
        &db.pool,
        &token,
        &format!("{base}?environment=acme&filter=resolved"),
    )
    .await;
    assert_eq!(body["total_count"], 0);
}

#[actix_web::test]
async fn project_environments_are_listed_by_name() {
    let (db, project_id, token) = setup().await;
    let now = Utc::now();
    ingest(&db.pool, project_id, "A", Some("lanza"), "u", now).await;
    ingest(&db.pool, project_id, "A", Some("acme"), "u", now).await;
    ingest(&db.pool, project_id, "B", Some("acme"), "u", now).await;
    ingest(&db.pool, project_id, "C", None, "u", now).await;

    let body = get(
        &db.pool,
        &token,
        &format!("/api/projects/{project_id}/environments"),
    )
    .await;
    // No environment is not one (Sentry leaves "" out).
    assert_eq!(body, json!(["acme", "lanza"]));
}

#[actix_web::test]
async fn issue_events_filter_by_environment() {
    let (db, project_id, token) = setup().await;
    let now = Utc::now();
    let issue = ingest(&db.pool, project_id, "Shared", Some("acme"), "u", now).await;
    ingest(&db.pool, project_id, "Shared", Some("lanza"), "u", now).await;
    ingest(&db.pool, project_id, "Shared", Some("lanza"), "u", now).await;

    let uri = format!("/api/projects/{project_id}/issues/{issue}/events");
    let all = get(&db.pool, &token, &uri).await;
    assert_eq!(all["items"].as_array().unwrap().len(), 3);
    let lanza = get(&db.pool, &token, &format!("{uri}?environment=lanza")).await;
    assert_eq!(lanza["items"].as_array().unwrap().len(), 2);
}

#[actix_web::test]
async fn a_releases_new_issues_in_an_environment_outlive_its_events() {
    let (db, project_id, _) = setup().await;
    let now = Utc::now();
    // Introduced by 1.0 in production; reached staging only with 2.0.
    let at = now - Duration::hours(1);
    ingest_release(
        &db.pool,
        project_id,
        "Mixed",
        Some("production"),
        Some("1.0"),
        "u",
        at,
    )
    .await;
    ingest_release(
        &db.pool,
        project_id,
        "Mixed",
        Some("staging"),
        Some("2.0"),
        "u",
        now,
    )
    .await;
    // Created by CI, so 1.0 keeps its row once its events are gone.
    rustrak::services::ReleaseService::create(
        &db.pool,
        project_id,
        rustrak::models::CreateRelease {
            version: "1.0".to_string(),
            reference: None,
            url: None,
        },
    )
    .await
    .unwrap();
    // Retention takes every event; the issue and where it began stay.
    sqlx::query("DELETE FROM events WHERE project_id = $1")
        .bind(project_id)
        .execute(&db.pool)
        .await
        .unwrap();

    let new_in = |environment: &str| {
        let filter = rustrak::services::release::SummaryFilter {
            environment: Some(environment.to_string()),
            ..Default::default()
        };
        let pool = db.pool.clone();
        async move {
            rustrak::services::ReleaseService::summaries(&pool, project_id, &filter)
                .await
                .unwrap()
                .into_iter()
                .find(|r| r.version == "1.0")
                .map(|r| r.new_issues)
        }
    };

    assert_eq!(new_in("production").await, Some(1));
    assert_eq!(new_in("staging").await, Some(0), "1.0 never ran in staging");
}

#[actix_web::test]
async fn environment_and_release_need_one_event_in_both() {
    let (db, project_id, token) = setup().await;
    let now = Utc::now();
    // One issue, seen in production on 1.0 and in staging on 2.0.
    ingest(&db.pool, project_id, "Mixed", Some("production"), "u", now).await;
    ingest(&db.pool, project_id, "Mixed", Some("staging"), "u", now).await;
    sqlx::query(
        "UPDATE events SET release = CASE environment \
         WHEN 'production' THEN '1.0' ELSE '2.0' END WHERE project_id = $1",
    )
    .bind(project_id)
    .execute(&db.pool)
    .await
    .unwrap();

    let body = get(
        &db.pool,
        &token,
        &format!("/api/projects/{project_id}/issues?environment=production&release=2.0"),
    )
    .await;
    assert_eq!(body["total_count"], 0, "no event is in production on 2.0");

    let scope = rustrak::services::scope::Scope {
        environment: Some("production"),
        release: Some("2.0"),
    };
    let summary =
        rustrak::services::stats::StatsService::project_summary(&db.pool, project_id, None, scope)
            .await
            .unwrap();
    assert_eq!(summary.open_issues, 0);
}

#[actix_web::test]
async fn environments_include_the_ones_only_sessions_arrived_in() {
    let (db, project_id, token) = setup().await;
    ingest(
        &db.pool,
        project_id,
        "Seen",
        Some("production"),
        "u",
        Utc::now(),
    )
    .await;
    sqlx::query(
        "INSERT INTO session_counts \
         (project_id, release, environment, bucket, total, errored, crashed, abnormal) \
         VALUES ($1, '1.0', 'canary', '2026-06-10T10:00:00Z', 1, 0, 0, 0)",
    )
    .bind(project_id)
    .execute(&db.pool)
    .await
    .unwrap();

    let body = get(
        &db.pool,
        &token,
        &format!("/api/projects/{project_id}/environments"),
    )
    .await;

    assert_eq!(body, json!(["canary", "production"]));
}

#[actix_web::test]
async fn list_stats_count_only_the_scopes_release() {
    let (db, project_id, _) = setup().await;
    let now = Utc::now();
    // The first user's event is on 1.0, the second's on 2.0.
    let issue = ingest(
        &db.pool,
        project_id,
        "Spread",
        Some("production"),
        "u1",
        now,
    )
    .await;
    set_unreleased(&db.pool, project_id, "1.0").await;
    ingest(
        &db.pool,
        project_id,
        "Spread",
        Some("production"),
        "u2",
        now,
    )
    .await;
    set_unreleased(&db.pool, project_id, "2.0").await;

    let stats = rustrak::services::IssueService::list_stats(
        &db.pool,
        &[issue],
        rustrak::services::scope::Scope {
            environment: None,
            release: Some("2.0"),
        },
    )
    .await
    .unwrap();

    let stats = &stats[&issue];
    assert_eq!(stats.user_count, 1);
    assert_eq!(stats.trend.iter().sum::<i64>(), 1);
}

/// Puts the project's events that have no release yet in `release`.
async fn set_unreleased(pool: &DbPool, project_id: i32, release: &str) {
    sqlx::query(
        "UPDATE events SET release = $1 \
         WHERE project_id = $2 AND (release IS NULL OR release = '')",
    )
    .bind(release)
    .bind(project_id)
    .execute(pool)
    .await
    .unwrap();
}
