//! Project-wide aggregate stats for the overview dashboard.
//!
//! Distinct from `SessionService` (which aggregates the pre-rolled
//! `session_counts`/`session_users` tables) and from `IssueService::stats`
//! (which is scoped to one issue): everything here spans a whole project's
//! `events` and `issues`.

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};

use crate::db::DbPool;
use crate::error::AppResult;
use crate::models::stats::{
    EventTimeseriesPoint, MetricDelta, ProjectListStats, ProjectStatsSummary,
};

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
    /// across. All-time requests (`period_hours` is `None`) are returned sparse:
    /// there is no lower bound to fill from.
    pub async fn event_timeseries(
        pool: &DbPool,
        project_id: i32,
        period_hours: Option<i64>,
        interval_hours: i64,
        environment: Option<&str>,
    ) -> AppResult<Vec<EventTimeseriesPoint>> {
        let interval_secs = interval_hours.max(1) * 3600;
        let points =
            query_event_timeseries(pool, project_id, period_hours, interval_secs, environment)
                .await?;

        let Some(hours) = period_hours else {
            return Ok(points);
        };

        Ok(zero_fill(points, hours, interval_secs, Utc::now()))
    }

    /// Project-wide counters for the window, each paired with the same counter
    /// over the window immediately before it.
    pub async fn project_summary(
        pool: &DbPool,
        project_id: i32,
        period_hours: Option<i64>,
        environment: Option<&str>,
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

        let events = count_events(pool, project_id, bounds, environment).await?;
        let new_issues = count_new_issues(pool, project_id, bounds, environment).await?;
        let open_issues = count_open_issues(pool, project_id, environment).await?;

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

    #[cfg(feature = "postgres")]
    {
        let sql = format!(
            r#"
            SELECT
                project_id,
                floor((extract(epoch FROM ingested_at) - {first}) / {bucket_secs})::bigint
                    AS bucket_idx,
                COUNT(*)::bigint AS events,
                COUNT(DISTINCT issue_id)::bigint AS issues
            FROM events
            WHERE project_id IN ({ids})
              AND event_type = 'error'
              AND ingested_at >= to_timestamp({first})
            GROUP BY 1, 2
            "#,
        );

        Ok(sqlx::query_as(sqlx::AssertSqlSafe(&*sql))
            .fetch_all(pool)
            .await?)
    }

    #[cfg(not(feature = "postgres"))]
    {
        // The WHERE clause keeps the numerator non-negative, so SQLite's
        // truncating integer division agrees with Postgres' `floor`.
        let sql = format!(
            r#"
            SELECT
                project_id,
                (CAST(strftime('%s', ingested_at) AS INTEGER) - {first}) / {bucket_secs}
                    AS bucket_idx,
                COUNT(*) AS events,
                COUNT(DISTINCT issue_id) AS issues
            FROM events
            WHERE project_id IN ({ids})
              AND event_type = 'error'
              AND CAST(strftime('%s', ingested_at) AS INTEGER) >= {first}
            GROUP BY 1, 2
            "#,
        );

        Ok(sqlx::query_as(sqlx::AssertSqlSafe(&*sql))
            .fetch_all(pool)
            .await?)
    }
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

    #[cfg(feature = "postgres")]
    let rows: Vec<(i32, i64, i64, i64, i64)> = {
        let sql = format!(
            r#"
            SELECT
                project_id,
                SUM(CASE WHEN status = 'unresolved' THEN 1 ELSE 0 END)::bigint AS open_issues,
                SUM(CASE WHEN status = 'unresolved' AND COALESCE(level, '') = '{fatal}'
                         THEN 1 ELSE 0 END)::bigint AS fatal_issues,
                SUM(CASE WHEN first_seen >= to_timestamp({current_start})
                         THEN 1 ELSE 0 END)::bigint AS new_current,
                SUM(CASE WHEN first_seen >= to_timestamp({first})
                          AND first_seen < to_timestamp({current_start})
                         THEN 1 ELSE 0 END)::bigint AS new_previous
            FROM issues
            WHERE project_id IN ({ids})
            GROUP BY 1
            "#,
        );

        sqlx::query_as(sqlx::AssertSqlSafe(&*sql))
            .fetch_all(pool)
            .await?
    };

    #[cfg(not(feature = "postgres"))]
    let rows: Vec<(i32, i64, i64, i64, i64)> = {
        let sql = format!(
            r#"
            SELECT
                project_id,
                SUM(CASE WHEN status = 'unresolved' THEN 1 ELSE 0 END) AS open_issues,
                SUM(CASE WHEN status = 'unresolved' AND COALESCE(level, '') = '{fatal}'
                         THEN 1 ELSE 0 END) AS fatal_issues,
                SUM(CASE WHEN CAST(strftime('%s', first_seen) AS INTEGER) >= {current_start}
                         THEN 1 ELSE 0 END) AS new_current,
                SUM(CASE WHEN CAST(strftime('%s', first_seen) AS INTEGER) >= {first}
                          AND CAST(strftime('%s', first_seen) AS INTEGER) < {current_start}
                         THEN 1 ELSE 0 END) AS new_previous
            FROM issues
            WHERE project_id IN ({ids})
            GROUP BY 1
            "#,
        );

        sqlx::query_as(sqlx::AssertSqlSafe(&*sql))
            .fetch_all(pool)
            .await?
    };

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
    environment: Option<&str>,
) -> AppResult<Vec<EventTimeseriesPoint>> {
    let [fatal, error, warning] = NAMED_LEVELS;

    #[cfg(feature = "postgres")]
    {
        let time_filter = match period_hours {
            Some(hours) => format!("AND ingested_at >= NOW() - '{hours} hours'::interval"),
            None => String::new(),
        };

        // COALESCE(level, '') keeps the four severity buckets exhaustive: on a
        // NULL level `level NOT IN (...)` is NULL, not true, so the row would
        // count toward `total` but toward none of the segments.
        let sql = format!(
            r#"
            SELECT
                to_timestamp(
                    floor(extract(epoch FROM ingested_at) / {interval_secs}) * {interval_secs}
                ) AS bucket,
                COUNT(*)::bigint AS total,
                SUM(CASE WHEN COALESCE(level, '') = '{fatal}'   THEN 1 ELSE 0 END)::bigint AS fatal,
                SUM(CASE WHEN COALESCE(level, '') = '{error}'   THEN 1 ELSE 0 END)::bigint AS error,
                SUM(CASE WHEN COALESCE(level, '') = '{warning}' THEN 1 ELSE 0 END)::bigint AS warning,
                SUM(CASE WHEN COALESCE(level, '') NOT IN ('{fatal}', '{error}', '{warning}')
                         THEN 1 ELSE 0 END)::bigint AS info
            FROM events
            WHERE project_id = $1
              AND event_type = 'error'
              AND ($2 IS NULL OR environment = $2)
              {time_filter}
            GROUP BY 1
            ORDER BY 1 ASC
            "#,
        );

        let rows: Vec<(DateTime<Utc>, i64, i64, i64, i64, i64)> =
            sqlx::query_as(sqlx::AssertSqlSafe(&*sql))
                .bind(project_id)
                .bind(environment)
                .fetch_all(pool)
                .await?;

        Ok(rows
            .into_iter()
            .map(
                |(bucket, total, fatal, error, warning, info)| EventTimeseriesPoint {
                    bucket,
                    total,
                    fatal,
                    error,
                    warning,
                    info,
                },
            )
            .collect())
    }

    #[cfg(not(feature = "postgres"))]
    {
        let time_filter = match period_hours {
            Some(hours) => format!(
                "AND datetime(ingested_at) >= datetime('now', '-' || '{hours}' || ' hours')"
            ),
            None => String::new(),
        };

        let sql = format!(
            r#"
            SELECT
                strftime(
                    '%Y-%m-%dT%H:%M:%SZ',
                    (CAST(strftime('%s', ingested_at) AS INTEGER) / {interval_secs}) * {interval_secs},
                    'unixepoch'
                ) AS bucket,
                COUNT(*) AS total,
                SUM(CASE WHEN COALESCE(level, '') = '{fatal}'   THEN 1 ELSE 0 END) AS fatal,
                SUM(CASE WHEN COALESCE(level, '') = '{error}'   THEN 1 ELSE 0 END) AS error,
                SUM(CASE WHEN COALESCE(level, '') = '{warning}' THEN 1 ELSE 0 END) AS warning,
                SUM(CASE WHEN COALESCE(level, '') NOT IN ('{fatal}', '{error}', '{warning}')
                         THEN 1 ELSE 0 END) AS info
            FROM events
            WHERE project_id = ?1
              AND event_type = 'error'
              AND (?2 IS NULL OR environment = ?2)
              {time_filter}
            GROUP BY 1
            ORDER BY 1 ASC
            "#,
        );

        let rows: Vec<(String, i64, i64, i64, i64, i64)> =
            sqlx::query_as(sqlx::AssertSqlSafe(&*sql))
                .bind(project_id)
                .bind(environment)
                .fetch_all(pool)
                .await?;

        Ok(rows
            .into_iter()
            .map(
                |(bucket, total, fatal, error, warning, info)| EventTimeseriesPoint {
                    bucket: crate::models::session::parse_ts(&bucket).unwrap_or_else(Utc::now),
                    total,
                    fatal,
                    error,
                    warning,
                    info,
                },
            )
            .collect())
    }
}

