//! Integration tests for `StatsService` (project overview stats).
//!
//! These seed raw `events` and `issues` rows and verify that:
//!   - the timeseries splits volume by severity and the segments sum to total
//!   - unknown levels fold into `info` instead of vanishing
//!   - transactions never leak into the error-volume series
//!   - buckets are zero-filled across the window
//!   - counters split the current window from the one before it
//!   - everything is scoped to one project

use crate::common::create_test_project;
use rustrak::db::DbPool;
use rustrak::services::release::SummaryFilter;
use rustrak::services::scope::Scope;
use rustrak::services::stats::StatsService;
use rustrak::services::ReleaseService;
use uuid::Uuid;

use crate::common::TestDb;

// ── seed helpers ─────────────────────────────────────────────────────────────

/// Insert one row into `events`, `hours_ago` and a half old.
///
/// The half hour keeps a seed off the edge of the hourly sparkline bars,
/// which are anchored to `now`: a whole number of hours lands on a bar's edge,
/// and rows seeded either side of a clock tick split across two bars.
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
            NOW() - ($4::text || ' hours 30 minutes')::interval,
            NOW() - ($4::text || ' hours 30 minutes')::interval,
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
            datetime('now', '-' || ?4 || ' hours', '-30 minutes'),
            datetime('now', '-' || ?4 || ' hours', '-30 minutes'),
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

