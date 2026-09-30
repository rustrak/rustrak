use chrono::{DateTime, Utc};
use sqlx::Row;

use crate::db::DbPool;
use crate::error::AppResult;
use crate::models::LogResponse;

pub struct LogService;

/// Optional equality filters for the log list. A `None` field matches every
/// row; a `Some` value restricts to exact matches on the denormalized column.
#[derive(Debug, Default, Clone)]
pub struct LogFilters {
    pub level: Option<String>,
    pub trace_id: Option<String>,
    pub environment: Option<String>,
}

impl LogService {
    /// Lists logs for a project with offset-based pagination (newest first by
    /// log timestamp), optionally filtered by level/trace_id. Returns (rows, total).
    ///
    /// Each filter uses a `($n IS NULL OR col = $m)` guard so a `None` is a no-op
    /// — static SQL, positional binds (dialect-safe across Postgres and SQLite).
    pub async fn list_offset(
        pool: &DbPool,
        project_id: i32,
        page: i64,
        per_page: i64,
        filters: &LogFilters,
    ) -> AppResult<(Vec<LogResponse>, i64)> {
        // Clamp at the service boundary so a direct caller passing page <= 0
        // can't produce a negative SQL OFFSET (the HTTP route already guards it).
        let page = page.max(1);
        let per_page = per_page.clamp(1, 100);
        let offset = (page - 1) * per_page;

        let total_count: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM logs
            WHERE project_id = $1
              AND ($2 IS NULL OR level = $3)
              AND ($4 IS NULL OR trace_id = $5)
              AND ($6 IS NULL OR environment = $7)
            "#,
        )
        .bind(project_id)
        .bind(filters.level.as_deref())
        .bind(filters.level.as_deref())
        .bind(filters.trace_id.as_deref())
        .bind(filters.trace_id.as_deref())
        .bind(filters.environment.as_deref())
        .bind(filters.environment.as_deref())
        .fetch_one(pool)
        .await?;

        let rows = sqlx::query(
            r#"
            SELECT id, trace_id, span_id, level, severity_number, body, environment,
                   attributes, timestamp, ingested_at
            FROM logs
            WHERE project_id = $1
              AND ($2 IS NULL OR level = $3)
              AND ($4 IS NULL OR trace_id = $5)
              AND ($6 IS NULL OR environment = $7)
            ORDER BY timestamp DESC, id DESC
            LIMIT $8 OFFSET $9
            "#,
        )
        .bind(project_id)
        .bind(filters.level.as_deref())
        .bind(filters.level.as_deref())
        .bind(filters.trace_id.as_deref())
        .bind(filters.trace_id.as_deref())
        .bind(filters.environment.as_deref())
        .bind(filters.environment.as_deref())
        .bind(per_page)
        .bind(offset)
        .fetch_all(pool)
        .await?;

        let logs: Vec<LogResponse> = rows
            .iter()
            .map(|row| {
                let timestamp: DateTime<Utc> = row.get("timestamp");
                let ingested_at: DateTime<Utc> = row.get("ingested_at");
                LogResponse {
                    id: row.get("id"),
                    environment: row.get("environment"),
                    trace_id: row.get("trace_id"),
                    span_id: row.get("span_id"),
                    level: row.get("level"),
                    severity_number: row.get("severity_number"),
                    body: row.get("body"),
                    attributes: row.get("attributes"),
                    timestamp,
                    ingested_at,
                }
            })
            .collect();

        Ok((logs, total_count.0))
    }
}