/// Window bounds as `(previous_start, current_start)`, or `None` for all time.
type Bounds = Option<(DateTime<Utc>, DateTime<Utc>)>;

async fn count_events(
    pool: &DbPool,
    project_id: i32,
    bounds: Bounds,
    environment: Option<&str>,
) -> AppResult<MetricDelta> {
    let Some((previous_start, current_start)) = bounds else {
        #[cfg(feature = "postgres")]
        let (total,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*)::bigint FROM events WHERE project_id = $1 AND event_type = 'error' AND ($2 IS NULL OR environment = $2)",
        )
        .bind(project_id)
        .bind(environment)
        .fetch_one(pool)
        .await?;

        #[cfg(not(feature = "postgres"))]
        let (total,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM events WHERE project_id = ?1 AND event_type = 'error' AND (?2 IS NULL OR environment = ?2)",
        )
        .bind(project_id)
        .bind(environment)
        .fetch_one(pool)
        .await?;

        return Ok(MetricDelta::new(total, None));
    };

    #[cfg(feature = "postgres")]
    let (current, previous): (i64, i64) = sqlx::query_as(
        r#"
        SELECT
            COALESCE(SUM(CASE WHEN ingested_at >= $2 THEN 1 ELSE 0 END), 0)::bigint,
            COALESCE(SUM(CASE WHEN ingested_at <  $2 THEN 1 ELSE 0 END), 0)::bigint
        FROM events
        WHERE project_id = $1
          AND event_type = 'error'
          AND ingested_at >= $3
          AND ($4 IS NULL OR environment = $4)
        "#,
    )
    .bind(project_id)
    .bind(current_start)
    .bind(previous_start)
    .bind(environment)
    .fetch_one(pool)
    .await?;

    #[cfg(not(feature = "postgres"))]
    let (current, previous): (i64, i64) = sqlx::query_as(
        r#"
        SELECT
            COALESCE(SUM(CASE WHEN datetime(ingested_at) >= datetime(?2) THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN datetime(ingested_at) <  datetime(?2) THEN 1 ELSE 0 END), 0)
        FROM events
        WHERE project_id = ?1
          AND event_type = 'error'
          AND datetime(ingested_at) >= datetime(?3)
          AND (?4 IS NULL OR environment = ?4)
        "#,
    )
    .bind(project_id)
    .bind(current_start.naive_utc())
    .bind(previous_start.naive_utc())
    .bind(environment)
    .fetch_one(pool)
    .await?;

    Ok(MetricDelta::new(current, Some(previous)))
}

