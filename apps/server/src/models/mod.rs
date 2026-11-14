use serde::de::{Deserializer, Error as _, SeqAccess, Visitor};
use std::{fmt, marker::PhantomData};

/// Bound allocations from nested log/span containers inside one envelope item.
pub(crate) const MAX_CONTAINER_ITEMS: usize = 1024;

pub(crate) fn deserialize_bounded_items<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    struct BoundedItems<T>(PhantomData<T>);

    impl<'de, T> Visitor<'de> for BoundedItems<T>
    where
        T: serde::Deserialize<'de>,
    {
        type Value = Vec<T>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a bounded sequence")
        }

        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut items = Vec::new();
            while let Some(item) = sequence.next_element()? {
                if items.len() >= MAX_CONTAINER_ITEMS {
                    return Err(A::Error::custom(format!(
                        "container contains more than {} items",
                        MAX_CONTAINER_ITEMS
                    )));
                }
                items.push(item);
            }
            Ok(items)
        }
    }

    deserializer.deserialize_seq(BoundedItems(PhantomData))
}

/// A Relay `Timestamp` field: epoch seconds or RFC 3339. An unparseable value
/// is `None`, as Relay annotates it away instead of failing the whole item, and
/// so is one at or before 1970, far past what Relay's `eap::time::normalize`
/// lets through for logs and spans (relay-event-normalization/src/eap/time.rs).
pub(crate) fn deserialize_timestamp<'de, D>(
    deserializer: D,
) -> Result<Option<chrono::DateTime<chrono::Utc>>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = <Option<serde_json::Value> as serde::Deserialize>::deserialize(deserializer)?;
    Ok(raw
        .as_ref()
        .and_then(crate::protocol::timestamp_from_value)
        .filter(|dt| *dt > chrono::DateTime::UNIX_EPOCH))
}

pub mod alert;
pub mod auth_token;
pub mod event;
pub mod gen_ai;
pub mod github;
pub mod grouping;
pub mod installation;
pub mod invitation;
pub mod issue;
pub mod log;
pub mod monitor;
pub mod project;
pub mod rbac;
pub mod release;
pub mod role;
pub mod saved_view;
pub mod schedule;
pub mod session;
pub mod source_file;
pub mod span_v2;
pub mod stats;
pub mod storage;
pub mod token;
pub mod transaction;
pub mod user;

pub use alert::{
    AlertHistory, AlertHistoryDay, AlertIntegration, AlertPayload, AlertRule, AlertRuleChannel,
    AlertRuleChannelInput, AlertRuleResponse, AlertStatus, AlertType, CreateAlertIntegration,
    CreateAlertRule, CustomWebhookConfig, EmailConfig, EmailRoutingOverride, IssueInfo,
    ProjectInfo, ProviderType, SlackBotTokenConfig, SlackConfig, SlackRoutingOverride,
    SlackWebhookConfig, UpdateAlertIntegration, UpdateAlertRule, WebhookConfig,
    WebhookRoutingOverride,
};
pub use auth_token::{AuthToken, AuthTokenCreatedResponse, AuthTokenResponse, CreateAuthToken};
pub use event::{AttachmentInfo, Event, EventDetailResponse, EventResponse, EventSummary};
pub use gen_ai::{
    AgentConversation, AgentConversationSummary, AgentDurationPoint, AgentModelRow, AgentSummary,
    AgentTimeseriesPoint, AgentToolRow, AgentTraceSummary, ConversationSpan, GenAiBreakdownRow,
};
pub use grouping::Grouping;
pub use installation::{Installation, QuotaWindows};
pub use invitation::{
    AcceptInvitation, CreateInvitation, Invitation, InvitationResponse, InvitationStatus,
};
pub use issue::{
    substatus_valid_for_status, BulkDeleteIssues, BulkUpdateIssues, FeedbackResponse, Issue,
    IssueResponse, UpdateIssueState, STATUS_IGNORED, STATUS_RESOLVED, STATUS_UNRESOLVED,
};
pub use log::{LogResponse, LogVolumePoint};
pub use project::{
    CreateProject, Product, Project, ProjectResponse, UpdateProject, SELECTABLE_PLATFORMS,
    VALID_PLATFORMS,
};
pub use release::{
    is_valid_version, CreateRelease, Release, ReleaseResponse, ReleaseSummary, UpdateRelease,
};
pub use role::{Permission, PersonalAccess, Role, RoleAssignment};
pub use span_v2::{parse_span_v2_container, SpanV2Entry};
pub use stats::{EventTimeseriesPoint, MetricDelta, ProjectStatsSummary};
pub use storage::{
    CleanupCounts, CleanupFilter, CleanupFrequency, CleanupKind, CleanupRequest, CleanupState,
    CleanupStatus, Compaction, KindAge, ProjectAge, ProjectRetention, ProjectStorage,
    RetentionResponse, RetentionRun, RetentionSettings, RetentionWindows, SourceMapGcResult,
    SourceMapStorage, StorageAge, StorageSummary, AGE_BUCKET_DAYS,
};
pub use transaction::{
    span_attributes, SpanDetailResponse, SpanResponse, TransactionDetailResponse,
    TransactionResponse, TransactionStatsResponse, TransactionTimeseriesPoint,
};
pub use user::{ChangePasswordRequest, CreateUserRequest, LoginRequest, User, UserRole};
