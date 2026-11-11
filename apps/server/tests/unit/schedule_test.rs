//! The shared schedule type: what monitors and scheduled jobs agree on.

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use rustrak::models::schedule::{parse_timezone, IntervalUnit, Schedule};

fn at(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

#[test]
fn upcoming_lists_the_next_runs_in_order() {
    let s = Schedule::Crontab {
        value: "0 */6 * * *".to_string(),
    };
    assert_eq!(
        s.upcoming(at("2026-01-01T05:30:00Z"), Tz::UTC, 3),
        vec![
            at("2026-01-01T06:00:00Z"),
            at("2026-01-01T12:00:00Z"),
            at("2026-01-01T18:00:00Z"),
        ]
    );
}

#[test]
fn upcoming_chains_an_interval_from_each_run() {
    let s = Schedule::Interval {
        value: 90,
        unit: IntervalUnit::Minute,
    };
    assert_eq!(
        s.upcoming(at("2026-01-01T00:00:00Z"), Tz::UTC, 2),
        vec![at("2026-01-01T01:30:00Z"), at("2026-01-01T03:00:00Z")]
    );
}

#[test]
fn validate_explains_what_is_wrong() {
    let seconds = Schedule::Crontab {
        value: "0 0 * * * *".to_string(),
    };
    assert!(seconds.validate().is_err());
    let never = Schedule::Crontab {
        value: "0 0 30 2 *".to_string(),
    };
    assert!(never.validate().unwrap_err().contains("never"));
    assert!(parse_timezone("Mars/Olympus").is_err());
}

#[test]
fn validate_rejects_intervals_too_long_to_step() {
    for (value, unit) in [
        (4_000_000_000, IntervalUnit::Month),
        (1_073_741_824, IntervalUnit::Year),
        (101, IntervalUnit::Year),
    ] {
        assert!(Schedule::Interval { value, unit }.validate().is_err());
    }
    let century = Schedule::Interval {
        value: 100,
        unit: IntervalUnit::Year,
    };
    assert!(century.validate().is_ok());
}

#[test]
fn an_interval_steps_over_the_spring_forward_gap() {
    let s = Schedule::Interval {
        value: 1,
        unit: IntervalUnit::Hour,
    };
    // 00:30 EST; 02:30 does not exist on 2026-03-08 in New York.
    assert_eq!(
        s.upcoming(at("2026-03-08T05:30:00Z"), Tz::America__New_York, 3),
        vec![
            at("2026-03-08T06:30:00Z"), // 01:30 EST
            at("2026-03-08T07:30:00Z"), // 03:30 EDT
            at("2026-03-08T08:30:00Z"), // 04:30 EDT
        ]
    );
}

#[test]
fn an_interval_never_goes_back_in_the_repeated_hour() {
    let s = Schedule::Interval {
        value: 10,
        unit: IntervalUnit::Minute,
    };
    // 01:10 EST, the second 01:10 of 2026-11-01 in New York.
    assert_eq!(
        s.next_after(at("2026-11-01T06:10:00Z"), Tz::America__New_York),
        at("2026-11-01T06:20:00Z")
    );
}

#[test]
fn an_interval_steps_over_a_two_hour_gap() {
    let s = Schedule::Interval {
        value: 1,
        unit: IntervalUnit::Day,
    };
    // Troll jumps from 01:00 +00 to 03:00 +02 on 2026-03-29, so 02:30 that
    // day does not exist.
    assert_eq!(
        s.next_after(at("2026-03-28T02:30:00Z"), Tz::Antarctica__Troll),
        at("2026-03-29T02:30:00Z") // 04:30 +02
    );
}
