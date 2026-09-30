//! Integration tests for SessionService::release_health
//!
//! These tests insert raw rows directly into session_counts / session_users
//! and verify that the query:
//!   - sums counters correctly across buckets (regression for SUM(BIGINT)→NUMERIC issue)
//!   - computes healthy = total - errored - crashed - abnormal
//!   - computes crash-free rates
//!   - respects the time-window filter
//!   - filters by project_id
//!   - orders by total DESC

use crate::common::TestDb;
use chrono::Utc;
use rustrak::db::DbPool;
use rustrak::services::session::SessionService;

// ── seed helpers ─────────────────────────────────────────────────────────────

/// Insert one row into session_counts.
/// `hours_ago` controls how old the bucket timestamp is.
#[allow(clippy::too_many_arguments)]
async fn seed_count(
    pool: &DbPool,
    project_id: i32,
    release: &str,
    env: &str,
    hours_ago: i64,
    total: i64,
    errored: i64,
    crashed: i64,
    abnormal: i64,
) {
    #[cfg(feature = "postgres")]
    sqlx::query(
        r#"
        INSERT INTO session_counts
            (project_id, release, environment, bucket, total, errored, crashed, abnormal)
        VALUES ($1, $2, $3, NOW() - ($4::text || ' hours')::interval, $5, $6, $7, $8)
        "#,
    )
    .bind(project_id)
    .bind(release)
    .bind(env)
    .bind(hours_ago.to_string())
    .bind(total)
    .bind(errored)
    .bind(crashed)
    .bind(abnormal)
    .execute(pool)
    .await
    .expect("seed_count failed");

    #[cfg(not(feature = "postgres"))]
    sqlx::query(
        r#"
        INSERT INTO session_counts
            (project_id, release, environment, bucket, total, errored, crashed, abnormal)
        VALUES (?1, ?2, ?3, datetime('now', '-' || ?4 || ' hours'), ?5, ?6, ?7, ?8)
        "#,
    )
    .bind(project_id)
    .bind(release)
    .bind(env)
    .bind(hours_ago.to_string())
    .bind(total)
    .bind(errored)
    .bind(crashed)
    .bind(abnormal)
    .execute(pool)
    .await
    .expect("seed_count failed");
}

#[actix_web::test]
async fn release_health_environment_filter_changes_rows_and_totals() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Environment Filter Project").await;
    seed_count(&db.pool, project_id, "1.0", "production", 1, 10, 0, 0, 0).await;
    seed_count(&db.pool, project_id, "1.0", "staging", 1, 20, 0, 0, 0).await;
    seed_count(&db.pool, project_id, "2.0", "production", 1, 30, 0, 0, 0).await;

    let (rows, total) =
        SessionService::release_health(&db.pool, project_id, None, 1, 1, Some("production"))
            .await
            .unwrap();
    assert_eq!(total, 2);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].environment, "production");
    let (rows, total) = SessionService::release_health_for_release(
        &db.pool,
        project_id,
        "1.0",
        None,
        1,
        10,
        Some("staging"),
    )
    .await
    .unwrap();
    assert_eq!(total, 1);
    assert_eq!(rows[0].total, 20);
    let (rows, total) = SessionService::release_health(&db.pool, project_id, None, 1, 10, None)
        .await
        .unwrap();
    assert_eq!(total, 3);
    assert_eq!(rows.len(), 3);
}

