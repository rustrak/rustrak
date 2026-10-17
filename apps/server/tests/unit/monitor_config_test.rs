//! Validation of a monitor's configuration, as received from an SDK's
//! `monitor_config` or from the dashboard. Limits mirror Sentry's
//! `monitors/constants.py` and `ConfigValidator`.

use rustrak::models::monitor::{IntervalUnit, MonitorConfigInput, Schedule};

fn input(schedule: Schedule) -> MonitorConfigInput {
    MonitorConfigInput {
        schedule: Some(schedule),
        ..Default::default()
    }
}

fn hourly() -> Schedule {
    Schedule::Crontab {
        value: "0 * * * *".to_string(),
    }
}

#[test]
fn missing_fields_take_sentry_defaults() {
    let c = input(hourly()).validate(None).unwrap();
    assert_eq!(c.checkin_margin, 1);
    assert_eq!(c.max_runtime, 30);
    assert_eq!(c.timezone, "UTC");
    assert_eq!(c.failure_issue_threshold, 1);
    assert_eq!(c.recovery_threshold, 1);
}

#[test]
fn a_schedule_is_required() {
    assert!(MonitorConfigInput::default().validate(None).is_err());
}

#[test]
fn invalid_crontab_is_rejected() {
    let bad = Schedule::Crontab {
        value: "every day".to_string(),
    };
    assert!(input(bad).validate(None).is_err());
}

#[test]
fn six_field_crontab_is_rejected_like_sentry() {
    let seconds = Schedule::Crontab {
        value: "0 0 * * * *".to_string(),
    };
    assert!(input(seconds).validate(None).is_err());
}

#[test]
fn crontab_aliases_are_accepted() {
    let daily = Schedule::Crontab {
        value: "@daily".to_string(),
    };
    assert!(input(daily).validate(None).is_ok());
}

#[test]
fn zero_interval_is_rejected() {
    let zero = Schedule::Interval {
        value: 0,
        unit: IntervalUnit::Minute,
    };
    assert!(input(zero).validate(None).is_err());
}

#[test]
fn unknown_timezone_is_rejected() {
    let mut i = input(hourly());
    i.timezone = Some("Mars/Olympus".to_string());
    assert!(i.validate(None).is_err());
}

#[test]
fn oversized_margin_and_runtime_are_clamped_not_rejected() {
    // Sentry's consumer clamps these on upsert instead of dropping the check-in.
    let mut i = input(hourly());
    i.checkin_margin = Some(1_000_000);
    i.max_runtime = Some(1_000_000);
    let c = i.validate(None).unwrap();
    assert_eq!(c.checkin_margin, 40_320);
    assert_eq!(c.max_runtime, 10_080);
}

#[test]
fn thresholds_outside_1_to_720_are_rejected() {
    let mut i = input(hourly());
    i.failure_issue_threshold = Some(0);
    assert!(i.validate(None).is_err());

    let mut i = input(hourly());
    i.recovery_threshold = Some(721);
    assert!(i.validate(None).is_err());
}

#[test]
fn omitted_fields_keep_the_existing_config() {
    // An SDK upsert that only sends a schedule must not reset thresholds a
    // person set in the dashboard (Sentry's `Monitor.update_config`).
    let mut first = input(hourly());
    first.failure_issue_threshold = Some(3);
    first.timezone = Some("Europe/Madrid".to_string());
    let existing = first.validate(None).unwrap();

    let updated = input(Schedule::Interval {
        value: 10,
        unit: IntervalUnit::Minute,
    })
    .validate(Some(&existing))
    .unwrap();

    assert_eq!(updated.failure_issue_threshold, 3);
    assert_eq!(updated.timezone, "Europe/Madrid");
    assert_eq!(
        updated.schedule,
        Schedule::Interval {
            value: 10,
            unit: IntervalUnit::Minute
        }
    );
}

#[test]
fn sdk_wire_format_deserializes() {
    let json = r#"{"schedule":{"type":"interval","value":2,"unit":"hour"},"checkin_margin":5,"max_runtime":10,"timezone":"America/Los_Angeles"}"#;
    let i: MonitorConfigInput = serde_json::from_str(json).unwrap();
    let c = i.validate(None).unwrap();
    assert_eq!(
        c.schedule,
        Schedule::Interval {
            value: 2,
            unit: IntervalUnit::Hour
        }
    );
    assert_eq!(c.checkin_margin, 5);
}
