//! Cron monitors (Sentry Crons).
//!
//! A monitor expects a job to check in on a schedule. The wire shape of
//! `monitor_config` is Relay's `MonitorConfig`
//! (relay-monitors/src/lib.rs), and the schedule arithmetic mirrors Sentry's
//! `monitors/schedule.py` so a monitor expects its next check-in at the same
//! instant here as it would in Sentry.

use crate::error::{AppError, AppResult, FieldErrorCode};
use crate::scheduler::schedule::{parse_timezone, truncate_to_minute};
use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

/// Minutes after the expected time before a check-in counts as missed.
pub const DEFAULT_CHECKIN_MARGIN: u32 = 1;
/// 28 days; Sentry's `MAX_MARGIN`.
pub const MAX_CHECKIN_MARGIN: u32 = 40_320;
/// Minutes an `in_progress` check-in may stay open before it times out.
pub const DEFAULT_MAX_RUNTIME: u32 = 30;
/// 7 days; Sentry's `MAX_TIMEOUT`.
pub const MAX_MAX_RUNTIME: u32 = 10_080;
/// Bounds of `failure_issue_threshold` and `recovery_threshold`.
pub const MAX_THRESHOLD: u64 = 720;

pub use crate::scheduler::schedule::{IntervalUnit, Schedule};

/// A validated monitor configuration, with every default filled in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct MonitorConfig {
    pub schedule: Schedule,
    /// Minutes.
    pub checkin_margin: u32,
    /// Minutes.
    pub max_runtime: u32,
    /// IANA name.
    pub timezone: String,
    pub failure_issue_threshold: u32,
    pub recovery_threshold: u32,
}

/// `monitor_config` as an SDK or the dashboard sends it: every field optional.
#[derive(Debug, Clone, Default, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct MonitorConfigInput {
    pub schedule: Option<Schedule>,
    pub checkin_margin: Option<u64>,
    pub max_runtime: Option<u64>,
    pub timezone: Option<String>,
    pub failure_issue_threshold: Option<u64>,
    pub recovery_threshold: Option<u64>,
}

impl MonitorConfigInput {
    /// Validates the input. A field left out keeps its value in `existing`
    /// (Sentry's `Monitor.update_config`), and otherwise takes the default.
    /// Margin and runtime are clamped rather than rejected, as Sentry's
    /// consumer does for SDK upserts.
    pub fn validate(self, existing: Option<&MonitorConfig>) -> AppResult<MonitorConfig> {
        let schedule = self
            .schedule
            .or_else(|| existing.map(|c| c.schedule.clone()))
            .ok_or_else(|| invalid("schedule is required"))?;
        schedule.validate().map_err(|e| invalid(&e))?;

        let timezone = self
            .timezone
            .or_else(|| existing.map(|c| c.timezone.clone()))
            .unwrap_or_else(|| "UTC".to_string());
        parse_timezone(&timezone).map_err(|e| invalid(&e))?;

        let minutes = |input: Option<u64>, current: Option<u32>, default: u32, max: u32| {
            input
                .map(|m| m.clamp(1, max as u64) as u32)
                .or(current)
                .unwrap_or(default)
        };
        let threshold = |input: Option<u64>, current: Option<u32>, name: &str| match input {
            Some(n) if !(1..=MAX_THRESHOLD).contains(&n) => Err(invalid(&format!(
                "{name} must be between 1 and {MAX_THRESHOLD}"
            ))),
            Some(n) => Ok(n as u32),
            None => Ok(current.unwrap_or(1)),
        };

        Ok(MonitorConfig {
            schedule,
            checkin_margin: minutes(
                self.checkin_margin,
                existing.map(|c| c.checkin_margin),
                DEFAULT_CHECKIN_MARGIN,
                MAX_CHECKIN_MARGIN,
            ),
            max_runtime: minutes(
                self.max_runtime,
                existing.map(|c| c.max_runtime),
                DEFAULT_MAX_RUNTIME,
                MAX_MAX_RUNTIME,
            ),
            timezone,
            failure_issue_threshold: threshold(
                self.failure_issue_threshold,
                existing.map(|c| c.failure_issue_threshold),
                "failure_issue_threshold",
            )?,
            recovery_threshold: threshold(
                self.recovery_threshold,
                existing.map(|c| c.recovery_threshold),
                "recovery_threshold",
            )?,
        })
    }
}

