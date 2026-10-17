//! The shared schedule type: what monitors and scheduled jobs agree on.

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use rustrak::scheduler::schedule::{parse_timezone, IntervalUnit, Schedule};

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
