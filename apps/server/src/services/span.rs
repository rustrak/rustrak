use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::{QueryBuilder, Row};

use crate::db::dialect::{bucket, bucket_time, since_utc};
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::{
    span_attributes, AgentConversation, AgentConversationSummary, AgentDurationPoint,
    AgentModelRow, AgentSummary, AgentTimeseriesPoint, AgentToolRow, AgentTraceSummary,
    ConversationSpan, GenAiBreakdownRow, SpanDetailResponse, SpanResponse,
};
use crate::services::transaction::percentile_cont;

/// Maps a row selecting the denormalized span columns into the shared
/// response shape. The queries spell the column list out literally — sqlx 0.9
/// only accepts `&'static str`, so a shared constant cannot be interpolated
/// into one; this function is what keeps the list and detail paths in step.
fn span_from_row(row: &<crate::db::Db as sqlx::Database>::Row) -> SpanResponse {
    SpanResponse {
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
    }
}

/// The scope every agents-dashboard aggregate is read through.
///
/// `environment` is bound as one parameter used twice — `($n IS NULL OR
/// environment = $n)` — so an absent filter is a no-op without branching the
/// SQL. Every producer stamps `environment` at write time, transaction
/// children and promoted agent roots included, so this matches whatever the
/// SDK reported rather than only standalone spans.
///
/// It does still only match spans that *carry* one: rows ingested before that
/// stamping landed have `NULL` and stay out of any filtered view. Showing
/// fewer rows is the right way to fail here — quietly folding unlabelled
/// spans into a "production only" view would make the number a lie.
#[derive(Debug, Default, Clone)]
pub struct AgentFilters {
    pub environment: Option<String>,
}

impl AgentFilters {
    /// The `AND` clause to splice into a WHERE.
    ///
    /// `index` is the positional parameter this filter's bind occupies, which
    /// differs per query — the breakdowns already spend `$2` on the operation
    /// type. Always emitted, even with no filter set, so the bind happens in
    /// every case and the numbering of later parameters never shifts.
    fn sql(&self, index: usize) -> String {
        format!("AND (${index} IS NULL OR environment = ${index})")
    }
}

/// What narrows and ranks the agent traces list, beyond [`AgentFilters`].
#[derive(Debug, Default, Clone)]
pub struct AgentTraceFilters {
    pub agent: Option<String>,
    pub model: Option<String>,
    pub tool: Option<String>,
    /// Only traces with a failing AI span.
    pub errors: bool,
    pub sort: AgentTraceSort,
}

/// How the agent traces are ranked; every ranking but newest is largest first.
#[derive(Debug, Default, Clone, Copy, serde::Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AgentTraceSort {
    #[default]
    Newest,
    /// The longest AI span, which is what the list shows as the duration.
    Duration,
    Tokens,
    LlmCalls,
    ToolCalls,
    Errors,
}

impl AgentTraceSort {
    fn order_by(self) -> &'static str {
        match self {
            Self::Newest => "started_at DESC",
            Self::Duration => "longest DESC",
            Self::Tokens => "total_tokens DESC",
            Self::LlmCalls => "llm_call_count DESC",
            Self::ToolCalls => "tool_call_count DESC",
            Self::Errors => "error_count DESC",
        }
    }
}

/// The `HAVING` of the traces list: binds `$3..=$6` -- agent, model, tool,
/// and whether only failing traces count.
const TRACE_HAVING: &str = "HAVING ($3 IS NULL OR SUM(CASE WHEN gen_ai_agent_name = $3 THEN 1 ELSE 0 END) > 0) \
       AND ($4 IS NULL OR SUM(CASE WHEN COALESCE(gen_ai_response_model, gen_ai_request_model) = $4 THEN 1 ELSE 0 END) > 0) \
       AND ($5 IS NULL OR SUM(CASE WHEN gen_ai_tool_name = $5 THEN 1 ELSE 0 END) > 0) \
       AND ($6 = FALSE OR SUM(CASE WHEN status IS NOT NULL AND status <> 'ok' THEN 1 ELSE 0 END) > 0)";

pub struct SpanService;

/// Rows of `(group_label, duration_ms, status)` reduced into per-group
/// latency and failure counts.
///
/// Percentiles are computed here rather than in SQL because
/// `percentile_cont` is a Postgres-only aggregate; pulling durations and
/// reducing in Rust is what the transaction stats already do, and it keeps
/// one code path across both dialects.
fn reduce_latency_groups(
    rows: Vec<(String, Option<f64>, Option<String>)>,
) -> Vec<(String, i64, i64, f64, f64)> {
    let mut by_group: HashMap<String, (Vec<f64>, i64, i64)> = HashMap::new();

    for (label, duration, status) in rows {
        let entry = by_group.entry(label).or_insert_with(|| (Vec::new(), 0, 0));
        entry.1 += 1;
        if let Some(ms) = duration {
            entry.0.push(ms);
        }
        if status.is_some_and(|s| s != "ok") {
            entry.2 += 1;
        }
    }

    let mut out: Vec<(String, i64, i64, f64, f64)> = by_group
        .into_iter()
        .map(|(label, (mut durations, count, errors))| {
            durations.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let avg = if durations.is_empty() {
                0.0
            } else {
                durations.iter().sum::<f64>() / durations.len() as f64
            };
            (label, count, errors, avg, percentile_cont(&durations, 0.95))
        })
        .collect();

    // Busiest first — the same ordering every other breakdown on the page
    // uses, so a reader's eye lands in the same place on each table.
    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    out
}

/// Optional equality filters for the span list. A `None` field matches every
/// row; a `Some` value restricts to exact matches on the denormalized column.
/// Matches spans regardless of origin (standalone or transaction-embedded) —
/// both share this table.
#[derive(Debug, Default, Clone)]
pub struct SpanFilters {
    pub op: Option<String>,
    pub status: Option<String>,
    pub trace_id: Option<String>,
    pub operation_type: Option<String>,
}

