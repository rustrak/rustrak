//! Project-wide aggregate stats for the overview dashboard.
//!
//! Distinct from `SessionService` (which aggregates the pre-rolled
//! `session_counts`/`session_users` tables) and from `IssueService::stats`
//! (which is scoped to one issue): everything here spans a whole project's
//! `events` and `issues`.

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};

use crate::db::dialect::{bucket, bucket_time, compare_time, epoch, since};
use crate::db::DbPool;
use crate::error::AppResult;
use crate::models::stats::{
    EventTimeseriesPoint, MetricDelta, ProjectListStats, ProjectStatsSummary,
};
use crate::services::scope::Scope;

/// Levels the timeseries breaks out by name. Anything else (`debug`, or a
/// level an SDK invented) folds into the `info` bucket so the four series
/// always sum to `total`.
const NAMED_LEVELS: [&str; 3] = ["fatal", "error", "warning"];

/// Bars in the project-list sparkline.
///
/// Fixed rather than derived from the window, so the column keeps one width
/// whether the user is looking at 24 hours or 30 days — only the bucket width
/// changes. Sentry instead varies the count per window (24 hourly bars for
/// `24h`, 30 daily bars for `30d`); it can afford that because its sparkline
/// sits in a card, not in a fixed table column.
pub const LIST_TREND_BUCKETS: i64 = 24;

pub struct StatsService;

impl StatsService {
    /// Time-bucketed error-event volume for a project, split by severity.
    ///
    /// Buckets are zero-filled across the whole window, so a quiet hour renders
    /// as a bar of height zero rather than a gap the chart would interpolate
    /// across. All-time requests (`period_hours` is `None`) fill from the
    /// first event's bucket.
    pub async fn event_timeseries(
        pool: &DbPool,
        project_id: i32,
        period_hours: Option<i64>,
        interval_hours: i64,
        scope: Scope<'_>,
    ) -> AppResult<Vec<EventTimeseriesPoint>> {
        let interval_secs = interval_hours.max(1) * 3600;
        let points =
            query_event_timeseries(pool, project_id, period_hours, interval_secs, scope).await?;

        let now = Utc::now();
        // Points come back oldest first; the first is the all-time lower bound.
        let first = points.first().map(|p| (now - p.bucket).num_hours());
        let Some(hours) = period_hours.or(first) else {
            return Ok(points);
        };

        Ok(zero_fill(points, hours, interval_secs, now))
    }

    /// Project-wide counters for the window, each paired with the same counter
    /// over the window immediately before it.
    ///
    /// In a `scope`, events count only there and issues only when they have
    /// been seen there: an environment's from `issue_environments`, a
    /// release's from its events.
    pub async fn project_summary(
        pool: &DbPool,
        project_id: i32,
        period_hours: Option<i64>,
        scope: Scope<'_>,
    ) -> AppResult<ProjectStatsSummary> {
        // The comparison window is the same length, immediately before the
        // current one, so every query below scans `[previous_start, now]` once
        // and splits the two halves with a CASE rather than running twice.
        let bounds = period_hours.map(|hours| {
            let now = Utc::now();
            (
                now - Duration::hours(hours * 2),
                now - Duration::hours(hours),
            )
        });

        let events = count_events(pool, project_id, bounds, scope).await?;
        let new_issues = count_new_issues(pool, project_id, bounds, scope).await?;
        let open_issues = count_open_issues(pool, project_id, scope).await?;

        Ok(ProjectStatsSummary {
            period_hours,
            events,
            new_issues,
            open_issues,
        })
    }

