use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::db::DbPool;
use crate::digest::processors::{clamp, extract_timestamp, relay_level};
use crate::error::{AppError, AppResult};
use crate::models::{AlertType, Event, EventSummary};
use crate::pagination::{EventCursor, SortOrder};
use crate::services::grouping::DenormalizedFields;

/// Columns the event list renders — deliberately not `SELECT *`: the full
/// `data` JSON blob is parsed per row by the `Event` mapping and then
/// discarded by the list response, the largest avoidable allocation in the
/// read path. Detail views load the payload via `get_by_id` instead.
const EVENT_LIST_COLUMNS: &str = "id, event_id, issue_id, timestamp, \
     calculated_type, calculated_value, level, platform, release, environment, event_type";

pub struct EventService;

impl EventService {
    /// Lists events with cursor-based pagination
    ///
    /// Uses KEYSET pagination on `(timestamp, id)` for efficient large dataset
    /// handling — `timestamp` is not unique within an issue (a burst of events
    /// can share it), so `id` is the tiebreaker that makes the keyset
    /// deterministic. The row-value comparison `(timestamp, id) < ($2, $3)`
    /// plans as an index range scan against `idx_events_issue_timestamp` on
    /// Postgres (confirmed via `EXPLAIN ANALYZE`, see spec Design Notes).
    /// SQLite 3.15+ supports row-value comparisons syntactically, but its
    /// query planner's choice of index for this shape has not been verified.
    /// Returns (events, has_more) where has_more indicates if there are more results.
    pub async fn list_paginated(
        pool: &DbPool,
        issue_id: Uuid,
        order: SortOrder,
        cursor: Option<&EventCursor>,
        limit: i64,
        environment: Option<&str>,
    ) -> AppResult<(Vec<EventSummary>, bool)> {
        // Fetch limit+1 to determine if there are more results
        let fetch_limit = limit + 1;

        let events = match (order, cursor) {
            // DESC (newest first) - no cursor
            (SortOrder::Desc, None) => {
                sqlx::query_as::<_, EventSummary>(sqlx::AssertSqlSafe(&*format!(
                    r#"
                    SELECT {EVENT_LIST_COLUMNS} FROM events
                    WHERE issue_id = $1 AND ($3 IS NULL OR environment = $3)
                    ORDER BY timestamp DESC, id DESC
                    LIMIT $2
                    "#
                )))
                .bind(issue_id)
                .bind(fetch_limit)
                .bind(environment)
                .fetch_all(pool)
                .await?
            }

            // DESC - with cursor
            (SortOrder::Desc, Some(c)) => {
                sqlx::query_as::<_, EventSummary>(sqlx::AssertSqlSafe(&*format!(
                    r#"
                    SELECT {EVENT_LIST_COLUMNS} FROM events
                    WHERE issue_id = $1 AND ($5 IS NULL OR environment = $5) AND (timestamp, id) < ($3, $4)
                    ORDER BY timestamp DESC, id DESC
                    LIMIT $2
                    "#
                )))
                .bind(issue_id)
                .bind(fetch_limit)
                .bind(c.last_timestamp)
                .bind(c.last_id)
                .bind(environment)
                .fetch_all(pool)
                .await?
            }

            // ASC (oldest first) - no cursor
            (SortOrder::Asc, None) => {
                sqlx::query_as::<_, EventSummary>(sqlx::AssertSqlSafe(&*format!(
                    r#"
                    SELECT {EVENT_LIST_COLUMNS} FROM events
                    WHERE issue_id = $1 AND ($3 IS NULL OR environment = $3)
                    ORDER BY timestamp ASC, id ASC
                    LIMIT $2
                    "#
                )))
                .bind(issue_id)
                .bind(fetch_limit)
                .bind(environment)
                .fetch_all(pool)
                .await?
            }

            // ASC - with cursor
            (SortOrder::Asc, Some(c)) => {
                sqlx::query_as::<_, EventSummary>(sqlx::AssertSqlSafe(&*format!(
                    r#"
                    SELECT {EVENT_LIST_COLUMNS} FROM events
                    WHERE issue_id = $1 AND ($5 IS NULL OR environment = $5) AND (timestamp, id) > ($3, $4)
                    ORDER BY timestamp ASC, id ASC
                    LIMIT $2
                    "#
                )))
                .bind(issue_id)
                .bind(fetch_limit)
                .bind(c.last_timestamp)
                .bind(c.last_id)
                .bind(environment)
                .fetch_all(pool)
                .await?
            }
        };

        let has_more = events.len() > limit as usize;
        let events: Vec<EventSummary> = events.into_iter().take(limit as usize).collect();

        Ok((events, has_more))
    }

    /// The issue's newest event, payload included.
    pub async fn latest_for_issue(pool: &DbPool, issue_id: Uuid) -> AppResult<Option<Event>> {
        Ok(sqlx::query_as::<_, Event>(
            "SELECT * FROM events WHERE issue_id = $1 ORDER BY timestamp DESC, id DESC LIMIT 1",
        )
        .bind(issue_id)
        .fetch_optional(pool)
        .await?)
    }

