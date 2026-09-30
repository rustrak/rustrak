//! Integration tests for `StatsService` (project overview stats).
//!
//! These seed raw `events` and `issues` rows and verify that:
//!   - the timeseries splits volume by severity and the segments sum to total
//!   - unknown levels fold into `info` instead of vanishing
//!   - transactions never leak into the error-volume series
//!   - buckets are zero-filled across the window
//!   - counters split the current window from the one before it
//!   - everything is scoped to one project

use rustrak::db::DbPool;
use rustrak::services::stats::StatsService;
use uuid::Uuid;

use crate::common::TestDb;

// ── seed helpers ─────────────────────────────────────────────────────────────

/// Insert one row into `events`, `hours_ago` old.
async fn seed_event(
    pool: &DbPool,
    project_id: i32,
    issue_id: Option<Uuid>,
    level: &str,
    event_type: &str,
    hours_ago: i64,
) {
    #[cfg(feature = "postgres")]
    sqlx::query(
        r#"
        INSERT INTO events
            (event_id, project_id, issue_id, data, timestamp, ingested_at, level, event_type)
        VALUES (
            $1, $2, $3, '{}'::jsonb,
            NOW() - ($4::text || ' hours')::interval,
            NOW() - ($4::text || ' hours')::interval,
            $5, $6
        )
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(project_id)
    .bind(issue_id)
    .bind(hours_ago.to_string())
    .bind(level)
    .bind(event_type)
    .execute(pool)
    .await
    .expect("seed_event failed");

    #[cfg(not(feature = "postgres"))]
    sqlx::query(
        r#"
        INSERT INTO events
            (event_id, project_id, issue_id, data, timestamp, ingested_at, level, event_type)
        VALUES (
            ?1, ?2, ?3, '{}',
            datetime('now', '-' || ?4 || ' hours'),
            datetime('now', '-' || ?4 || ' hours'),
            ?5, ?6
        )
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(project_id)
    .bind(issue_id)
    .bind(hours_ago.to_string())
    .bind(level)
    .bind(event_type)
    .execute(pool)
    .await
    .expect("seed_event failed");
}

/// Insert one row into `issues`, first seen `hours_ago` ago.
async fn seed_issue(pool: &DbPool, project_id: i32, digest_order: i32, hours_ago: i64) -> Uuid {
    let id = Uuid::new_v4();

    #[cfg(feature = "postgres")]
    sqlx::query(
        r#"
        INSERT INTO issues (id, project_id, digest_order, first_seen, last_seen, status)
        VALUES (
            $1, $2, $3,
            NOW() - ($4::text || ' hours')::interval,
            NOW() - ($4::text || ' hours')::interval,
            $5
        )
        "#,
    )
    .bind(id)
    .bind(project_id)
    .bind(digest_order)
    .bind(hours_ago.to_string())
    .bind("unresolved")
    .execute(pool)
    .await
    .expect("seed_issue failed");

    #[cfg(not(feature = "postgres"))]
    sqlx::query(
        r#"
        INSERT INTO issues (id, project_id, digest_order, first_seen, last_seen, status)
        VALUES (
            ?1, ?2, ?3,
            datetime('now', '-' || ?4 || ' hours'),
            datetime('now', '-' || ?4 || ' hours'),
            ?5
        )
        "#,
    )
    .bind(id)
    .bind(project_id)
    .bind(digest_order)
    .bind(hours_ago.to_string())
    .bind("unresolved")
    .execute(pool)
    .await
    .expect("seed_issue failed");

    id
}

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

// ── timeseries ───────────────────────────────────────────────────────────────

#[actix_web::test]
async fn environment_filters_overview_counts_and_chart() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Overview environments").await;
    let prod_issue = seed_issue(&db.pool, project_id, 1, 1).await;
    let stage_issue = seed_issue(&db.pool, project_id, 2, 1).await;
    seed_event(&db.pool, project_id, Some(prod_issue), "error", "error", 1).await;
    seed_event(
        &db.pool,
        project_id,
        Some(stage_issue),
        "warning",
        "error",
        1,
    )
    .await;
    seed_event(&db.pool, project_id, None, "fatal", "error", 1).await;
    sqlx::query(
        "UPDATE events SET environment = 'production' WHERE project_id = $1 AND level = 'error'",
    )
    .bind(project_id)
    .execute(&db.pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE events SET environment = 'staging' WHERE project_id = $1 AND level = 'warning'",
    )
    .bind(project_id)
    .execute(&db.pool)
    .await
    .unwrap();

    let summary = StatsService::project_summary(&db.pool, project_id, Some(24), Some("production"))
        .await
        .unwrap();
    assert_eq!(summary.events.current, 1);
    assert_eq!(summary.new_issues.current, 1);
    assert_eq!(summary.open_issues, 1);
    let chart =
        StatsService::event_timeseries(&db.pool, project_id, Some(24), 1, Some("production"))
            .await
            .unwrap();
    assert_eq!(chart.iter().map(|point| point.total).sum::<i64>(), 1);
    let all = StatsService::project_summary(&db.pool, project_id, Some(24), None)
        .await
        .unwrap();
    assert_eq!(all.events.current, 3);
    assert_eq!(all.open_issues, 2);
}

#[actix_web::test]
async fn environment_new_issues_use_first_matching_event_date() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "New issue environments").await;
    let issue = seed_issue(&db.pool, project_id, 1, 36).await;
    seed_event(&db.pool, project_id, Some(issue), "error", "error", 36).await;
    seed_event(&db.pool, project_id, Some(issue), "error", "error", 2).await;
    sqlx::query(
        "UPDATE events SET environment = CASE WHEN timestamp < $2 THEN 'production' ELSE 'staging' END WHERE issue_id = $1",
    )
    .bind(issue)
    .bind(chrono::Utc::now() - chrono::Duration::hours(24))
    .execute(&db.pool)
    .await
    .unwrap();

    let staging = StatsService::project_summary(&db.pool, project_id, Some(24), Some("staging"))
        .await
        .unwrap();
    assert_eq!(staging.new_issues.current, 1);
    assert_eq!(staging.new_issues.previous, Some(0));

    let production =
        StatsService::project_summary(&db.pool, project_id, Some(24), Some("production"))
            .await
            .unwrap();
    assert_eq!(production.new_issues.current, 0);
    assert_eq!(production.new_issues.previous, Some(1));
}

