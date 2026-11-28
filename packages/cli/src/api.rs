//! The response types the commands read, and the paths they are served at.
//!
//! These mirror `apps/server/openapi.json` but stay permissive: every struct
//! ignores fields it does not name, so a server that grows a field does not
//! break an older CLI, and only the fields a command actually prints are
//! declared. The path builders live here too, so a route is spelled once.

use serde::Deserialize;

/// `{items, total_count, page, per_page, total_pages}`: every offset list.
#[derive(Debug, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total_count: i64,
    #[serde(default)]
    pub page: i64,
    #[serde(default)]
    pub per_page: i64,
    #[serde(default)]
    pub total_pages: i64,
    /// Present only on capped lists (`transactions`).
    #[serde(default)]
    pub total_capped: bool,
}

/// `{items, next_cursor, has_more}`: the issue-events list.
#[derive(Debug, Deserialize)]
pub struct CursorPage<T> {
    pub items: Vec<T>,
    #[serde(default)]
    pub next_cursor: Option<String>,
    #[serde(default)]
    pub has_more: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Project {
    pub id: i32,
    pub name: String,
    pub slug: String,
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub dsn: Option<String>,
    #[serde(default)]
    pub stored_event_count: i64,
    #[serde(default)]
    pub digested_event_count: i64,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub stats: Option<ProjectListStats>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProjectListStats {
    #[serde(default)]
    pub open_issues: i64,
    #[serde(default)]
    pub fatal_issues: i64,
    #[serde(default)]
    pub events: Option<MetricDelta>,
    #[serde(default)]
    pub new_issues: Option<MetricDelta>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MetricDelta {
    #[serde(default)]
    pub current: i64,
    #[serde(default)]
    pub previous: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Issue {
    pub id: String,
    #[serde(default)]
    pub short_id: String,
    pub title: String,
    #[serde(default)]
    pub culprit: String,
    #[serde(default)]
    pub level: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub first_seen: String,
    #[serde(default)]
    pub last_seen: String,
    #[serde(default)]
    pub event_count: i64,
    #[serde(default)]
    pub user_count: Option<i64>,
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub release: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Event {
    pub id: String,
    #[serde(default)]
    pub event_id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub level: String,
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub release: String,
    #[serde(default)]
    pub environment: String,
    #[serde(default)]
    pub timestamp: String,
    #[serde(default)]
    pub event_type: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EventDetail {
    pub event_id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub level: String,
    #[serde(default)]
    pub timestamp: String,
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub release: String,
    #[serde(default)]
    pub environment: String,
    #[serde(default)]
    pub sdk_name: String,
    #[serde(default)]
    pub sdk_version: String,
    #[serde(default)]
    pub ingested_at: String,
    /// The raw Sentry event, which is what `--json` should show verbatim.
    #[serde(default)]
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LogEntry {
    pub id: String,
    #[serde(default)]
    pub level: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub timestamp: String,
    #[serde(default)]
    pub environment: Option<String>,
    #[serde(default)]
    pub trace_id: Option<String>,
    #[serde(default)]
    pub attributes: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Transaction {
    pub id: String,
    #[serde(default)]
    pub event_id: String,
    #[serde(default)]
    pub transaction_name: String,
    #[serde(default)]
    pub timestamp: String,
    #[serde(default)]
    pub duration_ms: Option<f64>,
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub environment: String,
    #[serde(default)]
    pub release: String,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TransactionStat {
    #[serde(default)]
    pub transaction_name: String,
    #[serde(default)]
    pub op: Option<String>,
    #[serde(default)]
    pub count: i64,
    #[serde(default)]
    pub p50_ms: f64,
    #[serde(default)]
    pub p95_ms: f64,
    #[serde(default)]
    pub p99_ms: f64,
    #[serde(default)]
    pub avg_ms: f64,
    #[serde(default)]
    pub failure_rate: f64,
    #[serde(default)]
    pub time_share: f64,
    #[serde(default)]
    pub last_seen: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Release {
    pub version: String,
    #[serde(default)]
    pub registered: bool,
    #[serde(default)]
    pub date_created: String,
    #[serde(default)]
    pub date_released: Option<String>,
    #[serde(default, rename = "ref")]
    pub git_ref: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub event_count: i64,
    #[serde(default)]
    pub new_issues: i64,
    #[serde(default)]
    pub sessions: i64,
    #[serde(default)]
    pub crashed: i64,
    #[serde(default)]
    pub crash_free_sessions: Option<f64>,
    #[serde(default)]
    pub crash_free_users: Option<f64>,
    #[serde(default)]
    pub adoption: Option<f64>,
    #[serde(default)]
    pub environments: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Monitor {
    pub id: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub is_muted: bool,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub config: MonitorConfig,
    #[serde(default)]
    pub environments: Vec<MonitorEnvironment>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct MonitorConfig {
    #[serde(default)]
    pub schedule: Option<serde_json::Value>,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub checkin_margin: Option<i64>,
    #[serde(default)]
    pub max_runtime: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MonitorEnvironment {
    #[serde(default)]
    pub environment: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub last_checkin: Option<String>,
    #[serde(default)]
    pub next_checkin: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CheckIn {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub environment: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub duration_ms: Option<i64>,
    #[serde(default)]
    pub date_added: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AlertRule {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub alert_type: String,
    #[serde(default)]
    pub is_enabled: bool,
    #[serde(default)]
    pub cooldown_minutes: i64,
    #[serde(default)]
    pub last_triggered_at: Option<String>,
    #[serde(default)]
    pub channels: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentSummary {
    #[serde(default)]
    pub agent_runs: i64,
    #[serde(default)]
    pub llm_calls: i64,
    #[serde(default)]
    pub tool_calls: i64,
    #[serde(default)]
    pub error_count: i64,
    #[serde(default)]
    pub total_tokens: f64,
    #[serde(default)]
    pub avg_duration_ms: f64,
    #[serde(default)]
    pub p95_duration_ms: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentTrace {
    #[serde(default)]
    pub trace_id: String,
    #[serde(default)]
    pub agent_names: Vec<String>,
    #[serde(default)]
    pub duration_ms: Option<f64>,
    #[serde(default)]
    pub total_tokens: f64,
    #[serde(default)]
    pub tool_call_count: i64,
    #[serde(default)]
    pub llm_call_count: i64,
    #[serde(default)]
    pub error_count: i64,
    #[serde(default)]
    pub started_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentModel {
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub requests: i64,
    #[serde(default)]
    pub errors: i64,
    #[serde(default)]
    pub avg_ms: f64,
    #[serde(default)]
    pub p95_ms: f64,
    #[serde(default)]
    pub input_tokens: f64,
    #[serde(default)]
    pub output_tokens: f64,
    #[serde(default)]
    pub total_tokens: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentTool {
    #[serde(default)]
    pub tool: String,
    #[serde(default)]
    pub calls: i64,
    #[serde(default)]
    pub errors: i64,
    #[serde(default)]
    pub avg_ms: f64,
    #[serde(default)]
    pub p95_ms: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProjectStats {
    #[serde(default)]
    pub period_hours: Option<i64>,
    #[serde(default)]
    pub events: Option<MetricDelta>,
    #[serde(default)]
    pub new_issues: Option<MetricDelta>,
    #[serde(default)]
    pub open_issues: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SessionSummary {
    #[serde(default)]
    pub total: i64,
    #[serde(default)]
    pub errored: i64,
    #[serde(default)]
    pub crashed: i64,
    #[serde(default)]
    pub abnormal: i64,
    #[serde(default)]
    pub crash_free_sessions_rate: Option<f64>,
    #[serde(default)]
    pub crash_free_users_rate: Option<f64>,
    #[serde(default)]
    pub active_releases: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActivityEntry {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub r#type: String,
    #[serde(default)]
    pub data: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub user_id: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TeamMember {
    #[serde(default)]
    pub id: i32,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub is_active: bool,
    #[serde(default)]
    pub last_login: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthToken {
    #[serde(default)]
    pub id: i32,
    #[serde(default)]
    pub token_prefix: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub last_used_at: Option<String>,
    #[serde(default)]
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreatedToken {
    pub id: i32,
    pub token: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct CurrentUser {
    #[serde(default)]
    pub id: i32,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub is_admin: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthResponse {
    pub user: CurrentUser,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MyAccess {
    #[serde(default)]
    pub is_owner: bool,
    #[serde(default)]
    pub instance: Vec<Permission>,
    #[serde(default)]
    pub projects: Vec<ProjectPermissions>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Permission {
    #[serde(default)]
    pub resource: String,
    #[serde(default)]
    pub action: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProjectPermissions {
    #[serde(default)]
    pub project_id: i32,
    #[serde(default)]
    pub permissions: Vec<Permission>,
}

/// `/health/version`.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct VersionInfo {
    #[serde(default)]
    pub version: String,
}

/// Path builders, so a route is written once and a typo is a compile error in
/// one place rather than a 404 at runtime.
pub mod paths {
    pub fn projects() -> &'static str {
        "/api/projects"
    }

    pub fn project(id: i32) -> String {
        format!("/api/projects/{id}")
    }

    pub fn issues(project: i32) -> String {
        format!("/api/projects/{project}/issues")
    }

    pub fn issue(project: i32, issue: &str) -> String {
        format!("/api/projects/{project}/issues/{issue}")
    }

    pub fn issue_events(project: i32, issue: &str) -> String {
        format!("/api/projects/{project}/issues/{issue}/events")
    }

    pub fn issue_activity(project: i32, issue: &str) -> String {
        format!("/api/projects/{project}/issues/{issue}/activity")
    }

    pub fn issue_comments(project: i32, issue: &str) -> String {
        format!("/api/projects/{project}/issues/{issue}/comments")
    }

    pub fn issue_seen(project: i32, issue: &str) -> String {
        format!("/api/projects/{project}/issues/{issue}/seen")
    }

    pub fn logs(project: i32) -> String {
        format!("/api/projects/{project}/logs")
    }

    pub fn logs_volume(project: i32) -> String {
        format!("/api/projects/{project}/logs/volume")
    }

    pub fn environments(project: i32) -> String {
        format!("/api/projects/{project}/environments")
    }

    pub fn transactions(project: i32) -> String {
        format!("/api/projects/{project}/transactions")
    }

    pub fn transaction_stats(project: i32) -> String {
        format!("/api/projects/{project}/transactions/stats")
    }

    pub fn releases(project: i32) -> String {
        format!("/api/projects/{project}/releases")
    }

    pub fn release(project: i32, version: &str) -> String {
        format!("/api/projects/{project}/releases/{version}")
    }

    pub fn monitors(project: i32) -> String {
        format!("/api/projects/{project}/monitors")
    }

    pub fn monitor(project: i32, slug: &str) -> String {
        format!("/api/projects/{project}/monitors/{slug}")
    }

    pub fn monitor_checkins(project: i32, slug: &str) -> String {
        format!("/api/projects/{project}/monitors/{slug}/checkins")
    }

    pub fn alert_rules(project: i32) -> String {
        format!("/api/projects/{project}/alert-rules")
    }

    pub fn stats_summary(project: i32) -> String {
        format!("/api/projects/{project}/stats/summary")
    }

    pub fn sessions_summary(project: i32) -> String {
        format!("/api/projects/{project}/sessions/summary")
    }

    pub fn agents_summary(project: i32) -> String {
        format!("/api/projects/{project}/agents/summary")
    }

    pub fn agents_traces(project: i32) -> String {
        format!("/api/projects/{project}/agents/traces")
    }

    pub fn agents_models(project: i32) -> String {
        format!("/api/projects/{project}/agents/models")
    }

    pub fn agents_tools(project: i32) -> String {
        format!("/api/projects/{project}/agents/tools")
    }

    pub fn event_detail(project: i32, event_id: &str) -> String {
        format!("/api/projects/{project}/events/sentry/{event_id}")
    }

    pub fn tokens() -> &'static str {
        "/api/tokens"
    }

    pub fn token(id: i32) -> String {
        format!("/api/tokens/{id}")
    }

    pub fn team() -> &'static str {
        "/api/team"
    }

    pub fn access_me() -> &'static str {
        "/api/access/me"
    }

    pub fn login() -> &'static str {
        "/auth/login"
    }

    pub fn me() -> &'static str {
        "/auth/me"
    }

    pub fn version() -> &'static str {
        "/health/version"
    }

    pub fn health() -> &'static str {
        "/health"
    }
}