fn invalid(message: &str) -> AppError {
    AppError::Validation(format!("Invalid monitor config: {message}"))
}

impl MonitorConfig {
    fn tz(&self) -> Tz {
        parse_timezone(&self.timezone).unwrap_or(Tz::UTC)
    }

    /// The first expected check-in after `reference`
    /// (Sentry's `get_next_schedule`).
    pub fn next_checkin(&self, reference: DateTime<Utc>) -> DateTime<Utc> {
        self.schedule.next_after(reference, self.tz())
    }

    /// [`Self::next_checkin`] plus the margin: once this passes without a
    /// check-in, the check-in is missed.
    pub fn next_checkin_latest(&self, reference: DateTime<Utc>) -> DateTime<Utc> {
        self.next_checkin(reference) + Duration::minutes(self.checkin_margin as i64)
    }

    /// The last expected check-in strictly before `reference`. An interval
    /// counts from `start` (Sentry's `get_prev_schedule`).
    pub fn prev_checkin(&self, start: DateTime<Utc>, reference: DateTime<Utc>) -> DateTime<Utc> {
        self.schedule.prev_before(start, reference, self.tz())
    }

    /// When an `in_progress` check-in that started at `started` times out
    /// (Sentry's `get_timeout_at`).
    pub fn timeout_at(&self, started: DateTime<Utc>) -> DateTime<Utc> {
        truncate_to_minute(started) + Duration::minutes(self.max_runtime as i64)
    }
}

/// Status of one check-in. `Missed` and `Timeout` are only ever set by the
/// server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum CheckInStatus {
    Ok,
    Error,
    InProgress,
    Missed,
    Timeout,
    #[serde(other)]
    Unknown,
}

/// A normalized `check_in` item.
#[derive(Debug, Clone)]
pub struct CheckIn {
    pub check_in_id: uuid::Uuid,
    pub monitor_slug: String,
    pub status: CheckInStatus,
    pub environment: String,
    pub duration: Option<f64>,
    pub monitor_config: Option<MonitorConfigInput>,
    pub trace_id: Option<String>,
}

/// Longest monitor slug; Sentry's `DEFAULT_SLUG_MAX_LENGTH`.
pub const MAX_SLUG_LENGTH: usize = 50;
/// Longest environment name (Relay's `MAX_ENVIRONMENT_LENGTH`).
pub const MAX_ENVIRONMENT_LENGTH: usize = 64;
const DEFAULT_ENVIRONMENT: &str = "production";

/// Wire shape of a `check_in` item (relay-monitors/src/lib.rs `CheckIn`).
#[derive(Deserialize)]
struct CheckInPayload {
    #[serde(default)]
    check_in_id: Option<uuid::Uuid>,
    #[serde(default)]
    monitor_slug: String,
    status: CheckInStatus,
    #[serde(default)]
    environment: Option<String>,
    #[serde(default)]
    duration: Option<f64>,
    #[serde(default)]
    monitor_config: Option<MonitorConfigInput>,
    #[serde(default)]
    contexts: Option<CheckInContexts>,
}

#[derive(Deserialize)]
struct CheckInContexts {
    trace: Option<TraceContext>,
}

#[derive(Deserialize)]
struct TraceContext {
    trace_id: String,
}

