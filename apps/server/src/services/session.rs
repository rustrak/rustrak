#[cfg(feature = "postgres")]
use chrono::DateTime;
use chrono::Utc;

use crate::db::DbPool;
use crate::error::AppResult;
use crate::models::session::{ReleaseHealthRow, SessionSummary, SessionTimeseriesPoint};

#[cfg(feature = "postgres")]
type PgHealthRow = (String, String, i64, i64, i64, i64, Option<f64>, Option<f64>);

/// `AND bucket >= ...` time filter for a `session_counts` (timestamptz) query.
/// Empty string when `period_hours` is `None` (no time filter).
#[cfg(feature = "postgres")]
fn pg_bucket_time_filter(period_hours: Option<i64>) -> String {
    match period_hours {
        Some(hours) => format!("AND bucket >= NOW() - '{hours} hours'::interval"),
        None => String::new(),
    }
}

/// `AND day >= ...` time filter for a `session_users` (date) query.
#[cfg(feature = "postgres")]
fn pg_day_time_filter(period_hours: Option<i64>) -> String {
    match period_hours {
        Some(hours) => format!("AND day >= (NOW() - '{hours} hours'::interval)::date"),
        None => String::new(),
    }
}

/// `AND bucket >= ...` time filter for a `session_counts` (timestamptz) query.
#[cfg(not(feature = "postgres"))]
fn sqlite_bucket_time_filter(period_hours: Option<i64>) -> String {
    match period_hours {
        Some(hours) => format!("AND bucket >= datetime('now', '-' || '{hours}' || ' hours')"),
        None => String::new(),
    }
}

/// `AND day >= ...` time filter for a `session_users` (date) query.
#[cfg(not(feature = "postgres"))]
fn sqlite_day_time_filter(period_hours: Option<i64>) -> String {
    match period_hours {
        Some(hours) => format!("AND day >= date('now', '-' || '{hours}' || ' hours')"),
        None => String::new(),
    }
}

pub struct SessionService;

impl SessionService {
    /// Query one page of per-release health stats for a project, plus the total
    /// number of (release, environment) groups the filters match.
    /// If `period_hours` is `None`, all data is returned (no time filter).
    pub async fn release_health(
        pool: &DbPool,
        project_id: i32,
        period_hours: Option<i64>,
        page: i64,
        per_page: i64,
        environment: Option<&str>,
    ) -> AppResult<(Vec<ReleaseHealthRow>, i64)> {
        let (rows, total) = query_release_health(
            pool,
            project_id,
            period_hours,
            None,
            page,
            per_page,
            environment,
        )
        .await?;
        Ok((rows, total))
    }

    /// Same as [`Self::release_health`], scoped server-side to a single release
    /// (all environments) instead of returning every release in the project.
    /// Powers the release detail page, which only needs one release's rows.
    pub async fn release_health_for_release(
        pool: &DbPool,
        project_id: i32,
        release: &str,
        period_hours: Option<i64>,
        page: i64,
        per_page: i64,
        environment: Option<&str>,
    ) -> AppResult<(Vec<ReleaseHealthRow>, i64)> {
        let (rows, total) = query_release_health(
            pool,
            project_id,
            period_hours,
            Some(release),
            page,
            per_page,
            environment,
        )
        .await?;
        Ok((rows, total))
    }

    /// Query project-wide session health, aggregated across all releases and environments.
    /// If `period_hours` is `None`, all data is returned (no time filter).
    pub async fn project_summary(
        pool: &DbPool,
        project_id: i32,
        period_hours: Option<i64>,
        environment: Option<&str>,
    ) -> AppResult<SessionSummary> {
        let summary = query_project_summary(pool, project_id, period_hours, environment).await?;
        Ok(summary)
    }

    /// Query a time-bucketed session trend for a project, aggregated across all
    /// releases and environments. `interval_hours` controls bucket width (e.g. 1
    /// for hourly points, 24 for daily). If `period_hours` is `None`, all data
    /// is returned (no time filter). Ordered chronologically (oldest first).
    pub async fn session_timeseries(
        pool: &DbPool,
        project_id: i32,
        period_hours: Option<i64>,
        interval_hours: i64,
        environment: Option<&str>,
    ) -> AppResult<Vec<SessionTimeseriesPoint>> {
        let points =
            query_session_timeseries(pool, project_id, period_hours, interval_hours, environment)
                .await?;
        Ok(points)
    }
}