    /// Aggregates for a whole page of the project list, in two queries total.
    ///
    /// The batching is the point: the project list renders up to 100 rows, and
    /// a per-row `event_timeseries` + `project_summary` pair would mean 200
    /// round trips per page load. `IssueService::list_stats` exists for the
    /// same reason on the issue list.
    ///
    /// Every id in `project_ids` is present in the result, zeroed if the
    /// project has no data — a caller rendering a row cannot do anything
    /// useful with a missing key except invent the zero itself.
    pub async fn list_stats(
        pool: &DbPool,
        project_ids: &[i32],
        period_hours: i64,
    ) -> AppResult<HashMap<i32, ProjectListStats>> {
        let mut out: HashMap<i32, ProjectListStats> = project_ids
            .iter()
            .map(|id| {
                (
                    *id,
                    ProjectListStats {
                        trend: vec![0i64; LIST_TREND_BUCKETS as usize],
                        events: MetricDelta::new(0, Some(0)),
                        new_issues: MetricDelta::new(0, Some(0)),
                        open_issues: 0,
                        fatal_issues: 0,
                    },
                )
            })
            .collect();

        if project_ids.is_empty() {
            return Ok(out);
        }

        // One grid of 2 x LIST_TREND_BUCKETS buckets ending exactly at now: the
        // later half is the sparkline and the current counts, the earlier half
        // is the comparison window.
        //
        // Anchored to `now` rather than to a clock boundary. Flooring `now` to
        // the bucket size instead makes the current window short by however far
        // into the bucket the request lands — up to a full bucket, and always
        // in the same direction — so every delta on the page understated the
        // current period against a full-length previous one. Sentry splits this
        // the same way: `get_session_stats` derives its comparison bounds from
        // `now` directly, and only the timeseries is bucket-aligned.
        //
        // The cost is that bars no longer start on the hour, which a sparkline
        // with no x-axis cannot show anyway; the gain is that every bucket
        // covers a whole interval, including the newest.
        let bucket_secs = ((period_hours * 3600) / LIST_TREND_BUCKETS).max(1);
        let now = Utc::now().timestamp();
        let first = now - LIST_TREND_BUCKETS * 2 * bucket_secs;
        let current_start = now - LIST_TREND_BUCKETS * bucket_secs;

        for (project_id, idx, events, issues) in
            query_list_volume(pool, project_ids, first, bucket_secs).await?
        {
            let Some(entry) = out.get_mut(&project_id) else {
                continue;
            };
            if idx < 0 {
                continue;
            }
            if idx < LIST_TREND_BUCKETS {
                entry.events.previous = Some(entry.events.previous.unwrap_or(0) + events);
            } else {
                entry.events.current += events;
                // Clamped rather than dropped. With the grid anchored to `now`
                // the top index is open: an event ingested in this very second
                // lands one past the last bar, as does one whose `ingested_at`
                // is slightly in the future from clock skew on the writer.
                // Both belong in the newest bar, not nowhere.
                let slot = ((idx - LIST_TREND_BUCKETS) as usize).min(entry.trend.len() - 1);
                entry.trend[slot] += issues;
            }
        }

        for (project_id, counts) in
            query_list_issue_counts(pool, project_ids, first, current_start).await?
        {
            if let Some(entry) = out.get_mut(&project_id) {
                let (open, fatal, new_current, new_previous) = counts;
                entry.open_issues = open;
                entry.fatal_issues = fatal;
                entry.new_issues = MetricDelta::new(new_current, Some(new_previous));
            }
        }

        Ok(out)
    }
}

/// Renders project ids for an `IN (...)` list.
///
/// Inlined rather than bound because the two dialects disagree on binding a
/// list (`= ANY($1)` in Postgres, N placeholders in SQLite) and this is the
/// one input where inlining is provably safe: `i32`'s `Display` can only ever
/// emit digits and a leading `-`.
fn id_list(project_ids: &[i32]) -> String {
    project_ids
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// `(project_id, bucket_index, error_events, distinct_active_issues)`.
///
/// The index is relative to `first`, so it runs `0..=2*LIST_TREND_BUCKETS`
/// rather than being an absolute clock-aligned timestamp — the grid is
/// anchored to the request instant, not to the hour. The caller clamps the
/// open upper end, which an event ingested in the same instant can reach.
///
/// Both figures come from one scan because they answer different questions
/// about the same rows: the count drives the Events column, the distinct issue
/// count drives the sparkline. `COUNT(DISTINCT issue_id)` skips NULLs, so an
/// event still awaiting digest raises volume without inventing an issue.
async fn query_list_volume(
    pool: &DbPool,
    project_ids: &[i32],
    first: i64,
    bucket_secs: i64,
) -> AppResult<Vec<(i32, i64, i64, i64)>> {
    let ids = id_list(project_ids);
    // The WHERE clause keeps the numerator non-negative, so integer division
    // is `floor`.
    let sql = format!(
        r#"
        SELECT
            project_id,
            ({ingested} - {first}) / {bucket_secs} AS bucket_idx,
            COUNT(*) AS events,
            COUNT(DISTINCT issue_id) AS issues
        FROM events
        WHERE project_id IN ({ids})
          AND event_type = 'error'
          AND {since_first}
        GROUP BY 1, 2
        "#,
        ingested = epoch("ingested_at"),
        since_first = compare_time("ingested_at", ">=", 1),
    );
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(&*sql))
        .bind(bucket_time(first)?)
        .fetch_all(pool)
        .await?)
}

