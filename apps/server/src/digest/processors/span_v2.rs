use super::span::{encode_span_rows, insert_span_rows, SpanRow};
use super::ProcessorCtx;
use crate::db::payload::{Column, Encoder, NO_DATA};
use crate::error::AppResult;
use crate::models::span_v2::{parse_span_v2_container, SpanV2Entry};
use crate::protocol::{clamp, clamp_opt};
use crate::services::gen_ai::extract_gen_ai_columns;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use uuid::Uuid;

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
        let mut roots = Vec::new();
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
            let op = entry.op(&flat);

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

            if entry.is_segment && SegmentRoot::has(op.as_deref()) {
                roots.push(SegmentRoot {
                    id: SegmentRoot::id(ctx.project_id, &entry.trace_id, &entry.span_id),
                    trace_id: clamp(&entry.trace_id, 64),
                    span_id: clamp(&entry.span_id, 32),
                    parent_span_id: clamp_opt(entry.parent_span_id.clone(), 32),
                    name: clamp(
                        flat["sentry.segment.name"]
                            .as_str()
                            .or(entry.name.as_deref())
                            .unwrap_or_default(),
                        200,
                    ),
                    op: clamp_opt(op.clone(), 64),
                    // Relay's order: an older SDK's specific status, then
                    // the span's own `ok` or `error`.
                    status: clamp_opt(
                        flat["sentry.status"]
                            .as_str()
                            .map(str::to_string)
                            .or(entry.status.clone()),
                        32,
                    ),
                    source: clamp(
                        flat["sentry.segment.name.source"]
                            .as_str()
                            .unwrap_or("unknown"),
                        32,
                    ),
                    origin: flat["sentry.origin"].as_str().map(str::to_string),
                    start_timestamp,
                    timestamp,
                    duration_ms,
                    platform: clamp(
                        platform
                            .as_deref()
                            .or_else(|| sdk_platform(flat["sentry.sdk.name"].as_str()?))
                            .unwrap_or_default(),
                        64,
                    ),
                    release: release.clone().unwrap_or_default(),
                    environment: environment.clone().unwrap_or_default(),
                    sdk_name: clamp(flat["sentry.sdk.name"].as_str().unwrap_or_default(), 255),
                    sdk_version: clamp(
                        flat["sentry.sdk.version"].as_str().unwrap_or_default(),
                        255,
                    ),
                });
            }

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
                description_id: None,
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

        let rows = encode_span_rows(&ctx.pool, ctx.project_id, ctx.keep_ai_spans, rows).await?;
        let encoder = match roots.is_empty() {
            true => None,
            false => Some(Encoder::current(&ctx.pool, Column::Transactions).await?),
        };
        let mut tx = ctx.pool.begin().await?;
        for root in roots {
            let payload = encoder
                .as_ref()
                .map(|encoder| encoder.encode(root.payload().to_string().as_bytes()));
            // Its id is made from the span's, so the same root sent again
            // is the same transaction.
            sqlx::query(
                "INSERT INTO transactions ( \
                     id, event_id, project_id, transaction_name, \
                     trace_id, span_id, parent_span_id, op, status, source, \
                     start_timestamp, timestamp, duration_ms, \
                     platform, environment, release, sdk_name, sdk_version, \
                     data, payload, remote_addr, ingested_at \
                 ) VALUES ($1, $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, \
                           $13, $14, $15, $16, $17, $18, $19, $20, $21) \
                 ON CONFLICT (project_id, event_id) DO NOTHING",
            )
            .bind(root.id)
            .bind(ctx.project_id)
            .bind(&root.name)
            .bind(&root.trace_id)
            .bind(&root.span_id)
            .bind(&root.parent_span_id)
            .bind(&root.op)
            .bind(&root.status)
            .bind(&root.source)
            .bind(root.start_timestamp)
            .bind(root.timestamp)
            .bind(root.duration_ms)
            .bind(&root.platform)
            .bind(&root.environment)
            .bind(&root.release)
            .bind(&root.sdk_name)
            .bind(&root.sdk_version)
            .bind(NO_DATA)
            .bind(payload)
            .bind(ctx.remote_addr.as_deref())
            .bind(ctx.ingested_at)
            .execute(&mut *tx)
            .await?;
        }
        insert_span_rows(&mut tx, ctx.project_id, rows).await?;
        tx.commit().await?;
        Ok(())
    }
}

/// The root of a segment: what an SDK that streams its spans sends where it
/// used to send a transaction, and what Sentry shows as one (`is_transaction`
/// is its `sentry.is_segment`). It is stored as a `transactions` row, which
/// is what Performance reads, beside its own span row. The segment's other
/// spans are not tied to it here: they arrive before it and long after it,
/// and are found by its id when it is read
/// (`TransactionService::add_segment`).
struct SegmentRoot {
    id: Uuid,
    trace_id: String,
    span_id: String,
    parent_span_id: Option<String>,
    name: String,
    op: Option<String>,
    status: Option<String>,
    source: String,
    origin: Option<String>,
    start_timestamp: DateTime<Utc>,
    timestamp: DateTime<Utc>,
    duration_ms: Option<f64>,
    platform: String,
    release: String,
    environment: String,
    sdk_name: String,
    sdk_version: String,
}

impl SegmentRoot {
    /// Not a web vital's or an interaction's span: Relay takes those out of
    /// segments (`relay-event-normalization/src/eap/mod.rs`), whatever the
    /// SDK marked them as.
    fn has(op: Option<&str>) -> bool {
        !op.is_some_and(|op| op.starts_with("ui.webvital.") || op.starts_with("ui.interaction."))
    }

    /// The same root always gives the same id, so a root delivered again is
    /// the same transaction. A version 8 UUID, since a bare hash has random
    /// version and variant bits, and the client checks a transaction's id
    /// with a strict `z.uuid()` that rejects most of them (#572).
    fn id(project_id: i32, trace_id: &str, span_id: &str) -> Uuid {
        let hash = Sha256::digest(format!("{project_id}:{trace_id}:{span_id}"));
        let bytes: [u8; 16] = hash[..16].try_into().expect("16 bytes");
        uuid::Builder::from_custom_bytes(bytes).into_uuid()
    }

    /// The transaction an SDK would have sent for it, without what its span
    /// row already holds: the attributes, and the vitals among them.
    fn payload(&self) -> serde_json::Value {
        let seconds = |at: DateTime<Utc>| at.timestamp_micros() as f64 / 1e6;
        serde_json::json!({
            "type": "transaction",
            "transaction": self.name,
            "transaction_info": { "source": self.source },
            "start_timestamp": seconds(self.start_timestamp),
            "timestamp": seconds(self.timestamp),
            "platform": self.platform,
            "release": self.release,
            "environment": self.environment,
            "sdk": { "name": self.sdk_name, "version": self.sdk_version },
            "contexts": { "trace": {
                "trace_id": self.trace_id,
                "span_id": self.span_id,
                "parent_span_id": self.parent_span_id,
                "op": self.op,
                "status": self.status,
                "origin": self.origin,
            } },
        })
    }
}

/// The platform an SDK is for, from its name (`sentry.javascript.nextjs`):
/// a streamed span carries no `sentry.platform`. Relay derives none, and
/// Sentry shows `other`.
fn sdk_platform(sdk_name: &str) -> Option<&str> {
    sdk_name.strip_prefix("sentry.")?.split('.').next()
}