#[actix_web::test]
async fn timeseries_on_empty_project_is_all_zeros() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Stats Empty").await;

    let points = StatsService::event_timeseries(&db.pool, project_id, Some(24), 1, None)
        .await
        .expect("query failed");

    // Zero-filled, not empty: a quiet day is 25 bars of height zero.
    assert_eq!(points.len(), 25);
    assert!(points.iter().all(|p| p.total == 0));
}

#[actix_web::test]
async fn timeseries_splits_volume_by_severity() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Stats Severity").await;

    seed_event(&db.pool, project_id, None, "fatal", "error", 2).await;
    seed_event(&db.pool, project_id, None, "error", "error", 2).await;
    seed_event(&db.pool, project_id, None, "error", "error", 2).await;
    seed_event(&db.pool, project_id, None, "warning", "error", 2).await;
    seed_event(&db.pool, project_id, None, "info", "error", 2).await;

    let points = StatsService::event_timeseries(&db.pool, project_id, Some(24), 1, None)
        .await
        .expect("query failed");

    assert_eq!(points.iter().map(|p| p.fatal).sum::<i64>(), 1);
    assert_eq!(points.iter().map(|p| p.error).sum::<i64>(), 2);
    assert_eq!(points.iter().map(|p| p.warning).sum::<i64>(), 1);
    assert_eq!(points.iter().map(|p| p.info).sum::<i64>(), 1);
    assert_eq!(points.iter().map(|p| p.total).sum::<i64>(), 5);
}