impl SpanService {
    /// Fetches one span with its full attribute bag.
    ///
    /// Scoped to the project, so a span id belonging to another project is a
    /// 404 rather than a leak. Attributes are normalized across producers by
    /// [`span_attributes`] — the raw column shape differs between the Spans
    /// Protocol v2 path and the two legacy ones, and no caller should have to
    /// know which wrote the row.
    pub async fn get_by_id(
        pool: &DbPool,
        project_id: i32,
        id: uuid::Uuid,
    ) -> AppResult<SpanDetailResponse> {
        let row = sqlx::query(
            r#"
            SELECT id, transaction_id, span_id, trace_id, parent_span_id,
                   op, description, status,
                   start_timestamp, timestamp, duration_ms, exclusive_time_ms,
                   is_segment, segment_id, platform, release, environment,
                   gen_ai_operation_type, gen_ai_agent_name,
                   gen_ai_request_model, gen_ai_response_model,
                   gen_ai_tool_name, gen_ai_conversation_id,
                   gen_ai_usage_input_tokens, gen_ai_usage_output_tokens,
                   gen_ai_usage_total_tokens,
                   tags, data
            FROM spans
            WHERE id = $1 AND project_id = $2
            "#,
        )
        .bind(id)
        .bind(project_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Span {} not found", id)))?;

        let data: serde_json::Value = row.get("data");

        Ok(SpanDetailResponse {
            span: span_from_row(&row),
            attributes: span_attributes(&data),
            tags: row.get("tags"),
        })
    }

    /// Lists spans for a project with offset-based pagination (newest by
    /// start_timestamp first), optionally filtered by op/status/trace_id.
    ///
    /// Each filter uses a `($n IS NULL OR col = $m)` guard so a `None` is a
    /// no-op — static SQL, positional binds (dialect-safe across Postgres and
    /// SQLite). Returns (rows, total).
    pub async fn list_offset(
        pool: &DbPool,
        project_id: i32,
        page: i64,
        per_page: i64,
        filters: &SpanFilters,
    ) -> AppResult<(Vec<SpanResponse>, i64)> {
        let page = page.max(1);
        let offset = crate::pagination::offset(page, per_page);

        let total_count: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM spans
            WHERE project_id = $1
              AND ($2 IS NULL OR op = $3)
              AND ($4 IS NULL OR status = $5)
              AND ($6 IS NULL OR trace_id = $7)
              AND ($8 IS NULL OR gen_ai_operation_type = $9)
            "#,
        )
        .bind(project_id)
        .bind(filters.op.as_deref())
        .bind(filters.op.as_deref())
        .bind(filters.status.as_deref())
        .bind(filters.status.as_deref())
        .bind(filters.trace_id.as_deref())
        .bind(filters.trace_id.as_deref())
        .bind(filters.operation_type.as_deref())
        .bind(filters.operation_type.as_deref())
        .fetch_one(pool)
        .await?;

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
            WHERE project_id = $1
              AND ($2 IS NULL OR op = $3)
              AND ($4 IS NULL OR status = $5)
              AND ($6 IS NULL OR trace_id = $7)
              AND ($8 IS NULL OR gen_ai_operation_type = $9)
            ORDER BY start_timestamp DESC, id DESC
            LIMIT $10 OFFSET $11
            "#,
        )
        .bind(project_id)
        .bind(filters.op.as_deref())
        .bind(filters.op.as_deref())
        .bind(filters.status.as_deref())
        .bind(filters.status.as_deref())
        .bind(filters.trace_id.as_deref())
        .bind(filters.trace_id.as_deref())
        .bind(filters.operation_type.as_deref())
        .bind(filters.operation_type.as_deref())
        .bind(per_page)
        .bind(offset)
        .fetch_all(pool)
        .await?;

        let spans: Vec<SpanResponse> = rows.iter().map(span_from_row).collect();

