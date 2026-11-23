//! Standalone log model (Sentry "log" item type — Relay's `OurLog`).
//!
//! Logs arrive batched inside an item container (`{"items":[OurLog, ...]}`).
//! [`LogContainer::parse`] expands that container into individual [`LogItem`]s.
//! Schema mirrors `relay-event-schema/src/protocol/ourlog/mod.rs`.

use crate::error::{AppError, AppResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A single structured log record (Relay's `OurLog`).
///
/// `attributes` keeps the OTel-style typed map verbatim
/// (`{"key":{"value":x,"type":"string"}}`) — denormalized fields are
/// surfaced separately for querying.
#[derive(Debug, Clone, Deserialize)]
pub struct LogItem {
    /// When the log was created (epoch seconds or RFC 3339 on the wire).
    /// `timestamp`, `trace_id`, `level` and `body` are required, as in
    /// Relay: a log missing one, or holding `null` or the wrong type, is
    /// dropped on its own.
    #[serde(deserialize_with = "crate::models::required_timestamp")]
    pub timestamp: DateTime<Utc>,
    /// Trace this log belongs to.
    pub trace_id: String,
    /// Span this log entry belongs to (optional).
    #[serde(default, deserialize_with = "crate::models::lenient")]
    pub span_id: Option<String>,
    /// Log level: trace/debug/info/warn/error/fatal/unknown.
    pub level: String,
    /// Log body (the message).
    pub body: String,
    /// Arbitrary typed attributes, stored verbatim.
    #[serde(default, deserialize_with = "crate::models::lenient")]
    pub attributes: serde_json::Value,
}

/// Wire shape of a log item container: `{"items":[OurLog, ...]}`.
#[derive(Debug, Deserialize)]
pub struct LogContainer<'a> {
    #[serde(
        borrow,
        default,
        deserialize_with = "crate::models::deserialize_bounded_items"
    )]
    items: Vec<&'a serde_json::value::RawValue>,
}

impl LogContainer<'_> {
    /// Parses a log item-container body into individual [`LogItem`]s.
    pub fn parse(body: &[u8]) -> AppResult<Vec<LogItem>> {
        let container: LogContainer = serde_json::from_slice(body)
            .map_err(|e| AppError::Validation(format!("Invalid log container JSON: {}", e)))?;
        let mut logs: Vec<LogItem> = crate::models::container_entries(container.items);
        // An empty trace id is no trace id, which Relay requires.
        logs.retain(|log| !log.trace_id.is_empty());
        Ok(logs)
    }
}

#[cfg(test)]
mod tests {
    use super::LogContainer;
    use crate::models::MAX_CONTAINER_ITEMS;

    #[test]
    fn rejects_log_container_with_too_many_items() {
        let item = r#"{"body":"x"}"#;
        let body = format!(
            r#"{{"items":[{}]}}"#,
            std::iter::repeat_n(item, MAX_CONTAINER_ITEMS + 1)
                .collect::<Vec<_>>()
                .join(",")
        );

        assert!(LogContainer::parse(body.as_bytes()).is_err());
    }

    /// Relay reads each log on its own (`DeleteParent`): a log whose
    /// `timestamp`, `trace_id`, `level` or `body` is missing, `null` or of
    /// the wrong type is dropped alone; a bad optional field is left empty.
    #[test]
    fn a_bad_log_does_not_fail_its_batch() {
        let body = br#"{"items":[
            {"timestamp":1781085600,"trace_id":"0123456789abcdef0123456789abcdef","level":"info","body":"kept","span_id":123},
            {"timestamp":1781085600,"trace_id":"0123456789abcdef0123456789abcdef","level":"info","body":null},
            {"timestamp":1781085600,"trace_id":"0123456789abcdef0123456789abcdef","level":5,"body":"x"},
            {"timestamp":1781085600,"trace_id":"","level":"info","body":"x"},
            {"trace_id":"0123456789abcdef0123456789abcdef","level":"info","body":"x"},
            42
        ]}"#;
        let logs = LogContainer::parse(body).expect("the batch parses");
        assert_eq!(logs.len(), 1);
        assert_eq!(
            (logs[0].body.as_str(), logs[0].span_id.as_deref()),
            ("kept", None)
        );
    }
}

/// Response model for a single stored log in the list view.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct LogResponse {
    pub id: Uuid,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub trace_id: Option<String>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub span_id: Option<String>,
    pub level: String,
    /// OTel severity number (1=trace … 21=fatal). None for unknown levels.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub severity_number: Option<i16>,
    pub body: String,
    /// Typed attribute map, as received.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub attributes: serde_json::Value,
    pub timestamp: DateTime<Utc>,
    pub ingested_at: DateTime<Utc>,
}

/// One bucket of the log volume: how many logs of each level it holds.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct LogVolumePoint {
    /// Where the bucket starts.
    pub bucket: DateTime<Utc>,
    pub trace: i64,
    pub debug: i64,
    pub info: i64,
    pub warn: i64,
    pub error: i64,
    pub fatal: i64,
}

impl LogVolumePoint {
    pub fn empty(at: i64) -> Self {
        Self {
            bucket: DateTime::from_timestamp(at, 0).unwrap_or_default(),
            trace: 0,
            debug: 0,
            info: 0,
            warn: 0,
            error: 0,
            fatal: 0,
        }
    }

    /// Counts `count` logs of `level`. A level outside the six, which
    /// ingestion keeps, counts as `info`, as an empty one is stored: the
    /// chart never holds fewer logs than the list.
    pub fn add(&mut self, level: &str, count: i64) {
        let slot = match level {
            "trace" => &mut self.trace,
            "debug" => &mut self.debug,
            "warn" => &mut self.warn,
            "error" => &mut self.error,
            "fatal" => &mut self.fatal,
            _ => &mut self.info,
        };
        *slot += count;
    }
}