/// Insert one row into session_users using today's date.
async fn seed_user(
    pool: &DbPool,
    project_id: i32,
    release: &str,
    env: &str,
    did: &str,
    crashed: bool,
) {
    #[cfg(feature = "postgres")]
    sqlx::query(
        r#"
        INSERT INTO session_users (project_id, release, environment, day, did, crashed)
        VALUES ($1, $2, $3, CURRENT_DATE, $4, $5)
        ON CONFLICT (project_id, release, environment, day, did)
        DO UPDATE SET crashed = session_users.crashed OR EXCLUDED.crashed
        "#,
    )
    .bind(project_id)
    .bind(release)
    .bind(env)
    .bind(did)
    .bind(crashed)
    .execute(pool)
    .await
    .expect("seed_user failed");

    #[cfg(not(feature = "postgres"))]
    sqlx::query(
        r#"
        INSERT INTO session_users (project_id, release, environment, day, did, crashed)
        VALUES (?1, ?2, ?3, date('now'), ?4, ?5)
        ON CONFLICT (project_id, release, environment, day, did)
        DO UPDATE SET crashed = MAX(session_users.crashed, excluded.crashed)
        "#,
    )
    .bind(project_id)
    .bind(release)
    .bind(env)
    .bind(did)
    .bind(if crashed { 1i64 } else { 0i64 })
    .execute(pool)
    .await
    .expect("seed_user failed");
}

/// Create a project and return its id.
async fn create_project(pool: &DbPool, name: &str) -> i32 {
    rustrak::services::ProjectService::create(
        pool,
        rustrak::models::CreateProject {
            name: name.to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .expect("create_project failed")
    .id
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[actix_web::test]
async fn test_release_health_empty_returns_empty() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Empty Project").await;

    let (rows, _total) =
        SessionService::release_health(&db.pool, project_id, Some(24), 1, 100, None)
            .await
            .expect("query failed");

    assert!(rows.is_empty());
}

#[actix_web::test]
async fn test_release_health_sums_across_buckets() {
    // This is the primary regression test for the SUM(BIGINT)→NUMERIC issue.
    // Two buckets for the same release — totals must be summed, not returned as separate rows.
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Sum Project").await;

    seed_count(&db.pool, project_id, "1.0.0", "production", 1, 100, 5, 3, 2).await;
    seed_count(
        &db.pool,
        project_id,
        "1.0.0",
        "production",
        2,
        200,
        10,
        6,
        4,
    )
    .await;

    let (rows, _total) =
        SessionService::release_health(&db.pool, project_id, Some(24), 1, 100, None)
            .await
            .expect("query failed");

    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.release, "1.0.0");
    assert_eq!(row.environment, "production");
    assert_eq!(row.total, 300);
    assert_eq!(row.errored, 15);
    assert_eq!(row.crashed, 9);
    assert_eq!(row.abnormal, 6);
}

#[actix_web::test]
async fn test_release_health_healthy_is_total_minus_unhealthy() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Healthy Project").await;

    // total=100, errored=5, crashed=3, abnormal=2 → healthy=90
    seed_count(&db.pool, project_id, "2.0.0", "staging", 1, 100, 5, 3, 2).await;

    let (rows, _total) =
        SessionService::release_health(&db.pool, project_id, Some(24), 1, 100, None)
            .await
            .expect("query failed");

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].healthy, 90);
}

#[actix_web::test]
async fn test_release_health_excludes_buckets_outside_window() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Window Project").await;

    // 1h ago: inside 24h window
    seed_count(&db.pool, project_id, "3.0.0", "production", 1, 50, 0, 0, 0).await;
    // 48h ago: outside 24h window — must be excluded
    seed_count(
        &db.pool,
        project_id,
        "3.0.0",
        "production",
        48,
        9999,
        0,
        0,
        0,
    )
    .await;

    let (rows, _total) =
        SessionService::release_health(&db.pool, project_id, Some(24), 1, 100, None)
            .await
            .expect("query failed");

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].total, 50, "old bucket must not contribute to total");
}

#[actix_web::test]
async fn test_release_health_no_period_returns_all_buckets() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "No Period Project").await;

    // 1h ago
    seed_count(&db.pool, project_id, "4.0.0", "production", 1, 10, 0, 0, 0).await;
    // 48h ago
    seed_count(&db.pool, project_id, "4.0.0", "production", 48, 20, 0, 0, 0).await;
    // 240h ago (10 days)
    seed_count(
        &db.pool,
        project_id,
        "4.0.0",
        "production",
        240,
        30,
        0,
        0,
        0,
    )
    .await;

    // No period filter → all buckets should be included
    let (rows, _total) = SessionService::release_health(&db.pool, project_id, None, 1, 100, None)
        .await
        .expect("query failed");

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].total, 60,
        "all buckets must contribute when no period is set"
    );
}