    /// Gets an event by ID
    pub async fn get_by_id(pool: &DbPool, id: Uuid) -> AppResult<Event> {
        let event = sqlx::query_as::<_, Event>("SELECT * FROM events WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Event {} not found", id)))?;

        Ok(event)
    }

    /// Gets an event by the client-supplied event id within its project.
    pub async fn get_by_event_id(
        pool: &DbPool,
        project_id: i32,
        event_id: Uuid,
    ) -> AppResult<Event> {
        sqlx::query_as::<_, Event>("SELECT * FROM events WHERE project_id = $1 AND event_id = $2")
            .bind(project_id)
            .bind(event_id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Event {} not found", event_id)))
    }

    /// Creates a new event and returns its primary key.
    ///
    /// Takes an executor rather than the pool so the digest can insert the
    /// event inside the same transaction that creates the issue and bumps its
    /// counters: those counters claim this row, so they have to commit or roll
    /// back together.
    ///
    /// Returns only the id: `RETURNING *` would hand the whole `data` blob
    /// back to be parsed into a second `serde_json::Value` tree that the
    /// digest then drops, doubling the payload's JSON work and peak memory.
    /// Callers that need the row load it with [`Self::get_by_id`].
    #[allow(clippy::too_many_arguments)]
    pub async fn create<'e, E>(
        executor: E,
        event_id: Uuid,
        project_id: i32,
        issue_id: Uuid,
        grouping_id: i32,
        event_data: &serde_json::Value,
        ingested_at: DateTime<Utc>,
        denormalized: &DenormalizedFields,
        remote_addr: Option<&str>,
        alert_type: Option<AlertType>,
        issue_event_count: Option<i32>,
    ) -> AppResult<Uuid>
    where
        E: sqlx::Executor<'e, Database = crate::db::Db>,
    {
        // Extract fields from event_data
        let timestamp = extract_timestamp(event_data, "timestamp").unwrap_or(ingested_at);

        // Each cut to its VARCHAR column, which Postgres enforces; Relay's
        // own limits are 200 for `release` and 64 for `environment`.
        let field = |value: Option<&serde_json::Value>, default: &str, max_chars: usize| {
            clamp(value.and_then(|v| v.as_str()).unwrap_or(default), max_chars)
        };
        let sdk = event_data.get("sdk");
        let level = event_data
            .get("level")
            .and_then(|v| v.as_str())
            .and_then(relay_level)
            .unwrap_or("error");
        let platform = field(event_data.get("platform"), "", 64);
        let release = field(event_data.get("release"), "", 200);
        let environment = field(event_data.get("environment"), "", 64);
        let server_name = field(event_data.get("server_name"), "", 255);
        let sdk_name = field(sdk.and_then(|s| s.get("name")), "", 255);
        let sdk_version = field(sdk.and_then(|s| s.get("version")), "", 255);

        // A feedback is stored apart from errors so the event counts and
        // charts, which read `event_type = 'error'`, leave it out.
        let event_type = match event_data.get("type").and_then(|v| v.as_str()) {
            Some("feedback") => "feedback",
            _ => "error",
        };

        // Store remote_addr as string
        let remote_addr_str: Option<String> = remote_addr.map(|s| s.to_string());

        // Generate primary key UUID in application for cross-DB compatibility
        let id = Uuid::new_v4();

        sqlx::query(
            r#"
            INSERT INTO events (
                id, event_id, project_id, issue_id, grouping_id, data,
                timestamp, ingested_at,
                calculated_type, calculated_value, "transaction",
                last_frame_filename, last_frame_module, last_frame_function,
                level, platform, release, environment, server_name,
                sdk_name, sdk_version, remote_addr, alert_type, issue_event_count, event_type
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25)
            "#,
        )
        .bind(id)
        .bind(event_id)
        .bind(project_id)
        .bind(issue_id)
        .bind(grouping_id)
        .bind(event_data)
        .bind(timestamp)
        .bind(ingested_at)
        .bind(&denormalized.calculated_type)
        .bind(&denormalized.calculated_value)
        .bind(&denormalized.transaction)
        .bind(&denormalized.last_frame_filename)
        .bind(&denormalized.last_frame_module)
        .bind(&denormalized.last_frame_function)
        .bind(level)
        .bind(platform)
        .bind(release)
        .bind(environment)
        .bind(server_name)
        .bind(sdk_name)
        .bind(sdk_version)
        .bind(remote_addr_str)
        .bind(alert_type)
        .bind(issue_event_count)
        .bind(event_type)
        .execute(executor)
        .await?;

        Ok(id)
    }

    /// Checks if an event with this event_id already exists in the project
    pub async fn exists(pool: &DbPool, project_id: i32, event_id: Uuid) -> AppResult<bool> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM events WHERE project_id = $1 AND event_id = $2",
        )
        .bind(project_id)
        .bind(event_id)
        .fetch_one(pool)
        .await?;

        Ok(count > 0)
    }

    /// Finds the issue for an already-digested event during durable retry.
    pub async fn issue_id_for_event(
        pool: &DbPool,
        project_id: i32,
        event_id: Uuid,
    ) -> AppResult<Option<Uuid>> {
        sqlx::query_scalar::<_, Option<Uuid>>(
            "SELECT issue_id FROM events WHERE project_id = $1 AND event_id = $2",
        )
        .bind(project_id)
        .bind(event_id)
        .fetch_optional(pool)
        .await
        .map(|issue_id| issue_id.flatten())
        .map_err(AppError::Database)
    }
}
