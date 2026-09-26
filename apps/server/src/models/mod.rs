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

pub mod alert;
pub mod auth_token;
pub mod event;
pub mod gen_ai;
pub mod grouping;
pub mod installation;
pub mod invitation;
pub mod issue;
pub mod log;
pub mod project;
pub mod project_member;
pub mod release;
pub mod session;
pub mod source_file;
pub mod span_v2;
pub mod stats;
pub mod storage;
pub mod transaction;
pub mod user;

pub use alert::{
    AlertHistory,
    AlertIntegration,
    AlertPayload,
    AlertRule,
    AlertRuleChannel,
    AlertRuleChannelInput,
    AlertRuleResponse,
    AlertStatus,
    AlertType,
    // New types
    // Legacy aliases
    ChannelType,
    CreateAlertIntegration,
    CreateAlertRule,
    CreateNotificationChannel,
    CustomWebhookConfig,
    EmailConfig,
    EmailRoutingOverride,
    IssueInfo,
    NotificationChannel,
    ProjectInfo,
    ProviderType,
    SlackBotTokenConfig,
    SlackConfig,
    SlackRoutingOverride,
    SlackWebhookConfig,
    UpdateAlertIntegration,
    UpdateAlertRule,
    UpdateNotificationChannel,
    WebhookConfig,
    WebhookRoutingOverride,
};
pub use auth_token::{AuthToken, AuthTokenCreatedResponse, AuthTokenResponse, CreateAuthToken};
pub use event::{Event, EventDetailResponse, EventResponse, EventSummary};
pub use gen_ai::{
    AgentDurationPoint, AgentModelRow, AgentSummary, AgentTimeseriesPoint, AgentToolRow,
    AgentTraceSummary, GenAiBreakdownRow,
};
pub use grouping::Grouping;
pub use installation::{Installation, QuotaWindows};
pub use invitation::{
    AcceptInvitation, CreateInvitation, Invitation, InvitationResponse, InvitationStatus,
};
pub use issue::{
    substatus_valid_for_status, BulkDeleteIssues, BulkUpdateIssues, Issue, IssueResponse,
    UpdateIssueState, STATUS_IGNORED, STATUS_RESOLVED, STATUS_UNRESOLVED,
};
pub use log::LogResponse;
pub use project::{
    CreateProject, Project, ProjectResponse, UpdateProject, SELECTABLE_PLATFORMS, VALID_PLATFORMS,
};
pub use project_member::{ProjectMember, ProjectMemberResponse, ProjectRole, UpsertProjectMember};
pub use release::{is_valid_version, CreateRelease, Release, ReleaseResponse, UpdateRelease};
pub use span_v2::{parse_span_v2_container, SpanV2Entry};
pub use stats::{EventTimeseriesPoint, MetricDelta, ProjectStatsSummary};
pub use storage::{
    CleanupCounts, CleanupFilter, CleanupRequest, ProjectStorage, SourceMapGcResult,
    SourceMapStorage, StorageSummary,
};
pub use transaction::{
    span_attributes, SpanDetailResponse, SpanResponse, TransactionDetailResponse,
    TransactionResponse, TransactionStatsResponse,
};
pub use user::{CreateUserRequest, LoginRequest, User, UserRole};
