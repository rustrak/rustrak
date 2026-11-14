use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::db::dialect::{bucket, compare_utc, epoch, json_member, push_compare_utc, since};
use crate::db::{Db, DbPool};
use crate::error::{AppError, AppResult};
use crate::models::rbac::ProjectScope;
use crate::models::{Grouping, Issue};
use crate::pagination::{IssueCategory, IssueFilter, IssueSort, SortOrder};
use crate::services::github;
use crate::services::grouping::DenormalizedFields;
use crate::services::scope::Scope;
use crate::services::IssueSocialService;
use serde::Serialize;
use sqlx::QueryBuilder;
use std::collections::HashMap;

/// A tag value with its event count within an issue.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TagValueCount {
    pub value: String,
    pub count: i64,
}

/// A single tag value's usage within an issue — the shape `GET
/// /issues/{id}/tags/{key}` returns per entry, matching Sentry's
/// `TagValueSerializerResponse` (src/sentry/tagstore/types.py:166-173):
/// a bare list of these, not `{key, values}`.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct IssueTagValue {
    pub key: String,
    /// Display name for the tag key. Real Sentry has a reserved-key display
    /// name table; Rustrak doesn't, so this currently just mirrors `key`.
    pub name: String,
    pub value: String,
    pub count: i64,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
}

/// A tag key with its most common values within an issue.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TagSummary {
    pub key: String,
    /// The scanned events carrying the tag, whatever its value: Sentry's
    /// `totalValues` (`GroupTagKey.count`), so each top value's share is its
    /// `count` over this.
    pub total_values: usize,
    pub top_values: Vec<TagValueCount>,
}

/// Per-issue aggregates computed from recent events (capped scan).
#[derive(Debug, Serialize, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct IssueAggregates {
    pub user_count: i64,
    pub tags: Vec<TagSummary>,
}

/// Max events scanned when computing per-issue aggregates (tags, user count).
const AGGREGATE_SCAN_CAP: i64 = 1000;

/// `issues.numeric_id` of a new row. Postgres has an identity column; SQLite
/// reads a counter that a trigger advances on insert (single writer, so the
/// read and the advance cannot interleave). Neither reuses a deleted id.
#[cfg(feature = "postgres")]
pub(crate) const NEXT_NUMERIC_ID: &str = "DEFAULT";
#[cfg(feature = "sqlite")]
pub(crate) const NEXT_NUMERIC_ID: &str = "(SELECT value + 1 FROM issue_numeric_id_counter)";

/// Per-issue supplementary stats for the issue list (unique user count + a
/// compact 24h hourly trend), computed from a single capped scan per issue.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct IssueListStats {
    pub user_count: i64,
    /// 24 hourly buckets covering the last 24h, oldest to newest.
    pub trend: Vec<i64>,
}

/// Events deleted per transaction when an issue is deleted on SQLite.
#[cfg(feature = "sqlite")]
const ISSUE_DELETE_BATCH: i64 = 1_000;

/// Hourly buckets in the issue-list trend column (Sentry's default "24h" window).
const LIST_TREND_BUCKETS: i64 = 24;
const LIST_TREND_BUCKET_SECS: i64 = 3600;

/// Who an issue list is narrowed to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Assignee {
    Nobody,
    User(i32),
}

/// The issue table's own filters, beyond status and scope. Empty narrows
/// nothing, which is what every caller but the table passes.
#[derive(Debug, Default, Clone)]
pub struct IssueFilters {
    pub levels: Vec<String>,
    pub priorities: Vec<String>,
    pub assigned: Option<Assignee>,
    pub first_seen_hours: Option<i64>,
    pub last_seen_hours: Option<i64>,
    pub events_min: Option<i64>,
    pub events_max: Option<i64>,
}

/// The status an update sets to resolve an issue until the next release.
pub const RESOLVED_IN_NEXT_RELEASE: &str = "resolvedInNextRelease";

pub struct IssueService;

/// Derives an initial issue priority from the event level, mirroring Sentry's
/// default priority assignment (high for errors/fatal, medium for warnings,
/// low otherwise).
pub(crate) fn derive_priority(level: Option<&str>) -> &'static str {
    match level {
        Some("fatal") | Some("error") => "high",
        Some("warning") => "medium",
        Some("info") | Some("debug") => "low",
        // Missing or unrecognized level: Sentry's `_get_priority_for_group`
        // (event_manager.py:2099-2134) falls through to MEDIUM here, not
        // HIGH and not LOW.
        _ => "medium",
    }
}

/// Extracts (key, value) tag pairs from an event payload. Handles both the
/// object form (`tags: {k: v}`) and the normalized array form
/// (`tags: [[k, v]]` or `tags: [{"key": k, "value": v}]`).
///
/// Each key once, the first occurrence kept: Relay deduplicates an event's
/// tags the same way (`normalize_event_tags`), so a key sent twice counts as
/// one event carrying it, not two.
fn extract_tags(tags: Option<&serde_json::Value>) -> Vec<(String, String)> {
    let mut seen = std::collections::HashSet::new();
    let mut tags = tags.map(extract_raw_tags).unwrap_or_default();
    tags.retain(|(key, _)| seen.insert(key.clone()));
    tags
}

/// The pairs of an event's `tags` member.
fn extract_raw_tags(tags: &serde_json::Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    match tags {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                if let Some(s) = value_to_tag_string(v) {
                    out.push((k.clone(), s));
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                if let Some(arr) = item.as_array() {
                    if arr.len() == 2 {
                        if let (Some(k), Some(v)) = (arr[0].as_str(), value_to_tag_string(&arr[1]))
                        {
                            out.push((k.to_string(), v));
                        }
                    }
                } else if let Some(obj) = item.as_object() {
                    if let (Some(k), Some(v)) = (
                        obj.get("key").and_then(|k| k.as_str()),
                        obj.get("value").and_then(value_to_tag_string_opt),
                    ) {
                        out.push((k.to_string(), v));
                    }
                }
            }
        }
        _ => {}
    }
    out
}

fn value_to_tag_string(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn value_to_tag_string_opt(v: &serde_json::Value) -> Option<String> {
    value_to_tag_string(v)
}

/// Extracts a stable user identity string from an already-extracted `user`
/// sub-object (as opposed to a full event `data` blob) — split out so
/// `list_stats`'s SQL can project just `data->'user'` instead of hydrating
/// the entire event payload for every event on the issue-list hot path.
pub(crate) fn user_identity_from_user_field(user: &serde_json::Value) -> Option<String> {
    for (field, key) in [
        ("id", "id"),
        ("username", "username"),
        ("email", "email"),
        ("ip_address", "ip"),
    ] {
        if let Some(s) = user.get(field).and_then(|v| v.as_str()) {
            if !s.is_empty() {
                return Some(format!("{key}:{s}"));
            }
        }
    }
    None
}

/// Sorts a value->count map into descending-count order (ties broken by value).
fn sort_counts(counts: HashMap<String, i64>) -> Vec<TagValueCount> {
    let mut v: Vec<TagValueCount> = counts
        .into_iter()
        .map(|(value, count)| TagValueCount { value, count })
        .collect();
    v.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.value.cmp(&b.value)));
    v
}

