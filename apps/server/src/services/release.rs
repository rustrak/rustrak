use std::collections::{BTreeSet, HashMap};

use chrono::{DateTime, Utc};

use crate::db::dialect::{since, since_day, since_utc};
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::session::crash_free_rate;
use crate::models::{
    is_valid_version, CommitAuthor, CommitInput, CommitResponse, CreateDeploy, CreateRelease,
    DeployResponse, OrgReleaseResponse, Release, ReleaseProject, ReleaseSummary, UpdateRelease,
};
use crate::services::ResolutionService;

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
    ///
    /// A new release resolves, in the same transaction, the issues pending
    /// "resolved in next release" on an older one (Sentry's `post_save`
    /// receiver running `clear_expired_resolutions`, only on creation).
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
            ON CONFLICT (project_id, version) DO NOTHING
            RETURNING {RELEASE_COLUMNS}
            "#
        );

        // Write-first (INSERT opens the tx) — deferred BEGIN deliberate, see db::begin_write.
        let mut tx = pool.begin().await?;
        // A resolve in progress has read the releases: it writes its pending
        // resolution before this one looks for pending resolutions.
        ResolutionService::lock_project(&mut tx, project_id).await?;
        let created = sqlx::query_as::<_, Release>(sqlx::AssertSqlSafe(&*query))
            .bind(project_id)
            .bind(version)
            .bind(&input.reference)
            .bind(&input.url)
            .bind(now)
            .fetch_optional(&mut *tx)
            .await?;
        let Some(release) = created else {
            tx.commit().await?;
            let existing = Self::get_by_version(pool, project_id, version).await?;
            return Ok((existing, false));
        };
        ResolutionService::clear_expired(&mut tx, project_id, &release.release_ref()).await?;
        tx.commit().await?;
        Ok((release, true))
    }

    /// A release across the given projects that have it, as Sentry's
    /// organization-level release: the earliest creation, the latest release
    /// date, and its projects. `None` when none of them has it.
    pub async fn org_release(
        pool: &DbPool,
        version: &str,
        project_ids: &[i32],
    ) -> AppResult<Option<OrgReleaseResponse>> {
        if project_ids.is_empty() {
            return Ok(None);
        }
        let mut query = sqlx::QueryBuilder::new(
            "SELECT p.id, p.slug, p.name, r.ref, r.url, r.date_created, r.date_released, r.id \
             FROM releases r JOIN projects p ON p.id = r.project_id WHERE r.version = ",
        );
        query
            .push_bind(version.trim())
            .push(" AND r.project_id IN (");
        let mut ids = query.separated(", ");
        for id in project_ids {
            ids.push_bind(*id);
        }
        query.push(") ORDER BY p.slug");
        type Row = (
            i32,
            String,
            String,
            Option<String>,
            Option<String>,
            DateTime<Utc>,
            Option<DateTime<Utc>>,
            i32,
        );
        let rows: Vec<Row> = query.build_query_as().fetch_all(pool).await?;
        let Some(first) = rows.first() else {
            return Ok(None);
        };
        let release_ids: Vec<i32> = rows.iter().map(|r| r.7).collect();
        let commits = Self::commits(pool, &release_ids).await?;
        Ok(Some(OrgReleaseResponse {
            commit_count: commits.len() as i64,
            last_commit: commits.into_iter().next(),
            version: version.trim().to_string(),
            reference: rows.iter().find_map(|r| r.3.clone()),
            url: rows.iter().find_map(|r| r.4.clone()),
            date_created: rows.iter().map(|r| r.5).min().unwrap_or(first.5),
            date_released: rows.iter().filter_map(|r| r.6).max(),
            projects: rows
                .into_iter()
                .map(|(id, slug, name, ..)| ReleaseProject { id, slug, name })
                .collect(),
        }))
    }

    /// Deletes a release from these projects, unless Sentry's
    /// `Release.safe_delete` would refuse: an issue started in it
    /// (`first_release`), or it has health data. `Err` holds Sentry's reason.
    pub async fn delete(
        pool: &DbPool,
        version: &str,
        project_ids: &[i32],
    ) -> AppResult<Result<(), &'static str>> {
        let version = version.trim();
        let in_projects = |sql: &str| {
            let mut query = sqlx::QueryBuilder::new(sql);
            query
                .push_bind(version.to_string())
                .push(" AND project_id IN (");
            let mut list = query.separated(", ");
            for id in project_ids {
                list.push_bind(*id);
            }
            query.push(")");
            query
        };
        let mut referenced =
            in_projects("SELECT EXISTS(SELECT 1 FROM issues WHERE first_release = ");
        referenced.push(")");
        if referenced
            .build_query_scalar::<bool>()
            .fetch_one(pool)
            .await?
        {
            return Ok(Err(
                "This release is referenced by active issues and cannot be removed.",
            ));
        }
        let mut health = in_projects("SELECT EXISTS(SELECT 1 FROM session_counts WHERE release = ");
        health.push(")");
        if health.build_query_scalar::<bool>().fetch_one(pool).await? {
            return Ok(Err("This release has health data and cannot be removed."));
        }
        in_projects("DELETE FROM releases WHERE version = ")
            .build()
            .execute(pool)
            .await?;
        Ok(Ok(()))
    }

    /// The commits of the first of these project releases that has any, in
    /// the order they were set. Every project of a release gets the same list.
    pub async fn commits(pool: &DbPool, release_ids: &[i32]) -> AppResult<Vec<CommitResponse>> {
        if release_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut query = sqlx::QueryBuilder::new(
            "SELECT release_id, sha, message, date_created, repository, author_name, author_email \
             FROM release_commits WHERE release_id IN (",
        );
        let mut list = query.separated(", ");
        for id in release_ids {
            list.push_bind(*id);
        }
        query.push(") ORDER BY release_id, position");
        type Row = (
            i32,
            String,
            Option<String>,
            Option<DateTime<Utc>>,
            Option<String>,
            Option<String>,
            Option<String>,
        );
        let rows: Vec<Row> = query.build_query_as().fetch_all(pool).await?;
        let Some(first) = rows.first().map(|r| r.0) else {
            return Ok(Vec::new());
        };
        Ok(rows
            .into_iter()
            .take_while(|r| r.0 == first)
            .map(
                |(_, id, message, date_created, repository, name, email)| CommitResponse {
                    id,
                    message,
                    date_created,
                    repository,
                    author: CommitAuthor { name, email },
                },
            )
            .collect())
    }

    /// Replaces the commits of `version` in each of these projects, in the
    /// given order: Sentry's `set_commits`. An empty list clears them.
    pub async fn set_commits(
        pool: &DbPool,
        version: &str,
        project_ids: &[i32],
        commits: &[CommitInput],
    ) -> AppResult<()> {
        let mut tx = pool.begin().await?;
        for &project_id in project_ids {
            let release_id: i32 = sqlx::query_scalar(
                "SELECT id FROM releases WHERE project_id = $1 AND version = $2",
            )
            .bind(project_id)
            .bind(version.trim())
            .fetch_one(&mut *tx)
            .await?;
            sqlx::query("DELETE FROM release_commits WHERE release_id = $1")
                .bind(release_id)
                .execute(&mut *tx)
                .await?;
            for (position, commit) in commits.iter().enumerate() {
                sqlx::query(
                    "INSERT INTO release_commits \
                     (release_id, position, sha, repository, author_name, author_email, message, date_created) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                )
                .bind(release_id)
                .bind(position as i32)
                .bind(&commit.id)
                .bind(&commit.repository)
                .bind(&commit.author_name)
                .bind(&commit.author_email)
                .bind(&commit.message)
                .bind(commit.timestamp)
                .execute(&mut *tx)
                .await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }

    /// The release before `version`, by creation, among these projects that
    /// has commits: where `set-commits` starts reading git
    /// (`organization_release_previous_commits.py`).
    pub async fn previous_with_commits(
        pool: &DbPool,
        version: &str,
        project_ids: &[i32],
    ) -> AppResult<Option<OrgReleaseResponse>> {
        if project_ids.is_empty() {
            return Ok(None);
        }
        let mut query =
            sqlx::QueryBuilder::new("SELECT r.version FROM releases r WHERE r.project_id IN (");
        let mut list = query.separated(", ");
        for id in project_ids {
            list.push_bind(*id);
        }
        query
            .push(") AND r.version <> ")
            .push_bind(version.trim())
            .push(
                " AND r.date_created < (SELECT MIN(cur.date_created) FROM releases cur WHERE cur.version = ",
            )
            .push_bind(version.trim())
            .push(
                ") AND EXISTS (SELECT 1 FROM release_commits c WHERE c.release_id = r.id) \
                 ORDER BY r.date_created DESC LIMIT 1",
            );
        let previous: Option<String> = query.build_query_scalar().fetch_optional(pool).await?;
        match previous {
            Some(previous) => Self::org_release(pool, &previous, project_ids).await,
            None => Ok(None),
        }
    }

    /// Records a deploy of `version` in each of these projects.
    pub async fn create_deploy(
        pool: &DbPool,
        version: &str,
        project_ids: &[i32],
        environment: &str,
        input: &CreateDeploy,
    ) -> AppResult<DeployResponse> {
        let date_finished = input.date_finished.unwrap_or_else(Utc::now);
        let mut tx = pool.begin().await?;
        let mut first: Option<i32> = None;
        for &project_id in project_ids {
            let id: i32 = sqlx::query_scalar(
                "INSERT INTO release_deploys (release_id, environment, name, url, date_started, date_finished) \
                 SELECT id, $3, $4, $5, $6, $7 FROM releases WHERE project_id = $1 AND version = $2 \
                 RETURNING id",
            )
            .bind(project_id)
            .bind(version.trim())
            .bind(environment)
            .bind(&input.name)
            .bind(&input.url)
            .bind(input.date_started)
            .bind(date_finished)
            .fetch_one(&mut *tx)
            .await?;
            first.get_or_insert(id);
        }
        tx.commit().await?;
        Ok(DeployResponse {
            id: first.unwrap_or_default().to_string(),
            environment: environment.to_string(),
            date_started: input.date_started,
            date_finished,
            name: input.name.clone(),
            url: input.url.clone(),
        })
    }

    /// The latest deploy of `version` per project and environment, newest
    /// first, as Sentry lists a release's deploys.
    pub async fn deploys(
        pool: &DbPool,
        version: &str,
        project_ids: &[i32],
    ) -> AppResult<Vec<DeployResponse>> {
        if project_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut query = sqlx::QueryBuilder::new(
            "SELECT CAST(d.id AS TEXT) AS id, d.environment, d.date_started, d.date_finished, d.name, d.url \
             FROM release_deploys d JOIN releases r ON r.id = d.release_id \
             WHERE r.version = ",
        );
        query
            .push_bind(version.trim())
            .push(" AND r.project_id IN (");
        let mut list = query.separated(", ");
        for id in project_ids {
            list.push_bind(*id);
        }
        query.push(
            ") AND NOT EXISTS (SELECT 1 FROM release_deploys newer \
               WHERE newer.release_id = d.release_id AND newer.environment = d.environment \
               AND (newer.date_finished > d.date_finished \
                    OR (newer.date_finished = d.date_finished AND newer.id > d.id))) \
             ORDER BY d.date_finished DESC, d.id DESC",
        );
        Ok(query.build_query_as().fetch_all(pool).await?)
    }

    /// Release versions, newest first, in the given projects or in all.
    pub async fn latest_versions(
        pool: &DbPool,
        project_ids: Option<&[i32]>,
        limit: i64,
    ) -> AppResult<Vec<String>> {
        let mut query = sqlx::QueryBuilder::new("SELECT version FROM releases");
        if let Some(ids) = project_ids {
            if ids.is_empty() {
                return Ok(Vec::new());
            }
            query.push(" WHERE project_id IN (");
            let mut list = query.separated(", ");
            for id in ids {
                list.push_bind(*id);
            }
            query.push(")");
        }
        query
            .push(" GROUP BY version ORDER BY MAX(date_created) DESC, version DESC LIMIT ")
            .push_bind(limit);
        Ok(query.build_query_scalar().fetch_all(pool).await?)
    }

    /// The projects that have a release with this version.
    pub async fn projects_with(pool: &DbPool, version: &str) -> AppResult<Vec<i32>> {
        Ok(sqlx::query_scalar(
            "SELECT project_id FROM releases WHERE version = $1 ORDER BY project_id",
        )
        .bind(version.trim())
        .fetch_all(pool)
        .await?)
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
            since_utc("ingested_at", hours)
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
                    registered: false,
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
            r.registered = true;
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
