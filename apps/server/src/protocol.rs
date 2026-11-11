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
        Some(serde_json::Value::String(s)) => {
            DateTime::parse_from_rfc3339(s).is_ok_and(|dt| dt.timestamp_millis() < 0)
        }
        _ => false,
    }
}

/// `level` as Relay's `Level::from_str` reads it. Relay removes anything else,
/// so the caller's default applies.
pub(crate) fn relay_level(raw: &str) -> Option<&'static str> {
    Some(match raw {
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
/// seconds or an RFC 3339 string.
pub(crate) fn timestamp_from_value(raw: &serde_json::Value) -> Option<DateTime<Utc>> {
    if let Some(epoch) = raw.as_f64() {
        return epoch_to_utc(epoch);
    }
    raw.as_str()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
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
