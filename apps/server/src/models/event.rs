use crate::models::AlertType;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

/// Event model - a single error or transaction occurrence
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Event {
    pub id: Uuid,
    pub event_id: Uuid,
    pub project_id: i32,
    /// NULL for transaction events (they don't belong to an issue)
    pub issue_id: Option<Uuid>,
    /// NULL for transaction events (they don't have a grouping)
    pub grouping_id: Option<i32>,
    pub data: serde_json::Value,
    pub timestamp: DateTime<Utc>,
    pub ingested_at: DateTime<Utc>,
    pub digested_at: DateTime<Utc>,
    pub calculated_type: String,
    pub calculated_value: String,
    pub transaction: String,
    pub last_frame_filename: String,
    pub last_frame_module: String,
    pub last_frame_function: String,
    pub level: String,
    pub platform: String,
    pub release: String,
    pub environment: String,
    pub server_name: String,
    pub sdk_name: String,
    pub sdk_version: String,
    pub remote_addr: Option<String>,
    pub alert_type: Option<AlertType>,
    /// "error" for error events, "transaction" for performance events
    pub event_type: String,
}

/// Response for API (list view)
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct EventResponse {
    pub id: Uuid,
    pub event_id: Uuid,
    pub issue_id: Option<Uuid>,
    pub title: String,
    pub timestamp: DateTime<Utc>,
    pub level: String,
    pub platform: String,
    pub release: String,
    pub environment: String,
    pub event_type: String,
}

/// Row shape for event list queries: every column the list response needs,
/// minus `data`. The list never returns the payload, so `SELECT *` was
/// loading and JSON-parsing the full event blob per row just to drop it —
/// the largest avoidable allocation in the read path.
#[derive(Debug, Clone, FromRow)]
pub struct EventSummary {
    pub id: Uuid,
    pub event_id: Uuid,
    pub issue_id: Option<Uuid>,
    pub timestamp: DateTime<Utc>,
    pub calculated_type: String,
    pub calculated_value: String,
    pub level: String,
    pub platform: String,
    pub release: String,
    pub environment: String,
    pub event_type: String,
}

impl EventSummary {
    /// Converts to API response format (list view), same shape as
    /// [`Event::to_response`].
    pub fn to_response(&self) -> EventResponse {
        EventResponse {
            id: self.id,
            event_id: self.event_id,
            issue_id: self.issue_id,
            title: event_title(&self.calculated_type, &self.calculated_value),
            timestamp: self.timestamp,
            level: self.level.clone(),
            platform: self.platform.clone(),
            release: self.release.clone(),
            environment: self.environment.clone(),
            event_type: self.event_type.clone(),
        }
    }
}

/// The issue title: "Type: first line of value", or just the type when the
/// value is empty. Shared by [`Event`] and [`EventSummary`].
fn event_title(calculated_type: &str, calculated_value: &str) -> String {
    if calculated_value.is_empty() {
        calculated_type.to_string()
    } else {
        let first_line = calculated_value.lines().next().unwrap_or("");
        format!("{}: {}", calculated_type, first_line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_title_is_just_the_type_when_the_value_is_empty() {
        assert_eq!(event_title("ValueError", ""), "ValueError");
    }

    #[test]
    fn event_title_prefixes_the_first_value_line() {
        assert_eq!(
            event_title("ValueError", "bad input\nmore"),
            "ValueError: bad input"
        );
    }

    #[test]
    fn event_summary_response_renders_the_shared_title() {
        let summary = EventSummary {
            id: Uuid::new_v4(),
            event_id: Uuid::new_v4(),
            issue_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            calculated_type: "ValueError".to_string(),
            calculated_value: "bad input\nmore".to_string(),
            level: "error".to_string(),
            platform: "python".to_string(),
            release: "1.0".to_string(),
            environment: "prod".to_string(),
            event_type: "error".to_string(),
        };

        let response = summary.to_response();
        assert_eq!(response.title, "ValueError: bad input");
        assert_eq!(response.level, "error");
        assert_eq!(response.platform, "python");
        assert_eq!(response.event_id, summary.event_id);
        assert_eq!(response.issue_id, summary.issue_id);
    }
}

/// Response for API (full detail)
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct EventDetailResponse {
    pub id: Uuid,
    pub event_id: Uuid,
    pub issue_id: Option<Uuid>,
    pub title: String,
    pub timestamp: DateTime<Utc>,
    pub ingested_at: DateTime<Utc>,
    pub level: String,
    pub platform: String,
    pub release: String,
    pub environment: String,
    pub server_name: String,
    pub sdk_name: String,
    pub sdk_version: String,
    pub event_type: String,
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub data: serde_json::Value,
}

impl Event {
    /// Generates the event title from type and value
    pub fn title(&self) -> String {
        event_title(&self.calculated_type, &self.calculated_value)
    }

    /// Converts to API response format (list view)
    pub fn to_response(&self) -> EventResponse {
        EventResponse {
            id: self.id,
            event_id: self.event_id,
            issue_id: self.issue_id,
            title: self.title(),
            timestamp: self.timestamp,
            level: self.level.clone(),
            platform: self.platform.clone(),
            release: self.release.clone(),
            environment: self.environment.clone(),
            event_type: self.event_type.clone(),
        }
    }

    /// Converts to API response format (full detail)
    pub fn to_detail_response(&self) -> EventDetailResponse {
        EventDetailResponse {
            id: self.id,
            event_id: self.event_id,
            issue_id: self.issue_id,
            title: self.title(),
            timestamp: self.timestamp,
            ingested_at: self.ingested_at,
            level: self.level.clone(),
            platform: self.platform.clone(),
            release: self.release.clone(),
            environment: self.environment.clone(),
            server_name: self.server_name.clone(),
            sdk_name: self.sdk_name.clone(),
            sdk_version: self.sdk_version.clone(),
            event_type: self.event_type.clone(),
            data: self.data.clone(),
        }
    }
}

/// Where one event sits among its issue's events, oldest first.
#[derive(Debug, Serialize, FromRow)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct EventNavigation {
    /// 1-based position of the event, oldest first.
    pub current_index: i64,
    pub total_count: i64,
    pub first_event_id: Option<Uuid>,
    pub last_event_id: Option<Uuid>,
    pub prev_event_id: Option<Uuid>,
    pub next_event_id: Option<Uuid>,
}