/// Insert one row into `issues`, first seen `hours_ago` and a half ago (see
/// `seed_event` for the half hour).
async fn seed_issue(pool: &DbPool, project_id: i32, digest_order: i32, hours_ago: i64) -> Uuid {
    let id = Uuid::new_v4();

    #[cfg(feature = "postgres")]
    sqlx::query(
        r#"
        INSERT INTO issues (id, project_id, digest_order, first_seen, last_seen, status)
        VALUES (
            $1, $2, $3,
            NOW() - ($4::text || ' hours 30 minutes')::interval,
            NOW() - ($4::text || ' hours 30 minutes')::interval,
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
            datetime('now', '-' || ?4 || ' hours', '-30 minutes'),
            datetime('now', '-' || ?4 || ' hours', '-30 minutes'),
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

// ── timeseries ───────────────────────────────────────────────────────────────

#[actix_web::test]
async fn timeseries_on_empty_project_is_all_zeros() {
    let db = TestDb::new().await;
    let project_id = create_test_project(&db.pool, "Stats Empty").await.id;

    let points =
        StatsService::event_timeseries(&db.pool, project_id, Some(24), 1, Scope::default())
            .await
            .expect("query failed");

    // Zero-filled, not empty: a quiet day is 25 bars of height zero.
    assert_eq!(points.len(), 25);
    assert!(points.iter().all(|p| p.total == 0));
}

#[actix_web::test]
async fn timeseries_splits_volume_by_severity() {
    let db = TestDb::new().await;
    let project_id = create_test_project(&db.pool, "Stats Severity").await.id;

    seed_event(&db.pool, project_id, None, "fatal", "error", 2).await;
    seed_event(&db.pool, project_id, None, "error", "error", 2).await;
    seed_event(&db.pool, project_id, None, "error", "error", 2).await;
    seed_event(&db.pool, project_id, None, "warning", "error", 2).await;
    seed_event(&db.pool, project_id, None, "info", "error", 2).await;

    let points =
        StatsService::event_timeseries(&db.pool, project_id, Some(24), 1, Scope::default())
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
    let project_id = create_test_project(&db.pool, "Stats Unknown Level")
        .await
        .id;

    seed_event(&db.pool, project_id, None, "debug", "error", 1).await;
    seed_event(&db.pool, project_id, None, "wat", "error", 1).await;
    seed_event(&db.pool, project_id, None, "", "error", 1).await;

    let points =
        StatsService::event_timeseries(&db.pool, project_id, Some(24), 1, Scope::default())
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
    let project_id = create_test_project(&db.pool, "Stats Txn").await.id;

    seed_event(&db.pool, project_id, None, "error", "error", 1).await;
    seed_event(&db.pool, project_id, None, "info", "transaction", 1).await;
    seed_event(&db.pool, project_id, None, "info", "transaction", 1).await;

    let points =
        StatsService::event_timeseries(&db.pool, project_id, Some(24), 1, Scope::default())
            .await
            .expect("query failed");

    assert_eq!(points.iter().map(|p| p.total).sum::<i64>(), 1);
}

#[actix_web::test]
async fn timeseries_respects_the_window() {
    let db = TestDb::new().await;
    let project_id = create_test_project(&db.pool, "Stats Window").await.id;

    seed_event(&db.pool, project_id, None, "error", "error", 1).await;
    seed_event(&db.pool, project_id, None, "error", "error", 100).await;

    let points =
        StatsService::event_timeseries(&db.pool, project_id, Some(24), 1, Scope::default())
            .await
            .expect("query failed");

    assert_eq!(points.iter().map(|p| p.total).sum::<i64>(), 1);
}

#[actix_web::test]
async fn timeseries_is_scoped_to_one_project() {
    let db = TestDb::new().await;
    let mine = create_test_project(&db.pool, "Stats Mine").await.id;
    let theirs = create_test_project(&db.pool, "Stats Theirs").await.id;

    seed_event(&db.pool, mine, None, "error", "error", 1).await;
    seed_event(&db.pool, theirs, None, "error", "error", 1).await;
    seed_event(&db.pool, theirs, None, "error", "error", 1).await;

    let points = StatsService::event_timeseries(&db.pool, mine, Some(24), 1, Scope::default())
        .await
        .expect("query failed");

    assert_eq!(points.iter().map(|p| p.total).sum::<i64>(), 1);
}

#[actix_web::test]
async fn timeseries_honours_a_wider_interval() {
    let db = TestDb::new().await;
    let project_id = create_test_project(&db.pool, "Stats Interval").await.id;

    seed_event(&db.pool, project_id, None, "error", "error", 1).await;

    let points =
        StatsService::event_timeseries(&db.pool, project_id, Some(24), 6, Scope::default())
            .await
            .expect("query failed");

    // 24h in 6h buckets, inclusive at both ends.
    assert_eq!(points.len(), 5);
    assert_eq!(points.iter().map(|p| p.total).sum::<i64>(), 1);
}

#[actix_web::test]
async fn timeseries_all_time_fills_from_the_first_event_to_now() {
    let db = TestDb::new().await;
    let project_id = create_test_project(&db.pool, "Stats All Time").await.id;

    seed_event(&db.pool, project_id, None, "error", "error", 1).await;
    seed_event(&db.pool, project_id, None, "error", "error", 100).await;

    let points = StatsService::event_timeseries(&db.pool, project_id, None, 24, Scope::default())
        .await
        .expect("query failed");

    // A daily bar for every day in between, so a quiet day reads as one.
    for pair in points.windows(2) {
        assert_eq!((pair[1].bucket - pair[0].bucket).num_hours(), 24);
    }
    assert!(points.len() >= 4);
    assert_eq!(points.iter().map(|p| p.total).sum::<i64>(), 2);
}

// ── summary ──────────────────────────────────────────────────────────────────

#[actix_web::test]
async fn summary_on_empty_project_is_zeroed() {
    let db = TestDb::new().await;
    let project_id = create_test_project(&db.pool, "Summary Empty").await.id;

    let summary = StatsService::project_summary(&db.pool, project_id, Some(24), Scope::default())
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
    let project_id = create_test_project(&db.pool, "Summary Events").await.id;

    // Inside the last 24h.
    seed_event(&db.pool, project_id, None, "error", "error", 1).await;
    seed_event(&db.pool, project_id, None, "error", "error", 10).await;
    // Inside the 24h before that.
    seed_event(&db.pool, project_id, None, "error", "error", 30).await;
    // Older than both windows, so counted in neither.
    seed_event(&db.pool, project_id, None, "error", "error", 100).await;

    let summary = StatsService::project_summary(&db.pool, project_id, Some(24), Scope::default())
        .await
        .expect("query failed");

    assert_eq!(summary.events.current, 2);
    assert_eq!(summary.events.previous, Some(1));
}

#[actix_web::test]
async fn summary_counts_new_issues_by_first_seen() {
    let db = TestDb::new().await;
    let project_id = create_test_project(&db.pool, "Summary New Issues").await.id;

    seed_issue(&db.pool, project_id, 1, 2).await;
    seed_issue(&db.pool, project_id, 2, 5).await;
    seed_issue(&db.pool, project_id, 3, 30).await;

    let summary = StatsService::project_summary(&db.pool, project_id, Some(24), Scope::default())
        .await
        .expect("query failed");

    assert_eq!(summary.new_issues.current, 2);
    assert_eq!(summary.new_issues.previous, Some(1));
}

#[actix_web::test]
async fn summary_open_issues_ignores_the_window() {
    let db = TestDb::new().await;
    let project_id = create_test_project(&db.pool, "Summary Open").await.id;

    // Far older than the 24h window: a backlog count, not a windowed rate.
    seed_issue(&db.pool, project_id, 1, 500).await;
    seed_issue(&db.pool, project_id, 2, 500).await;

    let summary = StatsService::project_summary(&db.pool, project_id, Some(24), Scope::default())
        .await
        .expect("query failed");

    assert_eq!(summary.new_issues.current, 0);
    assert_eq!(summary.open_issues, 2);
}

#[actix_web::test]
async fn summary_all_time_has_no_previous_period() {
    // Reporting 0 for "previous" over all time would render as a bogus +100%.
    let db = TestDb::new().await;
    let project_id = create_test_project(&db.pool, "Summary All Time").await.id;

    seed_event(&db.pool, project_id, None, "error", "error", 1).await;
    seed_event(&db.pool, project_id, None, "error", "error", 5_000).await;
    seed_issue(&db.pool, project_id, 1, 5_000).await;

    let summary = StatsService::project_summary(&db.pool, project_id, None, Scope::default())
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
    let mine = create_test_project(&db.pool, "Summary Mine").await.id;
    let theirs = create_test_project(&db.pool, "Summary Theirs").await.id;

    seed_event(&db.pool, mine, None, "error", "error", 1).await;
    seed_event(&db.pool, theirs, None, "error", "error", 1).await;
    seed_issue(&db.pool, mine, 1, 1).await;
    seed_issue(&db.pool, theirs, 1, 1).await;

    let summary = StatsService::project_summary(&db.pool, mine, Some(24), Scope::default())
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
    let project_id = create_test_project(&db.pool, "Summary Max Window").await.id;

    seed_event(&db.pool, project_id, None, "error", "error", 1).await;

    let summary =
        StatsService::project_summary(&db.pool, project_id, Some(90 * 24), Scope::default())
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
    let busy = create_test_project(&db.pool, "List Busy").await.id;
    let quiet = create_test_project(&db.pool, "List Quiet").await.id;

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
    let project_id = create_test_project(&db.pool, "List Buckets").await.id;

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
    let project_id = create_test_project(&db.pool, "List Trend Distinct")
        .await
        .id;

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
    let project_id = create_test_project(&db.pool, "List Trend Stack").await.id;

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
    let project_id = create_test_project(&db.pool, "List Trend Undigested")
        .await
        .id;

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
    let project_id = create_test_project(&db.pool, "List Equal Windows").await.id;

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
    let project_id = create_test_project(&db.pool, "List New Issues").await.id;

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
    let project_id = create_test_project(&db.pool, "List Delta").await.id;

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
    let project_id = create_test_project(&db.pool, "List Only Errors").await.id;

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
    let project_id = create_test_project(&db.pool, "List Issues").await.id;

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
    let mine = create_test_project(&db.pool, "List Mine").await.id;
    let theirs = create_test_project(&db.pool, "List Theirs").await.id;

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
    let asked = create_test_project(&db.pool, "List Asked").await.id;
    let unasked = create_test_project(&db.pool, "List Unasked").await.id;

    seed_event(&db.pool, unasked, None, "error", "error", 2).await;

    let stats = StatsService::list_stats(&db.pool, &[asked], 24)
        .await
        .expect("query failed");

    assert_eq!(stats.len(), 1);
    assert!(!stats.contains_key(&unasked));
}

// ── environment filter ───────────────────────────────────────────────────────

/// Moves every event of `issue_id` into `environment`, and records the issue
/// as seen there, the way digest does.
async fn place_in_environment(pool: &DbPool, project_id: i32, issue_id: Uuid, environment: &str) {
    sqlx::query("UPDATE events SET environment = $1 WHERE issue_id = $2")
        .bind(environment)
        .bind(issue_id)
        .execute(pool)
        .await
        .expect("set environment failed");

    sqlx::query(
        "INSERT INTO issue_environments (issue_id, project_id, environment, first_seen, last_seen) \
         VALUES ($1, $2, $3, $4, $4)",
    )
        .bind(issue_id)
        .bind(project_id)
        .bind(environment)
        .bind(chrono::Utc::now())
        .execute(pool)
        .await
        .expect("seed issue environment failed");
}

#[actix_web::test]
async fn timeseries_counts_only_the_asked_environment() {
    let db = TestDb::new().await;
    let project_id = create_test_project(&db.pool, "Stats Env Series").await.id;

    let prod = seed_issue(&db.pool, project_id, 1, 1).await;
    let staging = seed_issue(&db.pool, project_id, 2, 1).await;
    seed_event(&db.pool, project_id, Some(prod), "error", "error", 1).await;
    seed_event(&db.pool, project_id, Some(prod), "error", "error", 2).await;
    seed_event(&db.pool, project_id, Some(staging), "error", "error", 1).await;
    place_in_environment(&db.pool, project_id, prod, "production").await;
    place_in_environment(&db.pool, project_id, staging, "staging").await;

    let points = StatsService::event_timeseries(
        &db.pool,
        project_id,
        Some(24),
        1,
        Scope::environment(Some("production")),
    )
    .await
    .expect("query failed");
    assert_eq!(points.iter().map(|p| p.total).sum::<i64>(), 2);

    let all = StatsService::event_timeseries(&db.pool, project_id, Some(24), 1, Scope::default())
        .await
        .expect("query failed");
    assert_eq!(all.iter().map(|p| p.total).sum::<i64>(), 3);
}

#[actix_web::test]
async fn summary_counts_only_the_asked_environment() {
    let db = TestDb::new().await;
    let project_id = create_test_project(&db.pool, "Stats Env Summary").await.id;

    let prod = seed_issue(&db.pool, project_id, 1, 1).await;
    let staging = seed_issue(&db.pool, project_id, 2, 1).await;
    seed_event(&db.pool, project_id, Some(prod), "error", "error", 1).await;
    seed_event(&db.pool, project_id, Some(staging), "error", "error", 1).await;
    seed_event(&db.pool, project_id, Some(staging), "error", "error", 2).await;
    place_in_environment(&db.pool, project_id, prod, "production").await;
    place_in_environment(&db.pool, project_id, staging, "staging").await;

    let summary = StatsService::project_summary(
        &db.pool,
        project_id,
        Some(24),
        Scope::environment(Some("staging")),
    )
    .await
    .expect("query failed");

    assert_eq!(summary.events.current, 2);
    assert_eq!(summary.new_issues.current, 1);
    assert_eq!(summary.open_issues, 1);

    let all_time = StatsService::project_summary(
        &db.pool,
        project_id,
        None,
        Scope::environment(Some("production")),
    )
    .await
    .expect("query failed");
    assert_eq!(all_time.events.current, 1);
    assert_eq!(all_time.new_issues.current, 1);
}

// ── release filter ───────────────────────────────────────────────────────────

async fn place_in_release(pool: &DbPool, issue_id: Uuid, release: &str) {
    sqlx::query("UPDATE events SET release = $1 WHERE issue_id = $2")
        .bind(release)
        .bind(issue_id)
        .execute(pool)
        .await
        .expect("set release failed");
}

fn in_release(release: &str) -> Scope<'_> {
    Scope {
        release: Some(release),
        ..Scope::default()
    }
}

#[actix_web::test]
async fn timeseries_and_summary_count_only_the_asked_release() {
    let db = TestDb::new().await;
    let project_id = create_test_project(&db.pool, "Stats Release").await.id;

    let old = seed_issue(&db.pool, project_id, 1, 1).await;
    let new = seed_issue(&db.pool, project_id, 2, 1).await;
    seed_event(&db.pool, project_id, Some(old), "error", "error", 1).await;
    seed_event(&db.pool, project_id, Some(new), "error", "error", 1).await;
    seed_event(&db.pool, project_id, Some(new), "error", "error", 2).await;
    place_in_release(&db.pool, old, "1.0.0").await;
    place_in_release(&db.pool, new, "2.0.0").await;

    let points =
        StatsService::event_timeseries(&db.pool, project_id, Some(24), 1, in_release("2.0.0"))
            .await
            .expect("query failed");
    assert_eq!(points.iter().map(|p| p.total).sum::<i64>(), 2);

    let summary =
        StatsService::project_summary(&db.pool, project_id, Some(24), in_release("1.0.0"))
            .await
            .expect("query failed");
    assert_eq!(summary.events.current, 1);
    assert_eq!(summary.new_issues.current, 1);
    assert_eq!(summary.open_issues, 1);
}

#[actix_web::test]
async fn issue_list_shows_only_the_issues_seen_in_the_asked_release() {
    let db = TestDb::new().await;
    let project_id = create_test_project(&db.pool, "Issues Release").await.id;

    let old = seed_issue(&db.pool, project_id, 1, 1).await;
    let new = seed_issue(&db.pool, project_id, 2, 1).await;
    seed_event(&db.pool, project_id, Some(old), "error", "error", 1).await;
    seed_event(&db.pool, project_id, Some(new), "error", "error", 1).await;
    place_in_release(&db.pool, old, "1.0.0").await;
    place_in_release(&db.pool, new, "2.0.0").await;

    let (issues, total) = rustrak::services::IssueService::list_offset(
        &db.pool,
        project_id,
        rustrak::pagination::IssueSort::LastSeen,
        rustrak::pagination::SortOrder::Desc,
        rustrak::pagination::IssueFilter::All,
        rustrak::pagination::IssueCategory::default(),
        1,
        20,
        None,
        in_release("2.0.0"),
        &rustrak::services::issue::IssueFilters::default(),
    )
    .await
    .expect("query failed");

    assert_eq!(total, 1);
    assert_eq!(issues[0].id, new);
}

#[actix_web::test]
async fn releases_include_the_ones_only_sessions_arrived_in() {
    let db = TestDb::new().await;
    let project_id = create_test_project(&db.pool, "Healthy release").await.id;
    let issue = seed_issue(&db.pool, project_id, 1, 1).await;
    seed_event(&db.pool, project_id, Some(issue), "error", "error", 1).await;
    place_in_release(&db.pool, issue, "1.0.0").await;
    // A release with sessions and not one error.
    sqlx::query(
        "INSERT INTO session_counts \
         (project_id, release, environment, bucket, total, errored, crashed, abnormal) \
         VALUES ($1, '2.0.0', 'production', '2026-06-10T10:00:00Z', 1, 0, 0, 0)",
    )
    .bind(project_id)
    .execute(&db.pool)
    .await
    .unwrap();

    let releases: Vec<String> =
        ReleaseService::summaries(&db.pool, project_id, &SummaryFilter::default())
            .await
            .expect("query failed")
            .into_iter()
            .map(|summary| summary.version)
            .collect();

    assert!(releases.contains(&"2.0.0".to_string()), "{releases:?}");
    assert!(releases.contains(&"1.0.0".to_string()), "{releases:?}");
}
