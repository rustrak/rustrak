use crate::db::dialect::{bucket, bucket_time, since_day, since_stored};
use crate::db::DbPool;
use crate::error::AppResult;
use crate::models::session::{
    crash_free_rate, ReleaseHealthRow, SessionSummary, SessionTimeseriesPoint,
};
use crate::services::scope::Scope;

/// `(release, environment, total, errored, crashed, abnormal, users, crashed_users)`.
type HealthRow = (String, String, i64, i64, i64, i64, i64, i64);

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
    ) -> AppResult<(Vec<ReleaseHealthRow>, i64)> {
        query_release_health(pool, project_id, period_hours, None, page, per_page).await
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
    ) -> AppResult<(Vec<ReleaseHealthRow>, i64)> {
        query_release_health(
            pool,
            project_id,
            period_hours,
            Some(release),
            page,
            per_page,
        )
        .await
    }

    /// Query project-wide session health, aggregated across all releases and environments.
    /// If `period_hours` is `None`, all data is returned (no time filter).
    pub async fn project_summary(
        pool: &DbPool,
        project_id: i32,
        period_hours: Option<i64>,
        scope: Scope<'_>,
    ) -> AppResult<SessionSummary> {
        // Two independent aggregates (each always returns exactly one row)
        // combined with CROSS JOIN, instead of a single LEFT JOIN between
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
                counts.active_releases,
                users.users,
                users.crashed_users
            FROM (
                SELECT
                    CAST(COALESCE(SUM(total), 0) AS BIGINT)    AS total,
                    CAST(COALESCE(SUM(errored), 0) AS BIGINT)  AS errored,
                    CAST(COALESCE(SUM(crashed), 0) AS BIGINT)  AS crashed,
                    CAST(COALESCE(SUM(abnormal), 0) AS BIGINT) AS abnormal,
                    COUNT(DISTINCT CASE WHEN total > 0 THEN release END) AS active_releases
                FROM session_counts
                WHERE project_id = $1
                  AND ($2 IS NULL OR environment = $2)
                  AND ($3 IS NULL OR release = $3)
                  AND {counts_window}
            ) counts
            CROSS JOIN (
                SELECT
                    COUNT(DISTINCT did) AS users,
                    COUNT(DISTINCT CASE WHEN crashed THEN did END) AS crashed_users
                FROM session_users
                WHERE project_id = $1
                  AND ($2 IS NULL OR environment = $2)
                  AND ($3 IS NULL OR release = $3)
                  AND {users_window}
            ) users
            "#,
            counts_window = since_stored("bucket", period_hours),
            users_window = since_day("day", period_hours),
        );

        let (total, errored, crashed, abnormal, active_releases, users, crashed_users): (
            i64,
            i64,
            i64,
            i64,
            i64,
            i64,
            i64,
        ) = sqlx::query_as(sqlx::AssertSqlSafe(&*sql))
            .bind(project_id)
            .bind(scope.environment)
            .bind(scope.release)
            .fetch_one(pool)
            .await?;

        Ok(SessionSummary {
            total,
            errored,
            crashed,
            abnormal,
            crash_free_sessions_rate: crash_free_rate(crashed, total),
            crash_free_users_rate: crash_free_rate(crashed_users, users),
            active_releases,
        })
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
        scope: Scope<'_>,
    ) -> AppResult<Vec<SessionTimeseriesPoint>> {
        let sql = format!(
            r#"
            SELECT
                {bucket} AS bucket,
                CAST(SUM(total) AS BIGINT)   AS total,
                CAST(SUM(crashed) AS BIGINT) AS crashed
            FROM session_counts
            WHERE project_id = $1
              AND ($2 IS NULL OR environment = $2)
              AND ($3 IS NULL OR release = $3)
              AND {window}
            GROUP BY 1
            ORDER BY 1 ASC
            "#,
            bucket = bucket("bucket", interval_hours.max(1) * 3600),
            window = since_stored("bucket", period_hours),
        );

        let rows: Vec<(i64, i64, i64)> = sqlx::query_as(sqlx::AssertSqlSafe(&*sql))
            .bind(project_id)
            .bind(scope.environment)
            .bind(scope.release)
            .fetch_all(pool)
            .await?;

        rows.into_iter()
            .map(|(bucket, total, crashed)| {
                Ok(SessionTimeseriesPoint {
                    bucket: bucket_time(bucket)?,
                    total,
                    crashed,
                    crash_free_sessions_rate: crash_free_rate(crashed, total),
                })
            })
            .collect()
    }
}