/// One page of release health rows plus the total group count. `page` is
/// 1-indexed; both it and `per_page` are clamped by the caller-facing service
/// methods' route layer, and defensively again here.
async fn query_release_health(
    pool: &DbPool,
    project_id: i32,
    period_hours: Option<i64>,
    release: Option<&str>,
    page: i64,
    per_page: i64,
    environment: Option<&str>,
) -> Result<(Vec<ReleaseHealthRow>, i64), sqlx::Error> {
    let per_page = per_page.max(1);
    // Saturating, not plain arithmetic: `page` arrives straight from a query
    // string, so a huge value would overflow the multiplication — a panic
    // under overflow-checks, a negative offset (which Postgres rejects)
    // without them. Saturating keeps it an in-range offset past the last row,
    // which is just an empty page.
    let offset = page.max(1).saturating_sub(1).saturating_mul(per_page);

    #[cfg(feature = "postgres")]
    {
        let time_filter_sc = pg_bucket_time_filter(period_hours);
        let time_filter_su = pg_day_time_filter(period_hours);
        let release_filter = if release.is_some() {
            "AND release = $2"
        } else {
            ""
        };
        let environment_param = if release.is_some() { "$3" } else { "$2" };
        let environment_filter = format!(
            "AND ({} IS NULL OR environment = {})",
            environment_param, environment_param
        );
        // $2 is taken by the release filter when present, so LIMIT/OFFSET shift up.
        let (limit_param, offset_param) = if release.is_some() {
            ("$4", "$5")
        } else {
            ("$3", "$4")
        };

        let count_sql = format!(
            r#"
                SELECT COUNT(*) FROM (
                    SELECT 1
                    FROM session_counts
                    WHERE project_id = $1
                      {time_filter_sc}
                      {release_filter}
                      {environment_filter}
                    GROUP BY release, environment
                ) groups
                "#,
        );

        let mut count_query = sqlx::query_as(sqlx::AssertSqlSafe(&*count_sql)).bind(project_id);
        if let Some(r) = release {
            count_query = count_query.bind(r);
        }
        count_query = count_query.bind(environment);
        let (total,): (i64,) = count_query.fetch_one(pool).await?;

        // session_counts and session_users are aggregated in separate subqueries
        // (each producing at most one row per release+environment) and then
        // joined 1:1. A direct LEFT JOIN on (project_id, release, environment)
        // alone is a many-to-many join — session_counts has one row per bucket
        // and session_users one row per (day, did), so every counts row would
        // be duplicated once per matching users row, inflating SUM(total) etc.
        // by the distinct-user count.
        let sql = format!(
            r#"
                SELECT
                    counts.release,
                    counts.environment,
                    counts.total,
                    counts.errored,
                    counts.crashed,
                    counts.abnormal,
                    counts.crash_free_sessions_rate,
                    users.crash_free_users_rate
                FROM (
                    SELECT
                        release,
                        environment,
                        SUM(total)::bigint    AS total,
                        SUM(errored)::bigint  AS errored,
                        SUM(crashed)::bigint  AS crashed,
                        SUM(abnormal)::bigint AS abnormal,
                        CASE WHEN SUM(total) > 0
                             THEN 1.0 - SUM(crashed)::float8 / NULLIF(SUM(total), 0)
                             ELSE NULL END AS crash_free_sessions_rate
                    FROM session_counts
                    WHERE project_id = $1
                      {time_filter_sc}
                      {release_filter}
                      {environment_filter}
                    GROUP BY release, environment
                ) counts
                LEFT JOIN (
                    SELECT
                        release,
                        environment,
                        CASE WHEN COUNT(DISTINCT did) > 0
                             THEN 1.0 - COUNT(DISTINCT CASE WHEN crashed THEN did END)::float8
                                  / NULLIF(COUNT(DISTINCT did), 0)
                             ELSE NULL END AS crash_free_users_rate
                    FROM session_users
                    WHERE project_id = $1
                      {time_filter_su}
                      {release_filter}
                      {environment_filter}
                    GROUP BY release, environment
                ) users
                ON users.release = counts.release AND users.environment = counts.environment
                ORDER BY counts.total DESC, counts.release ASC, counts.environment ASC
                LIMIT {limit_param} OFFSET {offset_param}
                "#,
        );

        let mut query = sqlx::query_as(sqlx::AssertSqlSafe(&*sql)).bind(project_id);
        if let Some(r) = release {
            query = query.bind(r);
        }
        query = query.bind(environment);
        let rows: Vec<PgHealthRow> = query.bind(per_page).bind(offset).fetch_all(pool).await?;

        let items = rows
            .into_iter()
            .map(
                |(release, environment, total, errored, crashed, abnormal, cfsr, cfur)| {
                    let healthy = total - errored - crashed - abnormal;
                    ReleaseHealthRow {
                        release,
                        environment,
                        total,
                        errored,
                        crashed,
                        abnormal,
                        healthy,
                        crash_free_sessions_rate: cfsr,
                        crash_free_users_rate: cfur,
                    }
                },
            )
            .collect();

        Ok((items, total))
    }

    #[cfg(not(feature = "postgres"))]
    {
        let time_filter_sc = sqlite_bucket_time_filter(period_hours);
        let time_filter_su = sqlite_day_time_filter(period_hours);
        let release_filter = if release.is_some() {
            "AND release = ?2"
        } else {
            ""
        };
        let environment_param = if release.is_some() { "?3" } else { "?2" };
        let environment_filter = format!(
            "AND ({} IS NULL OR environment = {})",
            environment_param, environment_param
        );
        // ?2 is taken by the release filter when present, so LIMIT/OFFSET shift up.
        let (limit_param, offset_param) = if release.is_some() {
            ("?4", "?5")
        } else {
            ("?3", "?4")
        };

        let total_sql = format!(
            r#"
            SELECT COUNT(*) FROM (
                SELECT 1
                FROM session_counts
                WHERE project_id = ?1
                  {time_filter_sc}
                  {release_filter}
                  {environment_filter}
                GROUP BY release, environment
            ) groups
            "#,
        );

        let mut total_query = sqlx::query_as(sqlx::AssertSqlSafe(&*total_sql)).bind(project_id);
        if let Some(r) = release {
            total_query = total_query.bind(r);
        }
        total_query = total_query.bind(environment);
        let (total,): (i64,) = total_query.fetch_one(pool).await?;

        let sql = format!(
            r#"
            SELECT
                release,
                environment,
                SUM(total)    AS total,
                SUM(errored)  AS errored,
                SUM(crashed)  AS crashed,
                SUM(abnormal) AS abnormal
            FROM session_counts
            WHERE project_id = ?1
              {time_filter_sc}
              {release_filter}
              {environment_filter}
            GROUP BY release, environment
            ORDER BY SUM(total) DESC, release ASC, environment ASC
            LIMIT {limit_param} OFFSET {offset_param}
            "#,
        );

        let mut count_query = sqlx::query_as(sqlx::AssertSqlSafe(&*sql)).bind(project_id);
        if let Some(r) = release {
            count_query = count_query.bind(r);
        }
        count_query = count_query.bind(environment);
        let rows: Vec<(String, String, i64, i64, i64, i64)> = count_query
            .bind(per_page)
            .bind(offset)
            .fetch_all(pool)
            .await?;

        let mut result = Vec::with_capacity(rows.len());
        for (release, environment, total, errored, crashed, abnormal) in rows {
            let user_sql = format!(
                r#"
                SELECT
                    COUNT(DISTINCT did),
                    COUNT(DISTINCT CASE WHEN crashed = 1 THEN did END)
                FROM session_users
                WHERE project_id = ?1
                  AND release = ?2
                  AND environment = ?3
                  {}
                "#,
                time_filter_su
            );

            let (total_users, crashed_users): (i64, i64) =
                sqlx::query_as(sqlx::AssertSqlSafe(&*user_sql))
                    .bind(project_id)
                    .bind(&release)
                    .bind(&environment)
                    .fetch_one(pool)
                    .await?;

            let crash_free_sessions_rate = if total > 0 {
                Some(1.0 - crashed as f64 / total as f64)
            } else {
                None
            };
            let crash_free_users_rate = if total_users > 0 {
                Some(1.0 - crashed_users as f64 / total_users as f64)
            } else {
                None
            };

            let healthy = total - errored - crashed - abnormal;
            result.push(ReleaseHealthRow {
                release,
                environment,
                total,
                errored,
                crashed,
                abnormal,
                healthy,
                crash_free_sessions_rate,
                crash_free_users_rate,
            });
        }
        Ok((result, total))
    }
}