async fn count_new_issues(
    pool: &DbPool,
    project_id: i32,
    bounds: Bounds,
    environment: Option<&str>,
) -> AppResult<MetricDelta> {
    let Some((previous_start, current_start)) = bounds else {
        #[cfg(feature = "postgres")]
        let (total,): (i64,) =
            sqlx::query_as("SELECT COUNT(*)::bigint FROM issues WHERE project_id = $1 AND ($2 IS NULL OR EXISTS (SELECT 1 FROM events e WHERE e.issue_id = issues.id AND e.environment = $2))")
                .bind(project_id)
                .bind(environment)
                .fetch_one(pool)
                .await?;

        #[cfg(not(feature = "postgres"))]
        let (total,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM issues WHERE project_id = ?1 AND (?2 IS NULL OR EXISTS (SELECT 1 FROM events e WHERE e.issue_id = issues.id AND e.environment = ?2))")
            .bind(project_id)
            .bind(environment)
            .fetch_one(pool)
            .await?;

        return Ok(MetricDelta::new(total, None));
    };

    #[cfg(feature = "postgres")]
    let (current, previous): (i64, i64) = sqlx::query_as(
        r#"
        WITH firsts AS (
            SELECT CASE WHEN $4 IS NULL THEN i.first_seen ELSE (
                SELECT MIN(e.timestamp) FROM events e
                WHERE e.issue_id = i.id AND e.environment = $4
            ) END AS first_seen
            FROM issues i
            WHERE i.project_id = $1 AND ($4 IS NOT NULL OR i.first_seen >= $3)
        )
        SELECT
            COALESCE(SUM(CASE WHEN first_seen >= $2 THEN 1 ELSE 0 END), 0)::bigint,
            COALESCE(SUM(CASE WHEN first_seen <  $2 THEN 1 ELSE 0 END), 0)::bigint
        FROM firsts WHERE first_seen >= $3
        "#,
    )
    .bind(project_id)
    .bind(current_start)
    .bind(previous_start)
    .bind(environment)
    .fetch_one(pool)
    .await?;

    #[cfg(not(feature = "postgres"))]
    let (current, previous): (i64, i64) = sqlx::query_as(
        r#"
        WITH firsts AS (
            SELECT CASE WHEN ?4 IS NULL THEN i.first_seen ELSE (
                SELECT MIN(e.timestamp) FROM events e
                WHERE e.issue_id = i.id AND e.environment = ?4
            ) END AS first_seen
            FROM issues i
            WHERE i.project_id = ?1 AND (?4 IS NOT NULL OR datetime(i.first_seen) >= datetime(?3))
        )
        SELECT
            COALESCE(SUM(CASE WHEN datetime(first_seen) >= datetime(?2) THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN datetime(first_seen) <  datetime(?2) THEN 1 ELSE 0 END), 0)
        FROM firsts WHERE datetime(first_seen) >= datetime(?3)
        "#,
    )
    .bind(project_id)
    .bind(current_start.naive_utc())
    .bind(previous_start.naive_utc())
    .bind(environment)
    .fetch_one(pool)
    .await?;

    Ok(MetricDelta::new(current, Some(previous)))
}

/// Issues currently unresolved. A backlog size, not a windowed rate, so it has
/// no previous-period counterpart.
async fn count_open_issues(
    pool: &DbPool,
    project_id: i32,
    environment: Option<&str>,
) -> AppResult<i64> {
    #[cfg(feature = "postgres")]
    let (total,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*)::bigint FROM issues WHERE project_id = $1 AND status = 'unresolved' AND ($2 IS NULL OR EXISTS (SELECT 1 FROM events e WHERE e.issue_id = issues.id AND e.environment = $2))",
    )
    .bind(project_id)
    .bind(environment)
    .fetch_one(pool)
    .await?;

    #[cfg(not(feature = "postgres"))]
    let (total,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM issues WHERE project_id = ?1 AND status = 'unresolved' AND (?2 IS NULL OR EXISTS (SELECT 1 FROM events e WHERE e.issue_id = issues.id AND e.environment = ?2))",
    )
    .bind(project_id)
    .bind(environment)
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
