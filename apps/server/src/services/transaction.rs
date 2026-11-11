use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use crate::db::dialect::{like_pattern, since};
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::{
    SpanResponse, TransactionDetailResponse, TransactionResponse, TransactionStatsResponse,
    TransactionTimeseriesPoint,
};
use crate::services::scope::bucket_hours;

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
    /// Only the last this many hours; all time when `None`.
    pub period_hours: Option<i64>,
    /// Longest first instead of newest first.
    pub slowest: bool,
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
        let offset = crate::pagination::offset(page, per_page);

        let window = since("timestamp", filters.period_hours);
        let count_sql = format!(
            r#"
            SELECT COUNT(*) FROM transactions
            WHERE project_id = $1
              AND ($2 IS NULL OR transaction_name = $3)
              AND ($4 IS NULL OR op = $5)
              AND ($6 IS NULL OR status = $7)
              AND ($8 IS NULL OR environment = $9)
              AND ($10 IS NULL OR release = $11)
              AND {window}
            "#,
        );
        let total_count: (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(&*count_sql))
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

        let order = if filters.slowest {
            // NULL durations last on both backends, without NULLS LAST.
            "(duration_ms IS NULL), duration_ms DESC, id DESC"
        } else {
            "ingested_at DESC, id DESC"
        };
        let sql = format!(
            r#"
            SELECT id, event_id, transaction_name,
                   timestamp, start_timestamp, duration_ms,
                   platform, environment, release, ingested_at, status
            FROM transactions
            WHERE project_id = $1
              AND ($2 IS NULL OR transaction_name = $3)
              AND ($4 IS NULL OR op = $5)
              AND ($6 IS NULL OR status = $7)
              AND ($8 IS NULL OR environment = $9)
              AND ($10 IS NULL OR release = $11)
              AND {window}
            ORDER BY {order}
            LIMIT $12 OFFSET $13
            "#,
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
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
                    status: row.get("status"),
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

    /// Endpoints -- one per (transaction_name, op) -- with how often they
    /// ran, how long they took and how much of the time they take in all,
    /// most time-consuming first by default, as New Relic and Elastic rank
    /// them: a fast endpoint called constantly can cost more than a slow one
    /// called once.
    ///
    /// Postgres computes the percentiles in SQL as ordered-set aggregates, so
    /// the page is one round trip. SQLite has no `percentile_cont`, so each
    /// group's durations come back and [`percentile_cont`] reproduces it --
    /// acceptable because SQLite deployments are single-node and small.
    pub async fn stats(
        pool: &DbPool,
        project_id: i32,
        page: i64,
        per_page: i64,
        filter: &StatsFilter,
    ) -> AppResult<(Vec<TransactionStatsResponse>, i64)> {
        let offset = crate::pagination::offset(page, per_page);
        let (groups, time) = Self::totals(pool, project_id, filter).await?;
        let stats = Self::grouped(pool, project_id, filter, None, per_page, offset, time).await?;
        Ok((stats, groups))
    }

    /// One endpoint's figures in the same terms as [`Self::stats`], its share
    /// measured against every endpoint in the same window. `None` when it has
    /// no samples there.
    pub async fn stats_for_group(
        pool: &DbPool,
        project_id: i32,
        name: &str,
        op: Option<&str>,
        filter: &StatsFilter,
    ) -> AppResult<Option<TransactionStatsResponse>> {
        let (_, time) = Self::totals(pool, project_id, filter).await?;
        let mut rows =
            Self::grouped(pool, project_id, filter, Some((name, op)), 1, 0, time).await?;
        Ok(rows.pop())
    }

    /// How many endpoints the filter matches and the time they take in all.
    async fn totals(pool: &DbPool, project_id: i32, filter: &StatsFilter) -> AppResult<(i64, f64)> {
        let sql = format!(
            "SELECT COUNT(*), COALESCE(SUM(total), 0.0) FROM ( \
                 SELECT SUM(duration_ms) AS total FROM transactions \
                 WHERE project_id = $1 AND {} \
                 GROUP BY transaction_name, op \
             ) g",
            filter.condition()
        );
        let like = filter.like();
        let row: (i64, f64) = sqlx::query_as(sqlx::AssertSqlSafe(&*sql))
            .bind(project_id)
            .bind(filter.environment.as_deref())
            .bind(filter.environment.as_deref())
            .bind(like.as_deref())
            .bind(like.as_deref())
            .fetch_one(pool)
            .await?;
        Ok(row)
    }

    /// The groups the filter matches, or the one `group` names, in the
    /// filter's order.
    #[allow(clippy::too_many_arguments)]
    async fn grouped(
        pool: &DbPool,
        project_id: i32,
        filter: &StatsFilter,
        group: Option<(&str, Option<&str>)>,
        limit: i64,
        offset: i64,
        time: f64,
    ) -> AppResult<Vec<TransactionStatsResponse>> {
        #[cfg(feature = "postgres")]
        let percentiles = ", percentile_cont(0.50) WITHIN GROUP (ORDER BY duration_ms) AS p50, \
                             percentile_cont(0.95) WITHIN GROUP (ORDER BY duration_ms) AS p95, \
                             percentile_cont(0.99) WITHIN GROUP (ORDER BY duration_ms) AS p99";
        #[cfg(not(feature = "postgres"))]
        let percentiles = "";
        let (only, limit_at) = match group {
            Some(_) => (
                "AND transaction_name = $6 AND (op = $7 OR (op IS NULL AND $8 IS NULL))",
                9,
            ),
            None => ("", 6),
        };
        let sql = format!(
            "SELECT transaction_name, op, COUNT(*) AS cnt, {FAILS} AS fails, \
                    SUM(duration_ms) AS total, AVG(duration_ms) AS avg, \
                    MAX(timestamp) AS last_seen {percentiles} \
             FROM transactions \
             WHERE project_id = $1 AND {} {only} \
             GROUP BY transaction_name, op \
             ORDER BY {} DESC, transaction_name ASC, op ASC \
             LIMIT ${limit_at} OFFSET ${}",
            filter.condition(),
            filter.sort.order(),
            limit_at + 1,
        );
        let like = filter.like();
        let mut query = sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .bind(project_id)
            .bind(filter.environment.as_deref())
            .bind(filter.environment.as_deref())
            .bind(like.as_deref())
            .bind(like.as_deref());
        if let Some((name, op)) = group {
            query = query.bind(name).bind(op).bind(op);
        }
        let rows = query.bind(limit).bind(offset).fetch_all(pool).await?;

        let mut out = Vec::with_capacity(rows.len());
        for row in &rows {
            let transaction_name: String = row.get("transaction_name");
            let op: Option<String> = row.get("op");
            let count: i64 = row.get("cnt");
            let failures: i64 = row.get("fails");
            let total_ms: f64 = row.get("total");

            #[cfg(feature = "postgres")]
            let [p50_ms, p95_ms, p99_ms] =
                ["p50", "p95", "p99"].map(|c| row.get::<Option<f64>, _>(c).unwrap_or(0.0));
            #[cfg(not(feature = "postgres"))]
            let [p50_ms, p95_ms, p99_ms] = {
                let mut durations = Self::group_durations(
                    pool,
                    project_id,
                    &transaction_name,
                    op.as_deref(),
                    filter,
                )
                .await?;
                durations.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                [0.50, 0.95, 0.99].map(|p| percentile_cont(&durations, p))
            };

            out.push(TransactionStatsResponse {
                transaction_name,
                op,
                count,
                p50_ms,
                p95_ms,
                p99_ms,
                failure_rate: if count > 0 {
                    failures as f64 / count as f64
                } else {
                    0.0
                },
                avg_ms: row.get("avg"),
                total_ms,
                time_share: if time > 0.0 { total_ms / time } else { 0.0 },
                last_seen: row.get("last_seen"),
            });
        }
        Ok(out)
    }

    /// The durations of one (name, op) group under the filter.
    #[cfg(not(feature = "postgres"))]
    async fn group_durations(
        pool: &DbPool,
        project_id: i32,
        name: &str,
        op: Option<&str>,
        filter: &StatsFilter,
    ) -> AppResult<Vec<f64>> {
        let sql = format!(
            "SELECT duration_ms FROM transactions \
             WHERE project_id = $1 AND {} \
               AND transaction_name = $6 AND (op = $7 OR (op IS NULL AND $8 IS NULL))",
            filter.condition()
        );
        let like = filter.like();
        let rows: Vec<(f64,)> = sqlx::query_as(sqlx::AssertSqlSafe(&*sql))
            .bind(project_id)
            .bind(filter.environment.as_deref())
            .bind(filter.environment.as_deref())
            .bind(like.as_deref())
            .bind(like.as_deref())
            .bind(name)
            .bind(op)
            .bind(op)
            .fetch_all(pool)
            .await?;
        Ok(rows.into_iter().map(|(d,)| d).collect())
    }

    /// One endpoint over the last `period_hours`, or since its oldest call
    /// when `None`, bucket by bucket with no gaps: how often it ran and
    /// failed, and its p50 and p95.
    // ponytail: reads every sample of one endpoint in the window and buckets
    // them here, which is portable; move the bucketing into SQL if an
    // endpoint's window ever holds millions of samples.
    pub async fn group_timeseries(
        pool: &DbPool,
        project_id: i32,
        name: &str,
        op: Option<&str>,
        environment: Option<&str>,
        period_hours: Option<i64>,
    ) -> AppResult<Vec<TransactionTimeseriesPoint>> {
        let sql = format!(
            "SELECT timestamp, duration_ms, status FROM transactions \
             WHERE project_id = $1 AND transaction_name = $2 \
               AND (op = $3 OR (op IS NULL AND $4 IS NULL)) \
               AND duration_ms IS NOT NULL AND ($5 IS NULL OR environment = $6) AND {}",
            since("timestamp", period_hours)
        );
        let rows: Vec<(DateTime<Utc>, f64, Option<String>)> =
            sqlx::query_as(sqlx::AssertSqlSafe(&*sql))
                .bind(project_id)
                .bind(name)
                .bind(op)
                .bind(op)
                .bind(environment)
                .bind(environment)
                .fetch_all(pool)
                .await?;

        let now = Utc::now().timestamp();
        // All time: back to the oldest call, and never under a day.
        let period_hours = period_hours.unwrap_or_else(|| {
            let oldest = rows.iter().map(|(ts, ..)| ts.timestamp()).min();
            oldest.map_or(24, |ts| ((now - ts) / 3600 + 1).max(24))
        });
        let interval = bucket_hours(period_hours) * 3600;
        let align = |ts: i64| ts.div_euclid(interval) * interval;
        let mut buckets: BTreeMap<i64, (Vec<f64>, i64)> = BTreeMap::new();
        let mut at = align(now - period_hours * 3600);
        while at <= align(now) {
            buckets.insert(at, (Vec::new(), 0));
            at += interval;
        }
        for (timestamp, duration, status) in rows {
            // A clock running ahead still counts, in the current bucket, as
            // the stats count it.
            let at = align(timestamp.timestamp().min(now));
            if let Some((durations, failures)) = buckets.get_mut(&at) {
                durations.push(duration);
                *failures += i64::from(is_failure(status.as_deref()));
            }
        }

        Ok(buckets
            .into_iter()
            .map(|(at, (mut durations, failures))| {
                durations.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                TransactionTimeseriesPoint {
                    bucket: DateTime::from_timestamp(at, 0).unwrap_or_default(),
                    count: durations.len() as i64,
                    failures,
                    p50_ms: percentile_cont(&durations, 0.50),
                    p95_ms: percentile_cont(&durations, 0.95),
                }
            })
            .collect())
    }
}