#[actix_web::test]
async fn timeseries_segments_always_sum_to_total() {
    // `debug` and any level we do not model must land in `info`, or the
    // stacked segments would not add up to the bar height.
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Stats Unknown Level").await;

    seed_event(&db.pool, project_id, None, "debug", "error", 1).await;
    seed_event(&db.pool, project_id, None, "wat", "error", 1).await;
    seed_event(&db.pool, project_id, None, "", "error", 1).await;

    let points = StatsService::event_timeseries(&db.pool, project_id, Some(24), 1, None)
        .await
        .expect("query failed");

    for p in &points {
        assert_eq!(p.total, p.fatal + p.error + p.warning + p.info);
    }
    assert_eq!(points.iter().map(|p| p.info).sum::<i64>(), 3);
}

#[actix_web::test]
async fn timeseries_excludes_transactions() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Stats Txn").await;

    seed_event(&db.pool, project_id, None, "error", "error", 1).await;
    seed_event(&db.pool, project_id, None, "info", "transaction", 1).await;
    seed_event(&db.pool, project_id, None, "info", "transaction", 1).await;

    let points = StatsService::event_timeseries(&db.pool, project_id, Some(24), 1, None)
        .await
        .expect("query failed");

    assert_eq!(points.iter().map(|p| p.total).sum::<i64>(), 1);
}

#[actix_web::test]
async fn timeseries_respects_the_window() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Stats Window").await;

    seed_event(&db.pool, project_id, None, "error", "error", 1).await;
    seed_event(&db.pool, project_id, None, "error", "error", 100).await;

    let points = StatsService::event_timeseries(&db.pool, project_id, Some(24), 1, None)
        .await
        .expect("query failed");

    assert_eq!(points.iter().map(|p| p.total).sum::<i64>(), 1);
}

#[actix_web::test]
async fn timeseries_is_scoped_to_one_project() {
    let db = TestDb::new().await;
    let mine = create_project(&db.pool, "Stats Mine").await;
    let theirs = create_project(&db.pool, "Stats Theirs").await;

    seed_event(&db.pool, mine, None, "error", "error", 1).await;
    seed_event(&db.pool, theirs, None, "error", "error", 1).await;
    seed_event(&db.pool, theirs, None, "error", "error", 1).await;

    let points = StatsService::event_timeseries(&db.pool, mine, Some(24), 1, None)
        .await
        .expect("query failed");

    assert_eq!(points.iter().map(|p| p.total).sum::<i64>(), 1);
}

#[actix_web::test]
async fn timeseries_honours_a_wider_interval() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Stats Interval").await;

    seed_event(&db.pool, project_id, None, "error", "error", 1).await;

    let points = StatsService::event_timeseries(&db.pool, project_id, Some(24), 6, None)
        .await
        .expect("query failed");

    // 24h in 6h buckets, inclusive at both ends.
    assert_eq!(points.len(), 5);
    assert_eq!(points.iter().map(|p| p.total).sum::<i64>(), 1);
}

// ── summary ──────────────────────────────────────────────────────────────────

#[actix_web::test]
async fn summary_on_empty_project_is_zeroed() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Summary Empty").await;

    let summary = StatsService::project_summary(&db.pool, project_id, Some(24), None)
        .await
        .expect("query failed");

    assert_eq!(summary.period_hours, Some(24));
    assert_eq!(summary.events.current, 0);
    assert_eq!(summary.events.previous, Some(0));
    assert_eq!(summary.new_issues.current, 0);
    assert_eq!(summary.open_issues, 0);
}

#[actix_web::test]
async fn summary_splits_events_into_current_and_previous_windows() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Summary Events").await;

    // Inside the last 24h.
    seed_event(&db.pool, project_id, None, "error", "error", 1).await;
    seed_event(&db.pool, project_id, None, "error", "error", 10).await;
    // Inside the 24h before that.
    seed_event(&db.pool, project_id, None, "error", "error", 30).await;
    // Older than both windows, so counted in neither.
    seed_event(&db.pool, project_id, None, "error", "error", 100).await;

    let summary = StatsService::project_summary(&db.pool, project_id, Some(24), None)
        .await
        .expect("query failed");

    assert_eq!(summary.events.current, 2);
    assert_eq!(summary.events.previous, Some(1));
}

