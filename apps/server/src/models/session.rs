use chrono::{DateTime, NaiveDateTime, Utc};
use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};

fn de_non_negative_i64<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    let v = i64::deserialize(d)?;
    if v < 0 {
        Err(de::Error::custom("must be non-negative"))
    } else {
        Ok(v)
    }
}

/// Wire struct for a single `session` envelope item from a Sentry SDK.
#[derive(Debug, Default, Deserialize)]
pub struct SessionUpdate {
    pub sid: Option<String>,
    pub did: Option<String>,
    pub seq: Option<i64>,
    #[serde(default)]
    pub init: bool,
    pub started: Option<String>,
    pub timestamp: Option<String>,
    pub duration: Option<f64>,
    pub status: Option<SessionStatus>,
    #[serde(default, deserialize_with = "de_non_negative_i64")]
    pub errors: i64,
    pub attrs: Option<SessionAttributes>,
}

/// Wire struct for a `sessions` (pre-aggregated) envelope item.
#[derive(Debug, Deserialize)]
pub struct SessionAggregates {
    pub attrs: Option<SessionAttributes>,
    #[serde(default)]
    pub aggregates: Vec<SessionAggregateItem>,
}

/// One bucket inside a pre-aggregated `sessions` item.
#[derive(Debug, Deserialize)]
pub struct SessionAggregateItem {
    pub started: Option<String>,
    #[serde(default, deserialize_with = "de_non_negative_i64")]
    pub exited: i64,
    #[serde(default, deserialize_with = "de_non_negative_i64")]
    pub errored: i64,
    #[serde(default, deserialize_with = "de_non_negative_i64")]
    pub abnormal: i64,
    #[serde(default, deserialize_with = "de_non_negative_i64")]
    pub crashed: i64,
}

/// Common session attributes.
#[derive(Debug, Deserialize)]
pub struct SessionAttributes {
    pub release: Option<String>,
    pub environment: Option<String>,
}

/// Session terminal status values from the Sentry protocol.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Ok,
    Exited,
    Crashed,
    Abnormal,
    Errored,
}

/// Derived classification used by the aggregator for a single session update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionOutcome {
    Crashed,
    Abnormal,
    Errored,
    Healthy,
}

impl SessionStatus {
    /// Whether this status represents a terminal (final) session update.
    pub fn is_terminal(&self) -> bool {
        !matches!(self, SessionStatus::Ok)
    }
}

/// Classify a terminal session update into an aggregator outcome.
/// `errored` = terminal status is `Errored` OR `errors > 0`.
pub fn classify(status: &SessionStatus, errors: i64) -> SessionOutcome {
    match status {
        SessionStatus::Crashed => SessionOutcome::Crashed,
        SessionStatus::Abnormal => SessionOutcome::Abnormal,
        SessionStatus::Errored => SessionOutcome::Errored,
        SessionStatus::Exited | SessionStatus::Ok => {
            if errors > 0 {
                SessionOutcome::Errored
            } else {
                SessionOutcome::Healthy
            }
        }
    }
}

/// Per-release health row returned by the stats endpoint.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ReleaseHealthRow {
    pub release: String,
    pub environment: String,
    pub total: i64,
    pub errored: i64,
    pub crashed: i64,
    pub abnormal: i64,
    pub healthy: i64,
    pub crash_free_sessions_rate: Option<f64>,
    pub crash_free_users_rate: Option<f64>,
}

/// Project-wide session health summary, aggregated across all releases and environments.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SessionSummary {
    pub total: i64,
    pub errored: i64,
    pub crashed: i64,
    pub abnormal: i64,
    pub crash_free_sessions_rate: Option<f64>,
    pub crash_free_users_rate: Option<f64>,
    pub active_releases: i64,
}

/// One time-bucketed point in a project's session trend, aggregated across all
/// releases and environments. Powers trend charts (crash-free sessions over time).
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SessionTimeseriesPoint {
    pub bucket: DateTime<Utc>,
    pub total: i64,
    pub crashed: i64,
    pub crash_free_sessions_rate: Option<f64>,
}

/// Parse an ISO-8601 timestamp string into a UTC DateTime, returning None on failure.
pub fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
        .or_else(|| {
            NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f")
                .ok()
                .map(|ndt| ndt.and_utc())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_crashed() {
        assert_eq!(
            classify(&SessionStatus::Crashed, 0),
            SessionOutcome::Crashed
        );
    }

    #[test]
    fn classify_abnormal() {
        assert_eq!(
            classify(&SessionStatus::Abnormal, 0),
            SessionOutcome::Abnormal
        );
    }

    #[test]
    fn classify_errored_by_status() {
        assert_eq!(
            classify(&SessionStatus::Errored, 0),
            SessionOutcome::Errored
        );
    }

    #[test]
    fn classify_errored_by_errors_count() {
        // exited with errors > 0 → errored
        assert_eq!(classify(&SessionStatus::Exited, 3), SessionOutcome::Errored);
    }

    #[test]
    fn classify_healthy_exited() {
        assert_eq!(classify(&SessionStatus::Exited, 0), SessionOutcome::Healthy);
    }

    #[test]
    fn classify_healthy_ok() {
        assert_eq!(classify(&SessionStatus::Ok, 0), SessionOutcome::Healthy);
    }

    #[test]
    fn is_terminal_ok_is_not() {
        assert!(!SessionStatus::Ok.is_terminal());
    }

    #[test]
    fn is_terminal_exited_is() {
        assert!(SessionStatus::Exited.is_terminal());
    }

    #[test]
    fn parse_session_update_with_init() {
        let json = r#"{
            "sid": "abc123",
            "did": "user-1",
            "seq": 0,
            "init": true,
            "started": "2026-06-10T10:00:00.000Z",
            "status": "ok",
            "errors": 0,
            "attrs": {"release": "1.0.0", "environment": "production"}
        }"#;
        let update: SessionUpdate = serde_json::from_str(json).unwrap();
        assert!(update.init);
        assert_eq!(update.did.as_deref(), Some("user-1"));
        assert_eq!(
            update.attrs.as_ref().unwrap().release.as_deref(),
            Some("1.0.0")
        );
    }

    #[test]
    fn parse_session_aggregates() {
        let json = r#"{
            "attrs": {"release": "2.0.0", "environment": "staging"},
            "aggregates": [
                {"started": "2026-06-10T10:00:00Z", "exited": 5, "errored": 1, "crashed": 2, "abnormal": 0}
            ]
        }"#;
        let agg: SessionAggregates = serde_json::from_str(json).unwrap();
        assert_eq!(agg.aggregates.len(), 1);
        assert_eq!(agg.aggregates[0].crashed, 2);
    }

    #[test]
    fn parse_ts_rfc3339() {
        let ts = parse_ts("2026-06-10T10:00:00.000Z");
        assert!(ts.is_some());
    }

    #[test]
    fn parse_ts_invalid_returns_none() {
        let ts = parse_ts("not-a-date");
        assert!(ts.is_none());
    }
}