impl CheckIn {
    /// Parses and normalizes a `check_in` item body.
    pub fn parse(body: &[u8]) -> AppResult<Self> {
        let payload: CheckInPayload = serde_json::from_slice(body)
            .map_err(|e| AppError::Validation(format!("Invalid check-in JSON: {e}")))?;
        Self::new(
            payload.check_in_id.unwrap_or_default(),
            &payload.monitor_slug,
            payload.status,
            payload.environment,
            payload.duration,
            payload.monitor_config,
            payload.contexts.and_then(|c| c.trace).map(|t| t.trace_id),
        )
    }

    /// Normalizes a check-in from any source (envelope item or the HTTP
    /// endpoint).
    pub fn new(
        check_in_id: uuid::Uuid,
        monitor_slug: &str,
        status: CheckInStatus,
        environment: Option<String>,
        duration: Option<f64>,
        monitor_config: Option<MonitorConfigInput>,
        trace_id: Option<String>,
    ) -> AppResult<Self> {
        let monitor_slug = slugify_monitor_slug(monitor_slug);
        if monitor_slug.is_empty() {
            return Err(AppError::Validation(
                "check-in monitor_slug is empty".to_string(),
            ));
        }
        let environment = environment
            .filter(|e| !e.is_empty())
            .unwrap_or_else(|| DEFAULT_ENVIRONMENT.to_string());
        if environment.chars().count() > MAX_ENVIRONMENT_LENGTH {
            return Err(AppError::Validation(
                "check-in environment is longer than 64 characters".to_string(),
            ));
        }
        Ok(Self {
            check_in_id,
            monitor_slug,
            // Only the server decides a check-in was missed (Relay's
            // `CheckIn::normalize`).
            status: match status {
                CheckInStatus::Missed => CheckInStatus::Unknown,
                other => other,
            },
            environment,
            duration: duration.filter(|d| d.is_finite() && *d >= 0.0),
            monitor_config,
            trace_id,
        })
    }
}

/// Django's `slugify`, cut to 50 characters, as Sentry's consumer applies it
/// (`slugify_monitor_slug` in `monitors/types.py`).
// ponytail: non-ASCII letters are dropped instead of NFKD-folded ("café" →
// "caf" here, "cafe" in Sentry); add unicode-normalization if that matters.
pub fn slugify_monitor_slug(raw: &str) -> String {
    let mut slug = String::new();
    let mut pending_dash = false;
    for ch in raw.to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            if pending_dash && !slug.is_empty() {
                slug.push('-');
            }
            pending_dash = false;
            slug.push(ch);
        } else if ch == '-' || ch.is_whitespace() {
            pending_dash = true;
        }
    }
    let slug: String = slug
        .trim_matches('_')
        .chars()
        .take(MAX_SLUG_LENGTH)
        .collect();
    slug.trim_matches('-').to_string()
}

impl CheckInStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Error => "error",
            Self::InProgress => "in_progress",
            Self::Missed => "missed",
            Self::Timeout => "timeout",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse(s: &str) -> Self {
        serde_json::from_value(serde_json::Value::String(s.to_string())).unwrap_or(Self::Unknown)
    }

    /// A status that counts against a monitor (Sentry's `FAILED_STATUSES`).
    pub fn is_failure(self) -> bool {
        matches!(self, Self::Error | Self::Missed | Self::Timeout)
    }
}

/// Health of one monitor environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum MonitorEnvironmentStatus {
    /// Created, waiting for its first successful check-in.
    Active,
    Ok,
    /// An incident is open.
    Error,
}

impl MonitorEnvironmentStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Ok => "ok",
            Self::Error => "error",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "ok" => Self::Ok,
            "error" => Self::Error,
            _ => Self::Active,
        }
    }
}

/// Why a monitor failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureReason {
    Error,
    Missed,
    Timeout,
}

impl FailureReason {
    fn describe(self) -> &'static str {
        match self {
            Self::Error => "A failed check-in was detected",
            Self::Missed => "A missed check-in was detected",
            Self::Timeout => "A check-in timed out",
        }
    }
}