#[actix_web::test]
async fn test_release_health_excludes_other_projects() {
    let db = TestDb::new().await;
    let project_a = create_project(&db.pool, "Project A").await;
    let project_b = create_project(&db.pool, "Project B").await;

    seed_count(&db.pool, project_a, "1.0.0", "prod", 1, 10, 0, 0, 0).await;
    seed_count(&db.pool, project_b, "1.0.0", "prod", 1, 999, 0, 0, 0).await;

    let (rows, _total) =
        SessionService::release_health(&db.pool, project_a, Some(24), 1, 100, None)
            .await
            .expect("query failed");

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].total, 10,
        "other project's data must not be visible"
    );
}

#[actix_web::test]
async fn test_release_health_orders_by_total_desc() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Order Project").await;

    seed_count(&db.pool, project_id, "small", "prod", 1, 10, 0, 0, 0).await;
    seed_count(&db.pool, project_id, "large", "prod", 1, 500, 0, 0, 0).await;
    seed_count(&db.pool, project_id, "medium", "prod", 1, 100, 0, 0, 0).await;

    let (rows, _total) =
        SessionService::release_health(&db.pool, project_id, Some(24), 1, 100, None)
            .await
            .expect("query failed");

    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].release, "large");
    assert_eq!(rows[1].release, "medium");
    assert_eq!(rows[2].release, "small");
}

#[actix_web::test]
async fn test_release_health_paginates_and_reports_total() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Paginated Project").await;

    seed_count(&db.pool, project_id, "small", "prod", 1, 10, 0, 0, 0).await;
    seed_count(&db.pool, project_id, "large", "prod", 1, 500, 0, 0, 0).await;
    seed_count(&db.pool, project_id, "medium", "prod", 1, 100, 0, 0, 0).await;

    let (page1, total) = SessionService::release_health(&db.pool, project_id, Some(24), 1, 2, None)
        .await
        .expect("query failed");

    assert_eq!(total, 3, "total counts every group, not just this page");
    assert_eq!(page1.len(), 2);
    assert_eq!(page1[0].release, "large");
    assert_eq!(page1[1].release, "medium");

    let (page2, total2) =
        SessionService::release_health(&db.pool, project_id, Some(24), 2, 2, None)
            .await
            .expect("query failed");

    assert_eq!(total2, 3);
    assert_eq!(page2.len(), 1, "last page holds the remainder");
    assert_eq!(page2[0].release, "small");

    let (page3, _) = SessionService::release_health(&db.pool, project_id, Some(24), 3, 2, None)
        .await
        .expect("query failed");

    assert!(page3.is_empty(), "past the last page yields no rows");
}

#[actix_web::test]
async fn test_release_health_pagination_breaks_ties_deterministically() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Tie Project").await;

    // Identical totals: without a tiebreaker the DB is free to order these
    // differently per query, which would let offset pagination skip or repeat.
    seed_count(&db.pool, project_id, "b", "prod", 1, 100, 0, 0, 0).await;
    seed_count(&db.pool, project_id, "a", "prod", 1, 100, 0, 0, 0).await;
    seed_count(&db.pool, project_id, "c", "prod", 1, 100, 0, 0, 0).await;

    let mut seen = Vec::new();
    for page in 1..=3 {
        let (rows, _) =
            SessionService::release_health(&db.pool, project_id, Some(24), page, 1, None)
                .await
                .expect("query failed");
        assert_eq!(rows.len(), 1);
        seen.push(rows[0].release.clone());
    }

    assert_eq!(seen, vec!["a", "b", "c"], "ties order by release ASC");
}

#[actix_web::test]
async fn test_release_health_extreme_page_yields_empty_page() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Overflow Page Project").await;

    seed_count(&db.pool, project_id, "1.0.0", "prod", 1, 100, 0, 0, 0).await;

    // `page` comes straight off a query string. i64::MAX overflows the
    // offset multiplication: a panic under overflow-checks, a negative
    // offset Postgres rejects without them. Either way the caller saw a 500
    // instead of an empty page.
    let (rows, total) =
        SessionService::release_health(&db.pool, project_id, Some(24), i64::MAX, 100, None)
            .await
            .expect("an out-of-range page must not error");

    assert!(rows.is_empty());
    assert_eq!(total, 1, "the total still describes the whole result set");
}

