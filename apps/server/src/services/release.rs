use std::collections::{BTreeSet, HashMap};

use chrono::{DateTime, Utc};

use crate::db::dialect::{since, since_day};
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::session::crash_free_rate;
use crate::models::{is_valid_version, CreateRelease, Release, ReleaseSummary, UpdateRelease};

const RELEASE_COLUMNS: &str = "id, project_id, version, ref, url, date_created, date_released";

pub struct ReleaseService;

/// What narrows a release summary: the numbers, never the list of releases.
///
/// Which releases an environment lists is the list endpoint's to decide, as
/// Sentry's does; here a release keeps its row, with nothing to count.
#[derive(Debug, Default)]
pub struct SummaryFilter {
    pub environment: Option<String>,
    pub period_hours: Option<i64>,
    /// Keeps the versions containing it, ignoring case.
    pub query: Option<String>,
}

type RecordRow = (
    String,
    DateTime<Utc>,
    Option<DateTime<Utc>>,
    Option<String>,
    Option<String>,
);
type EventRow = (String, DateTime<Utc>, DateTime<Utc>, i64);
type SessionRow = (String, DateTime<Utc>, i64, i64);

impl ReleaseService {
    /// Creates a release for `(project_id, version)`.
    ///
    /// Idempotent by design: if the row already exists, the existing release
    /// is returned with `created = false` instead of erroring — matches
    /// sentry-cli's expectation that repeating `releases new` never fails.
    pub async fn create(
        pool: &DbPool,
        project_id: i32,
        input: CreateRelease,
    ) -> AppResult<(Release, bool)> {
        let version = input.version.trim();
        if !is_valid_version(version) {
            return Err(AppError::Validation(format!(
                "Invalid release version: {:?}",
                input.version
            )));
        }

        // date_created is bound explicitly (not left to the column's SQL-level
        // DEFAULT) so every row's timestamp goes through the same sqlx/chrono
        // encoding path. On SQLite, `date_created` is a TEXT column — mixing a
        // `datetime('now')`-generated string ("2026-07-18 16:30:33") with a
        // sqlx-bound `DateTime<Utc>` ("2026-07-18T16:30:33+00:00") would make
        // `date_created < $2` compare two different textual formats
        // lexicographically instead of chronologically (`' '` < `'T'` in
        // ASCII, so a DEFAULT-inserted row can spuriously compare as "older"
        // than it really is). Binding consistently avoids the mismatch.
        let now = Utc::now();
        let query = format!(
            r#"
            INSERT INTO releases (project_id, version, ref, url, date_created)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING {RELEASE_COLUMNS}
            "#
        );

        let result = sqlx::query_as::<_, Release>(sqlx::AssertSqlSafe(&*query))
            .bind(project_id)
            .bind(version)
            .bind(&input.reference)
            .bind(&input.url)
            .bind(now)
            .fetch_one(pool)
            .await;

        match result {
            Ok(release) => Ok((release, true)),
            Err(sqlx::Error::Database(ref db_err)) if db_err.is_unique_violation() => {
                let existing = Self::get_by_version(pool, project_id, version).await?;
                Ok((existing, false))
            }
            Err(e) => Err(AppError::Database(e)),
        }
    }

    /// Fetches a release by `(project_id, version)`.
    pub async fn get_by_version(
        pool: &DbPool,
        project_id: i32,
        version: &str,
    ) -> AppResult<Release> {
        let version = version.trim();
        let query = format!(
            "SELECT {RELEASE_COLUMNS} FROM releases WHERE project_id = $1 AND version = $2"
        );

        sqlx::query_as::<_, Release>(sqlx::AssertSqlSafe(&*query))
            .bind(project_id)
            .bind(version)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Release {} not found", version)))
    }

