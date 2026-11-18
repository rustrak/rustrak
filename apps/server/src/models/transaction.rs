use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

/// Response model for a single transaction in the list view
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TransactionResponse {
    pub id: Uuid,
    /// Sentry event ID: 32 hex digits, not necessarily an RFC 4122 UUID.
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub event_id: Uuid,
    pub transaction_name: String,
    pub timestamp: DateTime<Utc>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub start_timestamp: Option<DateTime<Utc>>,
    /// Duration in milliseconds (timestamp - start_timestamp). None if start_timestamp is missing.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub duration_ms: Option<f64>,
    pub platform: String,
    pub environment: String,
    pub release: String,
    pub ingested_at: DateTime<Utc>,
    /// Trace status (`ok`, `internal_error`...), if the SDK sent one.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub status: Option<String>,
}

/// Response model for a single transaction detail view.
///
/// Carries the same summary fields as [`TransactionResponse`] plus the full
/// Sentry payload under `data` (spans, contexts.trace, measurements, tags,
/// request, user). The frontend builds the span waterfall and metrics view
/// from `data`.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TransactionDetailResponse {
    pub id: Uuid,
    /// Sentry event ID: 32 hex digits, not necessarily an RFC 4122 UUID.
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub event_id: Uuid,
    pub transaction_name: String,
    pub timestamp: DateTime<Utc>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub start_timestamp: Option<DateTime<Utc>>,
    /// Duration in milliseconds (timestamp - start_timestamp). None if start_timestamp is missing.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub duration_ms: Option<f64>,
    pub platform: String,
    pub environment: String,
    pub release: String,
    pub ingested_at: DateTime<Utc>,
    /// Full Sentry transaction payload (spans, contexts, measurements, tags, request, user).
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub data: serde_json::Value,
}

/// A single indexed span — extracted from a transaction, OR standalone
/// (Sentry "span" item type). Both origins write into the same `spans`
/// table, so `transaction_id` is the only thing that distinguishes them.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SpanResponse {
    pub id: Uuid,
    /// The parent transaction, if this span was extracted from one.
    /// `None` for a standalone span.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub transaction_id: Option<Uuid>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub span_id: Option<String>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub trace_id: Option<String>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub parent_span_id: Option<String>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub op: Option<String>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub description: Option<String>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub status: Option<String>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub start_timestamp: Option<DateTime<Utc>>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub timestamp: Option<DateTime<Utc>>,
    /// Duration in milliseconds (timestamp - start_timestamp).
    #[cfg_attr(feature = "openapi", schema(required))]
    pub duration_ms: Option<f64>,
    /// Relay's exclusive (self) time in milliseconds, if provided by the SDK.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub exclusive_time_ms: Option<f64>,
    pub is_segment: bool,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub segment_id: Option<String>,
    /// Set for every span. A standalone span reports its own; a
    /// transaction-embedded one is stamped with its parent transaction's at
    /// write time, rather than being left NULL to be recovered by a JOIN —
    /// the agents dashboard filters `spans` directly and a NULL would drop
    /// the row from an environment-filtered view.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub platform: Option<String>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub release: Option<String>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub environment: Option<String>,
    /// gen_ai.* denormalized fields (story-ai-agent-monitoring.md, GH #180).
    /// All `None` unless the span was recognized as an AI span.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub gen_ai_operation_type: Option<String>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub gen_ai_agent_name: Option<String>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub gen_ai_request_model: Option<String>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub gen_ai_response_model: Option<String>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub gen_ai_tool_name: Option<String>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub gen_ai_conversation_id: Option<String>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub gen_ai_usage_input_tokens: Option<f64>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub gen_ai_usage_output_tokens: Option<f64>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub gen_ai_usage_total_tokens: Option<f64>,
}

