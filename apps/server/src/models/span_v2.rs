//! Sentry Spans Protocol v2 model (`application/vnd.sentry.items.span.v2+json`).
//!
//! Modern Sentry SDKs (verified: `@sentry/node` 10.65 + Vercel AI SDK's
//! `vercelAIIntegration()`) send OTel-instrumented spans — including every
//! AI Agent Monitoring span — in this batched, typed-attribute wire format,
//! not the legacy one-span-per-item format (`models::transaction`'s inline
//! span handling / `SpanProcessor`).
//!
//! Wire shape: `{"version":2,"items":[SpanV2Entry, ...]}`. Schema mirrors
//! `relay-event-schema/src/protocol/span_v2/mod.rs` — notably there is no
//! top-level `op` (it lives in `attributes["sentry.op"]`) and `end_timestamp`
//! replaces the legacy `timestamp` field name.

use crate::error::{AppError, AppResult};
use crate::protocol::infer_span_op;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Map, Value};

/// One span entry from a Spans Protocol v2 container.
#[derive(Debug, Clone, Deserialize)]
pub struct SpanV2Entry {
    #[serde(default)]
    pub trace_id: String,
    #[serde(default)]
    pub span_id: String,
    #[serde(default)]
    pub parent_span_id: Option<String>,
    /// Human-readable span label (maps to Rustrak's `description` column —
    /// v2 has no separate `op`/`description` split at the top level).
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub is_segment: bool,
    /// Required on the wire. `None` (absent) is a discard condition, not a
    /// default — mirrors Relay's `validate_timestamps`, which rejects a span
    /// with a missing start or end
    /// (relay-server/src/processing/spans/process.rs:367).
    #[serde(default, deserialize_with = "crate::models::deserialize_timestamp")]
    pub start_timestamp: Option<DateTime<Utc>>,
    #[serde(default, deserialize_with = "crate::models::deserialize_timestamp")]
    pub end_timestamp: Option<DateTime<Utc>>,
    /// Typed attribute map: `{"key": {"value": ..., "type": "string"|...}}`.
    /// Use [`Self::flat_attributes`] to unwrap into the plain `{key: value}`
    /// shape the rest of Rustrak's span-storage/gen_ai code expects.
    #[serde(default)]
    pub attributes: Map<String, Value>,
}

/// Whether an attribute's declared type admits its value.
///
/// The pairs are Relay's, from `normalize_attribute_types`
/// (relay-event-normalization/src/eap/mod.rs). Two are easy to get wrong:
/// `double` also admits a whole JSON number, because a value like `3` carries
/// no fractional part on the wire; and an unrecognized type is rejected rather
/// than passed through, even though `AttributeType::Unknown` exists for
/// forward compatibility — Relay removes those outright.
///
/// There is deliberately no arm for an object: `AttributeType` has no object
/// variant, so no well-formed v2 attribute is one.
fn type_matches_value(declared: Option<&str>, value: &Value) -> bool {
    match declared {
        Some("boolean") => value.is_boolean(),
        Some("integer") => value.is_i64() || value.is_u64(),
        Some("double") => value.is_f64() || value.is_i64() || value.is_u64(),
        Some("string") => value.is_string(),
        Some("array") => value.is_array(),
        _ => false,
    }
}

impl SpanV2Entry {
    /// Unwraps the typed attribute map into a flat `{key: value}` object,
    /// dropping the `{"value":..., "type":...}` wrapper.
    ///
    /// An attribute whose declared `type` disagrees with its `value` is
    /// dropped, as is one with a `type` the protocol does not define. This
    /// mirrors Relay's `normalize_attribute_types`
    /// (relay-event-normalization/src/eap/mod.rs), which removes the whole
    /// attribute rather than repairing it — its comment gives the reason, and
    /// it is not the Kafka-schema concern an earlier version of this comment
    /// claimed: downstream consumers need attribute shapes they can rely on.
    /// Rustrak has such a consumer in `models::transaction::span_attributes`,
    /// which reads a nested object as a legacy attribute bag; letting an
    /// object-valued attribute through would let one key masquerade as the
    /// whole bag and hide every sibling.
    ///
    /// A missing `value` is likewise omitted.
    pub fn flat_attributes(&self) -> Value {
        let mut flat = Map::with_capacity(self.attributes.len());
        for (key, attr) in &self.attributes {
            let Some(value) = attr.get("value") else {
                continue;
            };
            if !type_matches_value(attr.get("type").and_then(Value::as_str), value) {
                continue;
            }
            flat.insert(key.clone(), value.clone());
        }
        Value::Object(flat)
    }