#[actix_web::test]
async fn test_release_health_for_release_reports_scoped_total() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Scoped Total Project").await;

    seed_count(&db.pool, project_id, "1.0.0", "prod", 1, 100, 0, 0, 0).await;
    seed_count(&db.pool, project_id, "1.0.0", "staging", 1, 40, 0, 0, 0).await;
    seed_count(&db.pool, project_id, "2.0.0", "prod", 1, 999, 0, 0, 0).await;

    let (rows, total) = SessionService::release_health_for_release(
        &db.pool,
        project_id,
        "1.0.0",
        Some(24),
        1,
        1,
        None,
    )
    .await
    .expect("query failed");

    assert_eq!(total, 2, "total is scoped to the requested release");
    assert_eq!(rows.len(), 1, "per_page still bounds the page");
    assert_eq!(rows[0].environment, "prod");
}

#[actix_web::test]
async fn test_release_health_crash_free_sessions_rate() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "CFR Sessions Project").await;

    // 100 total, 10 crashed → crash_free_sessions_rate = 0.90
    seed_count(&db.pool, project_id, "1.0.0", "prod", 1, 100, 0, 10, 0).await;

    let (rows, _total) =
        SessionService::release_health(&db.pool, project_id, Some(24), 1, 100, None)
            .await
            .expect("query failed");

    assert_eq!(rows.len(), 1);
    let rate = rows[0]
        .crash_free_sessions_rate
        .expect("crash_free_sessions_rate must be Some when total > 0");
    assert!((rate - 0.9).abs() < 1e-9, "expected 0.9, got {}", rate);
}

#[actix_web::test]
async fn test_release_health_crash_free_sessions_rate_is_none_when_no_sessions() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Zero Sessions Project").await;

    // 0 total → rate must be None (CASE WHEN total > 0)
    seed_count(&db.pool, project_id, "1.0.0", "prod", 1, 0, 0, 0, 0).await;

    let (rows, _total) =
        SessionService::release_health(&db.pool, project_id, Some(24), 1, 100, None)
            .await
            .expect("query failed");

    // A bucket with all zeros should still surface via GROUP BY:
    assert_eq!(
        rows.len(),
        1,
        "zero-total bucket should produce one grouped row"
    );
    assert!(
        rows[0].crash_free_sessions_rate.is_none(),
        "rate must be None when total is 0"
    );
}

#[actix_web::test]
async fn test_release_health_crash_free_users_rate() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "CFR Users Project").await;

    seed_count(&db.pool, project_id, "1.0.0", "prod", 1, 10, 0, 2, 0).await;

    // 4 distinct users, 2 of them crashed → crash_free_users_rate = 0.50
    seed_user(&db.pool, project_id, "1.0.0", "prod", "user-1", false).await;
    seed_user(&db.pool, project_id, "1.0.0", "prod", "user-2", false).await;
    seed_user(&db.pool, project_id, "1.0.0", "prod", "user-3", true).await;
    seed_user(&db.pool, project_id, "1.0.0", "prod", "user-4", true).await;

    let (rows, _total) =
        SessionService::release_health(&db.pool, project_id, Some(24), 1, 100, None)
            .await
            .expect("query failed");

    assert_eq!(rows.len(), 1);
    let rate = rows[0]
        .crash_free_users_rate
        .expect("crash_free_users_rate must be Some when users exist");
    assert!((rate - 0.5).abs() < 1e-9, "expected 0.5, got {}", rate);
}

