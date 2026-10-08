use crate::error::{AppError, AppResult};
use crate::pagination::IssueFilter;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Canonical issue substatus values (Sentry-compatible). This is the complete
/// seven-value set Sentry defines; the constants below mirror it in full.
pub const SUBSTATUS_ONGOING: &str = "ongoing";
pub const SUBSTATUS_ESCALATING: &str = "escalating";
pub const SUBSTATUS_REGRESSED: &str = "regressed";
pub const SUBSTATUS_NEW: &str = "new";
pub const SUBSTATUS_ARCHIVED_UNTIL_ESCALATING: &str = "archived_until_escalating";
pub const SUBSTATUS_ARCHIVED_UNTIL_CONDITION_MET: &str = "archived_until_condition_met";
pub const SUBSTATUS_ARCHIVED_FOREVER: &str = "archived_forever";

/// Canonical issue status (Sentry-compatible).
pub const STATUS_UNRESOLVED: &str = "unresolved";
pub const STATUS_RESOLVED: &str = "resolved";
pub const STATUS_IGNORED: &str = "ignored";

/// Substatus values valid under `status = "unresolved"`.
const UNRESOLVED_SUBSTATUSES: &[&str] = &[
    SUBSTATUS_NEW,
    SUBSTATUS_ONGOING,
    SUBSTATUS_ESCALATING,
    SUBSTATUS_REGRESSED,
];

/// Substatus values valid under `status = "ignored"`.
const IGNORED_SUBSTATUSES: &[&str] = &[
    SUBSTATUS_ARCHIVED_UNTIL_ESCALATING,
    SUBSTATUS_ARCHIVED_UNTIL_CONDITION_MET,
    SUBSTATUS_ARCHIVED_FOREVER,
];

/// Whether `substatus` is a legal pairing with `status` (Sentry-compatible).
/// `resolved` issues never carry a substatus — Sentry's `GroupSubStatus` is
/// only meaningful under `unresolved` (new/ongoing/escalating/regressed) and
/// `ignored` (the three `archived_*` values).
pub fn substatus_valid_for_status(status: &str, substatus: &str) -> bool {
    match status {
        STATUS_UNRESOLVED => UNRESOLVED_SUBSTATUSES.contains(&substatus),
        STATUS_IGNORED => IGNORED_SUBSTATUSES.contains(&substatus),
        _ => false,
    }
}

/// Issue model - a group of similar events
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Issue {
    pub id: Uuid,
    pub project_id: i32,
    pub digest_order: i32,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub digested_event_count: i32,
    pub stored_event_count: i32,
    pub calculated_type: String,
    pub calculated_value: String,
    pub transaction: String,
    pub last_frame_filename: String,
    pub last_frame_module: String,
    pub last_frame_function: String,
    pub level: Option<String>,
    pub platform: Option<String>,
    // Status model (replaces the old is_resolved/is_muted booleans).
    pub status: String,
    pub substatus: Option<String>,
    pub priority: Option<String>,
    pub priority_locked_at: Option<DateTime<Utc>>,
    pub culprit: String,
    pub logger: String,
    pub status_details: String,
    pub assigned_to: Option<i32>,
    pub assignee_type: Option<String>,
    pub issue_type: String,
    pub issue_category: String,
    pub first_release: String,
    pub last_release: String,
}

/// Response for API
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct IssueResponse {
    pub id: Uuid,
    pub project_id: i32,
    pub short_id: String,
    pub title: String,
    pub value: String,
    pub culprit: String,
    pub logger: String,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub event_count: i32,
    pub level: Option<String>,
    pub platform: Option<String>,
    pub status: String,
    pub substatus: Option<String>,
    pub priority: Option<String>,
    pub assigned_to: Option<i32>,
    pub assignee_type: Option<String>,
    pub issue_type: String,
    pub issue_category: String,
    pub first_release: String,
    pub last_release: String,
    pub status_details: serde_json::Value,
    /// Deprecated: derived from `status`. Kept for the legacy client/UI until
    /// they migrate to `status`. Remove in the client/frontend phases of #165.
    pub is_resolved: bool,
    /// Deprecated: derived from `status`. See `is_resolved`.
    pub is_muted: bool,
    /// Only populated by the single-issue GET endpoint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_report_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_bookmarked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_subscribed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_seen: Option<bool>,
    /// Only populated by the issue list endpoint (bulk-computed per page).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_count: Option<i64>,
    /// 24 hourly buckets, oldest to newest. Only populated by the issue list
    /// endpoint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trend: Option<Vec<i64>>,
}

