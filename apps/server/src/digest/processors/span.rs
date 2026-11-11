use super::ProcessorCtx;
use crate::error::{AppError, AppResult};
use crate::protocol::{clamp_opt, extract_timestamp, is_stale};
use crate::services::gen_ai::{extract_gen_ai_columns, is_ai_span, GenAiColumns};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use uuid::Uuid;

pub struct SpanProcessor;

/// One row of the `spans` table, whichever producer it came from: a
/// standalone span item, a v2 container entry, or a transaction's child.
pub(crate) struct SpanRow {
    pub transaction_id: Option<Uuid>,
    pub span_id: Option<String>,
    pub trace_id: Option<String>,
    pub start_timestamp: Option<DateTime<Utc>>,
    pub timestamp: Option<DateTime<Utc>>,
    pub duration_ms: Option<f64>,
    pub exclusive_time_ms: Option<f64>,
    pub op: Option<String>,
    pub gen_ai: GenAiColumns,
    pub parent_span_id: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    pub segment_id: Option<String>,
    pub is_segment: bool,
    pub platform: Option<String>,
    pub release: Option<String>,
    pub environment: Option<String>,
    pub tags: Option<serde_json::Value>,
    pub data: serde_json::Value,
}

impl SpanRow {
    /// The row of one span object, from its own fields only: a producer that
    /// knows more (a transaction's id, trace and scope) sets it afterwards.
    pub(crate) fn from_span(mut span: serde_json::Value) -> Self {
        let start_timestamp = extract_timestamp(&span, "start_timestamp");
        let timestamp = extract_timestamp(&span, "timestamp");
        let duration_ms = match (start_timestamp, timestamp) {
            (Some(st), Some(ts)) => (ts - st)
                .num_microseconds()
                .map(|us| (us as f64 / 1000.0).max(0.0)),
            _ => None,
        };
        // Relay's span `op` max_chars is 128.
        let op = text(&span, "op", 128);
        // gen_ai.* normalization + cost, on the span's own `data` attributes
        // bag. Mutates it in place, so it runs before `span` moves into `data`.
        let gen_ai = match span.get_mut("data") {
            Some(span_data) => extract_gen_ai_columns(span_data, op.as_deref()),
            None => GenAiColumns::default(),
        };
        Self {
            transaction_id: None,
            span_id: text(&span, "span_id", 32),
            trace_id: text(&span, "trace_id", 64),
            start_timestamp,
            timestamp,
            duration_ms,
            // Relay's exclusive_time is already in milliseconds.
            exclusive_time_ms: span.get("exclusive_time").and_then(|v| v.as_f64()),
            op,
            gen_ai,
            parent_span_id: text(&span, "parent_span_id", 32),
            description: span
                .get("description")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            status: text(&span, "status", 32),
            segment_id: text(&span, "segment_id", 32),
            is_segment: span
                .get("is_segment")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            platform: text(&span, "platform", 64),
            release: text(&span, "release", 200),
            environment: text(&span, "environment", 64),
            // The spec promises an object; anything else is dropped.
            tags: span.get("tags").filter(|t| t.is_object()).cloned(),
            data: span,
        }
    }
}

/// Rows per INSERT. A span is 31 binds, so 256 stays well under both SQLite's
/// 32,766 and Postgres's 65,535 bind limits.
const SPAN_ROWS_PER_INSERT: usize = 256;