/// `(project_id, (open, fatal, new_in_current_window, new_in_previous_window))`.
///
/// The open/fatal pair is deliberately unwindowed: an issue that last fired a
/// week ago is still open, and an "Issues" column that emptied itself
/// overnight would read as "fixed" rather than "quiet". The new-issue pair is
/// windowed, and is what tells the UI whether things are getting worse — event
/// volume cannot, since one chatty issue can multiply it on its own.
///
/// Both live in one query because they share a scan of the same project's
/// issues; splitting them would double it for no gain.
async fn query_list_issue_counts(
    pool: &DbPool,
    project_ids: &[i32],
    first: i64,
    current_start: i64,
) -> AppResult<Vec<(i32, (i64, i64, i64, i64))>> {
    let ids = id_list(project_ids);
    let [fatal, _, _] = NAMED_LEVELS;

    let sql = format!(
        r#"
        SELECT
            project_id,
            SUM(CASE WHEN status = 'unresolved' THEN 1 ELSE 0 END) AS open_issues,
            SUM(CASE WHEN status = 'unresolved' AND COALESCE(level, '') = '{fatal}'
                     THEN 1 ELSE 0 END) AS fatal_issues,
            SUM(CASE WHEN {in_current} THEN 1 ELSE 0 END) AS new_current,
            SUM(CASE WHEN {since_first} AND {before_current} THEN 1 ELSE 0 END) AS new_previous
        FROM issues
        WHERE project_id IN ({ids}) AND issue_category = 'error'
        GROUP BY 1
        "#,
        in_current = compare_time("first_seen", ">=", 2),
        before_current = compare_time("first_seen", "<", 2),
        since_first = compare_time("first_seen", ">=", 1),
    );
    let rows: Vec<(i32, i64, i64, i64, i64)> = sqlx::query_as(sqlx::AssertSqlSafe(&*sql))
        .bind(bucket_time(first)?)
        .bind(bucket_time(current_start)?)
        .fetch_all(pool)
        .await?;

    Ok(rows
        .into_iter()
        .map(|(id, open, fatal, new_current, new_previous)| {
            (id, (open, fatal, new_current, new_previous))
        })
        .collect())
}

/// Expand sparse buckets to every bucket in `[now - hours, now]`.
fn zero_fill(
    points: Vec<EventTimeseriesPoint>,
    hours: i64,
    interval_secs: i64,
    now: DateTime<Utc>,
) -> Vec<EventTimeseriesPoint> {
    let align = |ts: i64| ts.div_euclid(interval_secs) * interval_secs;
    let last = align(now.timestamp());
    let first = align(now.timestamp() - hours * 3600);

    let mut by_bucket: HashMap<i64, EventTimeseriesPoint> = points
        .into_iter()
        .map(|p| (p.bucket.timestamp(), p))
        .collect();

    let mut filled = Vec::new();
    let mut ts = first;
    while ts <= last {
        filled.push(
            by_bucket
                .remove(&ts)
                .unwrap_or_else(|| EventTimeseriesPoint {
                    bucket: DateTime::from_timestamp(ts, 0).unwrap_or(now),
                    total: 0,
                    fatal: 0,
                    error: 0,
                    warning: 0,
                    info: 0,
                }),
        );
        ts += interval_secs;
    }

    // A row outside the generated range (clock skew, an event ingested with a
    // future timestamp) would otherwise be dropped silently.
    let mut leftovers: Vec<_> = by_bucket.into_values().collect();
    if !leftovers.is_empty() {
        filled.append(&mut leftovers);
        filled.sort_by_key(|p| p.bucket);
    }

    filled
}

