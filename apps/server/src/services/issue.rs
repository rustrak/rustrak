use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::db::{Db, DbPool};
use crate::error::{AppError, AppResult};
use crate::models::{Grouping, Issue};
use crate::pagination::{IssueCursor, IssueFilter, IssueSort, SortOrder};
use crate::services::grouping::DenormalizedFields;
use serde::Serialize;
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

/// Per-issue supplementary stats for the issue list (unique user count + a
/// compact 24h hourly trend), computed from a single capped scan per issue.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct IssueListStats {
    pub user_count: i64,
    /// 24 hourly buckets covering the last 24h, oldest to newest.
    pub trend: Vec<i64>,
}

/// Hourly buckets in the issue-list trend column (Sentry's default "24h" window).
const LIST_TREND_BUCKETS: i64 = 24;
const LIST_TREND_BUCKET_SECS: i64 = 3600;

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
fn extract_tags(data: &serde_json::Value) -> Vec<(String, String)> {
    let Some(tags) = data.get("tags") else {
        return Vec::new();
    };
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

/// Extracts a stable user identity (id, else email, else ip_address) for unique
/// user counting.
fn extract_user_identity(data: &serde_json::Value) -> Option<String> {
    user_identity_from_user_field(data.get("user")?)
}

/// Extracts a stable user identity string from an already-extracted `user`
/// sub-object (as opposed to a full event `data` blob) — split out so
/// `list_stats`'s SQL can project just `data->'user'` instead of hydrating
/// the entire event payload for every event on the issue-list hot path.
pub(crate) fn user_identity_from_user_field(user: &serde_json::Value) -> Option<String> {
    for field in ["id", "email", "username", "ip_address"] {
        if let Some(s) = user.get(field).and_then(|v| v.as_str()) {
            if !s.is_empty() {
                return Some(format!("{}:{}", field, s));
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
    /// Lists issues with cursor-based pagination
    ///
    /// Uses KEYSET pagination for efficient large dataset handling.
    /// Returns (issues, has_more) where has_more indicates if there are more results.
    pub async fn list_paginated(
        pool: &DbPool,
        project_id: i32,
        sort: IssueSort,
        order: SortOrder,
        include_resolved: bool,
        cursor: Option<&IssueCursor>,
        limit: i64,
    ) -> AppResult<(Vec<Issue>, bool)> {
        // Fetch limit+1 to determine if there are more results
        let fetch_limit = limit + 1;

        let issues = match (sort, order, cursor) {
            // digest_order DESC (default) - no cursor
            (IssueSort::DigestOrder, SortOrder::Desc, None) => {
                if include_resolved {
                    sqlx::query_as::<_, Issue>(
                        r#"
                        SELECT * FROM issues
                        WHERE project_id = $1
                        ORDER BY digest_order DESC
                        LIMIT $2
                        "#,
                    )
                    .bind(project_id)
                    .bind(fetch_limit)
                    .fetch_all(pool)
                    .await?
                } else {
                    sqlx::query_as::<_, Issue>(
                        r#"
                        SELECT * FROM issues
                        WHERE project_id = $1 AND status = 'unresolved'
                        ORDER BY digest_order DESC
                        LIMIT $2
                        "#,
                    )
                    .bind(project_id)
                    .bind(fetch_limit)
                    .fetch_all(pool)
                    .await?
                }
            }

            // digest_order DESC - with cursor
            (IssueSort::DigestOrder, SortOrder::Desc, Some(c)) => {
                let last_order = c.last_digest_order.unwrap_or(i32::MAX);
                if include_resolved {
                    sqlx::query_as::<_, Issue>(
                        r#"
                        SELECT * FROM issues
                        WHERE project_id = $1
                          AND digest_order < $3
                        ORDER BY digest_order DESC
                        LIMIT $2
                        "#,
                    )
                    .bind(project_id)
                    .bind(fetch_limit)
                    .bind(last_order)
                    .fetch_all(pool)
                    .await?
                } else {
                    sqlx::query_as::<_, Issue>(
                        r#"
                        SELECT * FROM issues
                        WHERE project_id = $1 AND status = 'unresolved'
                          AND digest_order < $3
                        ORDER BY digest_order DESC
                        LIMIT $2
                        "#,
                    )
                    .bind(project_id)
                    .bind(fetch_limit)
                    .bind(last_order)
                    .fetch_all(pool)
                    .await?
                }
            }

            // digest_order ASC - no cursor
            (IssueSort::DigestOrder, SortOrder::Asc, None) => {
                if include_resolved {
                    sqlx::query_as::<_, Issue>(
                        r#"
                        SELECT * FROM issues
                        WHERE project_id = $1
                        ORDER BY digest_order ASC
                        LIMIT $2
                        "#,
                    )
                    .bind(project_id)
                    .bind(fetch_limit)
                    .fetch_all(pool)
                    .await?
                } else {
                    sqlx::query_as::<_, Issue>(
                        r#"
                        SELECT * FROM issues
                        WHERE project_id = $1 AND status = 'unresolved'
                        ORDER BY digest_order ASC
                        LIMIT $2
                        "#,
                    )
                    .bind(project_id)
                    .bind(fetch_limit)
                    .fetch_all(pool)
                    .await?
                }
            }

            // digest_order ASC - with cursor
            (IssueSort::DigestOrder, SortOrder::Asc, Some(c)) => {
                let last_order = c.last_digest_order.unwrap_or(0);
                if include_resolved {
                    sqlx::query_as::<_, Issue>(
                        r#"
                        SELECT * FROM issues
                        WHERE project_id = $1
                          AND digest_order > $3
                        ORDER BY digest_order ASC
                        LIMIT $2
                        "#,
                    )
                    .bind(project_id)
                    .bind(fetch_limit)
                    .bind(last_order)
                    .fetch_all(pool)
                    .await?
                } else {
                    sqlx::query_as::<_, Issue>(
                        r#"
                        SELECT * FROM issues
                        WHERE project_id = $1 AND status = 'unresolved'
                          AND digest_order > $3
                        ORDER BY digest_order ASC
                        LIMIT $2
                        "#,
                    )
                    .bind(project_id)
                    .bind(fetch_limit)
                    .bind(last_order)
                    .fetch_all(pool)
                    .await?
                }
            }

            // last_seen DESC - no cursor
            (IssueSort::LastSeen, SortOrder::Desc, None) => {
                if include_resolved {
                    sqlx::query_as::<_, Issue>(
                        r#"
                        SELECT * FROM issues
                        WHERE project_id = $1
                        ORDER BY last_seen DESC, id DESC
                        LIMIT $2
                        "#,
                    )
                    .bind(project_id)
                    .bind(fetch_limit)
                    .fetch_all(pool)
                    .await?
                } else {
                    sqlx::query_as::<_, Issue>(
                        r#"
                        SELECT * FROM issues
                        WHERE project_id = $1 AND status = 'unresolved'
                        ORDER BY last_seen DESC, id DESC
                        LIMIT $2
                        "#,
                    )
                    .bind(project_id)
                    .bind(fetch_limit)
                    .fetch_all(pool)
                    .await?
                }
            }

            // last_seen DESC - with cursor
            (IssueSort::LastSeen, SortOrder::Desc, Some(c)) => {
                let last_seen = c.last_seen.unwrap_or_else(Utc::now);
                let last_id = c.last_id.unwrap_or(Uuid::nil());
                if include_resolved {
                    sqlx::query_as::<_, Issue>(
                        r#"
                        SELECT * FROM issues
                        WHERE project_id = $1
                          AND (last_seen < $3 OR (last_seen = $3 AND id < $4))
                        ORDER BY last_seen DESC, id DESC
                        LIMIT $2
                        "#,
                    )
                    .bind(project_id)
                    .bind(fetch_limit)
                    .bind(last_seen)
                    .bind(last_id)
                    .fetch_all(pool)
                    .await?
                } else {
                    sqlx::query_as::<_, Issue>(
                        r#"
                        SELECT * FROM issues
                        WHERE project_id = $1 AND status = 'unresolved'
                          AND (last_seen < $3 OR (last_seen = $3 AND id < $4))
                        ORDER BY last_seen DESC, id DESC
                        LIMIT $2
                        "#,
                    )
                    .bind(project_id)
                    .bind(fetch_limit)
                    .bind(last_seen)
                    .bind(last_id)
                    .fetch_all(pool)
                    .await?
                }
            }

            // last_seen ASC - no cursor
            (IssueSort::LastSeen, SortOrder::Asc, None) => {
                if include_resolved {
                    sqlx::query_as::<_, Issue>(
                        r#"
                        SELECT * FROM issues
                        WHERE project_id = $1
                        ORDER BY last_seen ASC, id ASC
                        LIMIT $2
                        "#,
                    )
                    .bind(project_id)
                    .bind(fetch_limit)
                    .fetch_all(pool)
                    .await?
                } else {
                    sqlx::query_as::<_, Issue>(
                        r#"
                        SELECT * FROM issues
                        WHERE project_id = $1 AND status = 'unresolved'
                        ORDER BY last_seen ASC, id ASC
                        LIMIT $2
                        "#,
                    )
                    .bind(project_id)
                    .bind(fetch_limit)
                    .fetch_all(pool)
                    .await?
                }
            }

            // last_seen ASC - with cursor
            (IssueSort::LastSeen, SortOrder::Asc, Some(c)) => {
                let last_seen = c.last_seen.unwrap_or(DateTime::UNIX_EPOCH);
                let last_id = c.last_id.unwrap_or(Uuid::nil());
                if include_resolved {
                    sqlx::query_as::<_, Issue>(
                        r#"
                        SELECT * FROM issues
                        WHERE project_id = $1
                          AND (last_seen > $3 OR (last_seen = $3 AND id > $4))
                        ORDER BY last_seen ASC, id ASC
                        LIMIT $2
                        "#,
                    )
                    .bind(project_id)
                    .bind(fetch_limit)
                    .bind(last_seen)
                    .bind(last_id)
                    .fetch_all(pool)
                    .await?
                } else {
                    sqlx::query_as::<_, Issue>(
                        r#"
                        SELECT * FROM issues
                        WHERE project_id = $1 AND status = 'unresolved'
                          AND (last_seen > $3 OR (last_seen = $3 AND id > $4))
                        ORDER BY last_seen ASC, id ASC
                        LIMIT $2
                        "#,
                    )
                    .bind(project_id)
                    .bind(fetch_limit)
                    .bind(last_seen)
                    .bind(last_id)
                    .fetch_all(pool)
                    .await?
                }
            }

            // list_paginated has no callers (superseded by list_offset, GH #165).
            // IssueCursor has no event-count field, so cursor-based EventCount
            // pagination isn't implemented — the live sort=event_count path is
            // IssueService::list_offset.
            (IssueSort::EventCount, _, _) => {
                return Err(AppError::Validation(
                    "EventCount sort is not supported for cursor-based pagination".to_string(),
                ));
            }
        };

        let has_more = issues.len() > limit as usize;
        let issues: Vec<Issue> = issues.into_iter().take(limit as usize).collect();

        Ok((issues, has_more))
    }

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
            WHERE project_id = $1 AND first_release = $2
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
        project_id: i32,
        sort: IssueSort,
        order: SortOrder,
        filter: IssueFilter,
        page: i64,
        per_page: i64,
        search: Option<&str>,
    ) -> AppResult<(Vec<Issue>, i64)> {
        if page < 1 {
            return Err(AppError::Validation(format!(
                "page must be >= 1, got {}",
                page
            )));
        }
        if !(1..=100).contains(&per_page) {
            return Err(AppError::Validation(format!(
                "per_page must be between 1 and 100, got {}",
                per_page
            )));
        }
        let offset = (page - 1) * per_page;

        // Build WHERE clause based on filter
        let status_clause = match filter {
            IssueFilter::Open => "project_id = $1 AND status = 'unresolved'",
            IssueFilter::Resolved => "project_id = $1 AND status = 'resolved'",
            IssueFilter::Muted => "project_id = $1 AND status = 'ignored'",
            IssueFilter::All => "project_id = $1",
        };

        // Build ORDER BY clause
        let order_clause = match (sort, order) {
            (IssueSort::DigestOrder, SortOrder::Desc) => "digest_order DESC",
            (IssueSort::DigestOrder, SortOrder::Asc) => "digest_order ASC",
            (IssueSort::LastSeen, SortOrder::Desc) => "last_seen DESC, id DESC",
            (IssueSort::LastSeen, SortOrder::Asc) => "last_seen ASC, id ASC",
            (IssueSort::EventCount, SortOrder::Desc) => "digested_event_count DESC, id DESC",
            (IssueSort::EventCount, SortOrder::Asc) => "digested_event_count ASC, id ASC",
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

        if let Some(pattern) = pattern {
            // $2 = search pattern, applied to both count and select.
            let search_clause = r#"AND (
                LOWER(calculated_type) LIKE $2 ESCAPE '\'
                OR LOWER(calculated_value) LIKE $2 ESCAPE '\'
                OR LOWER("transaction") LIKE $2 ESCAPE '\'
                OR LOWER(culprit) LIKE $2 ESCAPE '\'
                OR LOWER(last_frame_filename) LIKE $2 ESCAPE '\'
                OR LOWER(last_frame_module) LIKE $2 ESCAPE '\'
            )"#;
            let count_query = format!(
                "SELECT COUNT(*) FROM issues WHERE {} {}",
                status_clause, search_clause
            );
            let total_count: (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(&*count_query))
                .bind(project_id)
                .bind(&pattern)
                .fetch_one(pool)
                .await?;

            let select_query = format!(
                "SELECT * FROM issues WHERE {} {} ORDER BY {} LIMIT $3 OFFSET $4",
                status_clause, search_clause, order_clause
            );
            let issues = sqlx::query_as::<_, Issue>(sqlx::AssertSqlSafe(&*select_query))
                .bind(project_id)
                .bind(&pattern)
                .bind(per_page)
                .bind(offset)
                .fetch_all(pool)
                .await?;

            return Ok((issues, total_count.0));
        }

        // Get total count
        let count_query = format!("SELECT COUNT(*) FROM issues WHERE {}", status_clause);
        let total_count: (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(&*count_query))
            .bind(project_id)
            .fetch_one(pool)
            .await?;

        // Get paginated results
        let select_query = format!(
            "SELECT * FROM issues WHERE {} ORDER BY {} LIMIT $2 OFFSET $3",
            status_clause, order_clause
        );
        let issues = sqlx::query_as::<_, Issue>(sqlx::AssertSqlSafe(&*select_query))
            .bind(project_id)
            .bind(per_page)
            .bind(offset)
            .fetch_all(pool)
            .await?;

        Ok((issues, total_count.0))
    }

    /// Gets an issue by ID
    pub async fn get_by_id(pool: &DbPool, id: Uuid) -> AppResult<Issue> {
        let issue = sqlx::query_as::<_, Issue>("SELECT * FROM issues WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Issue {} not found", id)))?;

        Ok(issue)
    }

    /// Creates a new issue
    pub async fn create(
        pool: &DbPool,
        project_id: i32,
        timestamp: DateTime<Utc>,
        denormalized: &DenormalizedFields,
        level: Option<&str>,
        platform: Option<&str>,
    ) -> AppResult<Issue> {
        // Get the next digest_order for this project
        let max_order: Option<i32> =
            sqlx::query_scalar("SELECT MAX(digest_order) FROM issues WHERE project_id = $1")
                .bind(project_id)
                .fetch_one(pool)
                .await?;

        let digest_order = max_order.unwrap_or(0) + 1;

        // Generate UUID in application for cross-DB compatibility
        let issue_id = Uuid::new_v4();

        let priority = derive_priority(level);

        let issue = sqlx::query_as::<_, Issue>(
            r#"
            INSERT INTO issues (
                id, project_id, digest_order, first_seen, last_seen,
                digested_event_count, stored_event_count,
                calculated_type, calculated_value, "transaction",
                last_frame_filename, last_frame_module, last_frame_function,
                level, platform, status, substatus, priority, culprit, logger,
                first_release, last_release
            )
            VALUES (
                $1, $2, $3, $4, $4, 1, 1, $5, $6, $7, $8, $9, $10, $11, $12,
                'unresolved', 'new', $13, $14, $15, $16, $16
            )
            RETURNING *
            "#,
        )
        .bind(issue_id)
        .bind(project_id)
        .bind(digest_order)
        .bind(timestamp)
        .bind(&denormalized.calculated_type)
        .bind(&denormalized.calculated_value)
        .bind(&denormalized.transaction)
        .bind(&denormalized.last_frame_filename)
        .bind(&denormalized.last_frame_module)
        .bind(&denormalized.last_frame_function)
        .bind(level)
        .bind(platform)
        .bind(priority)
        .bind(&denormalized.culprit)
        .bind(&denormalized.logger)
        .bind(&denormalized.release)
        .fetch_one(pool)
        .await?;

        Ok(issue)
    }

    /// Updates an existing issue for a new event
    pub async fn update_for_new_event(
        pool: &DbPool,
        issue_id: Uuid,
        timestamp: DateTime<Utc>,
    ) -> AppResult<Issue> {
        let issue = sqlx::query_as::<_, Issue>(
            r#"
            UPDATE issues
            SET last_seen = $2,
                digested_event_count = digested_event_count + 1,
                stored_event_count = stored_event_count + 1
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(issue_id)
        .bind(timestamp)
        .fetch_one(pool)
        .await?;

        Ok(issue)
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

    /// Bulk-sets the status of multiple issues in one project.
    /// Returns the number of issues updated.
    ///
    /// Takes an open transaction rather than a pool so callers that also
    /// touch priority in the same request (see `bulk_update_issues`) can
    /// commit both changes atomically.
    pub async fn bulk_set_status(
        tx: &mut sqlx::Transaction<'_, Db>,
        project_id: i32,
        ids: &[Uuid],
        status: &str,
    ) -> AppResult<u64> {
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
            updated += res.rows_affected();
        }
        Ok(updated)
    }

    /// Bulk-sets the priority of multiple issues in one project, in a single
    /// query. Ids that don't exist or belong to another project are silently
    /// not counted, matching [`bulk_set_status`]'s semantics.
    ///
    /// Takes an open transaction — see [`bulk_set_status`] for why.
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

        let now = Utc::now();

        #[cfg(feature = "postgres")]
        {
            let res = sqlx::query(
                "UPDATE issues SET priority = $1, priority_locked_at = $2 \
                 WHERE project_id = $3 AND id = ANY($4)",
            )
            .bind(priority)
            .bind(now)
            .bind(project_id)
            .bind(ids)
            .execute(&mut **tx)
            .await?;
            Ok(res.rows_affected())
        }

        #[cfg(not(feature = "postgres"))]
        {
            use sqlx::QueryBuilder;
            let mut qb = QueryBuilder::new("UPDATE issues SET priority = ");
            qb.push_bind(priority);
            qb.push(", priority_locked_at = ");
            qb.push_bind(now);
            qb.push(" WHERE project_id = ");
            qb.push_bind(project_id);
            qb.push(" AND id IN (");
            let mut sep = qb.separated(", ");
            for id in ids {
                sep.push_bind(*id);
            }
            qb.push(")");
            let res = qb.build().execute(&mut **tx).await?;
            Ok(res.rows_affected())
        }
    }

    /// Bulk "resolve in next release": same semantics as
    /// [`resolve_in_next_release`], applied to many issues in one project.
    /// Returns the number of issues updated.
    ///
    /// Takes an open transaction — see [`bulk_set_status`] for why.
    ///
    /// [`bulk_set_status`]: Self::bulk_set_status
    pub async fn bulk_resolve_in_next_release(
        tx: &mut sqlx::Transaction<'_, Db>,
        project_id: i32,
        ids: &[Uuid],
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
    pub async fn bulk_delete(pool: &DbPool, project_id: i32, ids: &[Uuid]) -> AppResult<u64> {
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
            Self::delete(pool, *id).await?;
            deleted += 1;
        }
        Ok(deleted)
    }

    /// Aggregates the distinct values (with counts) for a single tag key across
    /// an issue's recent events. Dialect-safe: scans event JSON in Rust.
    pub async fn tag_values(
        pool: &DbPool,
        issue_id: Uuid,
        key: &str,
    ) -> AppResult<Vec<IssueTagValue>> {
        let rows: Vec<(serde_json::Value, DateTime<Utc>)> = sqlx::query_as(
            "SELECT data, timestamp FROM events WHERE issue_id = $1 ORDER BY digested_at DESC LIMIT $2",
        )
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
        for (data, timestamp) in &rows {
            for (k, v) in extract_tags(data) {
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
    pub async fn aggregates(pool: &DbPool, issue_id: Uuid) -> AppResult<IssueAggregates> {
        let rows: Vec<(serde_json::Value,)> = sqlx::query_as(
            "SELECT data FROM events WHERE issue_id = $1 ORDER BY digested_at DESC LIMIT $2",
        )
        .bind(issue_id)
        .bind(AGGREGATE_SCAN_CAP)
        .fetch_all(pool)
        .await?;

        let mut tag_counts: HashMap<String, HashMap<String, i64>> = HashMap::new();
        let mut users: std::collections::HashSet<String> = std::collections::HashSet::new();

        for (data,) in &rows {
            for (k, v) in extract_tags(data) {
                *tag_counts.entry(k).or_default().entry(v).or_insert(0) += 1;
            }
            if let Some(id) = extract_user_identity(data) {
                users.insert(id);
            }
        }

        let mut tags: Vec<TagSummary> = tag_counts
            .into_iter()
            .map(|(key, values)| {
                let total_values = values.len();
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

    /// Bulk-computes [`IssueListStats`] (user_count + 24h trend) for the given
    /// issues in a single time-bounded query (all events across all issues
    /// within the last 24h) — used by the issue list so it can show per-row
    /// Users/Trend columns in one request instead of the client firing one
    /// `aggregates`/`stats` call per visible row, and instead of awaiting one
    /// query per issue serially.
    pub async fn list_stats(
        pool: &DbPool,
        issue_ids: &[Uuid],
    ) -> AppResult<HashMap<Uuid, IssueListStats>> {
        if issue_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let now = Utc::now().timestamp();
        let start = now - LIST_TREND_BUCKET_SECS * LIST_TREND_BUCKETS;
        let start_dt = DateTime::<Utc>::from_timestamp(start, 0).unwrap_or_else(Utc::now);

        // Projects only `data->'user'` instead of the full event payload:
        // this runs on every issue-list page load (up to 100 issues,
        // uncapped events per issue within the 24h window), and the only
        // thing read from `data` is the user sub-object — pulling the whole
        // blob (stacktraces, breadcrumbs, contexts, tags, request) across
        // every matching event is needless bytes-on-the-wire and JSON
        // deserialization for a hot path. `aggregates()`/`tag_values()`
        // still fetch the full row since they also read tags from it.
        #[cfg(feature = "postgres")]
        let rows: Vec<(Uuid, Option<serde_json::Value>, DateTime<Utc>)> = sqlx::query_as(
            "SELECT issue_id, data -> 'user', ingested_at FROM events \
             WHERE issue_id = ANY($1) AND ingested_at >= $2",
        )
        .bind(issue_ids)
        .bind(start_dt)
        .fetch_all(pool)
        .await?;

        #[cfg(not(feature = "postgres"))]
        let rows: Vec<(Uuid, Option<serde_json::Value>, DateTime<Utc>)> = {
            use sqlx::QueryBuilder;
            let mut qb = QueryBuilder::new(
                "SELECT issue_id, json_extract(data, '$.user'), ingested_at FROM events WHERE issue_id IN (",
            );
            let mut sep = qb.separated(", ");
            for id in issue_ids {
                sep.push_bind(*id);
            }
            qb.push(") AND datetime(ingested_at) >= datetime(");
            qb.push_bind(start_dt.naive_utc());
            qb.push(")");
            qb.build_query_as().fetch_all(pool).await?
        };

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
        let mut users: HashMap<Uuid, std::collections::HashSet<String>> = HashMap::new();

        for (issue_id, user, ts) in &rows {
            let Some(stats) = out.get_mut(issue_id) else {
                continue;
            };
            if let Some(uid) = user.as_ref().and_then(user_identity_from_user_field) {
                users.entry(*issue_id).or_default().insert(uid);
            }
            let raw = (ts.timestamp() - start) / LIST_TREND_BUCKET_SECS;
            if raw >= 0 {
                let idx = (raw as usize).min(stats.trend.len() - 1);
                stats.trend[idx] += 1;
            }
        }

        for (issue_id, ids) in users {
            if let Some(stats) = out.get_mut(&issue_id) {
                stats.user_count = ids.len() as i64;
            }
        }

        Ok(out)
    }

    /// Computes a zero-filled event-count timeseries for an issue.
    ///
    /// Returns `buckets` points of `(bucket_start_unix, count)` covering
    /// `[now - bucket_secs*buckets, now]`. Bucketed in Rust for dialect safety.
    pub async fn stats(
        pool: &DbPool,
        issue_id: Uuid,
        bucket_secs: i64,
        buckets: i64,
    ) -> AppResult<Vec<(i64, i64)>> {
        let now = Utc::now().timestamp();
        let start = now - bucket_secs * buckets;
        let start_dt = DateTime::<Utc>::from_timestamp(start, 0).unwrap_or_else(Utc::now);

        // Bucket entirely in Rust (dialect-safe), but scope the fetch to the
        // requested window in SQL rather than capping by row count: this is
        // an exact timeseries, not the approximate tag/user aggregates
        // AGGREGATE_SCAN_CAP was designed for, so a row cap would silently
        // truncate older buckets to zero for high-volume issues. Filtering by
        // `ingested_at` needs a dialect-specific comparison — SQLite stores
        // it as TEXT, so `datetime(...)` normalizes both sides before
        // comparing.
        #[cfg(feature = "postgres")]
        let rows: Vec<(DateTime<Utc>,)> = sqlx::query_as(
            "SELECT ingested_at FROM events WHERE issue_id = $1 AND ingested_at >= $2 ORDER BY ingested_at DESC",
        )
        .bind(issue_id)
        .bind(start_dt)
        .fetch_all(pool)
        .await?;

        #[cfg(not(feature = "postgres"))]
        let rows: Vec<(DateTime<Utc>,)> = sqlx::query_as(
            "SELECT ingested_at FROM events WHERE issue_id = $1 AND datetime(ingested_at) >= datetime($2) ORDER BY ingested_at DESC",
        )
        .bind(issue_id)
        .bind(start_dt.naive_utc())
        .fetch_all(pool)
        .await?;

        let mut series: Vec<(i64, i64)> =
            (0..buckets).map(|i| (start + i * bucket_secs, 0)).collect();

        for (ts,) in rows {
            let raw = (ts.timestamp() - start) / bucket_secs;
            if raw < 0 {
                continue; // before the window
            }
            // Fold the exclusive upper edge (ts == now) into the last bucket.
            let idx = (raw as usize).min(series.len() - 1);
            series[idx].1 += 1;
        }
        Ok(series)
    }

    /// Hard-deletes an issue and all associated events and groupings (via CASCADE)
    pub async fn delete(pool: &DbPool, id: Uuid) -> AppResult<()> {
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

    #[test]
    fn test_extract_user_identity_still_reads_from_full_event_data() {
        // `extract_user_identity` (used by `aggregates()`, which needs the
        // full event blob anyway for tags) must keep unwrapping `data.user`
        // itself — only `list_stats`'s SQL-level projection changes.
        let data = serde_json::json!({
            "user": {"id": "u1"},
            "exception": {"values": [{"type": "TypeError"}]},
        });
        assert_eq!(extract_user_identity(&data), Some("id:u1".to_string()));
    }
}