/// One span with its full attribute bag — the detail counterpart to
/// [`SpanResponse`].
///
/// The list response deliberately omits attributes: `spans.data` is never
/// trimmed (unlike event data, which goes through `services::event_trim`), so
/// a single AI span can carry a whole prompt and a trace's worth of them would
/// dwarf the waterfall it is drawn from. Attributes are fetched one span at a
/// time instead, which is also how Sentry's own agent drawer works — the span
/// list is light and the details panel loads the selected node's attributes.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SpanDetailResponse {
    #[serde(flatten)]
    pub span: SpanResponse,
    /// The span's attribute bag, flattened to `{key: value}` regardless of
    /// which producer wrote the row — see [`span_attributes`]. This is the
    /// `gen_ai.*` payload (prompts, responses, tool arguments and results,
    /// system instructions, tool definitions) that the agents UI renders.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub attributes: serde_json::Value,
    /// Span tags, when the producer stored any. Spans Protocol v2 has no
    /// separate tags concept — everything is an attribute — so this is always
    /// `None` for a v2-origin span.
    #[cfg_attr(feature = "openapi", schema(required))]
    #[cfg_attr(feature = "openapi", schema(value_type = Option<Object>))]
    pub tags: Option<serde_json::Value>,
}

/// Extracts a span's attribute bag from the raw `spans.data` column.
///
/// The column holds two different shapes depending on which producer wrote
/// the row, and callers must not have to care:
///
/// - Spans Protocol v2 (`digest::processors::span_v2`) stores the already
///   flattened attribute bag itself, so `data` *is* the attributes.
/// - The legacy standalone producer (`digest::processors::span`) and the
///   transaction-embedded one (`digest::processors::transaction`) store the
///   whole span object, whose own `data` key holds the attributes — the same
///   key `extract_gen_ai_columns` normalizes in place.
///
/// A v2 attribute bag cannot contain an object — `SpanV2Entry::flat_attributes`
/// drops those on the way in, as Relay does. Rows written before that
/// validation existed could still hold one, so the wrapper is recognized by
/// two signals rather than one: the nested `data` object *and* the `span_id`
/// that every legacy payload carries at the top level, because all three
/// legacy producers store the span object itself. An attribute bag would only
/// have `span_id` if an SDK set an attribute by that exact name.
pub fn span_attributes(data: &serde_json::Value) -> serde_json::Value {
    let looks_like_a_span_payload = data.get("span_id").is_some();

    match data.get("data") {
        Some(nested) if nested.is_object() && looks_like_a_span_payload => nested.clone(),
        _ => data.clone(),
    }
}

/// Marks a `spans.data` written by [`SpanColumns::compact`]: the span object
/// without the keys its columns give back. Its value is `{"index": n}`, the
/// span's position in its transaction's `spans`, so the payload is rebuilt
/// in the order the SDK sent. `spans.data` holds one of three shapes:
///
/// - this key holding that object: a compact span object (transaction child
///   or legacy standalone span), expanded by [`SpanColumns::expand`];
/// - an object with `span_id` and no marker: a whole span object written
///   before compaction, or an AI root promoted from `contexts.trace`;
/// - anything else: a Spans v2 attribute bag.
///
/// An explicit key rather than the absence of `span_id`, which a span keeps
/// whenever its column was cut short. Its value is an object because a v2
/// bag never holds one (`SpanV2Entry::flat_attributes` drops them), so an
/// attribute of the same name is never read as the marker.
pub const SPAN_POSITION: &str = "_rustrak_position";

/// The `spans` columns that a span object's own top-level keys land in.
pub struct SpanColumns {
    pub span_id: Option<String>,
    pub parent_span_id: Option<String>,
    pub op: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    pub segment_id: Option<String>,
    pub start_timestamp: Option<DateTime<Utc>>,
    pub timestamp: Option<DateTime<Utc>>,
    pub exclusive_time_ms: Option<f64>,
    pub tags: Option<serde_json::Value>,
}

