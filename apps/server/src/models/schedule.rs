//! When something recurs: a five-field crontab or a fixed interval, read in a
//! timezone. Shared by cron monitors (when a check-in is expected) and the
//! internal scheduler (when a job runs), so both agree on every instant.
//!
//! The arithmetic mirrors Sentry's `monitors/schedule.py`: crontabs through
//! `croner`, intervals added on the wall clock as `dateutil.rrule` does.

use chrono::{
    DateTime, Duration, DurationRound, LocalResult, Months, NaiveDateTime, TimeZone, Utc,
};
use chrono_tz::Tz;
use croner::parser::{CronParser, Seconds, Year};
use croner::Cron;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Schedule {
    /// A five-field crontab, or an alias such as `@daily`.
    Crontab { value: String },
    /// Every `value` `unit`s, counted from the previous run.
    Interval { value: u32, unit: IntervalUnit },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum IntervalUnit {
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Year,
}

/// An IANA zone, or why it is not one.
pub fn parse_timezone(name: &str) -> Result<Tz, String> {
    Tz::from_str(name).map_err(|_| format!("unknown timezone {name}"))
}

/// Five fields, minute granularity: Sentry rejects seconds and years.
fn parse_crontab(value: &str) -> Result<Cron, String> {
    CronParser::builder()
        .seconds(Seconds::Disallowed)
        .year(Year::Disallowed)
        .build()
        .parse(value)
        .map_err(|e| format!("crontab: {e}"))
}

impl Schedule {
    /// Why the schedule can never run, if it can't.
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Schedule::Crontab { value } => {
                let cron = parse_crontab(value)?;
                // A pattern that never fires (`0 0 30 2 *`) parses fine.
                cron.find_next_occurrence(&Utc::now(), false)
                    .map(|_| ())
                    .map_err(|_| "crontab never matches a date".to_string())
            }
            Schedule::Interval { value: 0, .. } => Err("interval must be at least 1".to_string()),
            // Past this the date arithmetic overflows and a run never advances.
            Schedule::Interval { value, unit }
                if approx_seconds(*value, *unit) > approx_seconds(100, IntervalUnit::Year) =>
            {
                Err("interval must be at most 100 years".to_string())
            }
            Schedule::Interval { .. } => Ok(()),
        }
    }

    /// The first run strictly after `reference` (Sentry's
    /// `get_next_schedule`). An interval counts from `reference`.
    pub fn next_after(&self, reference: DateTime<Utc>, tz: Tz) -> DateTime<Utc> {
        let next = match self {
            Schedule::Crontab { value } => parse_crontab(value)
                .ok()
                .and_then(|cron| {
                    cron.find_next_occurrence(&reference.with_timezone(&tz), false)
                        .ok()
                })
                .map(|t| t.with_timezone(&Utc))
                // Validation rejects patterns without a next date; this only
                // guards one stored before that check.
                .unwrap_or(reference + Duration::days(1)),
            Schedule::Interval { value, unit } => step(tz, reference, *value, *unit, 1),
        };
        truncate_to_minute(next)
    }

    /// The last run strictly before `reference`. An interval counts from
    /// `start` (Sentry's `get_prev_schedule`).
    pub fn prev_before(
        &self,
        start: DateTime<Utc>,
        reference: DateTime<Utc>,
        tz: Tz,
    ) -> DateTime<Utc> {
        let prev = match self {
            Schedule::Crontab { value } => parse_crontab(value)
                .ok()
                .and_then(|cron| {
                    cron.find_previous_occurrence(&reference.with_timezone(&tz), false)
                        .ok()
                })
                .map(|t| t.with_timezone(&Utc))
                .unwrap_or(start),
            Schedule::Interval { value, unit } => {
                if reference <= start {
                    start
                } else {
                    // Estimate how many steps fit, then correct the estimate:
                    // months and years have no fixed length.
                    let approx = (reference - start).num_seconds() / approx_seconds(*value, *unit);
                    let mut k = approx.max(0) as u32;
                    while step(tz, start, *value, *unit, k + 1) < reference {
                        k += 1;
                    }
                    while k > 0 && step(tz, start, *value, *unit, k) >= reference {
                        k -= 1;
                    }
                    step(tz, start, *value, *unit, k)
                }
            }
        };
        truncate_to_minute(prev)
    }

    /// The next `count` runs after `from`, each counted from the one before.
    pub fn upcoming(&self, from: DateTime<Utc>, tz: Tz, count: usize) -> Vec<DateTime<Utc>> {
        std::iter::successors(Some(self.next_after(from, tz)), |prev| {
            Some(self.next_after(*prev, tz))
        })
        .take(count)
        .collect()
    }
}

fn approx_seconds(value: u32, unit: IntervalUnit) -> i64 {
    let unit_secs: i64 = match unit {
        IntervalUnit::Minute => 60,
        IntervalUnit::Hour => 3_600,
        IntervalUnit::Day => 86_400,
        IntervalUnit::Week => 604_800,
        IntervalUnit::Month => 2_629_746,
        IntervalUnit::Year => 31_556_952,
    };
    unit_secs * value.max(1) as i64
}

/// `from` plus `k` intervals, added on the wall clock as `dateutil.rrule`
/// does, so a daily run keeps its hour across DST.
fn step(tz: Tz, from: DateTime<Utc>, value: u32, unit: IntervalUnit, k: u32) -> DateTime<Utc> {
    let local = from.with_timezone(&tz).naive_local();
    let n = value as i64 * k as i64;
    let shifted: Option<NaiveDateTime> = match unit {
        IntervalUnit::Minute => local.checked_add_signed(Duration::minutes(n)),
        IntervalUnit::Hour => local.checked_add_signed(Duration::hours(n)),
        IntervalUnit::Day => local.checked_add_signed(Duration::days(n)),
        IntervalUnit::Week => local.checked_add_signed(Duration::weeks(n)),
        IntervalUnit::Month => add_months(local, n),
        IntervalUnit::Year => add_months(local, n * 12),
    };
    let Some(shifted) = shifted else {
        return from;
    };
    match tz.from_local_datetime(&shifted) {
        LocalResult::Single(t) => t.with_timezone(&Utc),
        // A wall time seen twice when clocks go back: the first one not
        // before `from`, so runs never go backwards.
        LocalResult::Ambiguous(first, second) => {
            let first = first.with_timezone(&Utc);
            if first >= from {
                first
            } else {
                second.with_timezone(&Utc)
            }
        }
        // A wall time skipped when clocks go forward: read it with the
        // offset in force just before the jump, as dateutil does. Gaps run
        // from half an hour to a whole day (Samoa, 2011).
        LocalResult::None => (1..=24)
            .find_map(|h| {
                tz.from_local_datetime(&(shifted - Duration::hours(h)))
                    .earliest()
                    .map(|t| t.with_timezone(&Utc) + Duration::hours(h))
            })
            .unwrap_or(from),
    }
}

fn add_months(local: NaiveDateTime, n: i64) -> Option<NaiveDateTime> {
    local.checked_add_months(Months::new(u32::try_from(n).ok()?))
}

/// Schedules have minute granularity.
pub fn truncate_to_minute(t: DateTime<Utc>) -> DateTime<Utc> {
    t.duration_trunc(Duration::minutes(1)).unwrap_or(t)
}
