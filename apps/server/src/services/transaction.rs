use chrono::DateTime;
use sqlx::Row;
use uuid::Uuid;

use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::{
    SpanResponse, TransactionDetailResponse, TransactionResponse, TransactionStatsResponse,
};

pub struct TransactionService;

/// Optional equality filters for the transaction list. A `None` field matches
/// every row; a `Some` value restricts to exact matches on the denormalized
/// column. Mirrors the op/status/environment/release facets Sentry exposes.
#[derive(Debug, Default, Clone)]
pub struct TransactionFilters {
    /// Exact transaction_name match — used to list one group's samples.
    pub name: Option<String>,
    pub op: Option<String>,
    pub status: Option<String>,
    pub environment: Option<String>,
    pub release: Option<String>,
}

impl TransactionService {
    /// Lists transactions for a project with offset-based pagination (newest first),
    /// optionally filtered by op/status/environment/release.
    ///
    /// Each filter uses a `($n IS NULL OR col = $m)` guard so a `None` is a no-op
    /// — static SQL, positional binds, no parameter reuse (dialect-safe across
    /// Postgres and SQLite). Ordered by ingested_at DESC. Returns (rows, total).
    pub async fn list_offset(
        pool: &DbPool,
        project_id: i32,
        page: i64,
        per_page: i64,
        filters: &TransactionFilters,
    ) -> AppResult<(Vec<TransactionResponse>, i64)> {
        let per_page = per_page.clamp(1, 100);
        let offset = (page - 1) * per_page;

        let total_count: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM transactions
            WHERE project_id = $1
              AND ($2 IS NULL OR transaction_name = $3)
              AND ($4 IS NULL OR op = $5)
              AND ($6 IS NULL OR status = $7)
              AND ($8 IS NULL OR environment = $9)
              AND ($10 IS NULL OR release = $11)
            "#,
        )
        .bind(project_id)
        .bind(filters.name.as_deref())
        .bind(filters.name.as_deref())
        .bind(filters.op.as_deref())
        .bind(filters.op.as_deref())
        .bind(filters.status.as_deref())
        .bind(filters.status.as_deref())
        .bind(filters.environment.as_deref())
        .bind(filters.environment.as_deref())
        .bind(filters.release.as_deref())
        .bind(filters.release.as_deref())
        .fetch_one(pool)
        .await?;

        let rows = sqlx::query(
            r#"
            SELECT id, event_id, transaction_name,
                   timestamp, start_timestamp, duration_ms,
                   platform, environment, release, ingested_at
            FROM transactions
            WHERE project_id = $1
              AND ($2 IS NULL OR transaction_name = $3)
              AND ($4 IS NULL OR op = $5)
              AND ($6 IS NULL OR status = $7)
              AND ($8 IS NULL OR environment = $9)
              AND ($10 IS NULL OR release = $11)
            ORDER BY ingested_at DESC, id DESC
            LIMIT $12 OFFSET $13
            "#,
        )
        .bind(project_id)
        .bind(filters.name.as_deref())
        .bind(filters.name.as_deref())
        .bind(filters.op.as_deref())
        .bind(filters.op.as_deref())
        .bind(filters.status.as_deref())
        .bind(filters.status.as_deref())
        .bind(filters.environment.as_deref())
        .bind(filters.environment.as_deref())
        .bind(filters.release.as_deref())
        .bind(filters.release.as_deref())
        .bind(per_page)
        .bind(offset)
        .fetch_all(pool)
        .await?;

        let transactions: Vec<TransactionResponse> = rows
            .iter()
            .map(|row| {
                let timestamp: DateTime<chrono::Utc> = row.get("timestamp");
                let start_timestamp: Option<DateTime<chrono::Utc>> = row.get("start_timestamp");
                TransactionResponse {
                    id: row.get("id"),
                    event_id: row.get("event_id"),
                    transaction_name: row.get("transaction_name"),
                    timestamp,
                    start_timestamp,
                    duration_ms: row.get("duration_ms"),
                    platform: row.get("platform"),
                    environment: row.get("environment"),
                    release: row.get("release"),
                    ingested_at: row.get("ingested_at"),
                }
            })
            .collect();