async fn query_event_timeseries(
    pool: &DbPool,
    project_id: i32,
    period_hours: Option<i64>,
    interval_secs: i64,
    scope: Scope<'_>,
) -> AppResult<Vec<EventTimeseriesPoint>> {
    let [fatal, error, warning] = NAMED_LEVELS;
    // COALESCE(level, '') keeps the four severity buckets exhaustive: on a
    // NULL level `level NOT IN (...)` is NULL, not true, so the row would
    // count toward `total` but toward none of the segments.
    let sql = format!(
        r#"
        SELECT
            {bucket} AS bucket,
            COUNT(*) AS total,
            SUM(CASE WHEN COALESCE(level, '') = '{fatal}'   THEN 1 ELSE 0 END) AS fatal,
            SUM(CASE WHEN COALESCE(level, '') = '{error}'   THEN 1 ELSE 0 END) AS error,
            SUM(CASE WHEN COALESCE(level, '') = '{warning}' THEN 1 ELSE 0 END) AS warning,
            SUM(CASE WHEN COALESCE(level, '') NOT IN ('{fatal}', '{error}', '{warning}')
                     THEN 1 ELSE 0 END) AS info
        FROM events
        WHERE project_id = $1
          AND event_type = 'error'
          AND ($2 IS NULL OR environment = $2)
          AND ($3 IS NULL OR release = $3)
          AND {window}
        GROUP BY 1
        ORDER BY 1 ASC
        "#,
        bucket = bucket("ingested_at", interval_secs),
        window = since("ingested_at", period_hours),
    );
    let rows: Vec<(i64, i64, i64, i64, i64, i64)> = sqlx::query_as(sqlx::AssertSqlSafe(&*sql))
        .bind(project_id)
        .bind(scope.environment)
        .bind(scope.release)
        .fetch_all(pool)
        .await?;
    rows.into_iter()
        .map(|(bucket, total, fatal, error, warning, info)| {
            Ok(EventTimeseriesPoint {
                bucket: bucket_time(bucket)?,
                total,
                fatal,
                error,
                warning,
                info,
            })
        })
        .collect()
}

/// Window bounds as `(previous_start, current_start)`, or `None` for all time.
type Bounds = Option<(DateTime<Utc>, DateTime<Utc>)>;

/// Narrows `issues` to the ones seen in the scope's environment (bound as
/// `$2`) and release (`$3`); either one `NULL` narrows nothing. With both, one
/// event has to be in both, as `IssueService::list` reads them.
const ISSUE_IN_SCOPE: &str = "($2 IS NULL OR id IN (
        SELECT issue_id FROM issue_environments WHERE project_id = $1 AND environment = $2))
    AND ($3 IS NULL OR id IN (
        SELECT issue_id FROM events WHERE project_id = $1 AND release = $3
        AND ($2 IS NULL OR environment = $2)))";

async fn count_events(
    pool: &DbPool,
    project_id: i32,
    bounds: Bounds,
    scope: Scope<'_>,
) -> AppResult<MetricDelta> {
    let Some((previous_start, current_start)) = bounds else {
        let (total,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM events WHERE project_id = $1 AND event_type = 'error' \
             AND ($2 IS NULL OR environment = $2) AND ($3 IS NULL OR release = $3)",
        )
        .bind(project_id)
        .bind(scope.environment)
        .bind(scope.release)
        .fetch_one(pool)
        .await?;
        return Ok(MetricDelta::new(total, None));
    };

    let sql = format!(
        r#"
        SELECT
            CAST(COALESCE(SUM(CASE WHEN {current} THEN 1 ELSE 0 END), 0) AS BIGINT),
            CAST(COALESCE(SUM(CASE WHEN {previous} THEN 1 ELSE 0 END), 0) AS BIGINT)
        FROM events
        WHERE project_id = $1
          AND ($2 IS NULL OR environment = $2)
          AND ($3 IS NULL OR release = $3)
          AND event_type = 'error'
          AND {window}
        "#,
        current = compare_time("ingested_at", ">=", 4),
        previous = compare_time("ingested_at", "<", 4),
        window = compare_time("ingested_at", ">=", 5),
    );
    let (current, previous): (i64, i64) = sqlx::query_as(sqlx::AssertSqlSafe(&*sql))
        .bind(project_id)
        .bind(scope.environment)
        .bind(scope.release)
        .bind(current_start)
        .bind(previous_start)
        .fetch_one(pool)
        .await?;

    Ok(MetricDelta::new(current, Some(previous)))
}