impl IssueService {
    /// Lists issues first seen in a given release, most recently introduced first.
    /// Powers the "New Issues" section of a release detail page.
    pub async fn top_issues_for_release(
        pool: &DbPool,
        project_id: i32,
        release: &str,
        limit: i64,
    ) -> AppResult<Vec<Issue>> {
        let issues = sqlx::query_as::<_, Issue>(
            r#"
            SELECT * FROM issues
            WHERE project_id = $1 AND first_release = $2 AND issue_category = 'error'
            ORDER BY first_seen DESC
            LIMIT $3
            "#,
        )
        .bind(project_id)
        .bind(release)
        .bind(limit)
        .fetch_all(pool)
        .await?;

        Ok(issues)
    }

    /// Lists issues with offset-based pagination
    ///
    /// Returns (issues, total_count) where total_count is the total matching issues.
    #[allow(clippy::too_many_arguments)]
    pub async fn list_offset(
        pool: &DbPool,
        projects: &ProjectScope,
        sort: IssueSort,
        order: SortOrder,
        filter: IssueFilter,
        category: IssueCategory,
        page: i64,
        per_page: i64,
        search: Option<&str>,
        scope: Scope<'_>,
        filters: &IssueFilters,
    ) -> AppResult<(Vec<Issue>, i64)> {
        if page < 1 {
            return Err(AppError::Validation(format!(
                "page must be >= 1, got {}",
                page
            )));
        }
        crate::pagination::per_page(per_page)?;
        let offset = crate::pagination::offset(page, per_page);

        let order_clause = match (sort, order) {
            (IssueSort::DigestOrder, SortOrder::Desc) => "digest_order DESC",
            (IssueSort::DigestOrder, SortOrder::Asc) => "digest_order ASC",
            (IssueSort::LastSeen, SortOrder::Desc) => "last_seen DESC, id DESC",
            (IssueSort::LastSeen, SortOrder::Asc) => "last_seen ASC, id ASC",
            (IssueSort::EventCount, SortOrder::Desc) => "digested_event_count DESC, id DESC",
            (IssueSort::EventCount, SortOrder::Asc) => "digested_event_count ASC, id ASC",
            (IssueSort::FirstSeen, SortOrder::Desc) => "first_seen DESC, id DESC",
            (IssueSort::FirstSeen, SortOrder::Asc) => "first_seen ASC, id ASC",
            (IssueSort::Priority, SortOrder::Desc) => {
                "CASE priority WHEN 'high' THEN 3 WHEN 'medium' THEN 2 WHEN 'low' THEN 1 \
                 ELSE 0 END DESC, last_seen DESC, id DESC"
            }
            (IssueSort::Priority, SortOrder::Asc) => {
                "CASE priority WHEN 'high' THEN 3 WHEN 'medium' THEN 2 WHEN 'low' THEN 1 \
                 ELSE 0 END ASC, last_seen DESC, id DESC"
            }
        };

        Self::list_window(
            pool,
            projects,
            order_clause,
            filter,
            category,
            offset,
            per_page,
            search,
            scope,
            filters,
        )
        .await
    }

