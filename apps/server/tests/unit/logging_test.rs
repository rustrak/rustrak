//! Unit tests for `RUSTRAK_LOG_TIMEZONE` log-timestamp handling.
//!
//! Scope: log formatting only. Does not touch event/issue timestamps (those stay
//! UTC in storage and the API regardless of this setting).

use crate::common::EnvGuard;
use chrono::{TimeZone, Utc};
use chrono_tz::Tz;
use rustrak::config::log_timezone;
use rustrak::logging::format_log_timestamp;

#[test]
fn test_resolve_log_timezone_unset_returns_none() {
    let mut env = EnvGuard::new();
    env.remove("RUSTRAK_LOG_TIMEZONE");

    assert_eq!(log_timezone(), Ok(None));
}

#[test]
fn test_resolve_log_timezone_valid_iana_name_returns_tz() {
    let mut env = EnvGuard::new();
    env.set("RUSTRAK_LOG_TIMEZONE", "Asia/Shanghai");

    assert_eq!(log_timezone(), Ok(Some(Tz::Asia__Shanghai)));
}

#[test]
fn test_format_log_timestamp_none_produces_utc_rfc3339() {
    let instant = Utc.with_ymd_and_hms(2026, 7, 14, 4, 30, 0).unwrap();

    let formatted = format_log_timestamp(instant, None);

    assert_eq!(formatted, "2026-07-14T04:30:00Z");
}

#[test]
fn test_format_log_timestamp_applies_zone_offset() {
    // 2026-07-14T04:30:00Z is the exact example from GH issue #179.
    let instant = Utc.with_ymd_and_hms(2026, 7, 14, 4, 30, 0).unwrap();

    let formatted = format_log_timestamp(instant, Some(Tz::Asia__Shanghai));

    // Asia/Shanghai is a fixed UTC+8 offset (no DST) — matches the issue's own
    // "Configured" example exactly.
    assert_eq!(formatted, "2026-07-14T12:30:00+08:00");
}

#[test]
fn test_format_log_timestamp_applies_dst_zone_offset() {
    // America/New_York: summer is UTC-4 (EDT), winter is UTC-5 (EST).
    let summer = Utc.with_ymd_and_hms(2026, 7, 14, 4, 30, 0).unwrap();
    let winter = Utc.with_ymd_and_hms(2026, 1, 14, 4, 30, 0).unwrap();

    let summer_formatted = format_log_timestamp(summer, Some(Tz::America__New_York));
    let winter_formatted = format_log_timestamp(winter, Some(Tz::America__New_York));

    assert_eq!(summer_formatted, "2026-07-14T00:30:00-04:00");
    assert_eq!(winter_formatted, "2026-01-13T23:30:00-05:00");
}

#[test]
fn test_resolve_log_timezone_invalid_name_is_reported_for_the_utc_fallback() {
    let mut env = EnvGuard::new();
    env.set("RUSTRAK_LOG_TIMEZONE", "Not/AZone");

    assert_eq!(log_timezone(), Err("Not/AZone".to_string()));
}
