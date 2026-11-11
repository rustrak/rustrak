//! Schedule arithmetic for cron monitors. The expected values are the
//! examples in Sentry's `monitors/schedule.py` docstrings, so a monitor
//! expects its next check-in at the same instant here as in Sentry.

use chrono::{DateTime, Utc};
use rustrak::models::monitor::{MonitorConfig, MonitorConfigInput};
use rustrak::models::schedule::{IntervalUnit, Schedule};

fn at(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

fn crontab(value: &str) -> MonitorConfig {
    config(Schedule::Crontab {
        value: value.to_string(),
    })
}

fn interval(value: u32, unit: IntervalUnit) -> MonitorConfig {
    config(Schedule::Interval { value, unit })
}

fn config(schedule: Schedule) -> MonitorConfig {
    MonitorConfigInput {
        schedule: Some(schedule),
        ..Default::default()
    }
    .validate(None)
    .unwrap()
}

#[test]
fn crontab_next_is_the_following_tick() {
    let c = crontab("0 * * * *");
    assert_eq!(
        c.next_checkin(at("2026-01-01T05:30:00Z")),
        at("2026-01-01T06:00:00Z")
    );
}

#[test]
fn crontab_next_is_strictly_after_the_reference() {
    let c = crontab("30 * * * *");
    assert_eq!(
        c.next_checkin(at("2026-01-01T05:30:00Z")),
        at("2026-01-01T06:30:00Z")
    );
}

#[test]
fn interval_next_is_anchored_at_the_reference_and_truncated_to_the_minute() {
    let c = interval(2, IntervalUnit::Hour);
    assert_eq!(
        c.next_checkin(at("2026-01-01T05:35:42Z")),
        at("2026-01-01T07:35:00Z")
    );
}

#[test]
fn crontab_is_evaluated_in_the_monitor_timezone_across_dst() {
    let c = MonitorConfigInput {
        schedule: Some(Schedule::Crontab {
            value: "0 9 * * *".to_string(),
        }),
        timezone: Some("Europe/Madrid".to_string()),
        ..Default::default()
    }
    .validate(None)
    .unwrap();

    // 09:00 Madrid is 08:00Z in winter (CET) and 07:00Z in summer (CEST).
    assert_eq!(
        c.next_checkin(at("2026-01-15T07:00:00Z")),
        at("2026-01-15T08:00:00Z")
    );
    assert_eq!(
        c.next_checkin(at("2026-07-15T06:00:00Z")),
        at("2026-07-15T07:00:00Z")
    );
}

#[test]
fn prev_checkin_is_the_last_tick_strictly_before_the_reference() {
    let start = at("2026-01-01T01:30:00Z");
    assert_eq!(
        crontab("0 * * * *").prev_checkin(start, at("2026-01-01T05:35:00Z")),
        at("2026-01-01T05:00:00Z")
    );
    assert_eq!(
        crontab("30 * * * *").prev_checkin(start, at("2026-01-01T05:30:00Z")),
        at("2026-01-01T04:30:00Z")
    );
}

#[test]
fn interval_prev_checkin_counts_from_the_start() {
    let c = interval(2, IntervalUnit::Hour);
    let start = at("2026-01-01T01:30:00Z");
    assert_eq!(
        c.prev_checkin(start, at("2026-01-01T05:30:00Z")),
        at("2026-01-01T03:30:00Z")
    );
    assert_eq!(
        c.prev_checkin(start, at("2026-01-01T05:35:00Z")),
        at("2026-01-01T05:30:00Z")
    );
}

#[test]
fn latest_checkin_adds_the_margin() {
    let c = MonitorConfigInput {
        schedule: Some(Schedule::Crontab {
            value: "0 * * * *".to_string(),
        }),
        checkin_margin: Some(5),
        ..Default::default()
    }
    .validate(None)
    .unwrap();
    assert_eq!(
        c.next_checkin_latest(at("2026-01-01T05:30:00Z")),
        at("2026-01-01T06:05:00Z")
    );
}

#[test]
fn month_interval_uses_calendar_months() {
    let c = interval(1, IntervalUnit::Month);
    assert_eq!(
        c.next_checkin(at("2026-01-15T10:00:00Z")),
        at("2026-02-15T10:00:00Z")
    );
}

#[test]
fn a_run_that_starts_just_before_its_slot_counts_from_that_slot() {
    let c = crontab("0 * * * *");
    let slot = Some(at("2026-01-01T06:00:00Z"));
    // The job's clock runs 0.35s ahead of the server's.
    let t = at("2026-01-01T05:59:59.650Z");
    assert_eq!(c.checkin_reference(t, slot, t), at("2026-01-01T06:00:00Z"));
    // Its result, after the slot, counts from itself.
    let done = at("2026-01-01T06:00:20Z");
    assert_eq!(c.checkin_reference(t, slot, done), done);
}

#[test]
fn a_run_on_time_or_well_before_its_slot_counts_from_itself() {
    let c = crontab("0 * * * *");
    let slot = Some(at("2026-01-01T06:00:00Z"));
    for s in [
        "2026-01-01T06:00:00Z",
        "2026-01-01T06:00:00.300Z",
        "2026-01-01T05:59:50Z",
        "2026-01-01T05:40:00Z",
    ] {
        assert_eq!(c.checkin_reference(at(s), slot, at(s)), at(s), "{s}");
    }
}

#[test]
fn a_late_or_long_run_never_fills_the_next_slot() {
    let c = crontab("* * * * *");
    let slot = Some(at("2026-01-01T06:00:00Z"));
    // A result 45s into its own minute, alone or after a start at 06:00.
    let done = at("2026-01-01T06:00:45Z");
    assert_eq!(c.checkin_reference(done, slot, done), done);
    assert_eq!(
        c.checkin_reference(at("2026-01-01T06:00:00Z"), slot, done),
        done
    );
    assert_eq!(c.next_checkin(done), at("2026-01-01T06:01:00Z"));
}

#[test]
fn a_first_run_with_no_expected_slot_takes_the_next_slot() {
    let c = crontab("0 * * * *");
    let t = at("2026-01-01T05:59:59.600Z");
    assert_eq!(c.checkin_reference(t, None, t), at("2026-01-01T06:00:00Z"));
    // Its result, after the slot, counts from itself.
    let done = at("2026-01-01T06:00:20Z");
    assert_eq!(c.checkin_reference(t, None, done), done);
    // On time or well before the slot, it counts from itself.
    for s in ["2026-01-01T06:00:00Z", "2026-01-01T05:59:50Z"] {
        assert_eq!(c.checkin_reference(at(s), None, at(s)), at(s), "{s}");
    }
}

#[test]
fn a_late_first_minutely_run_fills_the_next_slot_early() {
    // The trade-off in the doc comment: with no expected slot, a run that
    // starts in the last 10s of a minute takes the next minute's slot.
    let c = crontab("* * * * *");
    let started = at("2026-01-01T06:00:55Z");
    let done = at("2026-01-01T06:00:58Z");
    assert_eq!(
        c.checkin_reference(started, None, done),
        at("2026-01-01T06:01:00Z")
    );
}

#[test]
fn an_interval_counts_from_the_run_itself() {
    let c = interval(1, IntervalUnit::Minute);
    let t = at("2026-01-01T05:59:59.600Z");
    assert_eq!(
        c.checkin_reference(t, Some(at("2026-01-01T06:00:00Z")), t),
        t
    );
}