#[actix_web::test]
async fn test_release_health_total_not_inflated_by_multiple_users() {
    // Regression test: same many-to-many join issue as
    // test_project_summary_total_not_inflated_by_multiple_users, but for the
    // per-release endpoint.
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Release Health No Inflation").await;

    seed_count(&db.pool, project_id, "1.0.0", "prod", 1, 100, 0, 0, 0).await;
    seed_count(&db.pool, project_id, "1.0.0", "prod", 2, 50, 0, 0, 0).await;
    for i in 0..5 {
        seed_user(
            &db.pool,
            project_id,
            "1.0.0",
            "prod",
            &format!("user-{i}"),
            false,
        )
        .await;
    }

    let (rows, _total) =
        SessionService::release_health(&db.pool, project_id, Some(24), 1, 100, None)
            .await
            .expect("query failed");

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].total, 150,
        "total must not be inflated by the number of distinct users"
    );
}

#[actix_web::test]
async fn test_release_health_for_release_filters_server_side() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Scoped Release Project").await;

    seed_count(&db.pool, project_id, "1.0.0", "prod", 1, 100, 0, 0, 0).await;
    seed_count(&db.pool, project_id, "1.0.0", "staging", 1, 40, 0, 0, 0).await;
    seed_count(&db.pool, project_id, "2.0.0", "prod", 1, 999, 0, 0, 0).await;

    let (rows, _total) = SessionService::release_health_for_release(
        &db.pool,
        project_id,
        "1.0.0",
        Some(24),
        1,
        100,
        None,
    )
    .await
    .expect("query failed");

    assert_eq!(
        rows.len(),
        2,
        "only the requested release's rows, across environments"
    );
    assert!(rows.iter().all(|r| r.release == "1.0.0"));
    assert!(rows
        .iter()
        .any(|r| r.environment == "prod" && r.total == 100));
    assert!(rows
        .iter()
        .any(|r| r.environment == "staging" && r.total == 40));
}

// ── project_summary tests ───────────────────────────────────────────────────

#[actix_web::test]
async fn test_project_summary_empty_project_returns_zeros() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Empty Summary Project").await;

    let summary = SessionService::project_summary(&db.pool, project_id, Some(24), None)
        .await
        .expect("query failed");

    assert_eq!(summary.total, 0);
    assert_eq!(summary.errored, 0);
    assert_eq!(summary.crashed, 0);
    assert_eq!(summary.abnormal, 0);
    assert_eq!(summary.active_releases, 0);
    assert!(summary.crash_free_sessions_rate.is_none());
    assert!(summary.crash_free_users_rate.is_none());
}

#[actix_web::test]
async fn test_project_summary_aggregates_across_releases() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Summary Multi Release").await;

    seed_count(&db.pool, project_id, "1.0.0", "prod", 1, 100, 5, 10, 2).await;
    seed_count(&db.pool, project_id, "2.0.0", "prod", 1, 50, 1, 0, 0).await;

    let summary = SessionService::project_summary(&db.pool, project_id, Some(24), None)
        .await
        .expect("query failed");

    assert_eq!(summary.total, 150);
    assert_eq!(summary.errored, 6);
    assert_eq!(summary.crashed, 10);
    assert_eq!(summary.abnormal, 2);
    assert_eq!(
        summary.active_releases, 2,
        "must count distinct releases with total > 0"
    );
}

#[actix_web::test]
async fn test_project_summary_excludes_buckets_outside_window() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Summary Window Project").await;

    seed_count(&db.pool, project_id, "1.0.0", "prod", 1, 50, 0, 0, 0).await;
    seed_count(&db.pool, project_id, "1.0.0", "prod", 48, 9999, 0, 0, 0).await;

    let summary = SessionService::project_summary(&db.pool, project_id, Some(24), None)
        .await
        .expect("query failed");

    assert_eq!(summary.total, 50, "old bucket must not contribute");
}

