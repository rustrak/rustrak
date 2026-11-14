use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::{AlertType, Event, EventSummary};
use crate::pagination::{EventCursor, SortOrder};
use crate::protocol::{clamp, extract_timestamp, relay_level};
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

    /// Creates a new event and returns its primary key: one
    /// [`EventRow::insert_all`] of one row. The digest builds its rows itself.
    #[allow(clippy::too_many_arguments)]
    pub async fn create(
        pool: &DbPool,
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
    ) -> AppResult<Uuid> {
        let row = EventRow::new(
            event_data,
            ingested_at,
            denormalized,
            remote_addr,
            event_id,
            project_id,
        );
        let id = Uuid::new_v4();
        let link = EventLink {
            id,
            issue_id,
            grouping_id,
            alert_type,
            issue_event_count,
        };
        let mut conn = pool.acquire().await?;
        if EventRow::insert_all(&mut conn, [(&row, &link)])
            .await?
            .is_empty()
        {
            return Err(AppError::Conflict(format!(
                "Event {event_id} already exists"
            )));
        }
        Ok(id)
    }

    /// Checks if an event with this event_id already exists in the project
    pub async fn exists(pool: &DbPool, project_id: i32, event_id: Uuid) -> AppResult<bool> {
        let found: Option<i32> = sqlx::query_scalar(
            "SELECT 1 FROM events WHERE project_id = $1 AND event_id = $2 LIMIT 1",
        )
        .bind(project_id)
        .bind(event_id)
        .fetch_optional(pool)
        .await?;

        Ok(found.is_some())
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

/// Every `events` column an event's own payload decides, built once from
/// the parsed event so the tree can be dropped before the write. `data` is
/// the payload serialised once: bound as text (SQLite stores JSON as text,
/// Postgres casts it to JSONB).
pub struct EventRow {
    pub event_id: Uuid,
    pub project_id: i32,
    pub data: String,
    pub timestamp: DateTime<Utc>,
    pub ingested_at: DateTime<Utc>,
    pub calculated_type: String,
    pub calculated_value: String,
    pub transaction: String,
    pub last_frame_filename: String,
    pub last_frame_module: String,
    pub last_frame_function: String,
    pub level: &'static str,
    pub platform: String,
    pub release: String,
    pub environment: String,
    pub server_name: String,
    pub sdk_name: String,
    pub sdk_version: String,
    pub remote_addr: Option<String>,
    pub event_type: &'static str,
    pub user_identity: Option<String>,
}

/// The `events` columns the digest decides: the row's id and where it groups.
pub struct EventLink {
    pub id: Uuid,
    pub issue_id: Uuid,
    pub grouping_id: i32,
    pub alert_type: Option<AlertType>,
    pub issue_event_count: Option<i32>,
}

impl EventRow {
    pub fn new(
        event_data: &serde_json::Value,
        ingested_at: DateTime<Utc>,
        denormalized: &DenormalizedFields,
        remote_addr: Option<&str>,
        event_id: Uuid,
        project_id: i32,
    ) -> Self {
        // Each cut to its VARCHAR column, which Postgres enforces; Relay's
        // own limits are 200 for `release` and 64 for `environment`.
        let field = |value: Option<&serde_json::Value>, max_chars: usize| {
            clamp(value.and_then(|v| v.as_str()).unwrap_or(""), max_chars)
        };
        let sdk = event_data.get("sdk");
        Self {
            event_id,
            project_id,
            data: event_data.to_string(),
            timestamp: extract_timestamp(event_data, "timestamp").unwrap_or(ingested_at),
            ingested_at,
            calculated_type: denormalized.calculated_type.clone(),
            calculated_value: denormalized.calculated_value.clone(),
            transaction: denormalized.transaction.clone(),
            last_frame_filename: denormalized.last_frame_filename.clone(),
            last_frame_module: denormalized.last_frame_module.clone(),
            last_frame_function: denormalized.last_frame_function.clone(),
            level: event_data
                .get("level")
                .and_then(|v| v.as_str())
                .and_then(relay_level)
                .unwrap_or("error"),
            platform: field(event_data.get("platform"), 64),
            release: field(event_data.get("release"), 200),
            environment: field(event_data.get("environment"), 64),
            server_name: field(event_data.get("server_name"), 255),
            sdk_name: field(sdk.and_then(|s| s.get("name")), 255),
            sdk_version: field(sdk.and_then(|s| s.get("version")), 255),
            remote_addr: remote_addr.map(str::to_string),
            // A feedback is stored apart from errors so the event counts and
            // charts, which read `event_type = 'error'`, leave it out.
            event_type: match event_data.get("type").and_then(|v| v.as_str()) {
                Some("feedback") => "feedback",
                _ => "error",
            },
            user_identity: event_data
                .get("user")
                .and_then(crate::services::issue::user_identity_from_user_field),
        }
    }

    /// Inserts the rows in one statement, skipping any whose
    /// `(project_id, event_id)` is already stored, and returns the identities
    /// it did insert.
    ///
    /// Returns only those: `RETURNING *` would hand every `data` blob back.
    pub async fn insert_all<'r>(
        conn: &mut <crate::db::Db as sqlx::Database>::Connection,
        rows: impl IntoIterator<Item = (&'r EventRow, &'r EventLink)>,
    ) -> AppResult<Vec<(i32, Uuid)>> {
        let mut insert = sqlx::QueryBuilder::<crate::db::Db>::new(
            r#"INSERT INTO events (
                id, event_id, project_id, issue_id, grouping_id, data,
                timestamp, ingested_at,
                calculated_type, calculated_value, "transaction",
                last_frame_filename, last_frame_module, last_frame_function,
                level, platform, release, environment, server_name,
                sdk_name, sdk_version, remote_addr, alert_type, issue_event_count, event_type,
                user_identity
            ) "#,
        );
        insert.push_values(rows, |mut b, (row, link)| {
            b.push_bind(link.id)
                .push_bind(row.event_id)
                .push_bind(row.project_id)
                .push_bind(link.issue_id)
                .push_bind(link.grouping_id);
            #[cfg(feature = "postgres")]
            b.push("CAST(")
                .push_bind_unseparated(&row.data)
                .push_unseparated(" AS JSONB)");
            #[cfg(feature = "sqlite")]
            b.push_bind(&row.data);
            b.push_bind(row.timestamp)
                .push_bind(row.ingested_at)
                .push_bind(&row.calculated_type)
                .push_bind(&row.calculated_value)
                .push_bind(&row.transaction)
                .push_bind(&row.last_frame_filename)
                .push_bind(&row.last_frame_module)
                .push_bind(&row.last_frame_function)
                .push_bind(row.level)
                .push_bind(&row.platform)
                .push_bind(&row.release)
                .push_bind(&row.environment)
                .push_bind(&row.server_name)
                .push_bind(&row.sdk_name)
                .push_bind(&row.sdk_version)
                .push_bind(&row.remote_addr)
                .push_bind(link.alert_type)
                .push_bind(link.issue_event_count)
                .push_bind(row.event_type)
                .push_bind(&row.user_identity);
        });
        insert
            .push(" ON CONFLICT (project_id, event_id) DO NOTHING RETURNING project_id, event_id");
        // Not cached: each batch size is its own statement, and a cached
        // SQLite statement keeps a copy of its last bound values (sqlx
        // clears them only when it runs it again), so up to 100 of them per
        // connection would each keep a batch's payloads alive.
        Ok(insert
            .build_query_as::<(i32, Uuid)>()
            .persistent(false)
            .fetch_all(&mut *conn)
            .await?)
    }
}
