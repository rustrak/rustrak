//! One row per release, as the releases page reads it: the release record,
//! the events and new issues it brought, the environments it ran in and its
//! session health, merged from every place a release shows up.

use crate::common::TestDb;
use actix_web::test;
use chrono::{Duration, NaiveDate, Utc};
use rustrak::db::DbPool;
use rustrak::models::{CreateAuthToken, CreateProject, CreateRelease};
use rustrak::services::release::SummaryFilter;
use rustrak::services::{AuthTokenService, ProjectService, ReleaseService};
use serde_json::Value;
use uuid::Uuid;

async fn project(pool: &DbPool) -> i32 {
    ProjectService::create(
        pool,
        CreateProject {
            name: "Releases".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap()
    .id
}

async fn event(pool: &DbPool, project_id: i32, release: &str, env: &str, hours_ago: i64) {
    let at = Utc::now() - Duration::hours(hours_ago);
    sqlx::query(
        "INSERT INTO events (event_id, project_id, data, timestamp, ingested_at, level, event_type, release, environment) \
         VALUES ($1, $2, '{}', $3, $3, 'error', 'error', $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(project_id)
    .bind(at)
    .bind(release)
    .bind(env)
    .execute(pool)
    .await
    .unwrap();
}

async fn new_issue(pool: &DbPool, project_id: i32, release: &str, digest_order: i32) -> Uuid {
    let id = Uuid::new_v4();
    let now = Utc::now();
    sqlx::query(
        "INSERT INTO issues (id, project_id, digest_order, first_seen, last_seen, status, first_release) \
         VALUES ($1, $2, $3, $4, $4, 'unresolved', $5)",
    )
    .bind(id)
    .bind(project_id)
    .bind(digest_order)
    .bind(now)
    .bind(release)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn issue_seen_in(pool: &DbPool, project_id: i32, issue_id: Uuid, env: &str, release: &str) {
    let now = Utc::now();
    sqlx::query(
        "INSERT INTO issue_environments (issue_id, project_id, environment, first_seen, last_seen, first_release) \
         VALUES ($1, $2, $3, $4, $4, $5)",
    )
    .bind(issue_id)
    .bind(project_id)
    .bind(env)
    .bind(now)
    .bind(release)
    .execute(pool)
    .await
    .unwrap();
}

async fn sessions(
    pool: &DbPool,
    project_id: i32,
    release: &str,
    env: &str,
    hours_ago: i64,
    total: i64,
    crashed: i64,
) {
    sqlx::query(
        "INSERT INTO session_counts (project_id, release, environment, bucket, total, errored, crashed, abnormal) \
         VALUES ($1, $2, $3, $4, $5, 0, $6, 0)",
    )
    .bind(project_id)
    .bind(release)
    .bind(env)
    .bind(Utc::now() - Duration::hours(hours_ago))
    .bind(total)
    .bind(crashed)
    .execute(pool)
    .await
    .unwrap();
}

async fn user(pool: &DbPool, project_id: i32, release: &str, did: &str, crashed: bool) {
    let today: NaiveDate = Utc::now().date_naive();
    sqlx::query(
        "INSERT INTO session_users (project_id, release, environment, day, did, crashed) \
         VALUES ($1, $2, 'production', $3, $4, $5)",
    )
    .bind(project_id)
    .bind(release)
    .bind(today)
    .bind(did)
    .bind(crashed)
    .execute(pool)
    .await
    .unwrap();
}

/// 2.0.0 is only a record, 1.0.0 has events, issues and sessions, 0.9.0 only
/// sessions -- the three ways a release reaches this instance.
async fn seed(pool: &DbPool) -> i32 {
    let p = project(pool).await;
    event(pool, p, "1.0.0", "production", 2).await;
    event(pool, p, "1.0.0", "production", 48).await;
    event(pool, p, "1.0.0", "staging", 3).await;
    let issue = new_issue(pool, p, "1.0.0", 1).await;
    issue_seen_in(pool, p, issue, "production", "1.0.0").await;
    sessions(pool, p, "1.0.0", "production", 1, 80, 20).await;
    sessions(pool, p, "0.9.0", "production", 72, 20, 0).await;
    user(pool, p, "1.0.0", "a", false).await;
    user(pool, p, "1.0.0", "b", true).await;
    ReleaseService::create(
        pool,
        p,
        CreateRelease {
            version: "2.0.0".to_string(),
            reference: Some("abc123".to_string()),
            url: None,
        },
    )
    .await
    .unwrap();
    p
}

#[actix_web::test]
async fn every_release_is_one_row_newest_first() {
    let db = TestDb::new().await;
    let p = seed(&db.pool).await;

    let rows = ReleaseService::summaries(&db.pool, p, &SummaryFilter::default())
        .await
        .unwrap();

    let versions: Vec<_> = rows.iter().map(|r| r.version.as_str()).collect();
    assert_eq!(versions, ["2.0.0", "1.0.0", "0.9.0"]);

    let created = &rows[0];
    assert_eq!(created.reference.as_deref(), Some("abc123"));
    assert_eq!(created.event_count, 0);
    assert_eq!(created.sessions, 0);
    assert_eq!(created.adoption, None);

    let busy = &rows[1];
    assert_eq!(busy.event_count, 3);
    assert_eq!(busy.new_issues, 1);
    assert_eq!(busy.environments, ["production", "staging"]);
    assert_eq!(busy.sessions, 80);
    assert_eq!(busy.crash_free_sessions, Some(0.75));
    assert_eq!(busy.crash_free_users, Some(0.5));
    assert_eq!(busy.adoption, Some(0.8));
    assert!(busy.first_event.unwrap() < busy.last_event.unwrap());
}

#[actix_web::test]
async fn the_window_counts_only_recent_activity() {
    let db = TestDb::new().await;
    let p = seed(&db.pool).await;

    let filter = SummaryFilter {
        period_hours: Some(24),
        ..Default::default()
    };
    let rows = ReleaseService::summaries(&db.pool, p, &filter)
        .await
        .unwrap();
    let busy = rows.iter().find(|r| r.version == "1.0.0").unwrap();
    let old = rows.iter().find(|r| r.version == "0.9.0").unwrap();

    assert_eq!(busy.event_count, 2);
    assert_eq!(busy.adoption, Some(1.0));
    // Still listed: the window narrows the numbers, not the releases.
    assert_eq!(old.sessions, 0);
}

#[actix_web::test]
async fn an_environment_narrows_events_and_sessions() {
    let db = TestDb::new().await;
    let p = seed(&db.pool).await;

    let filter = SummaryFilter {
        environment: Some("staging".to_string()),
        ..Default::default()
    };
    let rows = ReleaseService::summaries(&db.pool, p, &filter)
        .await
        .unwrap();
    let busy = rows.iter().find(|r| r.version == "1.0.0").unwrap();

    assert_eq!(busy.event_count, 1);
    assert_eq!(busy.sessions, 0);
    // Sentry's per-environment count: the issue was only ever in production.
    assert_eq!(busy.new_issues, 0);
}

#[actix_web::test]
async fn an_environment_narrows_the_numbers_not_the_releases() {
    let db = TestDb::new().await;
    let p = seed(&db.pool).await;
    let all = ReleaseService::summaries(&db.pool, p, &SummaryFilter::default())
        .await
        .unwrap();

    let filter = SummaryFilter {
        environment: Some("staging".to_string()),
        ..Default::default()
    };
    let rows = ReleaseService::summaries(&db.pool, p, &filter)
        .await
        .unwrap();

    let versions: Vec<_> = rows.iter().map(|r| r.version.as_str()).collect();
    assert_eq!(versions, ["2.0.0", "1.0.0", "0.9.0"]);
    // Its age is the release's, not the environment's.
    assert_eq!(rows[1].date_created, all[1].date_created);
    assert_eq!(rows[1].first_event, all[1].first_event);
}

#[actix_web::test]
async fn a_query_keeps_the_versions_that_contain_it() {
    let db = TestDb::new().await;
    let p = seed(&db.pool).await;

    let filter = SummaryFilter {
        query: Some("1.0".to_string()),
        ..Default::default()
    };
    let rows = ReleaseService::summaries(&db.pool, p, &filter)
        .await
        .unwrap();
    let versions: Vec<_> = rows.iter().map(|r| r.version.as_str()).collect();

    assert_eq!(versions, ["1.0.0"]);
}

#[actix_web::test]
async fn the_api_pages_the_list_and_answers_one_release() {
    let db = TestDb::new().await;
    let p = seed(&db.pool).await;
    let token = AuthTokenService::create(&db.pool, CreateAuthToken::default())
        .await
        .unwrap()
        .token;
    let app = crate::common::app(&db.pool).await;
    let get = |uri: String| {
        test::TestRequest::get()
            .uri(&uri)
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request()
    };

    let page: Value =
        test::call_and_read_body_json(&app, get(format!("/api/projects/{p}/releases?per_page=2")))
            .await;
    assert_eq!(page["total_count"], 3);
    assert_eq!(page["items"].as_array().unwrap().len(), 2);
    assert_eq!(page["items"][0]["version"], "2.0.0");

    let one: Value =
        test::call_and_read_body_json(&app, get(format!("/api/projects/{p}/releases/1.0.0"))).await;
    assert_eq!(one["new_issues"], 1);

    let missing = test::call_service(&app, get(format!("/api/projects/{p}/releases/9.9.9"))).await;
    assert_eq!(missing.status(), 404);

    // As Sentry's list: an environment keeps the releases seen in it...
    let staging: Value = test::call_and_read_body_json(
        &app,
        get(format!("/api/projects/{p}/releases?environment=staging")),
    )
    .await;
    assert_eq!(staging["total_count"], 1);
    assert_eq!(staging["items"][0]["version"], "1.0.0");

    // ...while a release is still itself there, with nothing to count.
    let idle: Value = test::call_and_read_body_json(
        &app,
        get(format!(
            "/api/projects/{p}/releases/0.9.0?environment=staging"
        )),
    )
    .await;
    assert_eq!(idle["sessions"], 0);
}