/// A failure that crossed the monitor's threshold and must be reported as an
/// issue event.
#[derive(Debug, Clone)]
pub struct MonitorFailure {
    pub project_id: i32,
    pub monitor_slug: String,
    pub monitor_name: String,
    pub environment: String,
    pub monitor_environment_id: uuid::Uuid,
    pub check_in_id: uuid::Uuid,
    pub reason: FailureReason,
    pub occurred_at: DateTime<Utc>,
}

impl MonitorFailure {
    /// One fingerprint per monitor environment, like Sentry's
    /// `MonitorEnvironment.build_occurrence_fingerprint`: every failure of an
    /// environment lands in the same issue.
    pub fn fingerprint(monitor_environment_id: uuid::Uuid) -> String {
        format!("crons:{monitor_environment_id}")
    }

    /// The synthetic error event that carries this failure through the
    /// regular digest: grouping, issue creation, regressions and alerts.
    pub fn to_event(&self, event_id: uuid::Uuid) -> serde_json::Value {
        serde_json::json!({
            "event_id": event_id.simple().to_string(),
            "timestamp": self.occurred_at.timestamp() as f64,
            "platform": "other",
            "level": "error",
            "environment": self.environment,
            "transaction": self.monitor_slug,
            "fingerprint": [Self::fingerprint(self.monitor_environment_id)],
            "exception": {"values": [{
                "type": format!("Cron failure: {}", self.monitor_name),
                "value": self.reason.describe(),
                "mechanism": {"type": "cron_monitor", "handled": false},
            }]},
            "tags": {
                "monitor.slug": self.monitor_slug,
                "monitor.check_in_id": self.check_in_id.to_string(),
            },
        })
    }
}

/// One check-in, as the dashboard reads it.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CheckInResponse {
    pub id: uuid::Uuid,
    pub environment: String,
    pub status: CheckInStatus,
    pub duration_ms: Option<i64>,
    pub date_added: DateTime<Utc>,
    pub date_updated: DateTime<Utc>,
    pub expected_time: Option<DateTime<Utc>>,
}

/// One environment of a monitor, with its most recent check-ins for the
/// timeline.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct MonitorEnvironmentResponse {
    pub id: uuid::Uuid,
    pub environment: String,
    pub status: MonitorEnvironmentStatus,
    pub next_checkin: Option<DateTime<Utc>>,
    pub next_checkin_latest: Option<DateTime<Utc>>,
    pub last_checkin: Option<DateTime<Utc>>,
    /// Newest first.
    pub recent_checkins: Vec<CheckInResponse>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct MonitorResponse {
    pub id: uuid::Uuid,
    pub slug: String,
    pub name: String,
    pub config: MonitorConfig,
    pub is_muted: bool,
    pub created_at: DateTime<Utc>,
    pub environments: Vec<MonitorEnvironmentResponse>,
}

/// Body of `POST /api/projects/{project_id}/monitors`.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateMonitor {
    pub name: String,
    /// Derived from `name` when left out. This is the slug SDKs check in with.
    pub slug: Option<String>,
    pub config: MonitorConfigInput,
}

/// Body of `PUT /api/projects/{project_id}/monitors/{slug}`. Fields left out
/// keep their value, including each field of `config`.
#[derive(Debug, Clone, Default, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdateMonitor {
    pub name: Option<String>,
    pub config: Option<MonitorConfigInput>,
    pub is_muted: Option<bool>,
}

/// Longest monitor name.
pub const MAX_NAME_LENGTH: usize = 128;

/// A trimmed, non-empty name of at most [`MAX_NAME_LENGTH`] characters.
pub fn validate_monitor_name(name: &str) -> AppResult<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::Validation("Monitor name is required".to_string())
            .with_field("name", FieldErrorCode::Required));
    }
    if name.chars().count() > MAX_NAME_LENGTH {
        return Err(AppError::Validation(format!(
            "Monitor name is longer than {MAX_NAME_LENGTH} characters"
        ))
        .with_field("name", FieldErrorCode::TooLong));
    }
    Ok(name.to_string())
}