    /// Generic partial update (`ref`, `url`, `dateReleased`). Setting
    /// `date_released` IS "finalize" — there is no separate status flag.
    /// Fields left `None` in `input` keep their stored value (`COALESCE`).
    pub async fn finalize(
        pool: &DbPool,
        project_id: i32,
        version: &str,
        input: UpdateRelease,
    ) -> AppResult<Release> {
        let version = version.trim();
        let query = format!(
            r#"
            UPDATE releases
            SET ref = COALESCE($3, ref),
                url = COALESCE($4, url),
                date_released = COALESCE($5, date_released)
            WHERE project_id = $1 AND version = $2
            RETURNING {RELEASE_COLUMNS}
            "#
        );

        sqlx::query_as::<_, Release>(sqlx::AssertSqlSafe(&*query))
            .bind(project_id)
            .bind(version)
            .bind(&input.reference)
            .bind(&input.url)
            .bind(input.date_released)
            .fetch_one(pool)
            .await
            .map_err(|error| match error {
                sqlx::Error::RowNotFound => {
                    AppError::NotFound(format!("Release {} not found", version))
                }
                other => other.into(),
            })
    }

    /// Every release of a project, newest first, each with what it brought;
    /// with a query, only the versions containing it.
    ///
    /// A release reaches this instance three ways -- a CI-created record, the
    /// events tagged with it, the sessions reporting it -- and is listed if it
    /// arrived any of them.
    // ponytail: aggregates every release on each call and pages in memory;
    // push the paging into SQL if a project ever has thousands of releases.
    pub async fn summaries(
        pool: &DbPool,
        project_id: i32,
        filter: &SummaryFilter,
    ) -> AppResult<Vec<ReleaseSummary>> {
        let env = filter.environment.as_deref();
        let hours = filter.period_hours;

        let records: Vec<RecordRow> = sqlx::query_as(
            "SELECT version, date_created, date_released, ref, url FROM releases WHERE project_id = $1",
        )
        .bind(project_id)
        .fetch_all(pool)
        .await?;

        // The environment, like the window, is in the counts and not in
        // `WHERE`: a release with nothing in it keeps its row and its dates.
        let events_sql = format!(
            "SELECT release, MIN(ingested_at), MAX(ingested_at), \
                    COUNT(CASE WHEN {} AND ($2 IS NULL OR environment = $2) THEN 1 END) \
             FROM events \
             WHERE project_id = $1 AND release <> '' \
             GROUP BY release",
            since("ingested_at", hours)
        );
        let events: Vec<EventRow> = sqlx::query_as(sqlx::AssertSqlSafe(&*events_sql))
            .bind(project_id)
            .bind(env)
            .fetch_all(pool)
            .await?;

        // Sentry's `newGroups`: per environment when one is asked, but never
        // windowed -- it is what the release introduced, not when. In an
        // environment, the release has to have brought it there: its first
        // release in it, not the issue reaching it later with another one.
        // Read from `issue_environments`, so retention never shrinks it.
        let new_issues: Vec<(String, i64)> = sqlx::query_as(
            "SELECT first_release, COUNT(*) FROM issues \
             WHERE project_id = $1 AND first_release IS NOT NULL AND first_release <> '' \
               AND issue_category = 'error' \
               AND ($2 IS NULL OR id IN ( \
                   SELECT issue_id FROM issue_environments \
                   WHERE project_id = $1 AND environment = $2 \
                     AND first_release = issues.first_release)) \
             GROUP BY first_release",
        )
        .bind(project_id)
        .bind(env)
        .fetch_all(pool)
        .await?;

        let in_scope = format!(
            "{} AND ($2 IS NULL OR environment = $2)",
            since("bucket", hours)
        );
        let sessions_sql = format!(
            "SELECT release, MIN(bucket), \
                    CAST(SUM(CASE WHEN {in_scope} THEN total ELSE 0 END) AS BIGINT), \
                    CAST(SUM(CASE WHEN {in_scope} THEN crashed ELSE 0 END) AS BIGINT) \
             FROM session_counts \
             WHERE project_id = $1 \
             GROUP BY release"
        );
        let sessions: Vec<SessionRow> = sqlx::query_as(sqlx::AssertSqlSafe(&*sessions_sql))
            .bind(project_id)
            .bind(env)
            .fetch_all(pool)
            .await?;

        let users_sql = format!(
            "SELECT release, COUNT(DISTINCT did), COUNT(DISTINCT CASE WHEN crashed THEN did END) \
             FROM session_users \
             WHERE project_id = $1 AND ($2 IS NULL OR environment = $2) AND {} \
             GROUP BY release",
            since_day("day", hours)
        );
        let users: Vec<(String, i64, i64)> = sqlx::query_as(sqlx::AssertSqlSafe(&*users_sql))
            .bind(project_id)
            .bind(env)
            .fetch_all(pool)
            .await?;

        let environments: Vec<(String, String)> = sqlx::query_as(
            "SELECT release, environment FROM events \
             WHERE project_id = $1 AND release <> '' AND environment <> '' \
             GROUP BY release, environment \
             UNION \
             SELECT release, environment FROM session_counts \
             WHERE project_id = $1 AND environment <> '' \
             GROUP BY release, environment",
        )
        .bind(project_id)
        .fetch_all(pool)
        .await?;

        let mut rows: HashMap<String, ReleaseSummary> = HashMap::new();
        fn row<'a>(
            rows: &'a mut HashMap<String, ReleaseSummary>,
            version: &str,
            seen: DateTime<Utc>,
        ) -> &'a mut ReleaseSummary {
            let entry = rows
                .entry(version.to_string())
                .or_insert_with(|| ReleaseSummary {
                    version: version.to_string(),
                    date_created: seen,
                    date_released: None,
                    reference: None,
                    url: None,
                    first_event: None,
                    last_event: None,
                    event_count: 0,
                    new_issues: 0,
                    environments: Vec::new(),
                    sessions: 0,
                    crashed: 0,
                    crash_free_sessions: None,
                    crash_free_users: None,
                    adoption: None,
                });
            entry.date_created = entry.date_created.min(seen);
            entry
        }

        for (version, first, last, count) in events {
            let r = row(&mut rows, &version, first);
            r.first_event = Some(first);
            r.last_event = Some(last);
            r.event_count = count;
        }
        for (version, first, total, crashed) in sessions {
            let r = row(&mut rows, &version, first);
            r.sessions = total;
            r.crashed = crashed;
        }
        // Last, so the record's date wins over the first event's.
        for (version, created, released, reference, url) in records {
            let r = row(&mut rows, &version, created);
            r.date_created = created;
            r.date_released = released;
            r.reference = reference;
            r.url = url;
        }

        let total_sessions: i64 = rows.values().map(|r| r.sessions).sum();
        let new_issues: HashMap<_, _> = new_issues.into_iter().collect();
        let users: HashMap<_, _> = users
            .into_iter()
            .map(|(release, all, crashed)| (release, (all, crashed)))
            .collect();
        let mut environments_of: HashMap<String, BTreeSet<String>> = HashMap::new();
        for (release, environment) in environments {
            environments_of
                .entry(release)
                .or_default()
                .insert(environment);
        }

        let mut out: Vec<ReleaseSummary> = rows
            .into_values()
            .map(|mut r| {
                r.new_issues = new_issues.get(&r.version).copied().unwrap_or(0);
                r.environments = environments_of
                    .remove(&r.version)
                    .map(|set| set.into_iter().collect())
                    .unwrap_or_default();
                r.crash_free_sessions = crash_free_rate(r.crashed, r.sessions);
                if r.sessions > 0 {
                    r.adoption = Some(r.sessions as f64 / total_sessions as f64);
                }
                if let Some(&(all, crashed)) = users.get(&r.version) {
                    r.crash_free_users = crash_free_rate(crashed, all);
                }
                r
            })
            .collect();
        if let Some(query) = filter.query.as_deref().map(str::to_lowercase) {
            out.retain(|r| r.version.to_lowercase().contains(&query));
        }
        out.sort_by(|a, b| {
            b.date_created
                .cmp(&a.date_created)
                .then_with(|| b.version.cmp(&a.version))
        });
        Ok(out)
    }
}
