pub mod cursor;

pub use cursor::{EventCursor, IssueCursor, TransactionCursor};

use serde::{Deserialize, Serialize};

/// Default page size for pagination
pub const PAGE_SIZE: i64 = 20;

/// Paginated response wrapper (cursor-based)
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(bound = "T: utoipa::ToSchema"))]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

impl<T> PaginatedResponse<T> {
    pub fn new(items: Vec<T>, next_cursor: Option<String>, has_more: bool) -> Self {
        Self {
            items,
            next_cursor,
            has_more,
        }
    }
}

/// Offset-based paginated response wrapper
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(bound = "T: utoipa::ToSchema"))]
pub struct OffsetPaginatedResponse<T> {
    pub items: Vec<T>,
    pub total_count: i64,
    pub page: i64,
    pub per_page: i64,
    pub total_pages: i64,
}

impl<T> OffsetPaginatedResponse<T> {
    pub fn new(items: Vec<T>, total_count: i64, page: i64, per_page: i64) -> Self {
        let per_page = per_page.max(1);
        let total_pages = (total_count + per_page - 1) / per_page;
        Self {
            items,
            total_count,
            page,
            per_page,
            total_pages,
        }
    }
}

/// Sort mode for issues listing
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum IssueSort {
    /// Sort by digest_order (stable, unique per project)
    #[default]
    DigestOrder,
    /// Sort by last_seen (activity-based, may reorder)
    LastSeen,
    /// Sort by digested_event_count (most/least frequent issues)
    EventCount,
}

impl IssueSort {
    pub fn as_str(&self) -> &'static str {
        match self {
            IssueSort::DigestOrder => "digest_order",
            IssueSort::LastSeen => "last_seen",
            IssueSort::EventCount => "event_count",
        }
    }
}

impl std::fmt::Display for IssueSort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Sort order direction
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    Asc,
    #[default]
    Desc,
}

impl SortOrder {
    pub fn as_str(&self) -> &'static str {
        match self {
            SortOrder::Asc => "asc",
            SortOrder::Desc => "desc",
        }
    }

    #[allow(dead_code)] // Utility method for future use
    pub fn is_desc(&self) -> bool {
        matches!(self, SortOrder::Desc)
    }
}

impl std::fmt::Display for SortOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Query parameters for listing issues (offset-based)
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct ListIssuesQuery {
    /// Page number (1-indexed, default: 1)
    #[serde(default = "default_page")]
    #[cfg_attr(feature = "openapi", param(minimum = 1))]
    pub page: i64,

    /// Items per page (default: 20, max: 100)
    #[serde(default = "default_per_page")]
    #[cfg_attr(feature = "openapi", param(minimum = 1, maximum = 100))]
    pub per_page: i64,

    /// Sort mode (default: last_seen)
    #[serde(default)]
    pub sort: IssueSort,

    /// Sort order direction (default: desc)
    #[serde(default)]
    pub order: SortOrder,

    /// Filter: open (not resolved, not muted), resolved, muted, all
    #[serde(default)]
    pub filter: IssueFilter,

    /// Free-text search across type, value, transaction, and culprit.
    #[serde(default)]
    pub q: Option<String>,
    /// Match issues containing at least one event in this environment.
    pub environment: Option<String>,
}

fn default_page() -> i64 {
    1
}

fn default_per_page() -> i64 {
    PAGE_SIZE
}

/// Filter for issues listing
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum IssueFilter {
    /// Only open issues (not resolved and not muted)
    #[default]
    Open,
    /// Only resolved issues
    Resolved,
    /// Only muted issues
    Muted,
    /// All issues
    All,
}

/// Query parameters for listing transactions (offset-based)
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct ListTransactionsQuery {
    /// Page number (1-indexed, default: 1)
    #[serde(default = "default_page")]
    #[cfg_attr(feature = "openapi", param(minimum = 1))]
    pub page: i64,

    /// Items per page (default: 20, max: 100)
    #[serde(default = "default_per_page")]
    #[cfg_attr(feature = "openapi", param(minimum = 1, maximum = 100))]
    pub per_page: i64,

    /// Filter by exact transaction name (lists one group's samples).
    pub name: Option<String>,

    /// Filter by trace operation (contexts.trace.op), e.g. `http.server`.
    pub op: Option<String>,

    /// Filter by trace status (contexts.trace.status), e.g. `ok`.
    pub status: Option<String>,

    /// Filter by environment.
    pub environment: Option<String>,

    /// Filter by release.
    pub release: Option<String>,
}

/// Query parameters for listing logs (offset-based)
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct ListLogsQuery {
    /// Page number (1-indexed, default: 1)
    #[serde(default = "default_page")]
    #[cfg_attr(feature = "openapi", param(minimum = 1))]
    pub page: i64,

    /// Items per page (default: 20, max: 100)
    #[serde(default = "default_per_page")]
    #[cfg_attr(feature = "openapi", param(minimum = 1, maximum = 100))]
    pub per_page: i64,

    /// Filter by log level (trace/debug/info/warn/error/fatal).
    pub level: Option<String>,

    /// Filter by trace id.
    pub trace_id: Option<String>,
    pub environment: Option<String>,
}