/// One page of release health rows plus the total group count. `page` is
/// 1-indexed.
async fn query_release_health(
    pool: &DbPool,
    project_id: i32,
    period_hours: Option<i64>,
    release: Option<&str>,
    page: i64,
    per_page: i64,
) -> AppResult<(Vec<ReleaseHealthRow>, i64)> {
    let per_page = per_page.max(1);
    // Saturating, not plain arithmetic: `page` arrives straight from a query
    // string, so a huge value would overflow the multiplication — a panic
    // under overflow-checks, a negative offset (which Postgres rejects)
    // without them. Saturating keeps it an in-range offset past the last row,
    // which is just an empty page.
    let offset = crate::pagination::offset(page, per_page);
    let counts_window = since_stored("bucket", period_hours);
    let users_window = since_day("day", period_hours);
    // $2 is the release filter; NULL lists every release.
    let count_sql = format!(
        r#"
        SELECT COUNT(*) FROM (
            SELECT 1
            FROM session_counts
            WHERE project_id = $1
              AND ($2 IS NULL OR release = $2)
              AND {counts_window}
            GROUP BY release, environment
        ) groups
        "#,
    );
    let (total,): (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(&*count_sql))
        .bind(project_id)
        .bind(release)
        .fetch_one(pool)
        .await?;

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
            COALESCE(users.users, 0),
            COALESCE(users.crashed_users, 0)
        FROM (
            SELECT
                release,
                environment,
                CAST(SUM(total) AS BIGINT)    AS total,
                CAST(SUM(errored) AS BIGINT)  AS errored,
                CAST(SUM(crashed) AS BIGINT)  AS crashed,
                CAST(SUM(abnormal) AS BIGINT) AS abnormal
            FROM session_counts
            WHERE project_id = $1
              AND ($2 IS NULL OR release = $2)
              AND {counts_window}
            GROUP BY release, environment
        ) counts
        LEFT JOIN (
            SELECT
                release,
                environment,
                COUNT(DISTINCT did) AS users,
                COUNT(DISTINCT CASE WHEN crashed THEN did END) AS crashed_users
            FROM session_users
            WHERE project_id = $1
              AND ($2 IS NULL OR release = $2)
              AND {users_window}
            GROUP BY release, environment
        ) users
        ON users.release = counts.release AND users.environment = counts.environment
        ORDER BY counts.total DESC, counts.release ASC, counts.environment ASC
        LIMIT $3 OFFSET $4
        "#,
    );
    let rows: Vec<HealthRow> = sqlx::query_as(sqlx::AssertSqlSafe(&*sql))
        .bind(project_id)
        .bind(release)
        .bind(per_page)
        .bind(offset)
        .fetch_all(pool)
        .await?;

    let items = rows
        .into_iter()
        .map(
            |(release, environment, total, errored, crashed, abnormal, users, crashed_users)| {
                ReleaseHealthRow {
                    release,
                    environment,
                    total,
                    errored,
                    crashed,
                    abnormal,
                    // Sentry clamps healthy as max(0, init - errored_all), so an
                    // SDK that resends a terminal update cannot drive it
                    // negative (#388).
                    healthy: (total - errored - crashed - abnormal).max(0),
                    crash_free_sessions_rate: crash_free_rate(crashed, total),
                    crash_free_users_rate: crash_free_rate(crashed_users, users),
                }
            },
        )
        .collect();

    Ok((items, total))
}
