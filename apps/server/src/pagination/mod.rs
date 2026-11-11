pub mod cursor;

pub use cursor::EventCursor;

use serde::{Deserialize, Serialize};

/// The page size of a list that names no other.
pub const PAGE_SIZE: i64 = 20;

/// The largest page any list serves.
pub const MAX_PER_PAGE: i64 = 100;

/// A requested page size, refused outside 1..=[`MAX_PER_PAGE`] with Sentry's
/// own message (`clamp_pagination_per_page`, `utils/pagination_factory.py`):
/// a page silently smaller than asked for reads as the end of the list.
pub fn per_page(requested: i64) -> crate::error::AppResult<i64> {
    if (1..=MAX_PER_PAGE).contains(&requested) {
        Ok(requested)
    } else {
        Err(crate::error::AppError::Validation(format!(
            "Invalid per_page value. Must be between 1 and {MAX_PER_PAGE}."
        )))
    }
}

/// The rows before a 1-indexed `page`. `page` comes from the query string, so
/// a page below 1 is the first and one past any offset saturates: an empty
/// page rather than an overflow or a negative `OFFSET`.
pub fn offset(page: i64, per_page: i64) -> i64 {
    page.max(1)
        .saturating_sub(1)
        .saturating_mul(per_page.max(0))
}

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
        let total_pages = total_count.saturating_add(per_page - 1) / per_page;
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
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
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
    /// Sort by first_seen (newest or oldest issues)
    FirstSeen,
    /// Sort by priority: high, then medium, then low
    Priority,
}

impl IssueSort {
    pub fn as_str(&self) -> &'static str {
        match self {
            IssueSort::DigestOrder => "digest_order",
            IssueSort::LastSeen => "last_seen",
            IssueSort::EventCount => "event_count",
            IssueSort::FirstSeen => "first_seen",
            IssueSort::Priority => "priority",
        }
    }
}

impl std::fmt::Display for IssueSort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Sort order direction
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
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
}

impl std::fmt::Display for SortOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Serde default of a `page` query field.
pub fn default_page() -> i64 {
    1
}

/// Serde default of a `per_page` query field.
pub fn default_per_page() -> i64 {
    PAGE_SIZE
}

/// Filter for issues listing
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
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

/// Which kind of issue a list shows. Feedback is kept out of the default
/// list, as Sentry's `FeedbackGroup` is (`in_default_search=False`).
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum IssueCategory {
    /// Issues grouped from error events
    #[default]
    Error,
    /// User feedback, one issue per submission
    Feedback,
}

impl IssueCategory {
    /// The `issues.issue_category` value; also trusted SQL.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Feedback => "feedback",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::offset;

    #[test]
    fn offset_skips_the_pages_before() {
        assert_eq!(offset(1, 25), 0);
        assert_eq!(offset(3, 25), 50);
    }

    #[test]
    fn offset_never_goes_negative_or_overflows() {
        assert_eq!(offset(0, 25), 0);
        assert_eq!(offset(-5, 25), 0);
        assert_eq!(offset(i64::MAX, 100), i64::MAX);
    }
}