impl SpanColumns {
    /// The JSON each column gives back for its key. A timestamp gives back
    /// epoch seconds only at whole microseconds, the precision both backends
    /// store: an integer on a whole second, otherwise the float nearest its
    /// microseconds, which is what a JSON parser makes of a float written
    /// with at most six decimals.
    fn restored(&self) -> [(&'static str, Option<serde_json::Value>); 10] {
        use serde_json::{Number, Value};
        let text = |v: &Option<String>| v.as_deref().map(Value::from);
        let float = |v: f64| Number::from_f64(v).map(Value::Number);
        let time = |t: Option<DateTime<Utc>>| {
            t.filter(|t| t.timestamp_subsec_nanos() % 1000 == 0)
                .and_then(|t| match t.timestamp_micros() {
                    micros if micros % 1_000_000 == 0 => Some(Value::from(t.timestamp())),
                    // One rounding, from an integer exact below 2^53 µs.
                    micros => float(micros as f64 / 1e6),
                })
        };
        [
            ("span_id", text(&self.span_id)),
            ("parent_span_id", text(&self.parent_span_id)),
            ("op", text(&self.op)),
            ("description", text(&self.description)),
            ("status", text(&self.status)),
            ("segment_id", text(&self.segment_id)),
            ("start_timestamp", time(self.start_timestamp)),
            ("timestamp", time(self.timestamp)),
            ("exclusive_time", self.exclusive_time_ms.and_then(float)),
            ("tags", self.tags.clone()),
        ]
    }

    /// `span` without every key these columns give back exactly, marked with
    /// its `position`. A key whose column differs (cut short, an integer
    /// stored as a float, a sub-microsecond timestamp) stays, so
    /// [`Self::expand`] rebuilds the object the SDK sent. A span that is not
    /// [`Self::compactable`] is kept whole.
    pub fn compact(&self, mut span: serde_json::Value, position: usize) -> serde_json::Value {
        if !Self::compactable(&span) {
            return span;
        }
        let Some(object) = span.as_object_mut() else {
            return span;
        };
        for (key, value) in self.restored() {
            if value.is_some() && object.get(key) == value.as_ref() {
                object.remove(key);
            }
        }
        object.insert(
            SPAN_POSITION.to_string(),
            serde_json::json!({ "index": position }),
        );
        span
    }

    /// An object that does not already use the marker key.
    pub fn compactable(span: &serde_json::Value) -> bool {
        span.as_object()
            .is_some_and(|object| !object.contains_key(SPAN_POSITION))
    }

    /// The span object a compact `data` was made from, with its position;
    /// any other shape as is, with none.
    pub fn expand(&self, mut data: serde_json::Value) -> (Option<u64>, serde_json::Value) {
        let Some(object) = data.as_object_mut() else {
            return (None, data);
        };
        let Some(position) = object
            .get(SPAN_POSITION)
            .and_then(|marker| marker.as_object()?.get("index")?.as_u64())
        else {
            return (None, data);
        };
        object.remove(SPAN_POSITION);
        for (key, value) in self.restored() {
            if let Some(value) = value {
                object.entry(key).or_insert(value);
            }
        }
        (Some(position), data)
    }
}

/// Aggregate performance stats for one (transaction_name, op) group.
/// Powers the performance overview: throughput + latency percentiles +
/// failure rate per transaction. Percentiles are continuous (linear
/// interpolation, matching Postgres `percentile_cont`).
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TransactionStatsResponse {
    pub transaction_name: String,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub op: Option<String>,
    /// Number of transactions in this group.
    pub count: i64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    /// Fraction (0.0–1.0) of transactions whose trace status is set and not "ok".
    pub failure_rate: f64,
    pub avg_ms: f64,
    /// Every sample's duration added up: how much time the endpoint takes in all.
    pub total_ms: f64,
    /// Its share, 0 to 1, of the time every endpoint in the same read takes.
    pub time_share: f64,
    pub last_seen: DateTime<Utc>,
}

/// One bucket of an endpoint's trend: how often it ran, how often it failed
/// and how long it took.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TransactionTimeseriesPoint {
    pub bucket: DateTime<Utc>,
    pub count: i64,
    pub failures: i64,
    pub p50_ms: f64,
    pub p95_ms: f64,
}
