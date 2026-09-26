use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::{AlertType, Event, EventNavigation, EventSummary};
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
    ) -> AppResult<(Vec<EventSummary>, bool)> {
        // Fetch limit+1 to determine if there are more results
        let fetch_limit = limit + 1;

        let events = match (order, cursor) {
            // DESC (newest first) - no cursor
            (SortOrder::Desc, None) => {
                sqlx::query_as::<_, EventSummary>(sqlx::AssertSqlSafe(&*format!(
                    r#"
                    SELECT {EVENT_LIST_COLUMNS} FROM events
                    WHERE issue_id = $1
                    ORDER BY timestamp DESC, id DESC
                    LIMIT $2
                    "#
                )))
                .bind(issue_id)
                .bind(fetch_limit)
                .fetch_all(pool)
                .await?
            }

            // DESC - with cursor
            (SortOrder::Desc, Some(c)) => {
                sqlx::query_as::<_, EventSummary>(sqlx::AssertSqlSafe(&*format!(
                    r#"
                    SELECT {EVENT_LIST_COLUMNS} FROM events
                    WHERE issue_id = $1 AND (timestamp, id) < ($3, $4)
                    ORDER BY timestamp DESC, id DESC
                    LIMIT $2
                    "#
                )))
                .bind(issue_id)
                .bind(fetch_limit)
                .bind(c.last_timestamp)
                .bind(c.last_id)
                .fetch_all(pool)
                .await?
            }

            // ASC (oldest first) - no cursor
            (SortOrder::Asc, None) => {
                sqlx::query_as::<_, EventSummary>(sqlx::AssertSqlSafe(&*format!(
                    r#"
                    SELECT {EVENT_LIST_COLUMNS} FROM events
                    WHERE issue_id = $1
                    ORDER BY timestamp ASC, id ASC
                    LIMIT $2
                    "#
                )))
                .bind(issue_id)
                .bind(fetch_limit)
                .fetch_all(pool)
                .await?
            }

            // ASC - with cursor
            (SortOrder::Asc, Some(c)) => {
                sqlx::query_as::<_, EventSummary>(sqlx::AssertSqlSafe(&*format!(
                    r#"
                    SELECT {EVENT_LIST_COLUMNS} FROM events
                    WHERE issue_id = $1 AND (timestamp, id) > ($3, $4)
                    ORDER BY timestamp ASC, id ASC
                    LIMIT $2
                    "#
                )))
                .bind(issue_id)
                .bind(fetch_limit)
                .bind(c.last_timestamp)
                .bind(c.last_id)
                .fetch_all(pool)
                .await?
            }
        };

        let has_more = events.len() > limit as usize;
        let events: Vec<EventSummary> = events.into_iter().take(limit as usize).collect();

        Ok((events, has_more))
    }

    /// Gets an event by ID
    /// Where `event_id` sits among its issue's events, ordered like the
    /// ascending list: `(timestamp, id)`.
    pub async fn navigation(
        pool: &DbPool,
        issue_id: Uuid,
        event_id: Uuid,
    ) -> AppResult<EventNavigation> {
        let timestamp: DateTime<Utc> =
            sqlx::query_scalar("SELECT timestamp FROM events WHERE issue_id = $1 AND id = $2")
                .bind(issue_id)
                .bind(event_id)
                .fetch_optional(pool)
                .await?
                .ok_or_else(|| AppError::NotFound(format!("Event {} not found", event_id)))?;

        // Each subquery is an index range on `(issue_id, timestamp)`, so the
        // cost does not grow with the page the reader is on.
        let navigation = sqlx::query_as::<_, EventNavigation>(
            r#"
            SELECT
                (SELECT COUNT(*) FROM events
                  WHERE issue_id = $1 AND (timestamp, id) <= ($2, $3)) AS current_index,
                (SELECT COUNT(*) FROM events WHERE issue_id = $1) AS total_count,
                (SELECT id FROM events WHERE issue_id = $1
                  ORDER BY timestamp ASC, id ASC LIMIT 1) AS first_event_id,
                (SELECT id FROM events WHERE issue_id = $1
                  ORDER BY timestamp DESC, id DESC LIMIT 1) AS last_event_id,
                (SELECT id FROM events
                  WHERE issue_id = $1 AND (timestamp, id) < ($2, $3)
                  ORDER BY timestamp DESC, id DESC LIMIT 1) AS prev_event_id,
                (SELECT id FROM events
                  WHERE issue_id = $1 AND (timestamp, id) > ($2, $3)
                  ORDER BY timestamp ASC, id ASC LIMIT 1) AS next_event_id
            "#,
        )
        .bind(issue_id)
        .bind(timestamp)
        .bind(event_id)
        .fetch_one(pool)
        .await?;

        Ok(navigation)
    }

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
    ) -> AppResult<Uuid>
    where
        E: sqlx::Executor<'e, Database = crate::db::Db>,
    {
        // Extract fields from event_data
        let timestamp = event_data
            .get("timestamp")
            .and_then(|t| {
                if let Some(ts) = t.as_f64() {
                    DateTime::from_timestamp(ts as i64, ((ts.fract()) * 1_000_000_000.0) as u32)
                } else if let Some(ts_str) = t.as_str() {
                    DateTime::parse_from_rfc3339(ts_str)
                        .ok()
                        .map(|dt| dt.to_utc())
                } else {
                    None
                }
            })
            .unwrap_or(ingested_at);

        let level = event_data
            .get("level")
            .and_then(|l| l.as_str())
            .unwrap_or("error");

        let platform = event_data
            .get("platform")
            .and_then(|p| p.as_str())
            .unwrap_or("");

        let release = event_data
            .get("release")
            .and_then(|r| r.as_str())
            .unwrap_or("");

        let environment = event_data
            .get("environment")
            .and_then(|e| e.as_str())
            .unwrap_or("");

        let server_name = event_data
            .get("server_name")
            .and_then(|s| s.as_str())
            .unwrap_or("");

        let sdk_name = event_data
            .get("sdk")
            .and_then(|s| s.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or("");

        let sdk_version = event_data
            .get("sdk")
            .and_then(|s| s.get("version"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

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
                sdk_name, sdk_version, remote_addr, alert_type
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23)
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