    /// One window of the issues of `projects`: `filter`, `category`, `search`
    /// and `scope` as in [`Self::list_offset`], ordered by a trusted
    /// `order_clause`.
    ///
    /// Returns (issues, total_count) where total_count is the total matching issues.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn list_window(
        pool: &DbPool,
        projects: &ProjectScope,
        order_clause: &str,
        filter: IssueFilter,
        category: IssueCategory,
        offset: i64,
        per_page: i64,
        search: Option<&str>,
        scope: Scope<'_>,
        filters: &IssueFilters,
    ) -> AppResult<(Vec<Issue>, i64)> {
        let status_clause = filter_clause(filter, category, projects);
        // Bound from `$1`, in order.
        let mut binds: Vec<String> = Vec::new();

        // In an environment, only the issues seen there, with that
        // environment's first/last seen and count (Sentry's
        // `GroupEnvironment` and `__seen_stats_impl`). The rows read as
        // `issues`, so every clause below applies unchanged.
        let env_param = scope.environment.map(|env| {
            binds.push(env.to_string());
            binds.len()
        });
        let source = match env_param {
            Some(n) => format!(
                "(SELECT {ENV_SCOPED_COLUMNS} FROM issues i \
                 JOIN issue_environments e ON e.issue_id = i.id \
                 WHERE {} AND e.environment = ${n}) AS issues",
                project_clause("e.project_id", projects)
            ),
            None => "issues".to_string(),
        };

        // Normalize the search term into a case-insensitive LIKE pattern (works
        // on both Postgres and SQLite, unlike Postgres-only ILIKE). Literal
        // `%`/`_`/`\` in the user's term are escaped first so they match
        // themselves instead of acting as LIKE wildcards (the `%` added here
        // for the prefix/suffix match are the only real wildcards).
        let pattern = search.map(str::trim).filter(|s| !s.is_empty()).map(|s| {
            let escaped = s
                .to_lowercase()
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_");
            format!("%{}%", escaped)
        });
        let search_clause = match pattern {
            Some(pattern) => {
                binds.push(pattern);
                let n = binds.len();
                format!(
                    r#"AND (
                LOWER(calculated_type) LIKE ${n} ESCAPE '\'
                OR LOWER(calculated_value) LIKE ${n} ESCAPE '\'
                OR LOWER("transaction") LIKE ${n} ESCAPE '\'
                OR LOWER(culprit) LIKE ${n} ESCAPE '\'
                OR LOWER(last_frame_filename) LIKE ${n} ESCAPE '\'
                OR LOWER(last_frame_module) LIKE ${n} ESCAPE '\'
            )"#
                )
            }
            None => String::new(),
        };
        // In a release, only the issues that had an event in it -- in the
        // environment too when there is one: an issue seen in production on
        // 1.0 and in staging on 2.0 was never in production on 2.0.
        let release_clause = match scope.release {
            Some(release) => {
                binds.push(release.to_string());
                let env_clause = env_param
                    .map(|n| format!(" AND environment = ${n}"))
                    .unwrap_or_default();
                format!(
                    "AND id IN (SELECT issue_id FROM events \
                     WHERE {} AND release = ${}{env_clause})",
                    project_clause("project_id", projects),
                    binds.len()
                )
            }
            None => String::new(),
        };
        let filter_clause = narrowing_clause(filters, &mut binds);
        let from = format!(
            "FROM {source} WHERE {status_clause} {search_clause} {release_clause} {filter_clause}"
        );

        let count_query = format!("SELECT COUNT(*) {from}");
        let mut count = sqlx::query_as::<_, (i64,)>(sqlx::AssertSqlSafe(&*count_query));
        for b in &binds {
            count = count.bind(b);
        }
        let total_count = count.fetch_one(pool).await?;

        let select_query = format!(
            "SELECT * {from} ORDER BY {order_clause} LIMIT ${} OFFSET ${}",
            binds.len() + 1,
            binds.len() + 2
        );
        let mut select = sqlx::query_as::<_, Issue>(sqlx::AssertSqlSafe(&*select_query));
        for b in &binds {
            select = select.bind(b);
        }
        let issues = select.bind(per_page).bind(offset).fetch_all(pool).await?;

        Ok((issues, total_count.0))
    }

    /// The environments a project has seen, by name: its issues', and its
    /// sessions' -- a healthy environment has sessions and no issue at all.
    /// Sentry's environments are the project's, not its issues'.
    pub async fn environments(pool: &DbPool, project_id: i32) -> AppResult<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT environment FROM issue_environments WHERE project_id = $1 \
             UNION \
             SELECT environment FROM session_counts \
             WHERE project_id = $1 AND environment <> '' \
             ORDER BY environment",
        )
        .bind(project_id)
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(|(env,)| env).collect())
    }

    pub async fn get_by_id(pool: &DbPool, id: Uuid) -> AppResult<Issue> {
        let issue = sqlx::query_as::<_, Issue>("SELECT * FROM issues WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Issue {} not found", id)))?;

        Ok(issue)
    }

    /// The issue, if it belongs to `project_id`. Another project's issue is
    /// the same 404 as a missing one, so ids cannot be probed across projects.
    pub async fn get_in_project(pool: &DbPool, project_id: i32, id: Uuid) -> AppResult<Issue> {
        let issue = Self::get_by_id(pool, id).await?;
        if issue.project_id != project_id {
            return Err(AppError::NotFound(format!("Issue {} not found", id)));
        }
        Ok(issue)
    }

    /// Gets an issue by its numeric id (the Sentry-compatible API's id)
    pub async fn get_by_numeric_id(pool: &DbPool, numeric_id: i64) -> AppResult<Issue> {
        let issue = sqlx::query_as::<_, Issue>("SELECT * FROM issues WHERE numeric_id = $1")
            .bind(numeric_id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Issue {} not found", numeric_id)))?;

        Ok(issue)
    }

    /// Creates an issue for its first event, with the default category.
    pub async fn create(
        pool: &DbPool,
        project_id: i32,
        timestamp: DateTime<Utc>,
        denormalized: &DenormalizedFields,
        level: Option<&str>,
        platform: Option<&str>,
    ) -> AppResult<Issue> {
        let mut conn = pool.acquire().await?;
        let category = IssueCategory::Error.as_str();
        Self::create_in(
            &mut conn,
            project_id,
            timestamp,
            denormalized,
            level,
            platform,
            category,
        )
        .await
    }

    /// Creates an issue for its first event on the caller's connection, so
    /// the digest creates it in the transaction that groups the event. The
    /// caller serialises writers per project: `digest_order` is the next one.
    pub async fn create_in(
        conn: &mut <Db as sqlx::Database>::Connection,
        project_id: i32,
        timestamp: DateTime<Utc>,
        denormalized: &DenormalizedFields,
        level: Option<&str>,
        platform: Option<&str>,
        category: &str,
    ) -> AppResult<Issue> {
        let max_order: Option<i32> =
            sqlx::query_scalar("SELECT MAX(digest_order) FROM issues WHERE project_id = $1")
                .bind(project_id)
                .fetch_one(&mut *conn)
                .await?;
        Ok(sqlx::query_as::<_, Issue>(sqlx::AssertSqlSafe(format!(
            r#"
            INSERT INTO issues (
                id, project_id, digest_order, first_seen, last_seen,
                digested_event_count, stored_event_count,
                calculated_type, calculated_value, "transaction",
                last_frame_filename, last_frame_module, last_frame_function,
                level, platform, status, substatus, priority, culprit, logger,
                first_release, last_release, issue_type, issue_category, numeric_id
            )
            VALUES (
                $1, $2, $3, $4, $4, 1, 1, $5, $6, $7, $8, $9, $10, $11, $12,
                'unresolved', 'new', $13, $14, $15, $16, $16, $17, $17, {NEXT_NUMERIC_ID}
            )
            RETURNING *
            "#,
        )))
        .bind(Uuid::new_v4())
        .bind(project_id)
        .bind(max_order.unwrap_or(0) + 1)
        .bind(timestamp)
        .bind(&denormalized.calculated_type)
        .bind(&denormalized.calculated_value)
        .bind(&denormalized.transaction)
        .bind(&denormalized.last_frame_filename)
        .bind(&denormalized.last_frame_module)
        .bind(&denormalized.last_frame_function)
        .bind(level)
        .bind(platform)
        .bind(derive_priority(level))
        .bind(&denormalized.culprit)
        .bind(&denormalized.logger)
        .bind(&denormalized.release)
        .bind(category)
        .fetch_one(&mut *conn)
        .await?)
    }

    /// Sets an issue's status. `substatus_override` takes precedence when
    /// given (e.g. a client-supplied canonical substatus), but only if it is
    /// a legal pairing with `status` (see [`substatus_valid_for_status`]);
    /// otherwise the matching default substatus for `status` is used.
    ///
    /// Always clears `status_details` — it only carries the "resolved in
    /// next release" marker (see [`resolve_in_next_release`]), which no
    /// longer applies once the issue transitions through a normal status
    /// change.
    ///
    /// Accepted statuses: `unresolved`, `resolved`, `ignored`.
    ///
    /// [`substatus_valid_for_status`]: crate::models::substatus_valid_for_status
    pub async fn set_status(
        pool: &DbPool,
        id: Uuid,
        status: &str,
        substatus_override: Option<&str>,
    ) -> AppResult<Issue> {
        let default_substatus: Option<&str> = match status {
            crate::models::STATUS_UNRESOLVED => Some("ongoing"),
            crate::models::STATUS_RESOLVED => None,
            crate::models::STATUS_IGNORED => Some("archived_forever"),
            other => {
                return Err(AppError::Validation(format!("Invalid status: {}", other)));
            }
        };
        let substatus = match substatus_override {
            Some(s) if crate::models::substatus_valid_for_status(status, s) => Some(s),
            Some(s) => {
                return Err(AppError::Validation(format!(
                    "substatus '{}' is not valid for status '{}'",
                    s, status
                )));
            }
            None => default_substatus,
        };

        let issue = sqlx::query_as::<_, Issue>(
            r#"
            UPDATE issues
            SET status = $2, substatus = $3, status_details = '{}'
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(status)
        .bind(substatus)
        .fetch_one(pool)
        .await
        .map_err(|error| match error {
            sqlx::Error::RowNotFound => AppError::NotFound(format!("Issue {} not found", id)),
            other => other.into(),
        })?;

        Ok(issue)
    }

    /// Marks an issue as resolved
    pub async fn resolve(pool: &DbPool, id: Uuid) -> AppResult<Issue> {
        Self::set_status(pool, id, crate::models::STATUS_RESOLVED, None).await
    }

    /// Reopens an issue
    pub async fn unresolve(pool: &DbPool, id: Uuid) -> AppResult<Issue> {
        Self::set_status(pool, id, crate::models::STATUS_UNRESOLVED, None).await
    }

    /// Mutes (ignores) an issue
    pub async fn mute(pool: &DbPool, id: Uuid) -> AppResult<Issue> {
        Self::set_status(pool, id, crate::models::STATUS_IGNORED, None).await
    }

    /// Unmutes an issue (back to unresolved)
    pub async fn unmute(pool: &DbPool, id: Uuid) -> AppResult<Issue> {
        Self::set_status(pool, id, crate::models::STATUS_UNRESOLVED, None).await
    }

    /// Resolves an issue "in the next release": it is marked resolved now, but
    /// a regression is suppressed for further events from the current release.
    /// A subsequent deploy (see [`finalize_release`]) clears the marker.
    pub async fn resolve_in_next_release(pool: &DbPool, id: Uuid) -> AppResult<Issue> {
        sqlx::query_as::<_, Issue>(
            r#"
            UPDATE issues
            SET status = 'resolved', substatus = NULL, status_details = '{"in_next_release":true}'
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(id)
        .fetch_one(pool)
        .await
        .map_err(|error| match error {
            sqlx::Error::RowNotFound => AppError::NotFound(format!("Issue {} not found", id)),
            other => other.into(),
        })
    }

    /// Clears the "resolved in next release" marker on issues whose
    /// `last_release` maps to a release that shipped *before*
    /// `new_release_date_created`, in the same project. The comparison
    /// (`date_created <`, project-scoped) is the chronological equivalent of
    /// Sentry's `post_save` signal on `Release` (`clear_expired_resolutions`,
    /// compared on `Release.date_added`) — it replaces a previous
    /// implementation that compared `last_release` to the new version by
    /// string inequality, which cannot tell "older" from "different".
    /// Returns the number of issues finalized.
    ///
    /// Called on every `POST .../releases/` call, including the idempotent
    /// 208 branch (see `routes::releases::create_release`) — unlike Sentry,
    /// whose signal only fires on first creation. This is deliberate: the
    /// query above is naturally idempotent (a repeat run affects 0 rows once
    /// nothing is left to clear), so calling it unconditionally self-heals a
    /// prior call that created the release row but crashed before clearing
    /// ran, which Sentry's fire-and-forget task cannot do.
    pub async fn finalize_release(
        pool: &DbPool,
        project_id: i32,
        new_release_date_created: DateTime<Utc>,
    ) -> AppResult<u64> {
        let res = sqlx::query(
            r#"
            UPDATE issues
            SET status_details = '{}'
            WHERE project_id = $1
              AND status = 'resolved'
              AND status_details LIKE '%"in_next_release":true%'
              AND last_release IN (
                  SELECT version FROM releases
                  WHERE project_id = $1 AND date_created < $2
              )
            "#,
        )
        .bind(project_id)
        .bind(new_release_date_created)
        .execute(pool)
        .await?;
        Ok(res.rows_affected())
    }

    /// Sets an issue's priority (low/medium/high).
    pub async fn set_priority(pool: &DbPool, id: Uuid, priority: &str) -> AppResult<Issue> {
        if !matches!(priority, "low" | "medium" | "high") {
            return Err(AppError::Validation(format!(
                "Invalid priority: {}",
                priority
            )));
        }
        sqlx::query_as::<_, Issue>(
            "UPDATE issues SET priority = $2, priority_locked_at = $3 WHERE id = $1 RETURNING *",
        )
        .bind(id)
        .bind(priority)
        .bind(Utc::now())
        .fetch_one(pool)
        .await
        .map_err(|error| match error {
            sqlx::Error::RowNotFound => AppError::NotFound(format!("Issue {} not found", id)),
            other => other.into(),
        })
    }

    /// Assigns an issue to a user (or clears the assignment when `None`).
    pub async fn assign(
        pool: &DbPool,
        id: Uuid,
        assigned_to: Option<i32>,
        assignee_type: Option<&str>,
    ) -> AppResult<Issue> {
        sqlx::query_as::<_, Issue>(
            "UPDATE issues SET assigned_to = $2, assignee_type = $3 WHERE id = $1 RETURNING *",
        )
        .bind(id)
        .bind(assigned_to)
        .bind(assignee_type)
        .fetch_one(pool)
        .await
        .map_err(|error| match error {
            sqlx::Error::RowNotFound => AppError::NotFound(format!("Issue {} not found", id)),
            other => other.into(),
        })
    }

    /// Lists the grouping hashes that map to an issue.
    pub async fn list_hashes(pool: &DbPool, issue_id: Uuid) -> AppResult<Vec<Grouping>> {
        let hashes = sqlx::query_as::<_, Grouping>(
            "SELECT * FROM groupings WHERE issue_id = $1 ORDER BY id ASC",
        )
        .bind(issue_id)
        .fetch_all(pool)
        .await?;
        Ok(hashes)
    }

    /// Sets an issue's status the way a person does (a canonical status or
    /// [`RESOLVED_IN_NEXT_RELEASE`]): recorded in its activity and mirrored to
    /// the GitHub issues linked to it.
    pub async fn change_status(
        pool: &DbPool,
        id: Uuid,
        status: &str,
        substatus: Option<&str>,
        user_id: Option<i32>,
    ) -> AppResult<Issue> {
        let issue = if status == RESOLVED_IN_NEXT_RELEASE {
            Self::resolve_in_next_release(pool, id).await?
        } else {
            Self::set_status(pool, id, status, substatus).await?
        };
        IssueSocialService::record_status_change(pool, id, user_id, status).await?;
        github::sync::spawn("status sync", github::sync::mirror_status(pool.clone(), id));
        Ok(issue)
    }

    /// The bulk form of [`Self::change_status`] and of setting the priority:
    /// both in one transaction, so a request that sets both never leaves
    /// issues with the new status but the old priority. Ids of other projects
    /// are skipped. Returns the number of issues updated.
    pub async fn bulk_update(
        pool: &DbPool,
        project_id: i32,
        ids: &[Uuid],
        status: Option<&str>,
        priority: Option<&str>,
        user_id: Option<i32>,
    ) -> AppResult<u64> {
        // Write-first (UPDATE opens the tx) — deferred BEGIN deliberate, see db::begin_write.
        let mut tx = pool.begin().await?;
        let mut updated = 0u64;
        if let Some(status) = status {
            updated = Self::bulk_set_status(&mut tx, project_id, ids, status, user_id).await?;
        }
        if let Some(priority) = priority {
            let priority_updated =
                Self::bulk_set_priority(&mut tx, project_id, ids, priority).await?;
            if status.is_none() {
                updated = priority_updated;
            }
        }
        tx.commit().await?;

        if status.is_some() {
            for &id in ids {
                let pool = pool.clone();
                github::sync::spawn("status sync", async move {
                    if Self::get_by_id(&pool, id).await?.project_id == project_id {
                        github::sync::mirror_status(pool, id).await?;
                    }
                    Ok(())
                });
            }
        }
        Ok(updated)
    }

    /// Bulk-sets the status (a canonical one or [`RESOLVED_IN_NEXT_RELEASE`])
    /// of multiple issues in one project, recording each change in the issue's
    /// activity. Returns the number of issues updated.
    pub async fn bulk_set_status(
        tx: &mut sqlx::Transaction<'_, Db>,
        project_id: i32,
        ids: &[Uuid],
        status: &str,
        user_id: Option<i32>,
    ) -> AppResult<u64> {
        if status == RESOLVED_IN_NEXT_RELEASE {
            return Self::bulk_resolve_in_next_release(tx, project_id, ids, user_id).await;
        }
        let substatus: Option<&str> = match status {
            crate::models::STATUS_UNRESOLVED => Some("ongoing"),
            crate::models::STATUS_RESOLVED => None,
            crate::models::STATUS_IGNORED => Some("archived_forever"),
            other => return Err(AppError::Validation(format!("Invalid status: {}", other))),
        };

        let mut updated = 0u64;
        for id in ids {
            let res = sqlx::query(
                "UPDATE issues SET status = $2, substatus = $3, status_details = '{}' WHERE id = $1 AND project_id = $4",
            )
            .bind(id)
            .bind(status)
            .bind(substatus)
            .bind(project_id)
            .execute(&mut **tx)
            .await?;
            if res.rows_affected() > 0 {
                IssueSocialService::record_status_change_on(tx, *id, user_id, status).await?;
            }
            updated += res.rows_affected();
        }
        Ok(updated)
    }

    /// Bulk-sets the priority of multiple issues in one project, in a single
    /// query. Ids that don't exist or belong to another project are silently
    /// not counted, matching [`bulk_set_status`]'s semantics.
    ///
    /// Takes an open transaction so [`Self::bulk_update`] commits it together
    /// with the status.
    ///
    /// [`bulk_set_status`]: Self::bulk_set_status
    pub async fn bulk_set_priority(
        tx: &mut sqlx::Transaction<'_, Db>,
        project_id: i32,
        ids: &[Uuid],
        priority: &str,
    ) -> AppResult<u64> {
        if !matches!(priority, "low" | "medium" | "high") {
            return Err(AppError::Validation(format!(
                "Invalid priority: {}",
                priority
            )));
        }
        if ids.is_empty() {
            return Ok(0);
        }

        let mut qb = QueryBuilder::new("UPDATE issues SET priority = ");
        qb.push_bind(priority);
        qb.push(", priority_locked_at = ");
        qb.push_bind(Utc::now());
        qb.push(" WHERE project_id = ");
        qb.push_bind(project_id);
        qb.push(" AND id IN (");
        let mut sep = qb.separated(", ");
        for id in ids {
            sep.push_bind(*id);
        }
        qb.push(")");
        Ok(qb.build().execute(&mut **tx).await?.rows_affected())
    }

    /// Bulk "resolve in next release": same semantics as
    /// [`Self::resolve_in_next_release`], applied to many issues in one project.
    async fn bulk_resolve_in_next_release(
        tx: &mut sqlx::Transaction<'_, Db>,
        project_id: i32,
        ids: &[Uuid],
        user_id: Option<i32>,
    ) -> AppResult<u64> {
        let mut updated = 0u64;
        for id in ids {
            let res = sqlx::query(
                r#"
                UPDATE issues
                SET status = 'resolved', substatus = NULL, status_details = '{"in_next_release":true}'
                WHERE id = $1 AND project_id = $2
                "#,
            )
            .bind(id)
            .bind(project_id)
            .execute(&mut **tx)
            .await?;
            if res.rows_affected() > 0 {
                IssueSocialService::record_status_change_on(
                    tx,
                    *id,
                    user_id,
                    RESOLVED_IN_NEXT_RELEASE,
                )
                .await?;
            }
            updated += res.rows_affected();
        }
        Ok(updated)
    }

    /// Bulk-deletes multiple issues in one project. Returns the number deleted.
    ///
    /// Not atomic: each issue is deleted independently (reusing [`delete`],
    /// whose postgres/sqlite paths differ enough that wrapping the whole loop
    /// in one transaction isn't safe to do generically here). If a delete
    /// fails partway through, the ids processed so far are already gone and
    /// the caller only learns the count actually deleted, not which ones.
    pub async fn bulk_delete(
        pool: &DbPool,
        project_id: i32,
        ids: &[Uuid],
        errors: &crate::digest::processors::ErrorProcessor,
    ) -> AppResult<u64> {
        let mut deleted = 0u64;
        for id in ids {
            // Reuse the single-issue delete so project counters stay consistent.
            // Skip rows belonging to other projects.
            let belongs: Option<(i32,)> =
                sqlx::query_as("SELECT project_id FROM issues WHERE id = $1")
                    .bind(id)
                    .fetch_optional(pool)
                    .await?;
            if belongs.map(|(p,)| p) != Some(project_id) {
                continue;
            }
            Self::delete(pool, *id, errors).await?;
            deleted += 1;
        }
        Ok(deleted)
    }

    /// Deletes every issue in the project matching `filter`, with their events
    /// (CASCADE), in one statement rather than one round trip per issue.
    pub async fn delete_by_filter(
        pool: &DbPool,
        project_id: i32,
        filter: IssueFilter,
    ) -> AppResult<u64> {
        // `filter_clause` returns fixed SQL with the project id written in.
        // Bulk deletes act on the error list, never on feedback.
        let clause = filter_clause(
            filter,
            IssueCategory::Error,
            &ProjectScope::Only(vec![project_id]),
        );

        #[cfg(feature = "postgres")]
        let deleted = {
            let query = format!(
                "WITH deleted AS (
                    DELETE FROM issues WHERE {clause}
                    RETURNING stored_event_count, digested_event_count
                ), totals AS (
                    SELECT COUNT(*) AS issues,
                           COALESCE(SUM(stored_event_count), 0) AS stored,
                           COALESCE(SUM(digested_event_count), 0) AS digested
                    FROM deleted
                ), updated AS (
                    UPDATE projects SET
                        stored_event_count   = GREATEST(0, projects.stored_event_count   - totals.stored),
                        digested_event_count = GREATEST(0, projects.digested_event_count - totals.digested)
                    FROM totals
                    WHERE projects.id = $1
                )
                -- Count from `totals`, not the UPDATE: an admin can target a
                -- project id with no row, which must be 0 deleted, not an error.
                SELECT issues FROM totals"
            );
            let issues: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(&*query))
                .bind(project_id)
                .fetch_one(pool)
                .await?;
            issues as u64
        };

        #[cfg(feature = "sqlite")]
        let deleted = {
            // Same reason as `delete`: read-then-write needs BEGIN IMMEDIATE.
            let mut tx = crate::db::begin_write(pool).await?;
            let totals = format!(
                "SELECT COALESCE(SUM(stored_event_count), 0), COALESCE(SUM(digested_event_count), 0)
                 FROM issues WHERE {clause}"
            );
            let (stored, digested): (i64, i64) = sqlx::query_as(sqlx::AssertSqlSafe(&*totals))
                .fetch_one(&mut *tx)
                .await?;
            let delete = format!("DELETE FROM issues WHERE {clause}");
            let deleted = sqlx::query(sqlx::AssertSqlSafe(&*delete))
                .execute(&mut *tx)
                .await?
                .rows_affected();
            sqlx::query(
                "UPDATE projects SET
                    stored_event_count    = MAX(0, stored_event_count    - $1),
                    digested_event_count  = MAX(0, digested_event_count  - $2)
                WHERE id = $3",
            )
            .bind(stored)
            .bind(digested)
            .bind(project_id)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            deleted
        };

        Ok(deleted)
    }

    /// Aggregates the distinct values (with counts) for a single tag key across
    /// an issue's recent events. Dialect-safe: reads each event's `tags` and
    /// counts them in Rust.
    pub async fn tag_values(
        pool: &DbPool,
        issue_id: Uuid,
        key: &str,
    ) -> AppResult<Vec<IssueTagValue>> {
        // Only the member, never the whole payload: an event can be megabytes.
        let sql = format!(
            "SELECT {}, timestamp FROM events WHERE issue_id = $1 ORDER BY digested_at DESC LIMIT $2",
            json_member("data", &["tags"])
        );
        let rows: Vec<(Option<serde_json::Value>, DateTime<Utc>)> =
            sqlx::query_as(sqlx::AssertSqlSafe(sql))
                .bind(issue_id)
                .bind(AGGREGATE_SCAN_CAP)
                .fetch_all(pool)
                .await?;

        struct Acc {
            count: i64,
            first_seen: DateTime<Utc>,
            last_seen: DateTime<Utc>,
        }
        let mut acc: HashMap<String, Acc> = HashMap::new();
        for (tags, timestamp) in &rows {
            for (k, v) in extract_tags(tags.as_ref()) {
                if k != key {
                    continue;
                }
                acc.entry(v)
                    .and_modify(|a| {
                        a.count += 1;
                        a.first_seen = a.first_seen.min(*timestamp);
                        a.last_seen = a.last_seen.max(*timestamp);
                    })
                    .or_insert(Acc {
                        count: 1,
                        first_seen: *timestamp,
                        last_seen: *timestamp,
                    });
            }
        }

        let mut values: Vec<IssueTagValue> = acc
            .into_iter()
            .map(|(value, a)| IssueTagValue {
                key: key.to_string(),
                name: key.to_string(),
                value,
                count: a.count,
                first_seen: a.first_seen,
                last_seen: a.last_seen,
            })
            .collect();
        values.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.value.cmp(&b.value)));
        Ok(values)
    }

    /// Computes per-issue aggregates (unique user count + top tags) from a
    /// capped scan of recent events. Dialect-safe.
    ///
    /// The user comes from `data.user`, not `user_identity`: that column is
    /// only filled for events digested since it was added.
    pub async fn aggregates(pool: &DbPool, issue_id: Uuid) -> AppResult<IssueAggregates> {
        let sql = format!(
            "SELECT {}, {} FROM events WHERE issue_id = $1 ORDER BY digested_at DESC LIMIT $2",
            json_member("data", &["tags"]),
            json_member("data", &["user"])
        );
        type Row = (Option<serde_json::Value>, Option<serde_json::Value>);
        let rows: Vec<Row> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(issue_id)
            .bind(AGGREGATE_SCAN_CAP)
            .fetch_all(pool)
            .await?;

        let mut tag_counts: HashMap<String, HashMap<String, i64>> = HashMap::new();
        let mut users: std::collections::HashSet<String> = std::collections::HashSet::new();

        for (tags, user) in &rows {
            for (k, v) in extract_tags(tags.as_ref()) {
                *tag_counts.entry(k).or_default().entry(v).or_insert(0) += 1;
            }
            if let Some(id) = user.as_ref().and_then(user_identity_from_user_field) {
                users.insert(id);
            }
        }

        let mut tags: Vec<TagSummary> = tag_counts
            .into_iter()
            .map(|(key, values)| {
                let total_values = values.values().sum::<i64>().max(0) as usize;
                let mut top_values = sort_counts(values);
                top_values.truncate(10);
                TagSummary {
                    key,
                    total_values,
                    top_values,
                }
            })
            .collect();
        tags.sort_by(|a, b| a.key.cmp(&b.key));

        Ok(IssueAggregates {
            user_count: users.len() as i64,
            tags,
        })
    }

    /// Event counts of each issue in the last `segments` buckets of `interval`
    /// seconds, oldest first, as `[bucket_start_unix, count]`: Sentry's issue
    /// stats series, whose buckets are aligned to the epoch
    /// (`tsdb/base.py::normalize_to_epoch`) and end at the current one.
    pub async fn event_series(
        pool: &DbPool,
        issue_ids: &[Uuid],
        segments: i64,
        interval: i64,
        environment: Option<&str>,
    ) -> AppResult<HashMap<Uuid, Vec<[i64; 2]>>> {
        let last = Utc::now().timestamp() / interval * interval;
        let first = last - (segments - 1) * interval;
        let first_dt = DateTime::<Utc>::from_timestamp(first, 0).unwrap_or_else(Utc::now);

        let mut out: HashMap<Uuid, Vec<[i64; 2]>> = issue_ids
            .iter()
            .map(|id| {
                let series = (0..segments).map(|i| [first + i * interval, 0]).collect();
                (*id, series)
            })
            .collect();
        if issue_ids.is_empty() {
            return Ok(out);
        }

        let mut qb = QueryBuilder::new(format!(
            "SELECT issue_id, {} AS bucket, COUNT(*) FROM events WHERE issue_id IN (",
            bucket("timestamp", interval)
        ));
        let mut sep = qb.separated(", ");
        for id in issue_ids {
            sep.push_bind(*id);
        }
        qb.push(") AND ");
        push_compare_utc(&mut qb, "timestamp", ">=", first_dt);
        if let Some(env) = environment {
            qb.push(" AND environment = ");
            qb.push_bind(env);
        }
        qb.push(" GROUP BY issue_id, bucket");
        let rows: Vec<(Uuid, i64, i64)> = qb.build_query_as().fetch_all(pool).await?;

        for (issue_id, bucket, count) in rows {
            let idx = (bucket - first) / interval;
            if let Some(slot) = out
                .get_mut(&issue_id)
                .and_then(|series| series.get_mut(idx as usize))
            {
                slot[1] = count;
            }
        }
        Ok(out)
    }

    /// Each feedback issue's `contexts.feedback`, from its one event.
    pub async fn feedback_contexts(
        pool: &DbPool,
        issue_ids: &[Uuid],
    ) -> AppResult<HashMap<Uuid, serde_json::Value>> {
        if issue_ids.is_empty() {
            return Ok(HashMap::new());
        }
        // Only the context, not the whole event: a feedback event is small,
        // but this runs once per page of up to 100.
        let mut qb = QueryBuilder::new(format!(
            "SELECT issue_id, {} FROM events WHERE issue_id IN (",
            json_member("data", &["contexts", "feedback"])
        ));
        let mut sep = qb.separated(", ");
        for id in issue_ids {
            sep.push_bind(*id);
        }
        qb.push(")");
        let rows: Vec<(Uuid, Option<serde_json::Value>)> =
            qb.build_query_as().fetch_all(pool).await?;

        Ok(rows
            .into_iter()
            .filter_map(|(id, context)| Some((id, context?)))
            .collect())
    }

    /// Bulk-computes [`IssueListStats`] (user_count + 24h trend) for the given
    /// issues in two aggregate queries over the last 24h, so the issue list
    /// shows per-row Users/Trend columns in one request.
    ///
    /// Both are answered from `idx_events_issue_ingested` alone: the user is
    /// counted by its `user_identity` column, never by reading the payload,
    /// and the buckets are counted in SQL. Reading `data` here cost seconds
    /// per page on a busy project.
    pub async fn list_stats(
        pool: &DbPool,
        issue_ids: &[Uuid],
        scope: Scope<'_>,
    ) -> AppResult<HashMap<Uuid, IssueListStats>> {
        if issue_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let now = Utc::now().timestamp();
        let start = now - LIST_TREND_BUCKET_SECS * LIST_TREND_BUCKETS;
        let start_dt = DateTime::<Utc>::from_timestamp(start, 0).unwrap_or_else(Utc::now);

        let scoped = |select: String, group_by: &str| {
            let mut qb = QueryBuilder::<Db>::new(select);
            qb.push(" FROM events WHERE issue_id IN (");
            let mut sep = qb.separated(", ");
            for id in issue_ids {
                sep.push_bind(*id);
            }
            qb.push(") AND ");
            push_compare_utc(&mut qb, "ingested_at", ">=", start_dt);
            if let Some(env) = scope.environment {
                qb.push(" AND environment = ");
                qb.push_bind(env);
            }
            if let Some(release) = scope.release {
                qb.push(" AND release = ");
                qb.push_bind(release);
            }
            qb.push(group_by);
            qb
        };
        // `start` is computed, never request text.
        let buckets: Vec<(Uuid, i64, i64)> = scoped(
            format!(
                "SELECT issue_id, ({} - {start}) / {LIST_TREND_BUCKET_SECS}, COUNT(*)",
                epoch("ingested_at")
            ),
            " GROUP BY 1, 2",
        )
        .build_query_as()
        .fetch_all(pool)
        .await?;
        let users: Vec<(Uuid, i64)> = scoped(
            "SELECT issue_id, COUNT(DISTINCT user_identity)".to_string(),
            " GROUP BY 1",
        )
        .build_query_as()
        .fetch_all(pool)
        .await?;

        let mut out: HashMap<Uuid, IssueListStats> = issue_ids
            .iter()
            .map(|id| {
                (
                    *id,
                    IssueListStats {
                        user_count: 0,
                        trend: vec![0i64; LIST_TREND_BUCKETS as usize],
                    },
                )
            })
            .collect();
        for (issue_id, raw, count) in buckets {
            if let (Some(stats), true) = (out.get_mut(&issue_id), raw >= 0) {
                // Fold the exclusive upper edge (ts == now) into the last bucket.
                let idx = (raw as usize).min(stats.trend.len() - 1);
                stats.trend[idx] += count;
            }
        }
        for (issue_id, count) in users {
            if let Some(stats) = out.get_mut(&issue_id) {
                stats.user_count = count;
            }
        }

        Ok(out)
    }

    /// Computes a zero-filled event-count timeseries for an issue.
    ///
    /// Returns `buckets` points of `(bucket_start_unix, count)` covering
    /// `[now - bucket_secs*buckets, now]`, counted in SQL from
    /// `idx_events_issue_ingested` alone, as [`Self::list_stats`] does. The
    /// whole window, not a row cap: this is an exact series.
    pub async fn stats(
        pool: &DbPool,
        issue_id: Uuid,
        bucket_secs: i64,
        buckets: i64,
    ) -> AppResult<Vec<(i64, i64)>> {
        let now = Utc::now().timestamp();
        let start = now - bucket_secs * buckets;
        let start_dt = DateTime::<Utc>::from_timestamp(start, 0).unwrap_or_else(Utc::now);

        // `start` and `bucket_secs` are computed integers, never request text.
        let sql = format!(
            "SELECT ({} - {start}) / {bucket_secs}, COUNT(*) FROM events \
             WHERE issue_id = $1 AND {} GROUP BY 1",
            epoch("ingested_at"),
            compare_utc("ingested_at", ">=", 2)
        );
        let rows: Vec<(i64, i64)> = sqlx::query_as(sqlx::AssertSqlSafe(&*sql))
            .bind(issue_id)
            .bind(start_dt)
            .fetch_all(pool)
            .await?;

        let mut series: Vec<(i64, i64)> =
            (0..buckets).map(|i| (start + i * bucket_secs, 0)).collect();
        for (raw, count) in rows {
            if raw < 0 {
                continue; // before the window
            }
            // Fold the exclusive upper edge (ts == now) into the last bucket.
            let idx = (raw as usize).min(series.len() - 1);
            series[idx].1 += count;
        }
        Ok(series)
    }

    /// Hard-deletes an issue and all associated events and groupings (via
    /// CASCADE). On SQLite each write takes `errors`' write turn, the queue
    /// the digests wait in for the one writer.
    #[cfg_attr(feature = "postgres", allow(unused_variables))]
    pub async fn delete(
        pool: &DbPool,
        id: Uuid,
        errors: &crate::digest::processors::ErrorProcessor,
    ) -> AppResult<()> {
        #[cfg(feature = "postgres")]
        {
            let result = sqlx::query(
                "WITH deleted AS (
                    DELETE FROM issues WHERE id = $1
                    RETURNING project_id, stored_event_count, digested_event_count
                )
                UPDATE projects SET
                    stored_event_count    = GREATEST(0, projects.stored_event_count    - deleted.stored_event_count),
                    digested_event_count  = GREATEST(0, projects.digested_event_count  - deleted.digested_event_count)
                FROM deleted
                WHERE projects.id = deleted.project_id",
            )
            .bind(id)
            .execute(pool)
            .await?;

            if result.rows_affected() == 0 {
                return Err(AppError::NotFound(format!("Issue {} not found", id)));
            }
        }

        #[cfg(feature = "sqlite")]
        {
            // The events first, a batch per transaction, so a large issue
            // never holds the one write lock long enough to stall the digest.
            // Each batch takes the digests' write turn and takes its rows off
            // the stored counts in the same transaction, so a delete that
            // stops part-way leaves counts matching the rows left.
            loop {
                let turn = errors.write_turn().await;
                let mut tx = crate::db::begin_write(pool).await?;
                let deleted = sqlx::query(
                    "DELETE FROM events WHERE rowid IN \
                     (SELECT rowid FROM events WHERE issue_id = $1 LIMIT $2)",
                )
                .bind(id)
                .bind(ISSUE_DELETE_BATCH)
                .execute(&mut *tx)
                .await?
                .rows_affected() as i64;
                sqlx::query(
                    "UPDATE issues SET stored_event_count = MAX(0, stored_event_count - $1) \
                     WHERE id = $2",
                )
                .bind(deleted)
                .bind(id)
                .execute(&mut *tx)
                .await?;
                sqlx::query(
                    "UPDATE projects SET stored_event_count = MAX(0, stored_event_count - $1) \
                     WHERE id = (SELECT project_id FROM issues WHERE id = $2)",
                )
                .bind(deleted)
                .bind(id)
                .execute(&mut *tx)
                .await?;
                tx.commit().await?;
                drop(turn);
                if deleted < ISSUE_DELETE_BATCH {
                    break;
                }
                crate::db::maintenance::yield_to_writers().await;
            }

            let _turn = errors.write_turn().await;
            // BEGIN IMMEDIATE: this tx reads counters before it writes, so a
            // deferred BEGIN could hit SQLITE_BUSY_SNAPSHOT on the upgrade.
            let mut tx = crate::db::begin_write(pool).await?;

            let row = sqlx::query_as::<_, (i32, i32, i32)>(
                "SELECT project_id, stored_event_count, digested_event_count FROM issues WHERE id = $1",
            )
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;

            let (project_id, stored, digested) = match row {
                Some(r) => r,
                None => return Err(AppError::NotFound(format!("Issue {} not found", id))),
            };

            let delete_result = sqlx::query("DELETE FROM issues WHERE id = $1")
                .bind(id)
                .execute(&mut *tx)
                .await?;

            if delete_result.rows_affected() == 0 {
                return Err(AppError::NotFound(format!("Issue {} not found", id)));
            }

            sqlx::query(
                "UPDATE projects SET
                    stored_event_count    = MAX(0, stored_event_count    - $1),
                    digested_event_count  = MAX(0, digested_event_count  - $2)
                WHERE id = $3",
            )
            .bind(stored)
            .bind(digested)
            .bind(project_id)
            .execute(&mut *tx)
            .await?;

            tx.commit().await?;
        }

        Ok(())
    }
}