        Ok((transactions, total_count.0))
    }

    /// Fetches a single transaction by its primary key, scoped to a project.
    ///
    /// Returns the summary fields plus the full Sentry payload (`data`) so the
    /// frontend can render the span waterfall and metrics. Only rows with
    /// `event_type = 'transaction'` are returned — an error event id yields
    /// NotFound, keeping the transaction namespace isolated.
    pub async fn get_by_id(
        pool: &DbPool,
        project_id: i32,
        id: Uuid,
    ) -> AppResult<TransactionDetailResponse> {
        let row = sqlx::query(
            r#"
            SELECT id, event_id, transaction_name,
                   timestamp, start_timestamp, duration_ms,
                   platform, environment, release, ingested_at, data
            FROM transactions
            WHERE id = $1
              AND project_id = $2
            "#,
        )
        .bind(id)
        .bind(project_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Transaction {} not found", id)))?;

        let timestamp: DateTime<chrono::Utc> = row.get("timestamp");
        let start_timestamp: Option<DateTime<chrono::Utc>> = row.get("start_timestamp");

        Ok(TransactionDetailResponse {
            id: row.get("id"),
            event_id: row.get("event_id"),
            transaction_name: row.get("transaction_name"),
            timestamp,
            start_timestamp,
            duration_ms: row.get("duration_ms"),
            platform: row.get("platform"),
            environment: row.get("environment"),
            release: row.get("release"),
            ingested_at: row.get("ingested_at"),
            data: row.get("data"),
        })
    }

    /// Lists the indexed spans extracted from a transaction, ordered by
    /// start_timestamp (waterfall order). Scoped to the project so a foreign
    /// transaction id yields an empty list rather than leaking spans.
    pub async fn list_spans(
        pool: &DbPool,
        project_id: i32,
        transaction_id: Uuid,
    ) -> AppResult<Vec<SpanResponse>> {
        // A missing/cross-project id is NotFound, not an empty list — keeps the
        // endpoint's documented 404 honest and matches get_by_id.
        let exists: Option<(i32,)> =
            sqlx::query_as("SELECT 1 FROM transactions WHERE id = $1 AND project_id = $2")
                .bind(transaction_id)
                .bind(project_id)
                .fetch_optional(pool)
                .await?;
        if exists.is_none() {
            return Err(AppError::NotFound(format!(
                "Transaction {} not found",
                transaction_id
            )));
        }

        let rows = sqlx::query(
            r#"
            SELECT id, transaction_id, span_id, trace_id, parent_span_id,
                   op, description, status,
                   start_timestamp, timestamp, duration_ms, exclusive_time_ms,
                   is_segment, segment_id, platform, release, environment,
                   gen_ai_operation_type, gen_ai_agent_name,
                   gen_ai_request_model, gen_ai_response_model,
                   gen_ai_tool_name, gen_ai_conversation_id,
                   gen_ai_usage_input_tokens, gen_ai_usage_output_tokens, gen_ai_usage_total_tokens
            FROM spans
            WHERE transaction_id = $1
              AND project_id = $2
            ORDER BY start_timestamp ASC, id ASC
            "#,
        )
        .bind(transaction_id)
        .bind(project_id)
        .fetch_all(pool)
        .await?;

        let spans = rows
            .iter()
            .map(|row| SpanResponse {
                id: row.get("id"),
                transaction_id: row.get("transaction_id"),
                span_id: row.get("span_id"),
                trace_id: row.get("trace_id"),
                parent_span_id: row.get("parent_span_id"),
                op: row.get("op"),
                description: row.get("description"),
                status: row.get("status"),
                start_timestamp: row.get("start_timestamp"),
                timestamp: row.get("timestamp"),
                duration_ms: row.get("duration_ms"),
                exclusive_time_ms: row.get("exclusive_time_ms"),
                is_segment: row.get("is_segment"),
                segment_id: row.get("segment_id"),
                platform: row.get("platform"),
                release: row.get("release"),
                environment: row.get("environment"),
                gen_ai_operation_type: row.get("gen_ai_operation_type"),
                gen_ai_agent_name: row.get("gen_ai_agent_name"),
                gen_ai_request_model: row.get("gen_ai_request_model"),
                gen_ai_response_model: row.get("gen_ai_response_model"),
                gen_ai_tool_name: row.get("gen_ai_tool_name"),
                gen_ai_conversation_id: row.get("gen_ai_conversation_id"),
                gen_ai_usage_input_tokens: row.get("gen_ai_usage_input_tokens"),
                gen_ai_usage_output_tokens: row.get("gen_ai_usage_output_tokens"),
                gen_ai_usage_total_tokens: row.get("gen_ai_usage_total_tokens"),
            })
            .collect();

        Ok(spans)
    }

    /// Aggregate latency/throughput/failure stats per (transaction_name, op).
    ///
    /// Groups are returned most-frequent first, ready for the performance
    /// overview table. Percentiles are continuous (linear-interpolated); the
    /// two backends reach that same definition by different routes — see the
    /// per-backend implementations below.
    ///
    /// Postgres computes the percentiles in SQL as ordered-set aggregates, so
    /// the whole page is one round trip. The previous shared implementation
    /// looped over the page calling `group_durations`, which streamed *every*
    /// matching row back to compute percentiles in Rust: 20 groups × 100k rows
    /// was over a million floats on the wire per request, and it saturated the
    /// connection pool under normal dashboard traffic.
    #[cfg(feature = "postgres")]
    pub async fn stats(
        pool: &DbPool,
        project_id: i32,
        page: i64,
        per_page: i64,
        environment: Option<&str>,
    ) -> AppResult<(Vec<TransactionStatsResponse>, i64)> {
        let per_page = per_page.clamp(1, 100);
        let offset = (page.max(1) - 1) * per_page;

        let total = Self::group_count(pool, project_id, environment).await?;

        // ORDER BY includes op so the tie order is deterministic across
        // requests (stable offset pagination).
        let group_rows = sqlx::query(
            r#"
            SELECT transaction_name, op,
                   COUNT(*) AS cnt,
                   SUM(CASE WHEN status IS NOT NULL AND status <> 'ok' THEN 1 ELSE 0 END) AS fails,
                   percentile_cont(0.50) WITHIN GROUP (ORDER BY duration_ms) AS p50,
                   percentile_cont(0.95) WITHIN GROUP (ORDER BY duration_ms) AS p95,
                   percentile_cont(0.99) WITHIN GROUP (ORDER BY duration_ms) AS p99
            FROM transactions
            WHERE project_id = $1 AND duration_ms IS NOT NULL AND ($4 IS NULL OR environment = $4)
            GROUP BY transaction_name, op
            ORDER BY cnt DESC, transaction_name ASC, op ASC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(project_id)
        .bind(per_page)
        .bind(offset)
        .bind(environment)
        .fetch_all(pool)
        .await?;

        let stats = group_rows.iter().map(row_to_group_stats).collect();

        Ok((stats, total))
    }

    /// SQLite has no `percentile_cont`, so the durations come back to Rust and
    /// `percentile_cont` here reproduces the ordered-set aggregate. Acceptable
    /// because SQLite deployments are single-node and small by definition; the
    /// Postgres path above carries the production load.
    #[cfg(feature = "sqlite")]
    pub async fn stats(
        pool: &DbPool,
        project_id: i32,
        page: i64,
        per_page: i64,
        environment: Option<&str>,
    ) -> AppResult<(Vec<TransactionStatsResponse>, i64)> {
        let per_page = per_page.clamp(1, 100);
        let offset = (page.max(1) - 1) * per_page;

        let total = Self::group_count(pool, project_id, environment).await?;

        let group_rows = sqlx::query(
            r#"
            SELECT transaction_name, op,
                   COUNT(*) AS cnt,
                   SUM(CASE WHEN status IS NOT NULL AND status <> 'ok' THEN 1 ELSE 0 END) AS fails
            FROM transactions
            WHERE project_id = $1 AND duration_ms IS NOT NULL AND ($4 IS NULL OR environment = $4)
            GROUP BY transaction_name, op
            ORDER BY cnt DESC, transaction_name ASC, op ASC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(project_id)
        .bind(per_page)
        .bind(offset)
        .bind(environment)
        .fetch_all(pool)
        .await?;

        let mut stats = Vec::with_capacity(group_rows.len());
        for row in &group_rows {
            let name: String = row.get("transaction_name");
            let op: Option<String> = row.get("op");
            let count: i64 = row.get("cnt");
            let fails: i64 = row.get("fails");
            let durations =
                Self::group_durations(pool, project_id, &name, op.as_deref(), environment).await?;
            stats.push(build_group_stats(name, op, count, fails, durations));
        }

        Ok((stats, total))
    }

    /// Total number of (transaction_name, op) groups — the pagination total,
    /// shared by both backends' `stats`.
    async fn group_count(
        pool: &DbPool,
        project_id: i32,
        environment: Option<&str>,
    ) -> AppResult<i64> {
        let total: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM (
                SELECT 1 FROM transactions
                WHERE project_id = $1 AND duration_ms IS NOT NULL AND ($2 IS NULL OR environment = $2)
                GROUP BY transaction_name, op
            ) g
            "#,
        )
        .bind(project_id)
        .bind(environment)
        .fetch_one(pool)
        .await?;

        Ok(total.0)
    }

    /// Aggregate stats for a single (transaction_name, op) group — a direct
    /// lookup so the summary header works regardless of how many groups exist.
    /// Returns `None` when the group has no transactions.
    ///
    /// One aggregate query: the count, failure count and all three percentiles
    /// collapse in the database, so a group with 100k samples costs one row on
    /// the wire rather than 100k.
    #[cfg(feature = "postgres")]
    pub async fn stats_for_group(
        pool: &DbPool,
        project_id: i32,
        name: &str,
        op: Option<&str>,
        environment: Option<&str>,
    ) -> AppResult<Option<TransactionStatsResponse>> {
        // GROUP BY (not a bare aggregate) so an empty group yields zero rows
        // and maps to `None`, rather than one all-NULL row.
        let row = sqlx::query(
            r#"
            SELECT transaction_name, op,
                   COUNT(*) AS cnt,
                   SUM(CASE WHEN status IS NOT NULL AND status <> 'ok' THEN 1 ELSE 0 END) AS fails,
                   percentile_cont(0.50) WITHIN GROUP (ORDER BY duration_ms) AS p50,
                   percentile_cont(0.95) WITHIN GROUP (ORDER BY duration_ms) AS p95,
                   percentile_cont(0.99) WITHIN GROUP (ORDER BY duration_ms) AS p99
            FROM transactions
            WHERE project_id = $1 AND transaction_name = $2
              AND (op = $3 OR (op IS NULL AND $4 IS NULL))
              AND duration_ms IS NOT NULL
              AND ($5 IS NULL OR environment = $5)
            GROUP BY transaction_name, op
            "#,
        )
        .bind(project_id)
        .bind(name)
        .bind(op)
        .bind(op)
        .bind(environment)
        .fetch_optional(pool)
        .await?;

        Ok(row.as_ref().map(row_to_group_stats))
    }

    /// SQLite counterpart — see the note on `stats` for why the percentile math
    /// happens in Rust here.
    #[cfg(feature = "sqlite")]
    pub async fn stats_for_group(
        pool: &DbPool,
        project_id: i32,
        name: &str,
        op: Option<&str>,
        environment: Option<&str>,
    ) -> AppResult<Option<TransactionStatsResponse>> {
        let durations = Self::group_durations(pool, project_id, name, op, environment).await?;
        if durations.is_empty() {
            return Ok(None);
        }

        let fails: (i64,) = sqlx::query_as(
            r#"
            SELECT COALESCE(SUM(CASE WHEN status IS NOT NULL AND status <> 'ok' THEN 1 ELSE 0 END), 0)
            FROM transactions
            WHERE project_id = $1 AND transaction_name = $2
              AND (op = $3 OR (op IS NULL AND $4 IS NULL))
              AND duration_ms IS NOT NULL
              AND ($5 IS NULL OR environment = $5)
            "#,
        )
        .bind(project_id)
        .bind(name)
        .bind(op)
        .bind(op)
        .bind(environment)
        .fetch_one(pool)
        .await?;

        let count = durations.len() as i64;
        Ok(Some(build_group_stats(
            name.to_string(),
            op.map(str::to_string),
            count,
            fails.0,
            durations,
        )))
    }

    /// Fetches the durations of one (name, op) group. The NULL-safe op match
    /// avoids reusing a placeholder, keeping the SQL portable across dialects.
    #[cfg(feature = "sqlite")]
    async fn group_durations(
        pool: &DbPool,
        project_id: i32,
        name: &str,
        op: Option<&str>,
        environment: Option<&str>,
    ) -> AppResult<Vec<f64>> {
        let rows = sqlx::query(
            r#"
            SELECT duration_ms FROM transactions
            WHERE project_id = $1 AND transaction_name = $2
              AND (op = $3 OR (op IS NULL AND $4 IS NULL))
              AND duration_ms IS NOT NULL
              AND ($5 IS NULL OR environment = $5)
            "#,
        )
        .bind(project_id)
        .bind(name)
        .bind(op)
        .bind(op)
        .bind(environment)
        .fetch_all(pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| r.get::<f64, _>("duration_ms"))
            .collect())
    }
}