        Ok((spans, total_count.0))
    }

    /// Time-bucketed count of `gen_ai.operation.type = 'agent'` spans —
    /// powers the "Agent Runs" widget. `interval_hours` controls bucket
    /// width; `period_hours` is an optional lookback window (`None` = all
    /// time). Mirrors `services::session`'s dual-backend bucketing pattern.
    pub async fn agent_runs_timeseries(
        pool: &DbPool,
        project_id: i32,
        filters: &AgentFilters,
        period_hours: Option<i64>,
        interval_hours: i64,
    ) -> AppResult<Vec<AgentTimeseriesPoint>> {
        count_timeseries(
            pool,
            project_id,
            "agent",
            filters,
            period_hours,
            interval_hours,
        )
        .await
    }

    /// Time-bucketed avg/p95 duration for `agent`/`ai_client` spans — powers
    /// the "Duration" widget. Percentile is computed in Rust (continuous,
    /// linear-interpolated via `services::transaction::percentile_cont`)
    /// since SQLite lacks `percentile_cont` — same reasoning and reused
    /// helper as `TransactionService::stats`.
    pub async fn agent_duration_timeseries(
        pool: &DbPool,
        project_id: i32,
        filters: &AgentFilters,
        period_hours: Option<i64>,
        interval_hours: i64,
    ) -> AppResult<Vec<AgentDurationPoint>> {
        let scope = filters.sql(2);
        let time_filter = span_time_filter(period_hours);
        let sql = format!(
            r#"
            SELECT {bucket} AS bucket, duration_ms
            FROM spans
            WHERE project_id = $1
              AND gen_ai_operation_type IN ('agent', 'ai_client')
              AND duration_ms IS NOT NULL
              {scope}
              {time_filter}
            ORDER BY bucket ASC
            "#,
            bucket = bucket("start_timestamp", interval_hours.max(1) * 3600),
        );
        let raw: Vec<(i64, f64)> = sqlx::query_as(sqlx::AssertSqlSafe(&*sql))
            .bind(project_id)
            .bind(filters.environment.as_deref())
            .fetch_all(pool)
            .await?;
        let rows = raw
            .into_iter()
            .map(|(bucket, duration)| Ok((bucket_time(bucket)?, duration)))
            .collect::<AppResult<Vec<(DateTime<Utc>, f64)>>>()?;

        // Group consecutive same-bucket rows (already ORDER BY bucket ASC)
        // and compute avg/p95 per bucket in Rust.
        let mut points = Vec::new();
        let mut current_bucket: Option<DateTime<Utc>> = None;
        let mut current_durations: Vec<f64> = Vec::new();

        let flush = |bucket: DateTime<Utc>,
                     durations: &mut Vec<f64>,
                     points: &mut Vec<AgentDurationPoint>| {
            if durations.is_empty() {
                return;
            }
            durations.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let avg = durations.iter().sum::<f64>() / durations.len() as f64;
            let p95 = percentile_cont(durations, 0.95);
            points.push(AgentDurationPoint {
                bucket,
                avg_ms: avg,
                p95_ms: p95,
            });
            durations.clear();
        };

        for (bucket, duration) in rows {
            if current_bucket != Some(bucket) {
                if let Some(prev) = current_bucket {
                    flush(prev, &mut current_durations, &mut points);
                }
                current_bucket = Some(bucket);
            }
            current_durations.push(duration);
        }
        if let Some(bucket) = current_bucket {
            flush(bucket, &mut current_durations, &mut points);
        }

        Ok(points)
    }

    /// Top-`limit` models by LLM call count (`gen_ai.operation.type =
    /// 'ai_client'`) — powers the "LLM Calls by Model" widget.
    pub async fn llm_calls_by_model(
        pool: &DbPool,
        project_id: i32,
        filters: &AgentFilters,
        period_hours: Option<i64>,
        limit: i64,
    ) -> AppResult<Vec<GenAiBreakdownRow>> {
        breakdown_query(
            pool,
            project_id,
            "gen_ai_response_model",
            "ai_client",
            "CAST(COUNT(*) AS DOUBLE PRECISION)",
            period_hours,
            limit,
            filters,
        )
        .await
    }

    /// Top-`limit` models by total tokens used (`gen_ai.operation.type =
    /// 'ai_client'`) — powers the "Tokens Used by Model" widget.
    pub async fn tokens_by_model(
        pool: &DbPool,
        project_id: i32,
        filters: &AgentFilters,
        period_hours: Option<i64>,
        limit: i64,
    ) -> AppResult<Vec<GenAiBreakdownRow>> {
        breakdown_query(
            pool,
            project_id,
            "gen_ai_response_model",
            "ai_client",
            "CAST(SUM(COALESCE(gen_ai_usage_total_tokens, 0)) AS DOUBLE PRECISION)",
            period_hours,
            limit,
            filters,
        )
        .await
    }

    /// Top-`limit` tools by call count (`gen_ai.operation.type = 'tool'`) —
    /// powers the "Tool Calls by Tool" widget.
    pub async fn tool_calls_by_tool(
        pool: &DbPool,
        project_id: i32,
        filters: &AgentFilters,
        period_hours: Option<i64>,
        limit: i64,
    ) -> AppResult<Vec<GenAiBreakdownRow>> {
        breakdown_query(
            pool,
            project_id,
            "gen_ai_tool_name",
            "tool",
            "CAST(COUNT(*) AS DOUBLE PRECISION)",
            period_hours,
            limit,
            filters,
        )
        .await
    }

    /// Per-`trace_id` aggregate across all AI spans sharing that trace
    /// (standalone and transaction-embedded spans alike, since both share
    /// this table) — powers the "Traces" widget. Offset-paginated, newest
    /// trace first.
    ///
    /// Two-step: aggregate per trace_id (portable GROUP BY), then a
    /// per-trace follow-up to find the representative agent span's name and
    /// duration — same "group query, then per-group follow-up" shape as
    /// `TransactionService::stats`'s `group_durations` call.
    /// Headline totals for the dashboard, over the selected window.
    pub async fn agent_summary(
        pool: &DbPool,
        project_id: i32,
        period_hours: Option<i64>,
        filters: &AgentFilters,
    ) -> AppResult<AgentSummary> {
        let time_filter = span_time_filter(period_hours);

        let scope = filters.sql(2);
        let sql = format!(
            r#"
            SELECT
                SUM(CASE WHEN gen_ai_operation_type = 'agent' THEN 1 ELSE 0 END) AS agent_runs,
                SUM(CASE WHEN gen_ai_operation_type = 'ai_client' THEN 1 ELSE 0 END) AS llm_calls,
                SUM(CASE WHEN gen_ai_operation_type = 'tool' THEN 1 ELSE 0 END) AS tool_calls,
                SUM(CASE WHEN status IS NOT NULL AND status <> 'ok' THEN 1 ELSE 0 END) AS error_count,
                -- CAST for the same reason agent_traces needs it: with no
                -- usage anywhere every arm is a zero literal and SQLite
                -- infers INTEGER, which then fails to decode as f64.
                CAST(SUM(CASE WHEN gen_ai_operation_type = 'agent' THEN 0
                              ELSE COALESCE(gen_ai_usage_total_tokens, 0) END)
                     AS DOUBLE PRECISION) AS total_tokens
            FROM spans
            WHERE project_id = $1
              AND gen_ai_operation_type IS NOT NULL
              {scope}
              {time_filter}
            "#
        );

        let row = sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .bind(project_id)
            .bind(filters.environment.as_deref())
            .fetch_one(pool)
            .await?;

        // A project with no AI spans at all yields a single all-NULL row
        // rather than no row, so every aggregate needs its own zero default.
        let durations_sql = format!(
            r#"
            SELECT duration_ms FROM spans
            WHERE project_id = $1
              AND gen_ai_operation_type IN ('agent', 'ai_client')
              AND duration_ms IS NOT NULL
              {scope}
              {time_filter}
            "#
        );
        let durations: Vec<(f64,)> = sqlx::query_as(sqlx::AssertSqlSafe(&*durations_sql))
            .bind(project_id)
            .bind(filters.environment.as_deref())
            .fetch_all(pool)
            .await?;

        let mut values: Vec<f64> = durations.into_iter().map(|(ms,)| ms).collect();
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let avg_duration_ms = if values.is_empty() {
            0.0
        } else {
            values.iter().sum::<f64>() / values.len() as f64
        };

        let count =
            |key: &str| -> AppResult<i64> { Ok(row.try_get::<Option<i64>, _>(key)?.unwrap_or(0)) };
        Ok(AgentSummary {
            agent_runs: count("agent_runs")?,
            llm_calls: count("llm_calls")?,
            tool_calls: count("tool_calls")?,
            error_count: count("error_count")?,
            total_tokens: row
                .try_get::<Option<f64>, _>("total_tokens")?
                .unwrap_or(0.0),
            avg_duration_ms,
            p95_duration_ms: percentile_cont(&values, 0.95),
        })
    }

    /// Per-model table: volume, failures, latency and the full token split.
    pub async fn models_table(
        pool: &DbPool,
        project_id: i32,
        period_hours: Option<i64>,
        filters: &AgentFilters,
    ) -> AppResult<Vec<AgentModelRow>> {
        let time_filter = span_time_filter(period_hours);

        let scope = filters.sql(2);
        // Response model where reported, request model otherwise: a failed
        // call never gets a response model but still names what it called,
        // and dropping those rows would understate exactly the models that
        // are breaking.
        let sql = format!(
            r#"
            SELECT COALESCE(gen_ai_response_model, gen_ai_request_model) AS model,
                   duration_ms,
                   status,
                   CAST(COALESCE(gen_ai_usage_input_tokens, 0) AS DOUBLE PRECISION)
                       AS input_tokens,
                   CAST(COALESCE(gen_ai_usage_cached_input_tokens, 0) AS DOUBLE PRECISION)
                       AS cached_input_tokens,
                   CAST(COALESCE(gen_ai_usage_output_tokens, 0) AS DOUBLE PRECISION)
                       AS output_tokens,
                   CAST(COALESCE(gen_ai_usage_reasoning_output_tokens, 0) AS DOUBLE PRECISION)
                       AS reasoning_output_tokens,
                   CAST(COALESCE(gen_ai_usage_total_tokens, 0) AS DOUBLE PRECISION)
                       AS total_tokens
            FROM spans
            WHERE project_id = $1
              AND gen_ai_operation_type = 'ai_client'
              AND COALESCE(gen_ai_response_model, gen_ai_request_model) IS NOT NULL
              {scope}
              {time_filter}
            "#
        );

        let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .bind(project_id)
            .bind(filters.environment.as_deref())
            .fetch_all(pool)
            .await?;

        let mut tokens: HashMap<String, [f64; 5]> = HashMap::new();
        let mut latency_rows = Vec::with_capacity(rows.len());

        for row in &rows {
            let model: String = row.get("model");
            let entry = tokens.entry(model.clone()).or_insert([0.0; 5]);
            // CAST in SQL: SQLite hands back INTEGER for a zero literal.
            for (i, key) in [
                "input_tokens",
                "cached_input_tokens",
                "output_tokens",
                "reasoning_output_tokens",
                "total_tokens",
            ]
            .into_iter()
            .enumerate()
            {
                entry[i] += row.try_get::<f64, _>(key)?;
            }

            latency_rows.push((model, row.get("duration_ms"), row.get("status")));
        }

        Ok(reduce_latency_groups(latency_rows)
            .into_iter()
            .map(|(model, requests, errors, avg_ms, p95_ms)| {
                let t = tokens.get(&model).copied().unwrap_or([0.0; 5]);
                AgentModelRow {
                    model,
                    requests,
                    errors,
                    avg_ms,
                    p95_ms,
                    input_tokens: t[0],
                    cached_input_tokens: t[1],
                    output_tokens: t[2],
                    reasoning_output_tokens: t[3],
                    total_tokens: t[4],
                }
            })
            .collect())
    }

    /// Per-tool table: call volume, failures and latency.
    pub async fn tools_table(
        pool: &DbPool,
        project_id: i32,
        period_hours: Option<i64>,
        filters: &AgentFilters,
    ) -> AppResult<Vec<AgentToolRow>> {
        let time_filter = span_time_filter(period_hours);

        let scope = filters.sql(2);
        let sql = format!(
            r#"
            SELECT gen_ai_tool_name AS tool, duration_ms, status
            FROM spans
            WHERE project_id = $1
              AND gen_ai_operation_type = 'tool'
              AND gen_ai_tool_name IS NOT NULL
              {scope}
              {time_filter}
            "#
        );

        let rows: Vec<(String, Option<f64>, Option<String>)> =
            sqlx::query_as(sqlx::AssertSqlSafe(&*sql))
                .bind(project_id)
                .bind(filters.environment.as_deref())
                .fetch_all(pool)
                .await?;

        Ok(reduce_latency_groups(rows)
            .into_iter()
            .map(|(tool, calls, errors, avg_ms, p95_ms)| AgentToolRow {
                tool,
                calls,
                errors,
                avg_ms,
                p95_ms,
            })
            .collect())
    }

    /// Environments present in this project's AI spans — the options the
    /// filter offers, rather than a hardcoded production/staging pair.
    pub async fn agent_environments(pool: &DbPool, project_id: i32) -> AppResult<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            r#"
            SELECT DISTINCT environment FROM spans
            WHERE project_id = $1
              AND gen_ai_operation_type IS NOT NULL
              AND environment IS NOT NULL
            ORDER BY environment ASC
            "#,
        )
        .bind(project_id)
        .fetch_all(pool)
        .await?;

        Ok(rows.into_iter().map(|(env,)| env).collect())
    }

    /// A page of conversations, the most recently active first: every trace
    /// carrying a `gen_ai.conversation.id` on any span is a turn of it, and
    /// all the AI spans of its turns count -- several SDKs only tag the agent
    /// span with the id, not the model and tool calls under it.
    pub async fn agent_conversations(
        pool: &DbPool,
        project_id: i32,
        page: i64,
        per_page: i64,
        period_hours: Option<i64>,
        filters: &AgentFilters,
    ) -> AppResult<(Vec<AgentConversationSummary>, i64)> {
        let offset = crate::pagination::offset(page, per_page);
        let scope = filters.sql(2);
        let time_filter = span_time_filter(period_hours);
        let turns = format!(
            "SELECT DISTINCT gen_ai_conversation_id AS conversation_id, trace_id FROM spans \
             WHERE project_id = $1 AND gen_ai_conversation_id IS NOT NULL AND trace_id IS NOT NULL \
             {scope} {time_filter}"
        );

        // Counted through the page's own join: a conversation whose turns hold
        // no AI span is never a row, so it is never counted either.
        let total: (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT COUNT(DISTINCT t.conversation_id) FROM ({turns}) t \
             JOIN spans s ON s.project_id = $1 AND s.trace_id = t.trace_id \
                         AND s.gen_ai_operation_type IS NOT NULL"
        )))
        .bind(project_id)
        .bind(filters.environment.as_deref())
        .fetch_one(pool)
        .await?;

        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            r#"
            SELECT t.conversation_id,
                   COUNT(DISTINCT t.trace_id) AS turn_count,
                   SUM(CASE WHEN s.gen_ai_operation_type = 'ai_client' THEN 1 ELSE 0 END) AS llm_call_count,
                   SUM(CASE WHEN s.gen_ai_operation_type = 'tool' THEN 1 ELSE 0 END) AS tool_call_count,
                   CAST(SUM(CASE WHEN s.gen_ai_operation_type = 'agent' THEN 0
                                 ELSE COALESCE(s.gen_ai_usage_total_tokens, 0) END)
                        AS DOUBLE PRECISION) AS total_tokens,
                   SUM(CASE WHEN s.status IS NOT NULL AND s.status <> 'ok' THEN 1 ELSE 0 END) AS error_count,
                   COALESCE(MIN(s.start_timestamp), MIN(s.timestamp)) AS first_seen,
                   COALESCE(MAX(s.start_timestamp), MAX(s.timestamp)) AS last_seen
            FROM ({turns}) t
            JOIN spans s ON s.project_id = $1 AND s.trace_id = t.trace_id
                        AND s.gen_ai_operation_type IS NOT NULL
            GROUP BY t.conversation_id
            ORDER BY COALESCE(MAX(s.start_timestamp), MAX(s.timestamp)) IS NULL,
                     last_seen DESC, t.conversation_id ASC
            LIMIT $3 OFFSET $4
            "#
        )))
        .bind(project_id)
        .bind(filters.environment.as_deref())
        .bind(per_page)
        .bind(offset)
        .fetch_all(pool)
        .await?;

        // The page's agents in one query, of the same turns each row counts:
        // the window and environment too. One per row was a scan of the
        // turns per row.
        let ids: Vec<String> = rows.iter().map(|row| row.get("conversation_id")).collect();
        #[cfg(feature = "postgres")]
        let (in_page, ids_param) = ("= ANY($3)", ids);
        #[cfg(not(feature = "postgres"))]
        let (in_page, ids_param) = (
            "IN (SELECT value FROM json_each($3))",
            serde_json::to_string(&ids).unwrap_or_default(),
        );
        let agent_rows: Vec<(String, String)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            r#"
            SELECT t.conversation_id, s.gen_ai_agent_name
            FROM ({turns}) t
            JOIN spans s ON s.project_id = $1 AND s.trace_id = t.trace_id
                        AND s.gen_ai_operation_type = 'agent'
                        AND s.gen_ai_agent_name IS NOT NULL
            WHERE t.conversation_id {in_page}
            GROUP BY t.conversation_id, s.gen_ai_agent_name
            ORDER BY MIN(s.start_timestamp) ASC
            "#
        )))
        .bind(project_id)
        .bind(filters.environment.as_deref())
        .bind(ids_param)
        .fetch_all(pool)
        .await?;
        let mut agents: HashMap<String, Vec<String>> = HashMap::new();
        for (conversation_id, name) in agent_rows {
            agents.entry(conversation_id).or_default().push(name);
        }

        let mut conversations = Vec::with_capacity(rows.len());
        for row in &rows {
            let conversation_id: String = row.get("conversation_id");
            conversations.push(AgentConversationSummary {
                agent_names: agents.remove(&conversation_id).unwrap_or_default(),
                conversation_id,
                turn_count: row.get("turn_count"),
                llm_call_count: row.get("llm_call_count"),
                tool_call_count: row.get("tool_call_count"),
                total_tokens: row.get("total_tokens"),
                error_count: row.get("error_count"),
                first_seen: row.get("first_seen"),
                last_seen: row.get("last_seen"),
            });
        }
        Ok((conversations, total.0))
    }

    /// Every AI span of a conversation's turns, oldest first, with what went
    /// in and came out. `NotFound` when no span carries the id.
    pub async fn agent_conversation(
        pool: &DbPool,
        project_id: i32,
        conversation_id: &str,
    ) -> AppResult<AgentConversation> {
        let rows = sqlx::query(
            r#"
            SELECT id, trace_id, span_id, parent_span_id, gen_ai_operation_type,
                   gen_ai_agent_name, COALESCE(gen_ai_response_model, gen_ai_request_model) AS model,
                   gen_ai_tool_name, status, start_timestamp, duration_ms,
                   gen_ai_usage_total_tokens, data
            FROM spans
            WHERE project_id = $1 AND gen_ai_operation_type IS NOT NULL
              AND trace_id IN (SELECT trace_id FROM spans
                               WHERE project_id = $1 AND gen_ai_conversation_id = $2)
            ORDER BY start_timestamp ASC, id ASC
            "#,
        )
        .bind(project_id)
        .bind(conversation_id)
        .fetch_all(pool)
        .await?;
        if rows.is_empty() {
            return Err(AppError::NotFound(format!(
                "conversation {conversation_id} not found"
            )));
        }

        let spans = rows
            .iter()
            .map(|row| {
                let data: serde_json::Value = row.get("data");
                let attributes = span_attributes(&data);
                let text = |keys: &[&str]| {
                    keys.iter().find_map(|key| match attributes.get(*key)? {
                        serde_json::Value::String(s) if !s.is_empty() => Some(s.clone()),
                        serde_json::Value::Null => None,
                        other => Some(other.to_string()),
                    })
                };
                ConversationSpan {
                    id: row.get("id"),
                    trace_id: row.get("trace_id"),
                    span_id: row.get("span_id"),
                    parent_span_id: row.get("parent_span_id"),
                    operation_type: row.get("gen_ai_operation_type"),
                    agent_name: row.get("gen_ai_agent_name"),
                    model: row.get("model"),
                    tool_name: row.get("gen_ai_tool_name"),
                    status: row.get("status"),
                    start_timestamp: row.get("start_timestamp"),
                    duration_ms: row.get("duration_ms"),
                    total_tokens: row.get("gen_ai_usage_total_tokens"),
                    // The dashboard's chains, in its order (`model/gen-ai.ts`).
                    input: text(&[
                        "gen_ai.input.messages",
                        "gen_ai.request.messages",
                        "ai.input_messages",
                        "ai.prompt",
                        "gen_ai.tool.call.arguments",
                        "gen_ai.tool.input",
                    ]),
                    output: text(&[
                        "gen_ai.output.messages",
                        "gen_ai.response.text",
                        "gen_ai.tool.call.result",
                        "gen_ai.tool.output",
                    ]),
                    tool_calls: text(&["gen_ai.response.tool_calls"]),
                }
            })
            .collect();

        Ok(AgentConversation {
            conversation_id: conversation_id.to_string(),
            spans,
        })
    }

    pub async fn agent_traces(
        pool: &DbPool,
        project_id: i32,
        page: i64,
        per_page: i64,
        period_hours: Option<i64>,
        filters: &AgentFilters,
        trace: &AgentTraceFilters,
    ) -> AppResult<(Vec<AgentTraceSummary>, i64)> {
        let offset = crate::pagination::offset(page, per_page);

        let scope = filters.sql(2);
        let time_filter = span_time_filter(period_hours);

        let total_sql = format!(
            r#"
            SELECT COUNT(*) FROM (
                SELECT 1 FROM spans {TRACE_INDEX}
                WHERE project_id = $1 AND gen_ai_operation_type IS NOT NULL AND trace_id IS NOT NULL
                  {scope}
                  {time_filter}
                GROUP BY trace_id
                {TRACE_HAVING}
            ) g
            "#
        );
        let total: (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(&*total_sql))
            .bind(project_id)
            .bind(filters.environment.as_deref())
            .bind(trace.agent.as_deref())
            .bind(trace.model.as_deref())
            .bind(trace.tool.as_deref())
            .bind(trace.errors)
            .fetch_one(pool)
            .await?;
        let order_by = trace.sort.order_by();

        // total_tokens excludes operation_type='agent' rows: an 'agent' span
        // represents orchestration, not token consumption — its own
        // gen_ai.usage.* attributes, when present, are a client-side ROLLUP
        // of its 'ai_client' children (Sentry's SDKs accumulate child totals
        // onto the trace root — see story-span-v2-protocol.md's
        // root-span-promotion follow-up). Summing both would double-count.
        // This exclusion is unconditional (matches real Sentry's Traces
        // table query exactly — no root-only fallback: a trace with only a
        // promoted root/agent span and no ai_client children legitimately
        // reports 0 tokens in Sentry too, see tracesTable.tsx).
        let group_sql = format!(
            r#"
            SELECT trace_id,
                   -- An agent span carries the aggregate usage of its own
                   -- children, so counting it alongside them doubles the
                   -- trace's tokens. Sentry's Traces table excludes agent
                   -- runs from this sum for the same reason.
                   -- CAST keeps the sum floating even when every arm is a
                   -- zero literal: SQLite would otherwise infer INTEGER for a
                   -- trace whose spans report no usage and fail to decode.
                   CAST(SUM(CASE WHEN gen_ai_operation_type = 'agent' THEN 0
                                 ELSE COALESCE(gen_ai_usage_total_tokens, 0) END)
                        AS DOUBLE PRECISION) AS total_tokens,
                   SUM(CASE WHEN gen_ai_operation_type = 'tool' THEN 1 ELSE 0 END) AS tool_call_count,
                   SUM(CASE WHEN gen_ai_operation_type = 'ai_client' THEN 1 ELSE 0 END) AS llm_call_count,
                   -- `status IS NOT NULL` matters: most spans never report a
                   -- status at all, and `status <> 'ok'` alone is NULL (not
                   -- true) for those, so the CASE would never count them --
                   -- correct here, but only by accident. Being explicit keeps
                   -- it correct if the arms are ever reordered.
                   SUM(CASE WHEN status IS NOT NULL AND status <> 'ok' THEN 1 ELSE 0 END) AS error_count,
                   MIN(start_timestamp) AS started_at,
                   COALESCE(MAX(duration_ms), 0) AS longest
            FROM spans {TRACE_INDEX}
            WHERE project_id = $1 AND gen_ai_operation_type IS NOT NULL AND trace_id IS NOT NULL
              {scope}
              {time_filter}
            GROUP BY trace_id
            {TRACE_HAVING}
            ORDER BY {order_by}, started_at DESC, trace_id ASC
            LIMIT $7 OFFSET $8
            "#
        );
        let group_rows = sqlx::query(sqlx::AssertSqlSafe(&*group_sql))
            .bind(project_id)
            .bind(filters.environment.as_deref())
            .bind(trace.agent.as_deref())
            .bind(trace.model.as_deref())
            .bind(trace.tool.as_deref())
            .bind(trace.errors)
            .bind(per_page)
            .bind(offset)
            .fetch_all(pool)
            .await?;

        let trace_ids: Vec<String> = group_rows.iter().map(|row| row.get("trace_id")).collect();
        let mut durations = Self::representative_durations(pool, project_id, &trace_ids).await?;
        let mut agent_names = Self::trace_agent_names(pool, project_id, &trace_ids).await?;

        let mut traces = Vec::with_capacity(group_rows.len());
        for row in &group_rows {
            let trace_id: String = row.get("trace_id");
            let total_tokens: f64 = row.get("total_tokens");
            let tool_call_count: i64 = row.get("tool_call_count");
            let llm_call_count: i64 = row.get("llm_call_count");
            let error_count: i64 = row.get("error_count");
            let started_at: DateTime<Utc> = row.get("started_at");

            let duration_ms = durations.remove(&trace_id).unwrap_or(None);
            let agent_names = agent_names.remove(&trace_id).unwrap_or_default();

            traces.push(AgentTraceSummary {
                trace_id,
                agent_names,
                duration_ms,
                total_tokens,
                tool_call_count,
                llm_call_count,
                error_count,
                started_at,
            });
        }

        Ok((traces, total.0))
    }

    /// Duration of the span representing each trace: the earliest
    /// `agent`-type span if one exists, else the earliest AI span of any type.
    /// Batched over a whole page of traces — one query, not two per row.
    async fn representative_durations(
        pool: &DbPool,
        project_id: i32,
        trace_ids: &[String],
    ) -> AppResult<HashMap<String, Option<f64>>> {
        if trace_ids.is_empty() {
            return Ok(HashMap::new());
        }

        // ROW_NUMBER partitioned per trace reproduces the agent-first,
        // then-earliest preference that the two LIMIT 1 queries encoded.
        const RANKED: &str = r#"
            SELECT trace_id, duration_ms FROM (
                SELECT trace_id, duration_ms,
                       ROW_NUMBER() OVER (
                           PARTITION BY trace_id
                           ORDER BY CASE WHEN gen_ai_operation_type = 'agent' THEN 0 ELSE 1 END ASC,
                                    start_timestamp ASC
                       ) AS rn
                FROM spans
                WHERE project_id = "#;

        let mut qb = QueryBuilder::new(RANKED);
        qb.push_bind(project_id);
        qb.push(" AND gen_ai_operation_type IS NOT NULL AND trace_id IN (");
        let mut sep = qb.separated(", ");
        for trace_id in trace_ids {
            sep.push_bind(trace_id.clone());
        }
        qb.push(")) t WHERE rn = 1");
        let rows: Vec<(String, Option<f64>)> = qb.build_query_as().fetch_all(pool).await?;

        Ok(rows.into_iter().collect())
    }

    /// Every distinct agent name per trace, earliest first — a trace with
    /// handoffs runs more than one agent, and Sentry's Traces table lists
    /// them all. `gen_ai_agent_name` already carries the `function_id`
    /// fallback, resolved during gen_ai normalization at ingestion.
    async fn trace_agent_names(
        pool: &DbPool,
        project_id: i32,
        trace_ids: &[String],
    ) -> AppResult<HashMap<String, Vec<String>>> {
        if trace_ids.is_empty() {
            return Ok(HashMap::new());
        }

        const SELECT: &str = r#"
            SELECT trace_id, gen_ai_agent_name
            FROM spans
            WHERE gen_ai_operation_type = 'agent'
              AND gen_ai_agent_name IS NOT NULL
              AND project_id = "#;

        let mut qb = QueryBuilder::new(SELECT);
        qb.push_bind(project_id);
        qb.push(" AND trace_id IN (");
        let mut sep = qb.separated(", ");
        for trace_id in trace_ids {
            sep.push_bind(trace_id.clone());
        }
        qb.push(") ORDER BY start_timestamp ASC");
        let rows: Vec<(String, String)> = qb.build_query_as().fetch_all(pool).await?;

        let mut out: HashMap<String, Vec<String>> = HashMap::new();
        for (trace_id, agent_name) in rows {
            let names = out.entry(trace_id).or_default();
            // The same agent can span many rows; keep first-seen order.
            if !names.contains(&agent_name) {
                names.push(agent_name);
            }
        }
        Ok(out)
    }
}