async fn query_project_summary(
    pool: &DbPool,
    project_id: i32,
    period_hours: Option<i64>,
    environment: Option<&str>,
) -> Result<SessionSummary, sqlx::Error> {
    #[cfg(feature = "postgres")]
    {
        let time_filter_sc = pg_bucket_time_filter(period_hours);
        let time_filter_su = pg_day_time_filter(period_hours);

        // Two independent scalar subqueries (each always returns exactly one
        // row) combined with CROSS JOIN, instead of a single LEFT JOIN between
        // session_counts and session_users on (project_id, release,
        // environment) alone — that join has no day/did constraint, so it's
        // many-to-many and inflates SUM(total) etc. by the distinct-user count.
        let sql = format!(
            r#"
                SELECT
                    counts.total,
                    counts.errored,
                    counts.crashed,
                    counts.abnormal,
                    counts.crash_free_sessions_rate,
                    users.crash_free_users_rate,
                    counts.active_releases
                FROM (
                    SELECT
                        COALESCE(SUM(total), 0)::bigint    AS total,
                        COALESCE(SUM(errored), 0)::bigint  AS errored,
                        COALESCE(SUM(crashed), 0)::bigint  AS crashed,
                        COALESCE(SUM(abnormal), 0)::bigint AS abnormal,
                        CASE WHEN SUM(total) > 0
                             THEN 1.0 - SUM(crashed)::float8 / NULLIF(SUM(total), 0)
                             ELSE NULL END AS crash_free_sessions_rate,
                        COUNT(DISTINCT CASE WHEN total > 0 THEN release END)::bigint AS active_releases
                    FROM session_counts
                    WHERE project_id = $1
                      AND ($2 IS NULL OR environment = $2)
                      {}
                ) counts
                CROSS JOIN (
                    SELECT
                        CASE WHEN COUNT(DISTINCT did) > 0
                             THEN 1.0 - COUNT(DISTINCT CASE WHEN crashed THEN did END)::float8
                                  / NULLIF(COUNT(DISTINCT did), 0)
                             ELSE NULL END AS crash_free_users_rate
                    FROM session_users
                    WHERE project_id = $1
                      AND ($2 IS NULL OR environment = $2)
                      {}
                ) users
                "#,
            time_filter_sc, time_filter_su
        );

        let (
            total,
            errored,
            crashed,
            abnormal,
            crash_free_sessions_rate,
            crash_free_users_rate,
            active_releases,
        ): (i64, i64, i64, i64, Option<f64>, Option<f64>, i64) =
            sqlx::query_as(sqlx::AssertSqlSafe(&*sql))
                .bind(project_id)
                .bind(environment)
                .fetch_one(pool)
                .await?;

        Ok(SessionSummary {
            total,
            errored,
            crashed,
            abnormal,
            crash_free_sessions_rate,
            crash_free_users_rate,
            active_releases,
        })
    }

    #[cfg(not(feature = "postgres"))]
    {
        let time_filter_sc = sqlite_bucket_time_filter(period_hours);

        let sql = format!(
            r#"
            SELECT
                COALESCE(SUM(total), 0)    AS total,
                COALESCE(SUM(errored), 0)  AS errored,
                COALESCE(SUM(crashed), 0)  AS crashed,
                COALESCE(SUM(abnormal), 0) AS abnormal,
                COUNT(DISTINCT CASE WHEN total > 0 THEN release END) AS active_releases
            FROM session_counts
            WHERE project_id = ?1
              AND (?2 IS NULL OR environment = ?2)
              {}
            "#,
            time_filter_sc
        );

        let (total, errored, crashed, abnormal, active_releases): (i64, i64, i64, i64, i64) =
            sqlx::query_as(sqlx::AssertSqlSafe(&*sql))
                .bind(project_id)
                .bind(environment)
                .fetch_one(pool)
                .await?;

        let time_filter_su = sqlite_day_time_filter(period_hours);

        let user_sql = format!(
            r#"
            SELECT
                COUNT(DISTINCT did),
                COUNT(DISTINCT CASE WHEN crashed = 1 THEN did END)
            FROM session_users
            WHERE project_id = ?1
              AND (?2 IS NULL OR environment = ?2)
              {}
            "#,
            time_filter_su
        );

        let (total_users, crashed_users): (i64, i64) =
            sqlx::query_as(sqlx::AssertSqlSafe(&*user_sql))
                .bind(project_id)
                .bind(environment)
                .fetch_one(pool)
                .await?;

        let crash_free_sessions_rate = if total > 0 {
            Some(1.0 - crashed as f64 / total as f64)
        } else {
            None
        };
        let crash_free_users_rate = if total_users > 0 {
            Some(1.0 - crashed_users as f64 / total_users as f64)
        } else {
            None
        };

        Ok(SessionSummary {
            total,
            errored,
            crashed,
            abnormal,
            crash_free_sessions_rate,
            crash_free_users_rate,
            active_releases,
        })
    }
}