/// An issue's columns with first/last seen and count taken from its row in
/// `issue_environments` (`e`) instead of its own (`i`).
const ENV_SCOPED_COLUMNS: &str = "i.id, i.project_id, i.digest_order, i.numeric_id, \
    e.first_seen, e.last_seen, e.event_count AS digested_event_count, i.stored_event_count, \
    i.calculated_type, i.calculated_value, i.\"transaction\", i.last_frame_filename, \
    i.last_frame_module, i.last_frame_function, i.level, i.platform, i.status, i.substatus, \
    i.priority, i.priority_locked_at, i.culprit, i.logger, i.status_details, i.assigned_to, \
    i.assignee_type, i.issue_type, i.issue_category, i.first_release, i.last_release";

/// The `AND ...` for the table's own filters. Text values are bound, in
/// order after whatever `binds` already holds; numbers are inlined, which is
/// safe because an `i32`/`i64` can only render as digits and a sign.
fn narrowing_clause(filters: &IssueFilters, binds: &mut Vec<String>) -> String {
    let mut clause = String::new();

    // An issue stored without a level is an error, as Sentry reads an event
    // that sent none and as the list shows it: the filter has to agree.
    for (column, values) in [
        ("COALESCE(level, 'error')", &filters.levels),
        ("priority", &filters.priorities),
    ] {
        if values.is_empty() {
            continue;
        }
        let placeholders: Vec<String> = values
            .iter()
            .map(|value| {
                binds.push(value.clone());
                format!("${}", binds.len())
            })
            .collect();
        clause.push_str(&format!(" AND {column} IN ({})", placeholders.join(", ")));
    }

    match filters.assigned {
        Some(Assignee::Nobody) => clause.push_str(" AND assigned_to IS NULL"),
        Some(Assignee::User(id)) => clause.push_str(&format!(" AND assigned_to = {id}")),
        None => {}
    }

    for (column, hours) in [
        ("first_seen", filters.first_seen_hours),
        ("last_seen", filters.last_seen_hours),
    ] {
        if hours.is_some() {
            clause.push_str(&format!(" AND {}", since(column, hours)));
        }
    }

    if let Some(min) = filters.events_min {
        clause.push_str(&format!(" AND digested_event_count >= {min}"));
    }
    if let Some(max) = filters.events_max {
        clause.push_str(&format!(" AND digested_event_count <= {max}"));
    }

    clause
}