#[actix_web::test]
async fn test_project_summary_crash_free_rates() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Summary CFR Project").await;

    seed_count(&db.pool, project_id, "1.0.0", "prod", 1, 100, 0, 10, 0).await;
    seed_user(&db.pool, project_id, "1.0.0", "prod", "user-1", false).await;
    seed_user(&db.pool, project_id, "1.0.0", "prod", "user-2", true).await;

    let summary = SessionService::project_summary(&db.pool, project_id, Some(24), None)
        .await
        .expect("query failed");

    let sess_rate = summary
        .crash_free_sessions_rate
        .expect("must be Some when total > 0");
    assert!(
        (sess_rate - 0.9).abs() < 1e-9,
        "expected 0.9, got {sess_rate}"
    );

    let user_rate = summary
        .crash_free_users_rate
        .expect("must be Some when users exist");
    assert!(
        (user_rate - 0.5).abs() < 1e-9,
        "expected 0.5, got {user_rate}"
    );
}

#[actix_web::test]
async fn test_project_summary_total_not_inflated_by_multiple_users() {
    // Regression test: joining session_counts to session_users on
    // (release, environment) alone is a many-to-many join — every
    // session_counts row gets duplicated once per matching session_users row,
    // multiplying SUM(total) by the user count instead of leaving it correct.
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Summary No Inflation Project").await;

    // 2 count buckets (total 100 + 50 = 150) and 5 distinct users for the same
    // release/environment. A many-to-many join would multiply total by 5.
    seed_count(&db.pool, project_id, "1.0.0", "prod", 1, 100, 0, 0, 0).await;
    seed_count(&db.pool, project_id, "1.0.0", "prod", 2, 50, 0, 0, 0).await;
    for i in 0..5 {
        seed_user(
            &db.pool,
            project_id,
            "1.0.0",
            "prod",
            &format!("user-{i}"),
            false,
        )
        .await;
    }

    let summary = SessionService::project_summary(&db.pool, project_id, Some(24), None)
        .await
        .expect("query failed");

    assert_eq!(
        summary.total, 150,
        "total must not be inflated by the number of distinct users"
    );
}

#[actix_web::test]
async fn test_project_summary_active_releases_excludes_zero_total_releases() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Summary Active Releases").await;

    seed_count(&db.pool, project_id, "1.0.0", "prod", 1, 10, 0, 0, 0).await;
    seed_count(&db.pool, project_id, "0.9.0-empty", "prod", 1, 0, 0, 0, 0).await;

    let summary = SessionService::project_summary(&db.pool, project_id, Some(24), None)
        .await
        .expect("query failed");

    assert_eq!(
        summary.active_releases, 1,
        "release with total=0 must not count as active"
    );
}

#[actix_web::test]
async fn test_project_summary_excludes_other_projects() {
    let db = TestDb::new().await;
    let project_a = create_project(&db.pool, "Summary Project A").await;
    let project_b = create_project(&db.pool, "Summary Project B").await;

    seed_count(&db.pool, project_a, "1.0.0", "prod", 1, 10, 0, 0, 0).await;
    seed_count(&db.pool, project_b, "1.0.0", "prod", 1, 999, 0, 0, 0).await;

    let summary = SessionService::project_summary(&db.pool, project_a, Some(24), None)
        .await
        .expect("query failed");

    assert_eq!(
        summary.total, 10,
        "other project's data must not be visible"
    );
    assert_eq!(summary.active_releases, 1);
}

// ── session_timeseries tests ────────────────────────────────────────────────

#[actix_web::test]
async fn test_session_timeseries_empty_project_returns_empty() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Empty Timeseries Project").await;

    let points = SessionService::session_timeseries(&db.pool, project_id, Some(24), 1, None)
        .await
        .expect("query failed");

    assert!(points.is_empty());
}

#[actix_web::test]
async fn test_session_timeseries_aggregates_across_releases_in_same_bucket() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Timeseries Agg Project").await;

    // Both 1h ago, same hourly bucket, different releases — must be summed together.
    seed_count(&db.pool, project_id, "1.0.0", "prod", 1, 100, 5, 10, 2).await;
    seed_count(&db.pool, project_id, "2.0.0", "prod", 1, 50, 1, 0, 0).await;

    let points = SessionService::session_timeseries(&db.pool, project_id, Some(24), 1, None)
        .await
        .expect("query failed");

    assert_eq!(
        points.len(),
        1,
        "same hourly bucket must merge into one point"
    );
    assert_eq!(points[0].total, 150);
    assert_eq!(points[0].crashed, 10);
}