/// Query parameters for listing spans (offset-based)
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct ListSpansQuery {
    /// Page number (1-indexed, default: 1)
    #[serde(default = "default_page")]
    #[cfg_attr(feature = "openapi", param(minimum = 1))]
    pub page: i64,

    /// Items per page (default: 20, max: 100)
    #[serde(default = "default_per_page")]
    #[cfg_attr(feature = "openapi", param(minimum = 1, maximum = 100))]
    pub per_page: i64,

    /// Filter by span operation, e.g. `http.client`.
    pub op: Option<String>,

    /// Filter by span status.
    pub status: Option<String>,

    /// Filter by trace id — matches spans regardless of origin (standalone
    /// or extracted from a transaction), since both share this table.
    pub trace_id: Option<String>,

    /// Filter by gen_ai.operation.type
    /// (`agent`/`tool`/`handoff`/`ai_client`/`other`).
    pub operation_type: Option<String>,
}

/// Query parameters for AI Agent Monitoring time-series widgets (Agent Runs,
/// Estimated Cost, Duration).
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct AgentTimeseriesQuery {
    /// Lookback window in hours (default: all time, no filter).
    pub period_hours: Option<i64>,

    /// Restrict to spans reporting this environment (default: all).
    pub environment: Option<String>,

    /// Bucket width in hours (default: 1).
    #[serde(default = "default_interval_hours")]
    #[cfg_attr(feature = "openapi", param(minimum = 1))]
    pub interval_hours: i64,
}

fn default_interval_hours() -> i64 {
    1
}

/// Query parameters for AI Agent Monitoring breakdown widgets (LLM Calls by
/// Model, Tokens Used by Model, Tool Calls by Tool).
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct AgentBreakdownQuery {
    /// Lookback window in hours (default: all time, no filter).
    pub period_hours: Option<i64>,

    /// Restrict to spans reporting this environment (default: all).
    pub environment: Option<String>,

    /// Max rows returned (default: 3, matching Sentry's own widget cap).
    #[serde(default = "default_breakdown_limit")]
    #[cfg_attr(feature = "openapi", param(minimum = 1, maximum = 100))]
    pub limit: i64,
}

fn default_breakdown_limit() -> i64 {
    3
}

/// Query parameters for the agents-dashboard aggregates that return a whole
/// table rather than a top-N slice: summary, models, tools/stats.
///
/// Deliberately not `AgentBreakdownQuery`: that one carries `limit`, which
/// these endpoints have no way to honour, and reusing it published a
/// documented parameter they silently ignored.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct AgentWindowQuery {
    /// Lookback window in hours (default: all time, no filter).
    pub period_hours: Option<i64>,

    /// Restrict to spans reporting this environment (default: all).
    pub environment: Option<String>,
}

/// Query parameters for the AI Agent Monitoring Traces table (offset-based).
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct AgentTracesQuery {
    /// Lookback window in hours (default: all time, no filter). The dashboard
    /// applies one window to every widget, and this table is one of them.
    pub period_hours: Option<i64>,

    /// Restrict to spans reporting this environment (default: all).
    pub environment: Option<String>,

    /// Page number (1-indexed, default: 1)
    #[serde(default = "default_page")]
    #[cfg_attr(feature = "openapi", param(minimum = 1))]
    pub page: i64,

    /// Items per page (default: 20, max: 100)
    #[serde(default = "default_per_page")]
    #[cfg_attr(feature = "openapi", param(minimum = 1, maximum = 100))]
    pub per_page: i64,
}

/// Query parameters for the transaction stats overview (offset-based)
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct TransactionStatsQuery {
    /// Page number (1-indexed, default: 1)
    #[serde(default = "default_page")]
    #[cfg_attr(feature = "openapi", param(minimum = 1))]
    pub page: i64,

    /// Items per page (default: 20, max: 100)
    #[serde(default = "default_per_page")]
    #[cfg_attr(feature = "openapi", param(minimum = 1, maximum = 100))]
    pub per_page: i64,
    pub environment: Option<String>,
}

/// Query parameters for a single transaction group's aggregate stats.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct TransactionStatGroupQuery {
    /// Exact transaction name of the group.
    pub name: String,
    /// Trace operation of the group (omit for groups with no op).
    pub op: Option<String>,
    pub environment: Option<String>,
}

/// Query parameters for listing events
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct ListEventsQuery {
    /// Sort order direction (default: desc = newest first)
    #[serde(default)]
    pub order: SortOrder,

    /// Pagination cursor
    pub cursor: Option<String>,
    pub environment: Option<String>,
}

/// Query parameters for listing projects (offset-based)
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct ListProjectsQuery {
    /// Page number (1-indexed, default: 1)
    #[serde(default = "default_page")]
    #[cfg_attr(feature = "openapi", param(minimum = 1))]
    pub page: i64,

    /// Items per page (default: 20, max: 100)
    #[serde(default = "default_per_page")]
    #[cfg_attr(feature = "openapi", param(minimum = 1, maximum = 100))]
    pub per_page: i64,

    /// Sort order direction (default: desc = newest first)
    #[serde(default)]
    pub order: SortOrder,

    /// Window for the per-project stats on each row (e.g. `24h`, `7d`).
    ///
    /// Omitted means the caller does not want stats at all, and the extra
    /// aggregate queries are skipped entirely — this is what keeps the plain
    /// `GET /api/projects` that SDK tooling and the project picker use as
    /// cheap as it was before the list table grew a sparkline. Sentry's own
    /// projects endpoint gates its `stats` field on `?statsPeriod=` the same
    /// way.
    #[serde(default)]
    pub stats_period: Option<String>,
}