#[actix_web::test]
async fn summary_counts_new_issues_by_first_seen() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Summary New Issues").await;

    seed_issue(&db.pool, project_id, 1, 2).await;
    seed_issue(&db.pool, project_id, 2, 5).await;
    seed_issue(&db.pool, project_id, 3, 30).await;

    let summary = StatsService::project_summary(&db.pool, project_id, Some(24), None)
        .await
        .expect("query failed");

    assert_eq!(summary.new_issues.current, 2);
    assert_eq!(summary.new_issues.previous, Some(1));
}

#[actix_web::test]
async fn summary_open_issues_ignores_the_window() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Summary Open").await;

    // Far older than the 24h window: a backlog count, not a windowed rate.
    seed_issue(&db.pool, project_id, 1, 500).await;
    seed_issue(&db.pool, project_id, 2, 500).await;

    let summary = StatsService::project_summary(&db.pool, project_id, Some(24), None)
        .await
        .expect("query failed");

    assert_eq!(summary.new_issues.current, 0);
    assert_eq!(summary.open_issues, 2);
}

#[actix_web::test]
async fn summary_all_time_has_no_previous_period() {
    // Reporting 0 for "previous" over all time would render as a bogus +100%.
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Summary All Time").await;

    seed_event(&db.pool, project_id, None, "error", "error", 1).await;
    seed_event(&db.pool, project_id, None, "error", "error", 5_000).await;
    seed_issue(&db.pool, project_id, 1, 5_000).await;

    let summary = StatsService::project_summary(&db.pool, project_id, None, None)
        .await
        .expect("query failed");

    assert_eq!(summary.period_hours, None);
    assert_eq!(summary.events.current, 2);
    assert_eq!(summary.events.previous, None);
    assert_eq!(summary.new_issues.current, 1);
    assert_eq!(summary.new_issues.previous, None);
}

#[actix_web::test]
async fn summary_is_scoped_to_one_project() {
    let db = TestDb::new().await;
    let mine = create_project(&db.pool, "Summary Mine").await;
    let theirs = create_project(&db.pool, "Summary Theirs").await;

    seed_event(&db.pool, mine, None, "error", "error", 1).await;
    seed_event(&db.pool, theirs, None, "error", "error", 1).await;
    seed_issue(&db.pool, mine, 1, 1).await;
    seed_issue(&db.pool, theirs, 1, 1).await;

    let summary = StatsService::project_summary(&db.pool, mine, Some(24), None)
        .await
        .expect("query failed");

    assert_eq!(summary.events.current, 1);
    assert_eq!(summary.new_issues.current, 1);
    assert_eq!(summary.open_issues, 1);
}

/// The window is clamped before it reaches the query, so an absurd `?period=`
/// cannot ask the database for an inverted or unbounded range.
#[actix_web::test]
async fn summary_accepts_the_maximum_clamped_window() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "Summary Max Window").await;

    seed_event(&db.pool, project_id, None, "error", "error", 1).await;

    let summary = StatsService::project_summary(&db.pool, project_id, Some(90 * 24), None)
        .await
        .expect("query failed");

    assert_eq!(summary.events.current, 1);
}

// ── project-list stats (batched) ─────────────────────────────────────────────

