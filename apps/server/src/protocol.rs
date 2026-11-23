//! Pure value rules shared by models, services, the digest and routes: field
//! limits and shapes from the Sentry/Relay protocol, plus input shapes. No I/O
//! and no imports from the layers above.

use chrono::{DateTime, Datelike, TimeZone, Utc};

/// Cuts `s` to the `max_chars` of the VARCHAR column it is bound to. Postgres
/// rejects a longer value and fails the whole envelope, SQLite stores it, and
/// Relay truncates to its own `max_chars` instead of rejecting.
pub(crate) fn clamp(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

/// [`clamp`] for a nullable column.
pub(crate) fn clamp_opt(s: Option<String>, max_chars: usize) -> Option<String> {
    s.map(|s| clamp(&s, max_chars))
}

/// The instant `epoch` seconds after 1970, if Postgres can store it as well.
/// chrono reaches years Postgres rejects, and one such value failed the whole
/// insert; an absurd device clock now reads as no timestamp at all.
pub(crate) fn epoch_to_utc(epoch: f64) -> Option<DateTime<Utc>> {
    if !epoch.is_finite() {
        return None;
    }
    let secs = epoch.floor() as i64;
    let nanos = ((epoch - epoch.floor()) * 1_000_000_000.0) as u32;
    Utc.timestamp_opt(secs, nanos)
        .single()
        .filter(|dt| (1..=9999).contains(&dt.year()))
}

/// Whether `data[key]` is a timestamp from before 1970. Relay's
/// `TimestampProcessor` drops an event or transaction carrying one, and removes
/// such a span or breadcrumb from its event.
pub(crate) fn is_stale(data: &serde_json::Value, key: &str) -> bool {
    match data.get(key) {
        Some(serde_json::Value::Number(n)) => n.as_f64().is_some_and(|epoch| epoch < 0.0),
        Some(raw @ serde_json::Value::String(_)) => {
            timestamp_from_value(raw).is_some_and(|dt| dt.timestamp_millis() < 0)
        }
        _ => false,
    }
}

/// `level` as Relay's `Level` reads it (`FromValue`). Relay removes anything
/// else, so the caller's default applies.
pub(crate) fn relay_level(raw: &serde_json::Value) -> Option<&'static str> {
    // Python logging levels (`Level::from_python_level`).
    if let Some(n) = raw.as_u64() {
        return Some(match n {
            10 => "debug",
            20 => "info",
            30 => "warning",
            40 => "error",
            50 => "fatal",
            _ => return None,
        });
    }
    Some(match raw.as_str()? {
        "debug" => "debug",
        "info" | "log" => "info",
        "warning" => "warning",
        "error" => "error",
        "fatal" | "critical" => "fatal",
        _ => return None,
    })
}

/// A payload timestamp, as epoch seconds or RFC 3339.
pub(crate) fn extract_timestamp(data: &serde_json::Value, key: &str) -> Option<DateTime<Utc>> {
    timestamp_from_value(data.get(key)?)
}

/// Relay's `Timestamp` (relay-event-schema/src/protocol/types.rs): epoch
/// seconds, a naive datetime read as UTC, or an RFC 3339 string.
pub(crate) fn timestamp_from_value(raw: &serde_json::Value) -> Option<DateTime<Utc>> {
    if let Some(epoch) = raw.as_f64() {
        return epoch_to_utc(epoch);
    }
    parse_timestamp(raw.as_str()?)
}

/// A timestamp string as Relay reads one: a naive datetime first, as UTC,
/// then RFC 3339.
pub(crate) fn parse_timestamp(s: &str) -> Option<DateTime<Utc>> {
    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f")
        .map(|naive| naive.and_utc())
        .or_else(|_| DateTime::parse_from_rfc3339(s).map(|dt| dt.with_timezone(&Utc)))
        .ok()
        .filter(|dt| (1..=9999).contains(&dt.year()))
}