/// The index the traces list reads. SQLite's planner prefers the narrower
/// `(project_id, trace_id)` index for its GROUP BY and then reads every span's
/// row, prompts and all; the covering one answers it from the index alone
/// (~490ms to ~40ms over 300k AI spans). Postgres picks it by itself.
#[cfg(feature = "postgres")]
const TRACE_INDEX: &str = "";
#[cfg(not(feature = "postgres"))]
const TRACE_INDEX: &str = "INDEXED BY idx_spans_gen_ai_trace_cover";

/// `AND start_timestamp >= ...` time filter for a `spans` query.
fn span_time_filter(period_hours: Option<i64>) -> String {
    format!("AND {}", since_utc("start_timestamp", period_hours))
}

/// Time-bucketed `COUNT(*)` of spans matching `operation_type`.
async fn count_timeseries(
    pool: &DbPool,
    project_id: i32,
    operation_type: &str,
    filters: &AgentFilters,
    period_hours: Option<i64>,
    interval_hours: i64,
) -> AppResult<Vec<AgentTimeseriesPoint>> {
    bucketed_aggregate(
        pool,
        project_id,
        operation_type,
        "CAST(COUNT(*) AS DOUBLE PRECISION)",
        filters,
        period_hours,
        interval_hours,
    )
    .await
}