    /// Derives the semantic operation for this span from its (already
    /// flattened) attributes: `sentry.op`, or the one Relay infers where the
    /// span has no such attribute (`normalize_sentry_op`,
    /// relay-event-normalization/src/eap/mod.rs). A `sentry.op` that is there
    /// with no usable value is left alone, so that span has no op.
    pub fn op(&self, flat_attributes: &Value) -> Option<String> {
        if self.attributes.contains_key("sentry.op") {
            return flat_attributes
                .get("sentry.op")?
                .as_str()
                .map(str::to_string);
        }
        // Relay does not store an empty attribute.
        Some(infer_span_op(|name| flat_attributes.get(name))).filter(|op| !op.is_empty())
    }

    /// Reads a string-valued `sentry.*` attribute.
    ///
    /// v2 carries as attributes what the legacy schema kept as top-level span
    /// fields: Relay's own v1→v2 conversion promotes them with a `sentry.`
    /// prefix (relay-spans/src/v1_to_v2.rs:55-61), and the OTel→v2 path passes
    /// the same keys straight through (relay-spans/src/otel_to_sentry_v2.rs).
    fn attr_str(flat_attributes: &Value, key: &str) -> Option<String> {
        flat_attributes
            .get(key)
            .and_then(|v| v.as_str())
            .map(str::to_string)
    }

    /// Self time in milliseconds, from `sentry.exclusive_time`.
    ///
    /// Relay never *computes* this for v2 — the v1-only
    /// `compute_span_exclusive_time` has no EAP counterpart — so an absent
    /// attribute simply means no self time, never a value to derive.
    pub fn exclusive_time_ms(flat_attributes: &Value) -> Option<f64> {
        flat_attributes
            .get("sentry.exclusive_time")
            .and_then(|v| v.as_f64())
    }

    pub fn platform(flat_attributes: &Value) -> Option<String> {
        Self::attr_str(flat_attributes, "sentry.platform")
    }

    pub fn release(flat_attributes: &Value) -> Option<String> {
        Self::attr_str(flat_attributes, "sentry.release")
    }

    pub fn environment(flat_attributes: &Value) -> Option<String> {
        Self::attr_str(flat_attributes, "sentry.environment")
    }

    /// Segment root this span belongs to.
    ///
    /// Prefers the SDK-sent `sentry.segment.id` — v2 has no `segment_id` field,
    /// so a non-root span's link to its segment lives only here. Falls back to
    /// self-identification for a root, matching the legacy pipeline's
    /// "if is_segment { segment_id = span_id }"
    /// (relay-server/src/processing/legacy_spans/normalize.rs).
    pub fn segment_id(&self, flat_attributes: &Value) -> Option<String> {
        Self::attr_str(flat_attributes, "sentry.segment.id")
            .or_else(|| self.is_segment.then(|| self.span_id.clone()))
    }
}

/// Wire shape of a Spans Protocol v2 container: `{"version":2,"items":[...]}`.
///
/// `version` is intentionally not modeled: Relay's own `ContainerMetadata`
/// treats it as optional and purely advisory (client-IP/UA inference hints),
/// never a rejection gate — a missing `version` is a documented-valid state,
/// and Relay's own tests round-trip arbitrary values (e.g. `123`) unrejected
/// (relay-event-schema/src/protocol/span_v2/container.rs). The content-type
/// (`application/vnd.sentry.items.span.v2+json`) is what pins this item to
/// the v2 wire format, not the inner field.
#[derive(Debug, Deserialize)]
struct SpanV2Container {
    #[serde(default, deserialize_with = "crate::models::deserialize_bounded_items")]
    items: Vec<SpanV2Entry>,
}

impl SpanV2Container {
    /// Parses a v2 container body into individual [`SpanV2Entry`]s.
    pub fn parse(body: &[u8]) -> AppResult<Vec<SpanV2Entry>> {
        let container: SpanV2Container = serde_json::from_slice(body)
            .map_err(|e| AppError::Validation(format!("Invalid span v2 container JSON: {}", e)))?;
        Ok(container.items)
    }
}

/// Parses a Spans Protocol v2 container body into its entries.
pub fn parse_span_v2_container(body: &[u8]) -> AppResult<Vec<SpanV2Entry>> {
    SpanV2Container::parse(body)
}

#[cfg(test)]
mod tests {
    use super::parse_span_v2_container;
    use crate::models::MAX_CONTAINER_ITEMS;

    #[test]
    fn rejects_span_container_with_too_many_items() {
        let item = r#"{"span_id":"s","trace_id":"t"}"#;
        let body = format!(
            r#"{{"items":[{}]}}"#,
            std::iter::repeat_n(item, MAX_CONTAINER_ITEMS + 1)
                .collect::<Vec<_>>()
                .join(",")
        );

        assert!(parse_span_v2_container(body.as_bytes()).is_err());
    }
}