async fn query_session_timeseries(
    pool: &DbPool,
    project_id: i32,
    period_hours: Option<i64>,
    interval_hours: i64,
    environment: Option<&str>,
) -> Result<Vec<SessionTimeseriesPoint>, sqlx::Error> {
    let interval_seconds = interval_hours.max(1) * 3600;

    #[cfg(feature = "postgres")]
    {
        let time_filter = pg_bucket_time_filter(period_hours);

        let sql = format!(
            r#"
                SELECT
                    to_timestamp(
                        floor(extract(epoch FROM bucket) / {interval_seconds}) * {interval_seconds}
                    ) AS bucket,
                    SUM(total)::bigint   AS total,
                    SUM(crashed)::bigint AS crashed,
                    CASE WHEN SUM(total) > 0
                         THEN 1.0 - SUM(crashed)::float8 / NULLIF(SUM(total), 0)
                         ELSE NULL END AS crash_free_sessions_rate
                FROM session_counts
                WHERE project_id = $1
                  AND ($2 IS NULL OR environment = $2)
                  {time_filter}
                GROUP BY 1
                ORDER BY 1 ASC
                "#,
        );

        let rows: Vec<(DateTime<Utc>, i64, i64, Option<f64>)> =
            sqlx::query_as(sqlx::AssertSqlSafe(&*sql))
                .bind(project_id)
                .bind(environment)
                .fetch_all(pool)
                .await?;

        Ok(rows
            .into_iter()
            .map(
                |(bucket, total, crashed, crash_free_sessions_rate)| SessionTimeseriesPoint {
                    bucket,
                    total,
                    crashed,
                    crash_free_sessions_rate,
                },
            )
            .collect())
    }

    #[cfg(not(feature = "postgres"))]
    {
        let time_filter = sqlite_bucket_time_filter(period_hours);

        let sql = format!(
            r#"
            SELECT
                strftime('%Y-%m-%dT%H:%M:%SZ', (CAST(strftime('%s', bucket) AS INTEGER) / {interval_seconds}) * {interval_seconds}, 'unixepoch') AS bucket,
                SUM(total)   AS total,
                SUM(crashed) AS crashed
            FROM session_counts
            WHERE project_id = ?1
              AND (?2 IS NULL OR environment = ?2)
              {time_filter}
            GROUP BY 1
            ORDER BY 1 ASC
            "#,
        );

        let rows: Vec<(String, i64, i64)> = sqlx::query_as(sqlx::AssertSqlSafe(&*sql))
            .bind(project_id)
            .bind(environment)
            .fetch_all(pool)
            .await?;

        Ok(rows
            .into_iter()
            .map(|(bucket, total, crashed)| {
                let bucket = crate::models::session::parse_ts(&bucket).unwrap_or_else(Utc::now);
                let crash_free_sessions_rate = if total > 0 {
                    Some(1.0 - crashed as f64 / total as f64)
                } else {
                    None
                };
                SessionTimeseriesPoint {
                    bucket,
                    total,
                    crashed,
                    crash_free_sessions_rate,
                }
            })
            .collect())
    }
}