/// Request to update issue state.
///
/// `status`/`substatus`/`priority`/assignment are the canonical fields.
/// `is_resolved`/`is_muted` are accepted as a deprecated compatibility shim
/// and mapped onto `status`.
#[derive(Debug, Default, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdateIssueState {
    pub status: Option<String>,
    pub substatus: Option<String>,
    pub priority: Option<String>,
    /// `None` when omitted, `Some(None)` when the client sent an explicit
    /// `null` (clears the assignment), `Some(Some(id))` to assign.
    #[serde(default, deserialize_with = "deserialize_some")]
    pub assigned_to: Option<Option<i32>>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub assignee_type: Option<Option<String>>,
    // Deprecated compatibility fields.
    pub is_resolved: Option<bool>,
    pub is_muted: Option<bool>,
}

/// Wraps a field's normal deserialization in `Some`, so a present JSON value
/// (including explicit `null`) is distinguishable from the field being
/// omitted entirely, which leaves the outer `Option` at its `#[serde(default)]`.
fn deserialize_some<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    T::deserialize(deserializer).map(Some)
}

/// Maximum number of ids accepted by a single bulk mutate/delete request.
/// Matches Sentry's own `BULK_MUTATION_LIMIT`
/// (src/sentry/api/helpers/group_index/__init__.py:12: "Bulk mutations are
/// limited to 1000 items.").
pub const MAX_BULK_IDS: usize = 1000;

/// Rejects a bulk request whose `ids` list exceeds [`MAX_BULK_IDS`].
fn validate_bulk_ids_size(ids: &[Uuid]) -> AppResult<()> {
    if ids.len() > MAX_BULK_IDS {
        return Err(AppError::Validation(format!(
            "Too many ids: {} (max {})",
            ids.len(),
            MAX_BULK_IDS
        )));
    }
    Ok(())
}

/// Bulk mutate request: apply a status (and/or priority) to many issues.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct BulkUpdateIssues {
    pub ids: Vec<Uuid>,
    pub status: Option<String>,
    pub priority: Option<String>,
}

impl BulkUpdateIssues {
    /// Rejects the request if `ids` exceeds [`MAX_BULK_IDS`].
    pub fn validate_size(&self) -> AppResult<()> {
        validate_bulk_ids_size(&self.ids)
    }
}

/// Bulk delete request: the listed `ids`, or every issue matching `filter`.
/// A body with neither or both matches no variant and is rejected, so an empty
/// body can never mean "all".
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(untagged, deny_unknown_fields)]
pub enum BulkDeleteIssues {
    #[cfg_attr(feature = "openapi", schema(max_properties = 1))]
    Ids { ids: Vec<Uuid> },
    #[cfg_attr(feature = "openapi", schema(max_properties = 1))]
    Filter { filter: IssueFilter },
}

impl BulkDeleteIssues {
    /// Rejects an `ids` request larger than [`MAX_BULK_IDS`].
    pub fn validate_size(&self) -> AppResult<()> {
        match self {
            Self::Ids { ids } => validate_bulk_ids_size(ids),
            Self::Filter { .. } => Ok(()),
        }
    }
}

impl UpdateIssueState {
    /// Resolves the requested status, mapping the deprecated booleans when the
    /// canonical `status` field is absent. `is_resolved` wins over `is_muted`,
    /// matching the historical PATCH semantics.
    ///
    /// A `status` that is present but not one of the recognized literals is a
    /// client error, not a no-op — callers must reject the request rather
    /// than silently leaving the issue unchanged.
    ///
    /// `current_status` is the issue's status *before* this request is
    /// applied. It's consulted for both deprecated `false` shims:
    ///
    /// - `is_resolved: false` — the old boolean model's `unresolve` only
    ///   ever cleared the `is_resolved` column and never touched `is_muted`,
    ///   so unresolving an already-ignored (or already-unresolved) issue was
    ///   a no-op that left it ignored. Mapping it onto the new unified
    ///   `status` unconditionally would un-ignore issues that a client
    ///   "defensively" unresolves — so this arm only fires when the issue is
    ///   currently `resolved`.
    /// - `is_muted: false` — the old boolean model's `unmute` only ever
    ///   cleared the `is_muted` column, leaving `is_resolved` untouched, so
    ///   unmuting an already-resolved (or already-unresolved) issue was a
    ///   no-op. Mapping it onto the new unified `status` unconditionally
    ///   would reopen resolved issues that a client "defensively" unmutes —
    ///   so this arm only fires when the issue is currently `ignored`.
    pub fn resolved_status(&self, current_status: &str) -> AppResult<Option<&'static str>> {
        if let Some(status) = self.status.as_deref() {
            return match status {
                STATUS_RESOLVED => Ok(Some(STATUS_RESOLVED)),
                STATUS_IGNORED => Ok(Some(STATUS_IGNORED)),
                STATUS_UNRESOLVED => Ok(Some(STATUS_UNRESOLVED)),
                other => Err(AppError::Validation(format!("Invalid status: {}", other))),
            };
        }
        Ok(match (self.is_resolved, self.is_muted) {
            (Some(true), _) => Some(STATUS_RESOLVED),
            (Some(false), _) if current_status == STATUS_RESOLVED => Some(STATUS_UNRESOLVED),
            (Some(false), _) => None,
            (None, Some(true)) => Some(STATUS_IGNORED),
            (None, Some(false)) if current_status == STATUS_IGNORED => Some(STATUS_UNRESOLVED),
            (None, Some(false)) => None,
            (None, None) => None,
        })
    }

    /// Validates the requested substatus, if present, against the canonical
    /// set of allowed values (Sentry-compatible).
    pub fn validated_substatus(&self) -> AppResult<Option<&str>> {
        match self.substatus.as_deref() {
            None => Ok(None),
            Some(
                s @ (SUBSTATUS_ONGOING
                | SUBSTATUS_ESCALATING
                | SUBSTATUS_REGRESSED
                | SUBSTATUS_NEW
                | SUBSTATUS_ARCHIVED_UNTIL_ESCALATING
                | SUBSTATUS_ARCHIVED_UNTIL_CONDITION_MET
                | SUBSTATUS_ARCHIVED_FOREVER),
            ) => Ok(Some(s)),
            Some(other) => Err(AppError::Validation(format!(
                "Invalid substatus: {}",
                other
            ))),
        }
    }
}