/// Inserts `rows` with one multi-row INSERT per [`SPAN_ROWS_PER_INSERT`],
/// inside the caller's transaction. One statement per span was one database
/// round trip per span, on the ingest path the SDK waits on: a transaction
/// with 1,000 spans spent most of its time there.
///
/// A standalone span already stored under the same trace and span id is
/// skipped, which makes an SDK retry a no-op. The conflict target is a partial
/// index on `transaction_id IS NULL`, so a transaction's children never match.
///
/// With `keep_ai_spans` false -- the project turned agents off -- every AI
/// span is left out here, the one place all three span producers go
/// through, and the rest of the trace is stored as usual.
pub(crate) async fn insert_span_rows(
    conn: &mut <crate::db::Db as sqlx::Database>::Connection,
    project_id: i32,
    keep_ai_spans: bool,
    rows: Vec<SpanRow>,
) -> AppResult<()> {
    let mut rows = rows
        .into_iter()
        // An AI span by its attributes, or by its `op` alone when it came
        // without a `data` bag: the same test Relay applies (`is_ai_span`).
        .filter(|row| {
            keep_ai_spans
                || (row.gen_ai.operation_type.is_none()
                    && !is_ai_span(&serde_json::Value::Null, row.op.as_deref()))
        })
        .peekable();
    while rows.peek().is_some() {
        let mut query = sqlx::QueryBuilder::new(
            r#"INSERT INTO spans (
                id, transaction_id, project_id,
                span_id, trace_id, parent_span_id,
                op, description, status,
                start_timestamp, timestamp, duration_ms, exclusive_time_ms,
                is_segment, segment_id,
                platform, release, environment,
                tags, data,
                gen_ai_operation_type, gen_ai_agent_name,
                gen_ai_request_model, gen_ai_response_model,
                gen_ai_tool_name, gen_ai_conversation_id,
                gen_ai_usage_input_tokens, gen_ai_usage_output_tokens, gen_ai_usage_total_tokens,
                gen_ai_usage_cached_input_tokens, gen_ai_usage_reasoning_output_tokens
            ) "#,
        );
        query.push_values(rows.by_ref().take(SPAN_ROWS_PER_INSERT), |mut row, span| {
            row.push_bind(Uuid::new_v4())
                .push_bind(span.transaction_id)
                .push_bind(project_id)
                .push_bind(span.span_id)
                .push_bind(span.trace_id)
                .push_bind(span.parent_span_id)
                .push_bind(span.op)
                .push_bind(span.description)
                .push_bind(span.status)
                .push_bind(span.start_timestamp)
                .push_bind(span.timestamp)
                .push_bind(span.duration_ms)
                .push_bind(span.exclusive_time_ms)
                .push_bind(span.is_segment)
                .push_bind(span.segment_id)
                .push_bind(span.platform)
                .push_bind(span.release)
                .push_bind(span.environment)
                .push_bind(span.tags.map(sqlx::types::Json))
                .push_bind(sqlx::types::Json(span.data))
                .push_bind(span.gen_ai.operation_type)
                .push_bind(span.gen_ai.agent_name)
                .push_bind(span.gen_ai.request_model)
                .push_bind(span.gen_ai.response_model)
                .push_bind(span.gen_ai.tool_name)
                .push_bind(span.gen_ai.conversation_id)
                .push_bind(span.gen_ai.usage_input_tokens)
                .push_bind(span.gen_ai.usage_output_tokens)
                .push_bind(span.gen_ai.usage_total_tokens)
                .push_bind(span.gen_ai.usage_cached_input_tokens)
                .push_bind(span.gen_ai.usage_reasoning_output_tokens);
        });
        query.push(
            " ON CONFLICT (project_id, trace_id, span_id)
                WHERE transaction_id IS NULL
                  AND trace_id IS NOT NULL
                  AND span_id IS NOT NULL DO NOTHING",
        );
        query.build().execute(&mut *conn).await?;
    }
    Ok(())
}

/// A string field cut to its VARCHAR column, which Postgres enforces.
pub(crate) fn text(data: &serde_json::Value, key: &str, max_chars: usize) -> Option<String> {
    clamp_opt(
        data.get(key).and_then(|v| v.as_str()).map(str::to_string),
        max_chars,
    )
}

/// Bound parsed-span memory while batching.
const SPAN_BATCH_CHUNK: usize = 64;