/// Builds one group's response from a row whose percentiles were already
/// computed by the database (Postgres path). `p50`/`p95`/`p99` are nullable in
/// the row type only because `percentile_cont` is; the `duration_ms IS NOT
/// NULL` filter means a group that produced a row always has values.
#[cfg(feature = "postgres")]
fn row_to_group_stats(row: &<crate::db::Db as sqlx::Database>::Row) -> TransactionStatsResponse {
    let count: i64 = row.get("cnt");
    let failures: i64 = row.get("fails");

    TransactionStatsResponse {
        transaction_name: row.get("transaction_name"),
        op: row.get("op"),
        count,
        p50_ms: row.get::<Option<f64>, _>("p50").unwrap_or(0.0),
        p95_ms: row.get::<Option<f64>, _>("p95").unwrap_or(0.0),
        p99_ms: row.get::<Option<f64>, _>("p99").unwrap_or(0.0),
        failure_rate: if count > 0 {
            failures as f64 / count as f64
        } else {
            0.0
        },
    }
}

/// Builds one group's response from its durations + failure count.
#[cfg(feature = "sqlite")]
fn build_group_stats(
    transaction_name: String,
    op: Option<String>,
    count: i64,
    failures: i64,
    mut durations: Vec<f64>,
) -> TransactionStatsResponse {
    durations.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    TransactionStatsResponse {
        transaction_name,
        op,
        count,
        p50_ms: percentile_cont(&durations, 0.50),
        p95_ms: percentile_cont(&durations, 0.95),
        p99_ms: percentile_cont(&durations, 0.99),
        failure_rate: if count > 0 {
            failures as f64 / count as f64
        } else {
            0.0
        },
    }
}

/// Continuous percentile over a pre-sorted slice (linear interpolation between
/// closest ranks), matching Postgres `percentile_cont`. `p` is in [0.0, 1.0].
///
/// `pub(crate)` so other services needing the same dual-backend-portable
/// percentile math (e.g. `services::span`) can reuse it instead of
/// re-deriving it.
pub(crate) fn percentile_cont(sorted: &[f64], p: f64) -> f64 {
    match sorted.len() {
        0 => 0.0,
        1 => sorted[0],
        n => {
            let rank = p * (n - 1) as f64;
            let lo = rank.floor() as usize;
            let hi = rank.ceil() as usize;
            if lo == hi {
                sorted[lo]
            } else {
                let frac = rank - lo as f64;
                sorted[lo] + (sorted[hi] - sorted[lo]) * frac
            }
        }
    }
}