/// Insert one error event `minutes_ago` old.
///
/// Minute resolution because window-boundary tests need to place events at a
/// known offset inside an hour, which `seed_event`'s whole hours cannot do.
async fn seed_event_minutes_ago(pool: &DbPool, project_id: i32, minutes_ago: i64) {
    #[cfg(feature = "postgres")]
    sqlx::query(
        r#"
        INSERT INTO events
            (event_id, project_id, data, timestamp, ingested_at, level, event_type)
        VALUES (
            $1, $2, '{}'::jsonb,
            NOW() - ($3::text || ' minutes')::interval,
            NOW() - ($3::text || ' minutes')::interval,
            'error', 'error'
        )
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(project_id)
    .bind(minutes_ago.to_string())
    .execute(pool)
    .await
    .expect("seed_event_minutes_ago failed");

    #[cfg(not(feature = "postgres"))]
    sqlx::query(
        r#"
        INSERT INTO events
            (event_id, project_id, data, timestamp, ingested_at, level, event_type)
        VALUES (
            ?1, ?2, '{}',
            datetime('now', '-' || ?3 || ' minutes'),
            datetime('now', '-' || ?3 || ' minutes'),
            'error', 'error'
        )
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(project_id)
    .bind(minutes_ago.to_string())
    .execute(pool)
    .await
    .expect("seed_event_minutes_ago failed");
}

/// Insert one issue with an explicit `status` and `level`.
///
/// The `seed_issue` helper above hardcodes `unresolved` and leaves `level`
/// NULL, which cannot exercise the open/fatal split.
async fn seed_issue_with(
    pool: &DbPool,
    project_id: i32,
    digest_order: i32,
    status: &str,
    level: Option<&str>,
) -> Uuid {
    let id = Uuid::new_v4();

    #[cfg(feature = "postgres")]
    sqlx::query(
        r#"
        INSERT INTO issues (id, project_id, digest_order, first_seen, last_seen, status, level)
        VALUES ($1, $2, $3, NOW(), NOW(), $4, $5)
        "#,
    )
    .bind(id)
    .bind(project_id)
    .bind(digest_order)
    .bind(status)
    .bind(level)
    .execute(pool)
    .await
    .expect("seed_issue_with failed");

    #[cfg(not(feature = "postgres"))]
    sqlx::query(
        r#"
        INSERT INTO issues (id, project_id, digest_order, first_seen, last_seen, status, level)
        VALUES (?1, ?2, ?3, datetime('now'), datetime('now'), ?4, ?5)
        "#,
    )
    .bind(id)
    .bind(project_id)
    .bind(digest_order)
    .bind(status)
    .bind(level)
    .execute(pool)
    .await
    .expect("seed_issue_with failed");

    id
}

#[actix_web::test]
async fn list_stats_with_no_ids_does_not_query() {
    let db = TestDb::new().await;

    let stats = StatsService::list_stats(&db.pool, &[], 24)
        .await
        .expect("query failed");

    assert!(stats.is_empty());
}

/// Every requested project gets an entry, even one that has never seen an
/// event: the table renders a row for it regardless, and a missing key would
/// force the UI to invent the zero itself.
#[actix_web::test]
async fn list_stats_returns_an_entry_for_every_requested_project() {
    let db = TestDb::new().await;
    let busy = create_project(&db.pool, "List Busy").await;
    let quiet = create_project(&db.pool, "List Quiet").await;

    seed_event(&db.pool, busy, None, "error", "error", 2).await;

    let stats = StatsService::list_stats(&db.pool, &[busy, quiet], 24)
        .await
        .expect("query failed");

    assert_eq!(stats.len(), 2);
    assert_eq!(stats[&busy].events.current, 1);
    assert_eq!(stats[&quiet].events.current, 0);
    assert!(stats[&quiet].trend.iter().all(|&b| b == 0));
}

/// The sparkline is a fixed-width column, so the bucket count must not vary
/// with the window — only the bucket width does.
#[actix_web::test]
async fn list_stats_trend_bucket_count_is_fixed_across_windows() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "List Buckets").await;

    for hours in [24, 24 * 7, 24 * 30] {
        let stats = StatsService::list_stats(&db.pool, &[project_id], hours)
            .await
            .expect("query failed");

        assert_eq!(
            stats[&project_id].trend.len(),
            rustrak::services::stats::LIST_TREND_BUCKETS as usize,
            "window of {hours}h changed the bucket count"
        );
    }
}

/// The sparkline counts *distinct issues active* per bucket, not events. Ten
/// events from one issue in one hour is one bar of height one — otherwise a
/// single chatty issue would draw the same picture as a project falling apart.
#[actix_web::test]
async fn list_stats_trend_counts_distinct_issues_not_events() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "List Trend Distinct").await;

    let noisy = seed_issue(&db.pool, project_id, 1, 2).await;
    for _ in 0..10 {
        seed_event(&db.pool, project_id, Some(noisy), "error", "error", 2).await;
    }

    let stats = StatsService::list_stats(&db.pool, &[project_id], 24)
        .await
        .expect("query failed");

    let entry = &stats[&project_id];
    assert_eq!(entry.trend.iter().sum::<i64>(), 1, "one issue, one bar");
    assert_eq!(entry.events.current, 10, "but ten events");
}

