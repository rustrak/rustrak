use super::span::{insert_span_rows, SpanRow};
use super::ProcessorCtx;
use crate::error::AppResult;
use crate::models::span_v2::{parse_span_v2_container, SpanV2Entry};
use crate::protocol::{clamp, clamp_opt};
use crate::services::gen_ai::extract_gen_ai_columns;
use bytes::Bytes;

pub struct SpanV2Processor;

impl SpanV2Processor {
    /// Stores a Spans Protocol v2 batch (Sentry "span" item type,
    /// `application/vnd.sentry.items.span.v2+json`) into the shared `spans`
    /// table, one row per entry, with `transaction_id = NULL` — same
    /// destination as legacy standalone spans ([`super::SpanProcessor`]),
    /// just a different (batched, typed-attribute) wire format. Mirrors
    /// `LogsProcessor`'s container-expansion + single-transaction-batch
    /// pattern.
    pub async fn process(&self, work: Bytes, ctx: &ProcessorCtx) -> AppResult<()> {
        let entries = parse_span_v2_container(&work)?;

        let mut rows = Vec::with_capacity(entries.len());
        for entry in entries {
            // Mirrors the legacy SpanProcessor's validation (Relay's
            // DiscardReason::InvalidSpan/Timestamp) — skip only this entry,
            // not the whole batch, since a batch can carry multiple
            // independent spans.
            if entry.span_id.is_empty() || entry.trace_id.is_empty() {
                log::warn!("span v2 entry missing span_id/trace_id, skipping");
                continue;
            }
            // Both timestamps are required on the wire, and start must not be
            // after end. Mirrors Relay's `validate_timestamps` exactly,
            // including that `start == end` is valid
            // (relay-server/src/processing/spans/process.rs:367).
            let (Some(start_timestamp), Some(timestamp)) =
                (entry.start_timestamp, entry.end_timestamp)
            else {
                log::warn!("span v2 entry missing start_timestamp/end_timestamp, skipping");
                continue;
            };
            if start_timestamp > timestamp {
                log::warn!("span v2 entry start_timestamp is after end_timestamp, skipping");
                continue;
            }

            let mut flat = entry.flat_attributes();
            let op = SpanV2Entry::op(&flat);

            // gen_ai.* normalization — same shared entry point as the legacy
            // standalone-span and transaction-embedded producers. Mutates
            // `flat` in place so the raw `data` JSONB column stays
            // consistent with what was normalized.
            let gen_ai = extract_gen_ai_columns(&mut flat, op.as_deref());

            let duration_ms = (timestamp - start_timestamp)
                .num_microseconds()
                .map(|us| (us as f64 / 1000.0).max(0.0));

            // What the legacy schema kept as top-level span fields, v2 carries
            // as `sentry.*` attributes — read after gen_ai normalization so the
            // columns agree with the stored `data` bag.
            // Each cut to its VARCHAR column, which Postgres enforces.
            let segment_id = clamp_opt(entry.segment_id(&flat), 32);
            let exclusive_time_ms = SpanV2Entry::exclusive_time_ms(&flat);
            let platform = clamp_opt(SpanV2Entry::platform(&flat), 64);
            let release = clamp_opt(SpanV2Entry::release(&flat), 200);
            let environment = clamp_opt(SpanV2Entry::environment(&flat), 64);

            rows.push(SpanRow {
                transaction_id: None,
                span_id: Some(clamp(&entry.span_id, 32)),
                trace_id: Some(clamp(&entry.trace_id, 64)),
                start_timestamp: Some(start_timestamp),
                timestamp: Some(timestamp),
                duration_ms,
                exclusive_time_ms,
                op: clamp_opt(op, 128),
                gen_ai,
                parent_span_id: clamp_opt(entry.parent_span_id, 32),
                description: entry.name,
                status: clamp_opt(entry.status, 32),
                segment_id,
                is_segment: entry.is_segment,
                platform,
                release,
                environment,
                // v2 has no separate tags concept, everything lives in attributes.
                tags: None,
                data: serde_json::json!(flat),
            });
        }

        let mut tx = ctx.pool.begin().await?;
        insert_span_rows(&mut tx, ctx.project_id, ctx.keep_ai_spans, rows).await?;
        tx.commit().await?;
        Ok(())
    }
}