/// Builds one group's response from a row whose percentiles were already
/// computed by the database (Postgres path). `p50`/`p95`/`p99` are nullable in
/// the row type only because `percentile_cont` is; the `duration_ms IS NOT
/// NULL` filter means a group that produced a row always has values.
/// A failed sample: a trace status other than `ok`. As SQL, [`FAILS`].
fn is_failure(status: Option<&str>) -> bool {
    status.is_some_and(|s| s != "ok")
}

/// How many samples failed, as an aggregate.
const FAILS: &str = "SUM(CASE WHEN status IS NOT NULL AND status <> 'ok' THEN 1 ELSE 0 END)";

/// How the endpoints are ranked.
#[derive(Debug, Default, Clone, Copy, serde::Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum StatsSort {
    /// Most time taken in all, calls times duration.
    #[default]
    TimeSpent,
    /// Most called.
    Count,
    /// Slowest on average.
    Avg,
    /// Most often failing.
    FailureRate,
}

impl StatsSort {
    fn order(self) -> &'static str {
        match self {
            Self::TimeSpent => "SUM(duration_ms)",
            Self::Count => "COUNT(*)",
            Self::Avg => "AVG(duration_ms)",
            Self::FailureRate => "SUM(CASE WHEN status IS NOT NULL AND status <> 'ok' THEN 1 ELSE 0 END) * 1.0 / COUNT(*)",
        }
    }
}

/// What narrows the endpoint stats.
#[derive(Debug, Default, Clone)]
pub struct StatsFilter {
    pub environment: Option<String>,
    pub period_hours: Option<i64>,
    /// Keeps the endpoints whose name contains it, ignoring case.
    pub query: Option<String>,
    pub sort: StatsSort,
}

impl StatsFilter {
    /// The condition after `project_id = $1`, binding `$2..=$5`: environment
    /// twice and [`Self::like`] twice.
    fn condition(&self) -> String {
        format!(
            "duration_ms IS NOT NULL AND ($2 IS NULL OR environment = $3) \
             AND ($4 IS NULL OR LOWER(transaction_name) LIKE $5 ESCAPE '\\') AND {}",
            since("timestamp", self.period_hours)
        )
    }

    /// The query as a LIKE pattern, its own `%`, `_` and `\` taken literally.
    fn like(&self) -> Option<String> {
        like_pattern(self.query.as_deref())
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
