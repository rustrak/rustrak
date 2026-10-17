//! Parsing and normalization of a `check_in` envelope item, matching Relay's
//! `relay-monitors` crate and the slug handling of Sentry's consumer.

use rustrak::models::monitor::{CheckIn, CheckInStatus};
use uuid::Uuid;

fn parse(json: &str) -> rustrak::error::AppResult<CheckIn> {
    CheckIn::parse(json.as_bytes())
}

#[test]
fn parses_relays_integration_test_payload() {
    let c = parse(r#"{"check_in_id":"a460c25ff2554577b920fcfacae4e5eb","monitor_slug":"my-monitor","status":"in_progress","duration":21.0}"#).unwrap();
    assert_eq!(
        c.check_in_id,
        Uuid::parse_str("a460c25ff2554577b920fcfacae4e5eb").unwrap()
    );
    assert_eq!(c.monitor_slug, "my-monitor");
    assert_eq!(c.status, CheckInStatus::InProgress);
    assert_eq!(c.duration, Some(21.0));
    assert_eq!(c.environment, "production");
    assert!(c.monitor_config.is_none());
}

#[test]
fn status_is_required() {
    assert!(parse(r#"{"monitor_slug":"m"}"#).is_err());
}

#[test]
fn unrecognized_and_client_sent_missed_status_become_unknown() {
    // Relay: `#[serde(other)] Unknown`, and `missed` is rewritten to `unknown`
    // because only the server decides a check-in was missed.
    let other = parse(r#"{"monitor_slug":"m","status":"exploded"}"#).unwrap();
    assert_eq!(other.status, CheckInStatus::Unknown);
    let missed = parse(r#"{"monitor_slug":"m","status":"missed"}"#).unwrap();
    assert_eq!(missed.status, CheckInStatus::Unknown);
}

#[test]
fn a_missing_check_in_id_is_the_nil_uuid() {
    let c = parse(r#"{"monitor_slug":"m","status":"ok"}"#).unwrap();
    assert_eq!(c.check_in_id, Uuid::nil());
}

#[test]
fn slug_is_slugified_like_django() {
    let c = parse(r#"{"monitor_slug":"  My Nightly_Backup!! job ","status":"ok"}"#).unwrap();
    assert_eq!(c.monitor_slug, "my-nightly_backup-job");
}

#[test]
fn slug_is_truncated_to_50_characters() {
    let long = "a".repeat(80);
    let c = parse(&format!(r#"{{"monitor_slug":"{long}","status":"ok"}}"#)).unwrap();
    assert_eq!(c.monitor_slug.len(), 50);
}

#[test]
fn empty_slug_is_rejected() {
    assert!(parse(r#"{"monitor_slug":"","status":"ok"}"#).is_err());
    assert!(parse(r#"{"monitor_slug":"!!!","status":"ok"}"#).is_err());
    assert!(parse(r#"{"status":"ok"}"#).is_err());
}

#[test]
fn environment_over_64_characters_is_rejected() {
    let env = "e".repeat(65);
    assert!(parse(&format!(
        r#"{{"monitor_slug":"m","status":"ok","environment":"{env}"}}"#
    ))
    .is_err());
}

#[test]
fn empty_environment_is_production() {
    let c = parse(r#"{"monitor_slug":"m","status":"ok","environment":""}"#).unwrap();
    assert_eq!(c.environment, "production");
}

#[test]
fn trace_id_comes_from_the_trace_context() {
    let c = parse(r#"{"monitor_slug":"m","status":"ok","contexts":{"trace":{"trace_id":"5b8efff798038103d269b633813fc60c"}}}"#).unwrap();
    assert_eq!(
        c.trace_id.as_deref(),
        Some("5b8efff798038103d269b633813fc60c")
    );
}

#[test]
fn monitor_config_is_carried_for_upsert() {
    let c = parse(r#"{"monitor_slug":"m","status":"in_progress","monitor_config":{"schedule":{"type":"crontab","value":"0 * * * *"},"checkin_margin":5}}"#).unwrap();
    let config = c.monitor_config.unwrap();
    assert_eq!(config.checkin_margin, Some(5));
}