#[actix_web::test]
async fn test_session_timeseries_separates_buckets_outside_interval() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Timeseries Interval Project").await;

    // 1h ago and 5h ago: with interval_hours=1 these fall in different hourly buckets.
    seed_count(&db.pool, project_id, "1.0.0", "prod", 1, 100, 0, 0, 0).await;
    seed_count(&db.pool, project_id, "1.0.0", "prod", 5, 50, 0, 0, 0).await;

    let points = SessionService::session_timeseries(&db.pool, project_id, Some(24), 1, None)
        .await
        .expect("query failed");

    assert_eq!(
        points.len(),
        2,
        "buckets 4h apart must not merge at 1h interval"
    );
}

#[actix_web::test]
async fn test_session_timeseries_excludes_buckets_outside_window() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Timeseries Window Project").await;

    seed_count(&db.pool, project_id, "1.0.0", "prod", 1, 50, 0, 0, 0).await;
    seed_count(&db.pool, project_id, "1.0.0", "prod", 48, 9999, 0, 0, 0).await;

    let points = SessionService::session_timeseries(&db.pool, project_id, Some(24), 1, None)
        .await
        .expect("query failed");

    let total: i64 = points.iter().map(|p| p.total).sum();
    assert_eq!(
        total, 50,
        "old bucket outside the 24h window must be excluded"
    );
}

#[actix_web::test]
async fn test_session_timeseries_ordered_chronologically() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Timeseries Order Project").await;

    seed_count(&db.pool, project_id, "1.0.0", "prod", 1, 10, 0, 0, 0).await;
    seed_count(&db.pool, project_id, "1.0.0", "prod", 5, 20, 0, 0, 0).await;
    seed_count(&db.pool, project_id, "1.0.0", "prod", 10, 30, 0, 0, 0).await;

    let points = SessionService::session_timeseries(&db.pool, project_id, Some(24), 1, None)
        .await
        .expect("query failed");

    assert_eq!(points.len(), 3);
    assert!(
        points.windows(2).all(|w| w[0].bucket <= w[1].bucket),
        "points must be ordered oldest first"
    );
}

#[actix_web::test]
async fn test_session_timeseries_bucket_values_are_parsed_correctly() {
    // Regression test: on SQLite, `datetime(..., 'unixepoch')` returns a
    // space-separated timestamp ("2026-07-03 16:00:00"), but `parse_ts` only
    // accepts T-separated RFC3339/ISO-8601 strings. If the bucket SQL doesn't
    // emit a parseable format, every point silently falls back to `Utc::now()`
    // — the ordering-only test above can't catch that because "all buckets are
    // ~now" still happens to be non-decreasing.
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Timeseries Value Project").await;

    let before = Utc::now();
    seed_count(&db.pool, project_id, "1.0.0", "prod", 10, 30, 0, 0, 0).await;
    seed_count(&db.pool, project_id, "1.0.0", "prod", 1, 10, 0, 0, 0).await;

    let points = SessionService::session_timeseries(&db.pool, project_id, Some(24), 1, None)
        .await
        .expect("query failed");

    assert_eq!(points.len(), 2);
    // A fallback-to-now() bug would put both buckets within milliseconds of
    // `before`/`after`, nowhere near their real ~1h/~10h-ago offsets.
    let older = points[0].bucket;
    let newer = points[1].bucket;
    let gap = newer - older;
    assert!(
        gap.num_minutes() >= 8 * 60,
        "buckets 10h and 1h ago should be ~9h apart, got {gap}"
    );
    assert!(
        older < before - chrono::Duration::hours(8),
        "oldest bucket must be far in the past, not collapsed to now(): {older} vs before={before}"
    );
}

#[actix_web::test]
async fn test_session_timeseries_crash_free_rate() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Timeseries CFR Project").await;

    seed_count(&db.pool, project_id, "1.0.0", "prod", 1, 100, 0, 10, 0).await;

    let points = SessionService::session_timeseries(&db.pool, project_id, Some(24), 1, None)
        .await
        .expect("query failed");

    assert_eq!(points.len(), 1);
    let rate = points[0]
        .crash_free_sessions_rate
        .expect("must be Some when total > 0");
    assert!((rate - 0.9).abs() < 1e-9, "expected 0.9, got {rate}");
}