/// The address shape accepted for accounts and invitations.
pub(crate) fn is_valid_email(email: &str) -> bool {
    let parts: Vec<&str> = email.split('@').collect();
    if parts.len() != 2 {
        return false;
    }
    let (local, domain) = (parts[0], parts[1]);

    if local.is_empty() || local.len() > 64 {
        return false;
    }

    // Domain: non-empty, has at least one dot, not starting/ending with dot
    if domain.is_empty() || domain.len() > 255 {
        return false;
    }
    if !domain.contains('.') {
        return false;
    }
    if domain.starts_with('.') || domain.ends_with('.') {
        return false;
    }

    // Domain parts must not be empty (catches "user@.com" and "user@domain.")
    let domain_parts: Vec<&str> = domain.split('.').collect();
    if domain_parts.iter().any(|p| p.is_empty()) {
        return false;
    }

    // TLD must be at least 2 chars
    if let Some(tld) = domain_parts.last() {
        if tld.len() < 2 {
            return false;
        }
    }

    true
}

/// The `op` Relay gives a span item that has none, from the attributes
/// `attribute` finds: `derive_op_for_v2_span` (relay-spans/src/op.rs), in its
/// order. It runs after `normalize_attribute_names`, which is why each
/// attribute is also looked up under the names sentry-conventions deprecates.
pub(crate) fn infer_span_op<'a>(
    attribute: impl Fn(&str) -> Option<&'a serde_json::Value>,
) -> String {
    let any = |names: &[&str]| names.iter().find_map(|name| attribute(name));
    let text = |name: &str| attribute(name).and_then(|value| value.as_str());
    let method = [
        "http.request.method",
        "http.method",
        "http.request_method",
        "method",
    ];
    let op = if any(&method).is_some() {
        match any(&["sentry.kind", "otel.kind"]).and_then(|kind| kind.as_str()) {
            Some("client") => "http.client",
            Some("server") => "http.server",
            _ if attribute("sentry.http.prefetch").is_some() => "http.prefetch",
            _ => "http",
        }
    } else if any(&["db.system.name", "db.system"]).is_some() {
        "db"
    } else if let Some(operation) = text("gen_ai.operation.name") {
        return format!("gen_ai.{operation}");
    } else if attribute("rpc.service").is_some() {
        "rpc"
    } else if attribute("messaging.system").is_some() {
        "message"
    } else {
        text("faas.trigger").unwrap_or("default")
    };
    op.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn levels_read_as_relay_reads_them() {
        assert_eq!(relay_level(&json!("log")), Some("info"));
        assert_eq!(relay_level(&json!("critical")), Some("fatal"));
        assert_eq!(relay_level(&json!("ERROR")), None);
        // Python logging levels (`Level::from_python_level`).
        assert_eq!(relay_level(&json!(10)), Some("debug"));
        assert_eq!(relay_level(&json!(20)), Some("info"));
        assert_eq!(relay_level(&json!(30)), Some("warning"));
        assert_eq!(relay_level(&json!(40)), Some("error"));
        assert_eq!(relay_level(&json!(50)), Some("fatal"));
        assert_eq!(relay_level(&json!(35)), None);
        assert_eq!(relay_level(&json!(-10)), None);
    }

    #[test]
    fn a_naive_timestamp_reads_as_utc() {
        let expected = Utc.with_ymd_and_hms(2026, 11, 23, 10, 0, 0).unwrap();
        assert_eq!(
            timestamp_from_value(&json!("2026-11-23T10:00:00")),
            Some(expected)
        );
        assert_eq!(
            timestamp_from_value(&json!("2026-11-23T10:00:00.250")),
            Some(expected + chrono::Duration::milliseconds(250))
        );
        assert_eq!(
            timestamp_from_value(&json!("2026-11-23T10:00:00Z")),
            Some(expected)
        );
        assert_eq!(timestamp_from_value(&json!("yesterday")), None);
    }

    #[test]
    fn a_naive_timestamp_before_1970_is_stale() {
        assert!(is_stale(
            &json!({"timestamp": "1969-12-31T23:00:00"}),
            "timestamp"
        ));
        assert!(!is_stale(
            &json!({"timestamp": "2026-11-23T10:00:00"}),
            "timestamp"
        ));
    }
}