/// Shared time-bucketing core for the "Agent Runs" widget —
/// `aggregate_expr` is a pre-cast SQL expression (`CAST(... AS DOUBLE
/// PRECISION)`) so both dialects decode into `f64` uniformly.
#[allow(clippy::too_many_arguments)]
async fn bucketed_aggregate(
    pool: &DbPool,
    project_id: i32,
    operation_type: &str,
    aggregate_expr: &str,
    filters: &AgentFilters,
    period_hours: Option<i64>,
    interval_hours: i64,
) -> AppResult<Vec<AgentTimeseriesPoint>> {
    // $3, not $2: this query already spends $2 on the operation type.
    let scope = filters.sql(3);
    let time_filter = span_time_filter(period_hours);
    let sql = format!(
        r#"
        SELECT
            {bucket} AS bucket,
            {aggregate_expr} AS value,
            CAST(SUM(CASE WHEN status IS NOT NULL AND status <> 'ok' THEN 1 ELSE 0 END) AS BIGINT) AS errors
        FROM spans
        WHERE project_id = $1
          AND gen_ai_operation_type = $2
          {scope}
          {time_filter}
        GROUP BY 1
        ORDER BY 1 ASC
        "#,
        bucket = bucket("start_timestamp", interval_hours.max(1) * 3600),
    );
    let rows: Vec<(i64, f64, i64)> = sqlx::query_as(sqlx::AssertSqlSafe(&*sql))
        .bind(project_id)
        .bind(operation_type)
        .bind(filters.environment.as_deref())
        .fetch_all(pool)
        .await?;
    rows.into_iter()
        .map(|(bucket, value, errors)| {
            Ok(AgentTimeseriesPoint {
                bucket: bucket_time(bucket)?,
                value,
                errors,
            })
        })
        .collect()
}