#[actix_web::test]
async fn test_session_timeseries_excludes_other_projects() {
    let db = TestDb::new().await;
    let project_a = create_project(&db.pool, "Timeseries Project A").await;
    let project_b = create_project(&db.pool, "Timeseries Project B").await;

    seed_count(&db.pool, project_a, "1.0.0", "prod", 1, 10, 0, 0, 0).await;
    seed_count(&db.pool, project_b, "1.0.0", "prod", 1, 999, 0, 0, 0).await;

    let points = SessionService::session_timeseries(&db.pool, project_a, Some(24), 1, None)
        .await
        .expect("query failed");

    let total: i64 = points.iter().map(|p| p.total).sum();
    assert_eq!(total, 10, "other project's data must not be visible");
}

#[actix_web::test]
async fn test_release_health_multiple_releases_independent() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Multi Release Project").await;

    seed_count(&db.pool, project_id, "1.0.0", "prod", 1, 100, 5, 10, 2).await;
    seed_count(&db.pool, project_id, "2.0.0", "prod", 1, 50, 1, 0, 0).await;

    let (rows, _total) =
        SessionService::release_health(&db.pool, project_id, Some(24), 1, 100, None)
            .await
            .expect("query failed");

    assert_eq!(rows.len(), 2);
    // rows[0] = 1.0.0 (total=100), rows[1] = 2.0.0 (total=50)
    let r1 = rows.iter().find(|r| r.release == "1.0.0").unwrap();
    let r2 = rows.iter().find(|r| r.release == "2.0.0").unwrap();

    assert_eq!(r1.total, 100);
    assert_eq!(r1.healthy, 83); // 100-5-10-2
    assert_eq!(r2.total, 50);
    assert_eq!(r2.healthy, 49); // 50-1-0-0
}

// ── route-level tests ────────────────────────────────────────────────────────

/// Minimal app wiring for the sessions routes. Bearer auth needs a real token,
/// so the caller creates one from the same pool.
async fn create_test_token(pool: &DbPool) -> String {
    rustrak::services::AuthTokenService::create(
        pool,
        rustrak::models::CreateAuthToken {
            description: Some("Test token".to_string()),
        },
    )
    .await
    .expect("Failed to create test token")
    .token
}

#[actix_web::test]
async fn test_get_stats_returns_paginated_envelope() {
    use actix_web::{test, web, App};

    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project_id = create_project(&db.pool, "Stats Route Project").await;

    seed_count(&db.pool, project_id, "1.0.0", "prod", 1, 300, 0, 0, 0).await;
    seed_count(&db.pool, project_id, "2.0.0", "prod", 1, 200, 0, 0, 0).await;
    seed_count(&db.pool, project_id, "3.0.0", "prod", 1, 100, 0, 0, 0).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .configure(rustrak::routes::sessions::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{project_id}/sessions/stats?page=2&per_page=2"
        ))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["total_count"], 3);
    assert_eq!(body["page"], 2);
    assert_eq!(body["per_page"], 2);
    assert_eq!(body["total_pages"], 2);

    let items = body["items"].as_array().expect("items must be an array");
    assert_eq!(items.len(), 1, "second page holds the remainder");
    assert_eq!(items[0]["release"], "3.0.0");
}

#[actix_web::test]
async fn test_get_stats_defaults_to_first_page() {
    use actix_web::{test, web, App};

    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project_id = create_project(&db.pool, "Stats Default Page Project").await;

    seed_count(&db.pool, project_id, "1.0.0", "prod", 1, 10, 0, 0, 0).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .configure(rustrak::routes::sessions::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{project_id}/sessions/stats"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["page"], 1, "page defaults to 1 when omitted");
    assert_eq!(body["per_page"], 20, "per_page defaults to the page size");
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
}