async fn count_new_issues(
    pool: &DbPool,
    project_id: i32,
    bounds: Bounds,
    scope: Scope<'_>,
) -> AppResult<MetricDelta> {
    let Some((previous_start, current_start)) = bounds else {
        let sql = format!(
            "SELECT COUNT(*) FROM issues WHERE project_id = $1 \
             AND issue_category = 'error' AND {ISSUE_IN_SCOPE}"
        );
        let (total,): (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(&*sql))
            .bind(project_id)
            .bind(scope.environment)
            .bind(scope.release)
            .fetch_one(pool)
            .await?;
        return Ok(MetricDelta::new(total, None));
    };

    let sql = format!(
        r#"
        SELECT
            CAST(COALESCE(SUM(CASE WHEN {current} THEN 1 ELSE 0 END), 0) AS BIGINT),
            CAST(COALESCE(SUM(CASE WHEN {previous} THEN 1 ELSE 0 END), 0) AS BIGINT)
        FROM issues
        WHERE project_id = $1
          AND issue_category = 'error'
          AND {window}
          AND {ISSUE_IN_SCOPE}
        "#,
        current = compare_time("first_seen", ">=", 4),
        previous = compare_time("first_seen", "<", 4),
        window = compare_time("first_seen", ">=", 5),
    );
    let (current, previous): (i64, i64) = sqlx::query_as(sqlx::AssertSqlSafe(&*sql))
        .bind(project_id)
        .bind(scope.environment)
        .bind(scope.release)
        .bind(current_start)
        .bind(previous_start)
        .fetch_one(pool)
        .await?;

    Ok(MetricDelta::new(current, Some(previous)))
}

/// Issues currently unresolved. A backlog size, not a windowed rate, so it has
/// no previous-period counterpart.
async fn count_open_issues(pool: &DbPool, project_id: i32, scope: Scope<'_>) -> AppResult<i64> {
    let sql = format!(
        "SELECT COUNT(*) FROM issues WHERE project_id = $1 AND status = 'unresolved' \
         AND issue_category = 'error' AND {ISSUE_IN_SCOPE}"
    );

    let (total,): (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(&*sql))
        .bind(project_id)
        .bind(scope.environment)
        .bind(scope.release)
        .fetch_one(pool)
        .await?;

    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(ts: i64, total: i64) -> EventTimeseriesPoint {
        EventTimeseriesPoint {
            bucket: DateTime::from_timestamp(ts, 0).unwrap(),
            total,
            fatal: 0,
            error: total,
            warning: 0,
            info: 0,
        }
    }

    /// A fixed `now` on an exact hour boundary keeps the expected bucket
    /// timestamps readable.
    fn now() -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000, 0).unwrap()
    }

    #[test]
    fn zero_fill_covers_every_bucket_in_the_window() {
        let hour = 3600;
        let filled = zero_fill(vec![], 6, hour, now());
        // Six hours back through the current bucket, inclusive at both ends.
        assert_eq!(filled.len(), 7);
        assert!(filled.iter().all(|p| p.total == 0));
    }

    #[test]
    fn zero_fill_keeps_existing_buckets_in_place() {
        let hour = 3600;
        let base = now().timestamp().div_euclid(hour) * hour;
        let filled = zero_fill(vec![point(base - 2 * hour, 5)], 6, hour, now());

        let hit: Vec<_> = filled.iter().filter(|p| p.total > 0).collect();
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0].total, 5);
        assert_eq!(hit[0].bucket.timestamp(), base - 2 * hour);
    }

    #[test]
    fn zero_fill_emits_buckets_in_ascending_order() {
        let hour = 3600;
        let filled = zero_fill(vec![], 12, hour, now());
        let mut sorted = filled.iter().map(|p| p.bucket).collect::<Vec<_>>();
        sorted.sort();
        assert_eq!(sorted, filled.iter().map(|p| p.bucket).collect::<Vec<_>>());
    }

    #[test]
    fn zero_fill_keeps_rows_outside_the_generated_range() {
        // An event ingested with a future timestamp still has to appear.
        let hour = 3600;
        let base = now().timestamp().div_euclid(hour) * hour;
        let filled = zero_fill(vec![point(base + 5 * hour, 3)], 6, hour, now());

        assert!(filled.iter().any(|p| p.total == 3));
        let mut sorted = filled.iter().map(|p| p.bucket).collect::<Vec<_>>();
        sorted.sort();
        assert_eq!(sorted, filled.iter().map(|p| p.bucket).collect::<Vec<_>>());
    }

    #[test]
    fn zero_fill_respects_a_multi_hour_interval() {
        let six_hours = 6 * 3600;
        let filled = zero_fill(vec![], 24, six_hours, now());
        assert_eq!(filled.len(), 5);
        for pair in filled.windows(2) {
            assert_eq!(
                pair[1].bucket.timestamp() - pair[0].bucket.timestamp(),
                six_hours
            );
        }
    }
}
