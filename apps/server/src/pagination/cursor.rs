use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

/// Cursor for paginating Events (compound keyset on (timestamp, id)).
///
/// `timestamp` (the SDK-reported event time, matching Sentry's own per-event
/// ordering — not `ingested_at`) is not unique within an issue — a burst of
/// events can share the same timestamp. The `id` tiebreaker makes the keyset
/// deterministic so no row is silently skipped or repeated across pages.
/// Replaces the old `digest_order`-based cursor (see
/// `20260719000000_drop_event_digest_order`): that counter could collide
/// after retention cleanup decremented it past a value a surviving event
/// still used. No backward-compat decoding of the old `{order,
/// last_digest_order}` shape is provided — a decode failure returns a clean
/// 400 and the client refetches page 1.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventCursor {
    /// Direction: "asc" or "desc"
    pub order: String,
    /// Last timestamp seen, used as the primary keyset boundary
    pub last_timestamp: DateTime<Utc>,
    /// Last id seen, used as the tiebreaker for equal timestamp values
    pub last_id: Uuid,
}

impl EventCursor {
    pub fn new(order: &str, last_timestamp: DateTime<Utc>, last_id: Uuid) -> Self {
        Self {
            order: order.to_string(),
            last_timestamp,
            last_id,
        }
    }

    pub fn encode(&self) -> AppResult<String> {
        let json = serde_json::to_string(self)
            .map_err(|e| AppError::Internal(format!("Cursor serialization failed: {}", e)))?;
        Ok(URL_SAFE_NO_PAD.encode(json.as_bytes()))
    }

    pub fn decode(s: &str) -> AppResult<Self> {
        let bytes = URL_SAFE_NO_PAD
            .decode(s)
            .map_err(|_| AppError::Validation("Invalid cursor encoding".to_string()))?;

        let json = String::from_utf8(bytes)
            .map_err(|_| AppError::Validation("Invalid cursor encoding".to_string()))?;

        serde_json::from_str(&json)
            .map_err(|_| AppError::Validation("Invalid cursor format".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_cursor_encode_decode() {
        let last_timestamp = "2026-07-19T12:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let last_id = Uuid::new_v4();
        let cursor = EventCursor::new("asc", last_timestamp, last_id);

        let encoded = cursor.encode().unwrap();
        let decoded = EventCursor::decode(&encoded).unwrap();

        assert_eq!(decoded.order, "asc");
        assert_eq!(decoded.last_timestamp, last_timestamp);
        assert_eq!(decoded.last_id, last_id);
    }

    #[test]
    fn test_event_cursor_rejects_old_digest_order_shape() {
        // Pre-fix cursor shape: `{"order": "...", "last_digest_order": N}`.
        // No backward-compat handling is provided (see Boundaries in the
        // digest_order-collision spec) -- a decode failure here is
        // intentional and forces the client to refetch page 1 with a fresh
        // cursor rather than silently misinterpreting stale fields.
        let old_shape_json = r#"{"order":"desc","last_digest_order":100}"#;
        let encoded = URL_SAFE_NO_PAD.encode(old_shape_json.as_bytes());

        let result = EventCursor::decode(&encoded);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_cursor() {
        let result = EventCursor::decode("not-valid-base64!!!");
        assert!(result.is_err());
    }
}