impl SpanProcessor {
    /// Parses and validates one standalone span payload (Relay's legacy
    /// "span" schema — one flat span object per envelope item).
    fn parse_item(work: &[u8]) -> AppResult<SpanRow> {
        let data: serde_json::Value = serde_json::from_slice(work)
            .map_err(|e| AppError::Validation(format!("Invalid span JSON: {e}")))?;

        // span_id and trace_id are required for a standalone span to be
        // accepted.
        for key in ["span_id", "trace_id"] {
            if data
                .get(key)
                .and_then(|v| v.as_str())
                .is_none_or(str::is_empty)
            {
                return Err(AppError::Validation(format!("span missing {key}")));
            }
        }

        // Relay's `TimestampProcessor` removes a span from before 1970.
        if is_stale(&data, "start_timestamp") || is_stale(&data, "timestamp") {
            return Err(AppError::Validation(
                "span timestamp is before 1970".to_string(),
            ));
        }

        // Both timestamps are required — Relay's `validate_standalone_span`
        // rejects the span outright when either is absent.
        let start_timestamp = extract_timestamp(&data, "start_timestamp")
            .ok_or_else(|| AppError::Validation("span missing start_timestamp".to_string()))?;
        let timestamp = extract_timestamp(&data, "timestamp")
            .ok_or_else(|| AppError::Validation("span missing timestamp".to_string()))?;

        // Mirrors Relay's DiscardReason::Timestamp.
        if start_timestamp > timestamp {
            return Err(AppError::Validation(
                "span start_timestamp is after timestamp".to_string(),
            ));
        }

        Ok(SpanRow::from_span(data))
    }

    /// Writes one chunk in one transaction.
    async fn insert_all(&self, ctx: &ProcessorCtx, parsed: Vec<SpanRow>) -> AppResult<()> {
        let mut tx = ctx.pool.begin().await?;
        insert_span_rows(&mut tx, ctx.project_id, ctx.keep_ai_spans, parsed).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Writes items in transactions of at most [`SPAN_BATCH_CHUNK`],
    /// interleaving parse and write so only one chunk's trees are alive;
    /// malformed items are skipped (the old per-item tolerance).
    pub async fn process_batch(&self, items: Vec<Bytes>, ctx: &ProcessorCtx) -> AppResult<()> {
        let mut chunk: Vec<SpanRow> = Vec::with_capacity(SPAN_BATCH_CHUNK);
        for work in items {
            match Self::parse_item(&work) {
                Ok(item) => {
                    chunk.push(item);
                    if chunk.len() == SPAN_BATCH_CHUNK {
                        self.insert_all(ctx, std::mem::take(&mut chunk)).await?;
                    }
                }
                Err(e) => log::warn!("span item rejected: {e:?}"),
            }
        }
        if !chunk.is_empty() {
            self.insert_all(ctx, chunk).await?;
        }
        Ok(())
    }
}

impl SpanProcessor {
    /// Stores a standalone span payload into `spans` with
    /// `transaction_id = NULL`. Mirrors `TransactionProcessor::insert_span`'s
    /// column extraction so both producers write a consistent row shape.
    /// Single items share the batched write path.
    pub async fn process(&self, work: Bytes, ctx: &ProcessorCtx) -> AppResult<()> {
        let item = Self::parse_item(&work)?;
        self.insert_all(ctx, vec![item]).await
    }
}

#[cfg(test)]
mod tests {
    use super::SpanProcessor;

    #[test]
    fn parse_item_validates_shape() {
        assert!(SpanProcessor::parse_item(br#"{}"#).is_err());
        assert!(SpanProcessor::parse_item(
            br#"{"span_id":"s","trace_id":"t","start_timestamp":1.0,"timestamp":2.0}"#
        )
        .is_ok());
    }

    #[test]
    fn parse_item_drops_non_object_tags() {
        let span = |tags: &str| {
            let item = format!(
                r#"{{"span_id":"s","trace_id":"t","start_timestamp":1.0,"timestamp":2.0,"tags":{tags}}}"#
            );
            SpanProcessor::parse_item(item.as_bytes()).unwrap().tags
        };
        assert_eq!(span(r#"[["k","v"]]"#), None);
        assert_eq!(span(r#"{"k":"v"}"#), Some(serde_json::json!({"k": "v"})));
    }
}
