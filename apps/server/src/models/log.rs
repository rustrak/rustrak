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
    /// Epoch seconds when the log was created.
    #[serde(default)]
    pub timestamp: f64,
    /// Trace this log belongs to (required by the protocol).
    #[serde(default)]
    pub trace_id: String,
    /// Span this log entry belongs to (optional).
    #[serde(default)]
    pub span_id: Option<String>,
    /// Log level: trace/debug/info/warn/error/fatal/unknown.
    #[serde(default)]
    pub level: String,
    /// Log body (the message).
    #[serde(default)]
    pub body: String,
    /// Arbitrary typed attributes, stored verbatim.
    #[serde(default)]
    pub attributes: serde_json::Value,
}

/// Wire shape of a log item container: `{"items":[OurLog, ...]}`.
#[derive(Debug, Deserialize)]
pub struct LogContainer {
    #[serde(default, deserialize_with = "crate::models::deserialize_bounded_items")]
    items: Vec<LogItem>,
}

impl LogContainer {
    /// Parses a log item-container body into individual [`LogItem`]s.
    pub fn parse(body: &[u8]) -> AppResult<Vec<LogItem>> {
        let container: LogContainer = serde_json::from_slice(body)
            .map_err(|e| AppError::Validation(format!("Invalid log container JSON: {}", e)))?;
        Ok(container.items)
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
}

/// Response model for a single stored log in the list view.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct LogResponse {
    pub id: Uuid,
    pub environment: Option<String>,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub level: String,
    /// OTel severity number (1=trace … 21=fatal). None for unknown levels.
    pub severity_number: Option<i16>,
    pub body: String,
    /// Typed attribute map, as received.
    pub attributes: serde_json::Value,
    pub timestamp: DateTime<Utc>,
    pub ingested_at: DateTime<Utc>,
}