/// Two issues firing in the same bucket must stack, or the sparkline could
/// never rise above one.
#[actix_web::test]
async fn list_stats_trend_stacks_concurrent_issues() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "List Trend Stack").await;

    let first = seed_issue(&db.pool, project_id, 1, 2).await;
    let second = seed_issue(&db.pool, project_id, 2, 2).await;
    seed_event(&db.pool, project_id, Some(first), "error", "error", 2).await;
    seed_event(&db.pool, project_id, Some(second), "error", "error", 2).await;

    let stats = StatsService::list_stats(&db.pool, &[project_id], 24)
        .await
        .expect("query failed");

    assert_eq!(stats[&project_id].trend.iter().sum::<i64>(), 2);
}

/// An event that has not been digested yet has no `issue_id`. It is real
/// volume, but it is not evidence of an issue.
#[actix_web::test]
async fn list_stats_trend_ignores_events_with_no_issue() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "List Trend Undigested").await;

    seed_event(&db.pool, project_id, None, "error", "error", 2).await;

    let stats = StatsService::list_stats(&db.pool, &[project_id], 24)
        .await
        .expect("query failed");

    let entry = &stats[&project_id];
    assert_eq!(entry.events.current, 1);
    assert_eq!(entry.trend.iter().sum::<i64>(), 0);
}

/// Both comparison windows must cover the same elapsed time, whatever moment
/// the request lands on.
///
/// Regression test for a grid anchored to the clock instead of to `now`: the
/// current window was short by however far into the bucket the request
/// arrived — up to a full hour, and always in the same direction — so every
/// delta understated the current period against a full-length previous one.
///
/// Seeds one event per hour on both sides of the boundary. Symmetric input, so
/// any asymmetry in the result is the window bounds and nothing else.
#[actix_web::test]
async fn list_stats_windows_cover_equal_spans() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "List Equal Windows").await;

    // Half-hour offsets keep every event clear of a bucket edge, so the test
    // fails on a genuinely lopsided window rather than on rounding.
    for i in 0..24 {
        seed_event_minutes_ago(&db.pool, project_id, i * 60 + 30).await;
        seed_event_minutes_ago(&db.pool, project_id, (i + 24) * 60 + 30).await;
    }

    let stats = StatsService::list_stats(&db.pool, &[project_id], 24)
        .await
        .expect("query failed");

    let entry = &stats[&project_id];
    assert_eq!(
        entry.events.current,
        entry.events.previous.expect("previous must be present"),
        "24 events an hour apart on each side must count equally; \
         a difference means the two windows are not the same length"
    );
    assert_eq!(entry.events.current, 24);
}

/// The "issues are climbing" signal the row colour depends on.
#[actix_web::test]
async fn list_stats_splits_new_issues_across_the_two_windows() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "List New Issues").await;

    // 2h ago -> current; 30h ago -> previous; 60h ago -> outside both.
    seed_issue(&db.pool, project_id, 1, 2).await;
    seed_issue(&db.pool, project_id, 2, 2).await;
    seed_issue(&db.pool, project_id, 3, 2).await;
    seed_issue(&db.pool, project_id, 4, 30).await;
    seed_issue(&db.pool, project_id, 5, 60).await;

    let stats = StatsService::list_stats(&db.pool, &[project_id], 24)
        .await
        .expect("query failed");

    let entry = &stats[&project_id];
    assert_eq!(entry.new_issues.current, 3);
    assert_eq!(entry.new_issues.previous, Some(1));
    // Still open regardless of when they were born.
    assert_eq!(entry.open_issues, 5);
}

