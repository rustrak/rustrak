use crate::models::session::{SessionAggregates, SessionUpdate};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Headers of the envelope (first JSON line)
/// Some fields are parsed but not currently used - kept for future logging/debugging
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct EnvelopeHeaders {
    /// Event ID (required for items of type "event")
    pub event_id: Option<String>,

    /// Full DSN (optional, for self-auth)
    pub dsn: Option<String>,

    /// Timestamp of sending (RFC3339)
    pub sent_at: Option<String>,

    /// SDK info
    pub sdk: Option<SdkInfo>,
}

/// SDK information from envelope headers
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct SdkInfo {
    pub name: Option<String>,
    pub version: Option<String>,
}

/// Headers of an item within the envelope
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct ItemHeaders {
    /// Item type: "event", "session", "transaction", etc.
    #[serde(rename = "type")]
    pub item_type: String,

    /// Payload length in bytes (optional)
    pub length: Option<usize>,

    /// Content type of the payload
    pub content_type: Option<String>,
}

/// Typed representation of a parsed envelope item.
///
/// Selective typing: Session/Sessions carry typed structs (small, stable schemas).
/// Event/Transaction carry raw bytes (large/evolving schemas — deserialized in the processor).
/// Other is the forward-compatible catch-all that never panics.
///
/// Payloads are [`bytes::Bytes`] zero-copy slices of the envelope buffer, so a
/// multi-item envelope is held once in memory, shared via reference counting.
#[derive(Debug)]
pub enum EnvelopeItemKind {
    Event(Bytes),
    Transaction(Bytes),
    Session(SessionUpdate),
    Sessions(SessionAggregates),
    /// Standalone logs (Sentry "log" item type). Carries the raw item-container
    /// body (`{"items":[OurLog, ...]}`) — expanded into individual logs in the
    /// processor, mirroring Relay's `LogsProcessor`.
    Log(Bytes),
    /// Standalone spans (Sentry "span" item type, OTel-style — not attached to
    /// a parent transaction). Unlike `Log`, this is NOT a container: each
    /// envelope item holds exactly one flat span JSON object, mirroring
    /// Relay's legacy (default) `Span` schema.
    Span(Bytes),
    /// Standalone spans, Spans Protocol v2 (`"type":"span"` +
    /// `content_type: application/vnd.sentry.items.span.v2+json`). Unlike
    /// [`Self::Span`], this IS a batched container — one envelope item can
    /// carry many spans. Modern Sentry SDKs (verified: `@sentry/node` 10.65 +
    /// Vercel AI SDK) send AI-instrumented spans this way, not as `Span`.
    SpanV2Batch(Bytes),
    /// A cron monitor check-in (Sentry Crons). Relay processes only the
    /// first one in an envelope.
    CheckIn(Bytes),
    Other(String, Bytes),
}

impl EnvelopeItemKind {
    /// Returns true for item types that require an associated event_id.
    /// Mirrors Relay's `Item::requires_event()`.
    pub fn requires_event(&self) -> bool {
        match self {
            Self::Event(_) | Self::Transaction(_) => true,
            Self::Other(t, _) if t == "attachment" || t == "security" => true,
            _ => false,
        }
    }
}

/// `content_type` of a Spans Protocol v2 item — distinguishes it from the
/// legacy standalone-span item, which shares the same `"type":"span"` header
/// but no (or a plain `application/json`) content type.
pub const SPAN_V2_CONTENT_TYPE: &str = "application/vnd.sentry.items.span.v2+json";

impl From<(ItemHeaders, Bytes)> for EnvelopeItemKind {
    fn from((headers, payload): (ItemHeaders, Bytes)) -> Self {
        match headers.item_type.as_str() {
            "event" => Self::Event(payload),
            "transaction" => Self::Transaction(payload),
            "log" => Self::Log(payload),
            "check_in" => Self::CheckIn(payload),
            "span" => {
                if headers.content_type.as_deref() == Some(SPAN_V2_CONTENT_TYPE) {
                    Self::SpanV2Batch(payload)
                } else {
                    Self::Span(payload)
                }
            }
            "session" => match serde_json::from_slice(&payload) {
                Ok(s) => Self::Session(s),
                Err(e) => {
                    log::warn!("session item: bad JSON, treating as Other: {}", e);
                    Self::Other("session".into(), payload)
                }
            },
            "sessions" => match serde_json::from_slice(&payload) {
                Ok(s) => Self::Sessions(s),
                Err(e) => {
                    log::warn!("sessions item: bad JSON, treating as Other: {}", e);
                    Self::Other("sessions".into(), payload)
                }
            },
            other => Self::Other(other.to_owned(), payload),
        }
    }
}

/// Result of parsing an envelope
#[derive(Debug)]
pub struct ParsedEnvelope {
    pub headers: EnvelopeHeaders,
    pub items: Vec<EnvelopeItemKind>,
}

/// Event metadata for the digest worker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMetadata {
    pub event_id: String,
    pub project_id: i32,
    pub ingested_at: DateTime<Utc>,
    pub remote_addr: Option<String>,
}
