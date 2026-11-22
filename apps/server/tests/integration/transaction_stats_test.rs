//! The performance page's reads: endpoints ranked by the time they take in
//! all, narrowed by window, environment and name; one endpoint's figures and
//! its trend over the window; and its slowest samples.

use crate::common::TestDb;
use chrono::{Duration, Utc};
use rustrak::db::DbPool;
use rustrak::models::CreateProject;
use rustrak::services::transaction::{StatsFilter, StatsSort, TransactionFilters};
use rustrak::services::{ProjectService, TransactionService};
use uuid::Uuid;

async fn project(pool: &DbPool) -> i32 {
    ProjectService::create(
        pool,
        CreateProject {
            name: "Perf".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap()
    .id
}

#[allow(clippy::too_many_arguments)]
async fn txn(
    pool: &DbPool,
    project_id: i32,
    name: &str,
    duration_ms: f64,
    status: &str,
    env: &str,
    hours_ago: i64,
) {
    let at = Utc::now() - Duration::hours(hours_ago);
    sqlx::query(
        "INSERT INTO transactions (id, event_id, project_id, transaction_name, op, status, \
         start_timestamp, timestamp, duration_ms, environment, data) \
         VALUES ($1, $2, $3, $4, 'http.server', $5, $6, $6, $7, $8, '{}')",
    )
    .bind(Uuid::new_v4())
    .bind(Uuid::new_v4())
    .bind(project_id)
    .bind(name)
    .bind(status)
    .bind(at)
    .bind(duration_ms)
    .bind(env)
    .execute(pool)
    .await
    .unwrap();
}

/// `/search` is called often and fast, `/report` rarely and slow: by count
/// the first leads, by time spent the second.
async fn seed(pool: &DbPool) -> i32 {
    let p = project(pool).await;
    for _ in 0..10 {
        txn(pool, p, "GET /search", 10.0, "ok", "production", 1).await;
    }
    txn(pool, p, "GET /report", 900.0, "ok", "production", 1).await;
    txn(
        pool,
        p,
        "GET /report",
        1100.0,
        "internal_error",
        "production",
        2,
    )
    .await;
    txn(pool, p, "GET /old", 50.0, "ok", "staging", 72).await;
    p
}

#[actix_web::test]
async fn endpoints_rank_by_the_time_they_take_in_all() {
    let db = TestDb::new().await;
    let p = seed(&db.pool).await;

    let (rows, total) = TransactionService::stats(&db.pool, p, 1, 20, &StatsFilter::default())
        .await
        .unwrap();

    assert_eq!(total, 3);
    let names: Vec<_> = rows.iter().map(|r| r.transaction_name.as_str()).collect();
    assert_eq!(names, ["GET /report", "GET /search", "GET /old"]);

    let report = &rows[0];
    assert_eq!(report.total_ms, 2000.0);
    assert_eq!(report.avg_ms, 1000.0);
    assert_eq!(report.failure_rate, 0.5);
    // 2000 of the 2150 ms spent in all.
    assert!((report.time_share - 2000.0 / 2150.0).abs() < 1e-9);
    assert!(report.last_seen > Utc::now() - Duration::hours(2));
}

#[actix_web::test]
async fn the_list_narrows_and_sorts_as_asked() {
    let db = TestDb::new().await;
    let p = seed(&db.pool).await;

    let by_count = StatsFilter {
        sort: StatsSort::Count,
        ..Default::default()
    };
    let (rows, _) = TransactionService::stats(&db.pool, p, 1, 20, &by_count)
        .await
        .unwrap();
    assert_eq!(rows[0].transaction_name, "GET /search");

    let recent = StatsFilter {
        period_hours: Some(24),
        ..Default::default()
    };
    let (_, total) = TransactionService::stats(&db.pool, p, 1, 20, &recent)
        .await
        .unwrap();
    assert_eq!(total, 2);

    let staging = StatsFilter {
        environment: Some("staging".to_string()),
        ..Default::default()
    };
    let (rows, _) = TransactionService::stats(&db.pool, p, 1, 20, &staging)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].transaction_name, "GET /old");

    // Case-blind, and `_` is a character, not a wildcard.
    let named = StatsFilter {
        query: Some("REPORT".to_string()),
        ..Default::default()
    };
    let (rows, _) = TransactionService::stats(&db.pool, p, 1, 20, &named)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    let wildcard = StatsFilter {
        query: Some("_".to_string()),
        ..Default::default()
    };
    let (rows, _) = TransactionService::stats(&db.pool, p, 1, 20, &wildcard)
        .await
        .unwrap();
    assert!(rows.is_empty());
}