impl Issue {
    /// Whether the issue is resolved (derived from status).
    pub fn is_resolved(&self) -> bool {
        self.status == STATUS_RESOLVED
    }

    /// Whether the issue is muted/ignored (derived from status).
    pub fn is_muted(&self) -> bool {
        self.status == STATUS_IGNORED
    }

    /// Generates the issue title from type and value
    pub fn title(&self) -> String {
        if self.calculated_value.is_empty() {
            self.calculated_type.clone()
        } else {
            match self
                .calculated_value
                .lines()
                .map(|l| l.trim())
                .find(|l| !l.is_empty())
            {
                Some(first_line) => format!("{}: {}", self.calculated_type, first_line),
                None => self.calculated_type.clone(),
            }
        }
    }

    /// Generates the short_id (e.g., "PROJECT-1")
    pub fn short_id(&self, project_slug: &str) -> String {
        format!("{}-{}", project_slug.to_uppercase(), self.digest_order)
    }

    /// Converts to API response format
    pub fn to_response(&self, project_slug: &str) -> IssueResponse {
        let status_details = serde_json::from_str(&self.status_details)
            .unwrap_or(serde_json::Value::Object(Default::default()));

        IssueResponse {
            id: self.id,
            project_id: self.project_id,
            short_id: self.short_id(project_slug),
            title: self.title(),
            value: self.calculated_value.trim().to_string(),
            culprit: self.culprit.clone(),
            logger: self.logger.clone(),
            first_seen: self.first_seen,
            last_seen: self.last_seen,
            event_count: self.digested_event_count,
            level: self.level.clone(),
            platform: self.platform.clone(),
            status: self.status.clone(),
            substatus: self.substatus.clone(),
            priority: self.priority.clone(),
            assigned_to: self.assigned_to,
            assignee_type: self.assignee_type.clone(),
            issue_type: self.issue_type.clone(),
            issue_category: self.issue_category.clone(),
            first_release: self.first_release.clone(),
            last_release: self.last_release.clone(),
            status_details,
            is_resolved: self.is_resolved(),
            is_muted: self.is_muted(),
            user_report_count: None,
            is_bookmarked: None,
            is_subscribed: None,
            has_seen: None,
            user_count: None,
            trend: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bulk_delete_rejects_ids_over_max() {
        let ids = (0..=MAX_BULK_IDS).map(|_| Uuid::new_v4()).collect();
        let body = BulkDeleteIssues::Ids { ids };
        assert!(body.validate_size().is_err());
    }

    #[test]
    fn test_bulk_delete_needs_exactly_one_of_ids_or_filter() {
        let parse = |body| serde_json::from_value::<BulkDeleteIssues>(body);
        assert!(parse(serde_json::json!({})).is_err());
        assert!(parse(serde_json::json!({ "ids": [], "filter": "all" })).is_err());
        assert!(matches!(
            parse(serde_json::json!({ "filter": "resolved" })),
            Ok(BulkDeleteIssues::Filter {
                filter: IssueFilter::Resolved
            })
        ));
    }

    #[cfg(feature = "openapi")]
    #[test]
    fn test_bulk_delete_schema_limits_each_selector_to_one_property() {
        use utoipa::PartialSchema;

        let schema = serde_json::to_value(BulkDeleteIssues::schema()).unwrap();
        let branches = schema["oneOf"].as_array().unwrap();
        assert_eq!(branches.len(), 2);
        for branch in branches {
            // With one required selector, a second property (including an invalid
            // other selector) must never validate against either branch.
            assert_eq!(branch["required"].as_array().unwrap().len(), 1);
            assert_eq!(branch["maxProperties"], 1);
        }
    }

    #[test]
    fn test_bulk_update_accepts_1000_ids_matching_sentry_limit() {
        // Real Sentry's BULK_MUTATION_LIMIT is 1000
        // (src/sentry/api/helpers/group_index/__init__.py:12).
        let ids = (0..1000).map(|_| Uuid::new_v4()).collect();
        let body = BulkUpdateIssues {
            ids,
            status: None,
            priority: None,
        };
        assert!(body.validate_size().is_ok());
    }

    #[test]
    fn test_bulk_update_rejects_1001_ids() {
        let ids = (0..1001).map(|_| Uuid::new_v4()).collect();
        let body = BulkUpdateIssues {
            ids,
            status: None,
            priority: None,
        };
        assert!(body.validate_size().is_err());
    }

    #[test]
    fn test_substatus_valid_for_status_unresolved_pairings() {
        for s in [
            SUBSTATUS_NEW,
            SUBSTATUS_ONGOING,
            SUBSTATUS_ESCALATING,
            SUBSTATUS_REGRESSED,
        ] {
            assert!(
                substatus_valid_for_status(STATUS_UNRESOLVED, s),
                "{} should be valid under unresolved",
                s
            );
        }
    }

    #[test]
    fn test_substatus_valid_for_status_ignored_pairings() {
        for s in [
            SUBSTATUS_ARCHIVED_UNTIL_ESCALATING,
            SUBSTATUS_ARCHIVED_UNTIL_CONDITION_MET,
            SUBSTATUS_ARCHIVED_FOREVER,
        ] {
            assert!(
                substatus_valid_for_status(STATUS_IGNORED, s),
                "{} should be valid under ignored",
                s
            );
        }
    }

    #[test]
    fn test_substatus_invalid_for_resolved() {
        // `resolved` issues never carry a substatus in real Sentry.
        for s in [
            SUBSTATUS_NEW,
            SUBSTATUS_ONGOING,
            SUBSTATUS_ESCALATING,
            SUBSTATUS_REGRESSED,
            SUBSTATUS_ARCHIVED_UNTIL_ESCALATING,
            SUBSTATUS_ARCHIVED_UNTIL_CONDITION_MET,
            SUBSTATUS_ARCHIVED_FOREVER,
        ] {
            assert!(
                !substatus_valid_for_status(STATUS_RESOLVED, s),
                "{} should NOT be valid under resolved",
                s
            );
        }
    }

    #[test]
    fn test_substatus_cross_pairings_rejected() {
        // Unresolved-only substatuses must not be accepted under `ignored`.
        for s in [
            SUBSTATUS_NEW,
            SUBSTATUS_ONGOING,
            SUBSTATUS_ESCALATING,
            SUBSTATUS_REGRESSED,
        ] {
            assert!(!substatus_valid_for_status(STATUS_IGNORED, s));
        }
        // Ignored-only substatuses must not be accepted under `unresolved`.
        for s in [
            SUBSTATUS_ARCHIVED_UNTIL_ESCALATING,
            SUBSTATUS_ARCHIVED_UNTIL_CONDITION_MET,
            SUBSTATUS_ARCHIVED_FOREVER,
        ] {
            assert!(!substatus_valid_for_status(STATUS_UNRESOLVED, s));
        }
    }

    #[test]
    fn test_resolve_shim_is_noop_on_ignored_issue() {
        // Old boolean model: unresolve() did `SET is_resolved = FALSE`
        // unconditionally and never touched is_muted. On an already-ignored
        // issue (is_resolved already FALSE, is_muted = TRUE) that was a true
        // no-op — the issue stayed ignored. A defensive {"is_resolved":
        // false} PATCH must not reopen it.
        let body = UpdateIssueState {
            is_resolved: Some(false),
            ..Default::default()
        };
        assert_eq!(
            body.resolved_status(STATUS_IGNORED).unwrap(),
            None,
            "is_resolved:false must be a no-op on an ignored issue"
        );
    }

    #[test]
    fn test_resolve_shim_transitions_resolved_issue_to_unresolved() {
        let body = UpdateIssueState {
            is_resolved: Some(false),
            ..Default::default()
        };
        assert_eq!(
            body.resolved_status(STATUS_RESOLVED).unwrap(),
            Some(STATUS_UNRESOLVED)
        );
    }

    #[test]
    fn test_resolve_shim_is_noop_on_already_unresolved_issue() {
        let body = UpdateIssueState {
            is_resolved: Some(false),
            ..Default::default()
        };
        assert_eq!(body.resolved_status(STATUS_UNRESOLVED).unwrap(), None);
    }
}