#[actix_web::test]
async fn list_stats_splits_the_current_window_from_the_one_before_it() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "List Delta").await;

    // One issue per age so the sparkline can be asserted alongside the
    // counters: 2h ago -> current window; 30h ago -> previous; 60h -> neither.
    let recent = seed_issue(&db.pool, project_id, 1, 2).await;
    let older = seed_issue(&db.pool, project_id, 2, 30).await;
    let ancient = seed_issue(&db.pool, project_id, 3, 60).await;

    seed_event(&db.pool, project_id, Some(recent), "error", "error", 2).await;
    seed_event(&db.pool, project_id, Some(recent), "error", "error", 2).await;
    seed_event(&db.pool, project_id, Some(older), "error", "error", 30).await;
    seed_event(&db.pool, project_id, Some(ancient), "error", "error", 60).await;

    let stats = StatsService::list_stats(&db.pool, &[project_id], 24)
        .await
        .expect("query failed");

    let entry = &stats[&project_id];
    assert_eq!(entry.events.current, 2);
    assert_eq!(entry.events.previous, Some(1));
    // Only the issue active inside the current window reaches the sparkline:
    // the one 30h ago is in the comparison window, the one 60h ago in neither.
    assert_eq!(entry.trend.iter().sum::<i64>(), 1);
}

/// Transactions and sessions share the `events` table; only errors belong in
/// an error-volume sparkline.
#[actix_web::test]
async fn list_stats_counts_only_error_events() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "List Only Errors").await;

    seed_event(&db.pool, project_id, None, "error", "error", 2).await;
    seed_event(&db.pool, project_id, None, "info", "transaction", 2).await;

    let stats = StatsService::list_stats(&db.pool, &[project_id], 24)
        .await
        .expect("query failed");

    assert_eq!(stats[&project_id].events.current, 1);
}

/// Issue counters are a snapshot of what is broken right now, deliberately
/// independent of the event window — an issue that stopped firing yesterday
/// is still open today.
#[actix_web::test]
async fn list_stats_counts_open_issues_and_the_fatal_subset() {
    let db = TestDb::new().await;
    let project_id = create_project(&db.pool, "List Issues").await;

    seed_issue_with(&db.pool, project_id, 1, "unresolved", Some("fatal")).await;
    seed_issue_with(&db.pool, project_id, 2, "unresolved", Some("error")).await;
    seed_issue_with(&db.pool, project_id, 3, "unresolved", None).await;
    seed_issue_with(&db.pool, project_id, 4, "resolved", Some("fatal")).await;
    seed_issue_with(&db.pool, project_id, 5, "ignored", Some("fatal")).await;

    let stats = StatsService::list_stats(&db.pool, &[project_id], 24)
        .await
        .expect("query failed");

    let entry = &stats[&project_id];
    assert_eq!(entry.open_issues, 3);
    // Resolved and ignored fatals are not "currently on fire".
    assert_eq!(entry.fatal_issues, 1);
}

#[actix_web::test]
async fn list_stats_never_mixes_two_projects() {
    let db = TestDb::new().await;
    let mine = create_project(&db.pool, "List Mine").await;
    let theirs = create_project(&db.pool, "List Theirs").await;

    seed_event(&db.pool, mine, None, "error", "error", 2).await;
    for _ in 0..4 {
        seed_event(&db.pool, theirs, None, "error", "error", 2).await;
    }
    seed_issue_with(&db.pool, mine, 1, "unresolved", Some("fatal")).await;
    seed_issue_with(&db.pool, theirs, 1, "unresolved", Some("fatal")).await;
    seed_issue_with(&db.pool, theirs, 2, "unresolved", Some("fatal")).await;

    let stats = StatsService::list_stats(&db.pool, &[mine, theirs], 24)
        .await
        .expect("query failed");

    assert_eq!(stats[&mine].events.current, 1);
    assert_eq!(stats[&mine].fatal_issues, 1);
    assert_eq!(stats[&theirs].events.current, 4);
    assert_eq!(stats[&theirs].fatal_issues, 2);
}

/// A project the caller never asked about must not appear, even though the
/// batched queries scan the same tables.
#[actix_web::test]
async fn list_stats_omits_projects_outside_the_requested_set() {
    let db = TestDb::new().await;
    let asked = create_project(&db.pool, "List Asked").await;
    let unasked = create_project(&db.pool, "List Unasked").await;

    seed_event(&db.pool, unasked, None, "error", "error", 2).await;

    let stats = StatsService::list_stats(&db.pool, &[asked], 24)
        .await
        .expect("query failed");

    assert_eq!(stats.len(), 1);
    assert!(!stats.contains_key(&unasked));
}