#[actix_web::test]
async fn one_endpoint_answers_for_the_window() {
    let db = TestDb::new().await;
    let p = seed(&db.pool).await;

    let group = TransactionService::stats_for_group(
        &db.pool,
        p,
        "GET /report",
        Some("http.server"),
        &StatsFilter {
            period_hours: Some(24),
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(group.count, 2);
    // Against every endpoint in the window: /search's 100 ms count too.
    assert!((group.time_share - 2000.0 / 2100.0).abs() < 1e-9);
}

#[actix_web::test]
async fn an_endpoint_trend_has_every_bucket_of_the_window() {
    let db = TestDb::new().await;
    let p = seed(&db.pool).await;

    let points = TransactionService::group_timeseries(
        &db.pool,
        p,
        "GET /report",
        Some("http.server"),
        None,
        Some(24),
    )
    .await
    .unwrap();

    // Hourly over a day: 24 or 25 buckets, depending on the clock.
    assert!(points.len() >= 24);
    assert_eq!(points.iter().map(|p| p.count).sum::<i64>(), 2);
    assert_eq!(points.iter().map(|p| p.failures).sum::<i64>(), 1);
    assert!(points.iter().any(|p| p.p95_ms == 1100.0));
}

#[actix_web::test]
async fn an_all_time_trend_reaches_back_to_the_oldest_call() {
    let db = TestDb::new().await;
    let p = seed(&db.pool).await;

    // Three days back: past any default window a missing one could fall to.
    let points = TransactionService::group_timeseries(
        &db.pool,
        p,
        "GET /old",
        Some("http.server"),
        None,
        None,
    )
    .await
    .unwrap();

    assert_eq!(points.iter().map(|p| p.count).sum::<i64>(), 1);
    // 73 hours in 3-hour buckets, every one present: 25 or 26.
    assert!(points.len() >= 25);
}

#[actix_web::test]
async fn a_sample_from_a_clock_ahead_lands_in_the_current_bucket() {
    let db = TestDb::new().await;
    let p = project(&db.pool).await;
    txn(&db.pool, p, "GET /ahead", 10.0, "ok", "production", -2).await;

    let points = TransactionService::group_timeseries(
        &db.pool,
        p,
        "GET /ahead",
        Some("http.server"),
        None,
        Some(24),
    )
    .await
    .unwrap();

    assert_eq!(points.last().map(|p| p.count), Some(1));
}

#[actix_web::test]
async fn samples_keep_to_the_window() {
    let db = TestDb::new().await;
    let p = seed(&db.pool).await;

    let filters = TransactionFilters {
        period_hours: Some(24),
        ..Default::default()
    };
    let (rows, total) = TransactionService::list_offset(&db.pool, p, 1, 20, &filters)
        .await
        .unwrap();
    assert_eq!(total, 12);
    assert!(rows.iter().all(|r| r.transaction_name != "GET /old"));
}

#[actix_web::test]
async fn samples_can_come_slowest_first() {
    let db = TestDb::new().await;
    let p = seed(&db.pool).await;

    let filters = TransactionFilters {
        name: Some("GET /report".to_string()),
        slowest: true,
        ..Default::default()
    };
    let (rows, _) = TransactionService::list_offset(&db.pool, p, 1, 20, &filters)
        .await
        .unwrap();
    assert_eq!(rows[0].duration_ms, Some(1100.0));
    assert_eq!(rows[0].status.as_deref(), Some("internal_error"));
}

/// `cancelled` and `unknown` are neither a success nor a failure, as in
/// Sentry's `failure_rate()`, so they leave the failure figures alone.
#[actix_web::test]
async fn cancelled_and_unknown_do_not_count_as_failures() {
    let db = TestDb::new().await;
    let p = project(&db.pool).await;
    txn(&db.pool, p, "GET /pay", 10.0, "ok", "production", 1).await;
    txn(
        &db.pool,
        p,
        "GET /pay",
        20.0,
        "internal_error",
        "production",
        1,
    )
    .await;
    txn(&db.pool, p, "GET /pay", 30.0, "cancelled", "production", 1).await;
    txn(&db.pool, p, "GET /pay", 40.0, "unknown", "production", 1).await;
    txn(
        &db.pool,
        p,
        "GET /health",
        5.0,
        "cancelled",
        "production",
        1,
    )
    .await;

    // The ranked list: one failure out of four samples, over both readings
    // of a failure (the aggregate and the failure-rate sort).
    let by_failure_rate = StatsFilter {
        sort: StatsSort::FailureRate,
        ..Default::default()
    };
    let (rows, _) = TransactionService::stats(&db.pool, p, 1, 20, &by_failure_rate)
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].transaction_name, "GET /pay");
    assert_eq!(rows[0].count, 4);
    assert_eq!(rows[0].failure_rate, 0.25);
    assert_eq!(rows[1].transaction_name, "GET /health");
    assert_eq!(rows[1].failure_rate, 0.0);

    // The trend over the window: the same one failure, from the same rule.
    let points = TransactionService::group_timeseries(
        &db.pool,
        p,
        "GET /pay",
        Some("http.server"),
        None,
        Some(24),
    )
    .await
    .unwrap();
    assert_eq!(points.iter().map(|p| p.count).sum::<i64>(), 4);
    assert_eq!(points.iter().map(|p| p.failures).sum::<i64>(), 1);
}