/// Shared "GROUP BY column, aggregate, top N" query for the 3 breakdown
/// widgets (LLM Calls by Model, Tokens by Model, Tool Calls by Tool).
/// `group_column`/`aggregate_expr` are Rust string literals the caller
/// controls (never user input) — safe to interpolate via `format!`, same
/// trust level as `services::session`'s interval interpolation.
// One more parameter than clippy's default ceiling. Bundling them into a
// struct would buy nothing: this is a private helper with three call sites in
// this file, all of them naming every field.
#[allow(clippy::too_many_arguments)]
async fn breakdown_query(
    pool: &DbPool,
    project_id: i32,
    group_column: &str,
    operation_type: &str,
    aggregate_expr: &str,
    period_hours: Option<i64>,
    limit: i64,
    filters: &AgentFilters,
) -> AppResult<Vec<GenAiBreakdownRow>> {
    // The `param(minimum/maximum)` on AgentBreakdownQuery is OpenAPI docs only
    // — Postgres rejects a negative LIMIT and SQLite reads it as "no cap".
    let limit = limit.clamp(1, 100);

    let time_filter = span_time_filter(period_hours);

    let scope = filters.sql(3);

    let sql = format!(
        r#"
        SELECT {group_column} AS label, {aggregate_expr} AS value
        FROM spans
        WHERE project_id = $1
          AND gen_ai_operation_type = $2
          AND {group_column} IS NOT NULL
          {scope}
          {time_filter}
        GROUP BY {group_column}
        ORDER BY value DESC, {group_column} ASC
        LIMIT $4
        "#,
    );
    let rows: Vec<(String, f64)> = sqlx::query_as(sqlx::AssertSqlSafe(&*sql))
        .bind(project_id)
        .bind(operation_type)
        .bind(filters.environment.as_deref())
        .bind(limit)
        .fetch_all(pool)
        .await?;

    Ok(rows
        .into_iter()
        .map(|(label, value)| GenAiBreakdownRow { label, value })
        .collect())
}

#[cfg(all(test, not(feature = "postgres")))]
mod tests {
    use super::*;

    /// `start_timestamp` is stored as RFC 3339 (`T`-separated), so a bound
    /// in `datetime()`'s space-separated form let in the whole calendar day
    /// before the window.
    #[tokio::test]
    async fn span_time_filter_leaves_out_spans_before_the_window() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query("CREATE TABLE spans (start_timestamp TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        let start = |minutes: i64| Utc::now() - chrono::Duration::minutes(minutes);
        for minutes in [24 * 60 + 1, 24 * 60 - 1] {
            sqlx::query("INSERT INTO spans VALUES ($1)")
                .bind(start(minutes))
                .execute(&pool)
                .await
                .unwrap();
        }
        let sql = format!(
            "SELECT COUNT(*) FROM spans WHERE 1 = 1 {}",
            span_time_filter(Some(24))
        );
        let (n,): (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(n, 1, "only the span inside the last 24 hours");
    }
}