/// `column` within `projects`. The ids are written into the SQL, not bound:
/// they are integers the server resolved from the reader's access, and a
/// literal keeps the `(project_id, ...)` indexes usable on both backends.
fn project_clause(column: &str, projects: &ProjectScope) -> String {
    match projects {
        ProjectScope::All => "1 = 1".to_string(),
        ProjectScope::Only(ids) if ids.is_empty() => "1 = 0".to_string(),
        ProjectScope::Only(ids) => {
            let ids: Vec<String> = ids.iter().map(i32::to_string).collect();
            format!("{column} IN ({})", ids.join(", "))
        }
    }
}

/// WHERE clause for an issue filter within `projects`.
fn filter_clause(filter: IssueFilter, category: IssueCategory, projects: &ProjectScope) -> String {
    let status = match filter {
        IssueFilter::Open => " AND status = 'unresolved'",
        IssueFilter::Resolved => " AND status = 'resolved'",
        IssueFilter::Muted => " AND status = 'ignored'",
        IssueFilter::All => "",
    };
    format!(
        "{}{status} AND issue_category = '{}'",
        project_clause("project_id", projects),
        category.as_str()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_priority_fatal_is_high() {
        assert_eq!(derive_priority(Some("fatal")), "high");
    }

    #[test]
    fn test_derive_priority_error_is_high() {
        assert_eq!(derive_priority(Some("error")), "high");
    }

    #[test]
    fn test_derive_priority_warning_is_medium() {
        assert_eq!(derive_priority(Some("warning")), "medium");
    }

    #[test]
    fn test_derive_priority_info_is_low() {
        assert_eq!(derive_priority(Some("info")), "low");
    }

    #[test]
    fn test_derive_priority_debug_is_low() {
        assert_eq!(derive_priority(Some("debug")), "low");
    }

    #[test]
    fn test_derive_priority_missing_level_is_medium() {
        // Real Sentry's `_get_priority_for_group` (event_manager.py:2099-2134)
        // falls through to PriorityLevel.MEDIUM when the level is absent —
        // not HIGH.
        assert_eq!(derive_priority(None), "medium");
    }

    #[test]
    fn test_derive_priority_unrecognized_level_is_medium() {
        // Same fallthrough as the missing-level case, for a level string
        // Sentry doesn't recognize either.
        assert_eq!(derive_priority(Some("critical")), "medium");
    }

    #[test]
    fn test_user_identity_from_user_field_prefers_id() {
        let user = serde_json::json!({"id": "u1", "email": "a@b.com"});
        assert_eq!(
            user_identity_from_user_field(&user),
            Some("id:u1".to_string())
        );
    }

    #[test]
    fn test_user_identity_from_user_field_falls_back_to_email() {
        let user = serde_json::json!({"email": "a@b.com"});
        assert_eq!(
            user_identity_from_user_field(&user),
            Some("email:a@b.com".to_string())
        );
    }

    #[test]
    fn test_user_identity_from_user_field_follows_sentry_order() {
        let user =
            serde_json::json!({"email": "a@b.com", "username": "bob", "ip_address": "1.2.3.4"});
        assert_eq!(
            user_identity_from_user_field(&user),
            Some("username:bob".to_string())
        );
        let user = serde_json::json!({"ip_address": "1.2.3.4"});
        assert_eq!(
            user_identity_from_user_field(&user),
            Some("ip:1.2.3.4".to_string())
        );
    }

    #[test]
    fn test_user_identity_from_user_field_skips_empty_strings() {
        let user = serde_json::json!({"id": "", "username": "bob"});
        assert_eq!(
            user_identity_from_user_field(&user),
            Some("username:bob".to_string())
        );
    }

    #[test]
    fn test_user_identity_from_user_field_none_when_no_recognized_field() {
        let user = serde_json::json!({"segment": "beta"});
        assert_eq!(user_identity_from_user_field(&user), None);
    }
}
