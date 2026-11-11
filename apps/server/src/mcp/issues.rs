use actix_web::web;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::schemars::{self, JsonSchema};
use rmcp::{tool, tool_router};
use serde::Deserialize;

use super::{done, reply, uuid, InProject, RustrakMcp};
use crate::error::AppResult;
use crate::models::{BulkDeleteIssues, BulkUpdateIssues, UpdateIssueState};
use crate::pagination::IssueFilter;
use crate::routes::issues;

#[derive(Deserialize, JsonSchema)]
pub struct IssueArgs {
    /// Project ID
    project_id: i32,
    /// Issue ID
    issue_id: String,
}

impl IssueArgs {
    pub(super) fn path(&self) -> AppResult<web::Path<(i32, uuid::Uuid)>> {
        Ok(web::Path::from((
            self.project_id,
            uuid("issue_id", &self.issue_id)?,
        )))
    }
}

/// An issue and the handler's own query or body for it.
#[derive(Deserialize, JsonSchema)]
pub struct OnIssue<T> {
    #[serde(flatten)]
    pub(super) issue: IssueArgs,
    #[serde(flatten)]
    pub(super) rest: T,
}

#[derive(Deserialize, JsonSchema)]
pub struct UpdateStatusArgs {
    #[serde(flatten)]
    issue: IssueArgs,
    /// New status: unresolved, resolved, ignored or resolvedInNextRelease
    status: String,
    /// low, medium or high
    priority: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct AssignArgs {
    #[serde(flatten)]
    issue: IssueArgs,
    /// User ID to assign, or null to clear
    assigned_to: Option<i32>,
    /// e.g. "user" or "team"
    assignee_type: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct BulkDeleteArgs {
    /// Project ID
    project_id: i32,
    /// Issue IDs to delete
    ids: Option<Vec<uuid::Uuid>>,
    /// Delete every issue in this state: open, resolved, muted or all
    filter: Option<IssueFilter>,
}

#[derive(Deserialize, JsonSchema)]
pub struct TagValuesArgs {
    #[serde(flatten)]
    issue: IssueArgs,
    /// Tag key, e.g. "browser"
    key: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct StatsArgs {
    #[serde(flatten)]
    issue: IssueArgs,
    /// 24h (default) or 30d
    window: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ToggleArgs {
    #[serde(flatten)]
    issue: IssueArgs,
    /// true to turn it on, false to turn it off
    enabled: bool,
}

#[tool_router(router = issue_tools, vis = "pub(super)")]
impl RustrakMcp {
    /// `PATCH` an issue with a fixed body.
    async fn update_issue(&self, a: &IssueArgs, body: UpdateIssueState) -> CallToolResult {
        reply(async {
            issues::update_issue(self.pool(), a.path()?, web::Json(body), self.actor()).await
        })
        .await
    }

    #[tool(
        description = "List issues for a Rustrak project. Returns paginated grouped error occurrences.",
        annotations(read_only_hint = true)
    )]
    async fn list_issues(
        &self,
        Parameters(a): Parameters<InProject<issues::ListIssuesQuery>>,
    ) -> CallToolResult {
        reply(async {
            issues::list_issues(
                self.pool(),
                web::Path::from(a.project_id),
                web::Query(a.rest),
                self.actor(),
            )
            .await
        })
        .await
    }

    #[tool(
        description = "List user feedback for a Rustrak project: each message with the sender's name, email, page, the error event it is about and its attachments (such as a screenshot). Resolve or mute (spam) one with the issue tools, using its id.",
        annotations(read_only_hint = true)
    )]
    async fn list_feedback(
        &self,
        Parameters(a): Parameters<InProject<issues::ListFeedbackQuery>>,
    ) -> CallToolResult {
        reply(async {
            issues::list_feedback(
                self.pool(),
                web::Path::from(a.project_id),
                web::Query(a.rest),
                self.actor(),
            )
            .await
        })
        .await
    }

    #[tool(
        description = "Get a single issue by ID.",
        annotations(read_only_hint = true)
    )]
    async fn get_issue(&self, Parameters(a): Parameters<IssueArgs>) -> CallToolResult {
        reply(async { issues::get_issue(self.pool(), a.path()?, self.actor()).await }).await
    }

    #[tool(
        description = "Mark an issue as resolved.",
        annotations(destructive_hint = false)
    )]
    async fn resolve_issue(&self, Parameters(a): Parameters<IssueArgs>) -> CallToolResult {
        self.update_issue(&a, status("resolved")).await
    }

    #[tool(
        description = "Mark an issue as unresolved (re-open it).",
        annotations(destructive_hint = false)
    )]
    async fn unresolve_issue(&self, Parameters(a): Parameters<IssueArgs>) -> CallToolResult {
        self.update_issue(&a, status("unresolved")).await
    }

    #[tool(
        description = "Mute an issue so it no longer triggers alerts.",
        annotations(destructive_hint = false)
    )]
    async fn mute_issue(&self, Parameters(a): Parameters<IssueArgs>) -> CallToolResult {
        self.update_issue(&a, status("ignored")).await
    }

    #[tool(
        description = "Permanently delete an issue and all its events.",
        annotations(destructive_hint = true)
    )]
    async fn delete_issue(&self, Parameters(a): Parameters<IssueArgs>) -> CallToolResult {
        done(
            async { issues::delete_issue(self.pool(), a.path()?, self.actor()).await },
            format!("Issue {} deleted successfully.", a.issue_id),
        )
        .await
    }

    #[tool(
        description = "Set an issue status (unresolved, resolved, ignored, or resolvedInNextRelease) and optionally its priority.",
        annotations(destructive_hint = false)
    )]
    async fn update_issue_status(
        &self,
        Parameters(a): Parameters<UpdateStatusArgs>,
    ) -> CallToolResult {
        let body = UpdateIssueState {
            status: Some(a.status),
            priority: a.priority,
            ..Default::default()
        };
        self.update_issue(&a.issue, body).await
    }

    #[tool(
        description = "Assign an issue to a user (or clear the assignment with assigned_to=null).",
        annotations(destructive_hint = false)
    )]
    async fn assign_issue(&self, Parameters(a): Parameters<AssignArgs>) -> CallToolResult {
        // An absent assignee reaches the handler as `null`, which clears it.
        let body = UpdateIssueState {
            assigned_to: Some(a.assigned_to),
            assignee_type: Some(a.assignee_type),
            ..Default::default()
        };
        self.update_issue(&a.issue, body).await
    }

    #[tool(
        description = "Set the status and/or priority of multiple issues at once.",
        annotations(destructive_hint = false)
    )]
    async fn bulk_update_issues(
        &self,
        Parameters(a): Parameters<InProject<BulkUpdateIssues>>,
    ) -> CallToolResult {
        reply(async {
            issues::bulk_update_issues(
                self.pool(),
                web::Path::from(a.project_id),
                Ok(web::Json(a.rest)),
                self.actor(),
            )
            .await
        })
        .await
    }

    #[tool(
        description = "Permanently delete issues and all their events. Specify exactly one of ids or filter; filter=all deletes every issue in the project.",
        annotations(destructive_hint = true)
    )]
    async fn bulk_delete_issues(
        &self,
        Parameters(a): Parameters<BulkDeleteArgs>,
    ) -> CallToolResult {
        let body = match (a.ids, a.filter) {
            (Some(ids), None) => BulkDeleteIssues::Ids { ids },
            (None, Some(filter)) => BulkDeleteIssues::Filter { filter },
            _ => return super::refuse("Specify exactly one of ids or filter."),
        };
        reply(async {
            issues::delete_issues(self.pool(), a.project_id, Ok(web::Json(body)), self.actor())
                .await
        })
        .await
    }

    #[tool(
        description = "List the grouping hashes that map to an issue.",
        annotations(read_only_hint = true)
    )]
    async fn get_issue_hashes(&self, Parameters(a): Parameters<IssueArgs>) -> CallToolResult {
        reply(async { issues::get_issue_hashes(self.pool(), a.path()?, self.actor()).await }).await
    }

    #[tool(
        description = "List the distinct values (with counts) for a tag key across an issue.",
        annotations(read_only_hint = true)
    )]
    async fn get_issue_tag_values(
        &self,
        Parameters(a): Parameters<TagValuesArgs>,
    ) -> CallToolResult {
        reply(async {
            let (project_id, issue_id) = a.issue.path()?.into_inner();
            issues::get_issue_tag_values(
                self.pool(),
                web::Path::from((project_id, issue_id, a.key.clone())),
                self.actor(),
            )
            .await
        })
        .await
    }

    #[tool(
        description = "Get per-issue aggregates: unique affected user count and top tags.",
        annotations(read_only_hint = true)
    )]
    async fn get_issue_aggregates(&self, Parameters(a): Parameters<IssueArgs>) -> CallToolResult {
        reply(async { issues::get_issue_aggregates(self.pool(), a.path()?, self.actor()).await })
            .await
    }

    #[tool(
        description = "Get an event-count timeseries for an issue (24h or 30d).",
        annotations(read_only_hint = true)
    )]
    async fn get_issue_stats(&self, Parameters(a): Parameters<StatsArgs>) -> CallToolResult {
        let query = [(
            "window".to_string(),
            a.window.clone().unwrap_or("24h".into()),
        )];
        reply(async {
            issues::get_issue_stats(
                self.pool(),
                a.issue.path()?,
                web::Query(query.into_iter().collect()),
                self.actor(),
            )
            .await
        })
        .await
    }

    #[tool(
        description = "List an issue's activity log (status changes, comments/notes, …).",
        annotations(read_only_hint = true)
    )]
    async fn get_issue_activity(&self, Parameters(a): Parameters<IssueArgs>) -> CallToolResult {
        reply(async { issues::get_issue_activity(self.pool(), a.path()?, self.actor()).await })
            .await
    }

    #[tool(
        description = "Add a comment (note) to an issue.",
        annotations(destructive_hint = false)
    )]
    async fn comment_on_issue(
        &self,
        Parameters(a): Parameters<OnIssue<issues::CommentRequest>>,
    ) -> CallToolResult {
        reply(async {
            issues::create_issue_comment(
                self.pool(),
                a.issue.path()?,
                Ok(web::Json(a.rest)),
                self.actor(),
            )
            .await
        })
        .await
    }

    #[tool(
        description = "Set or clear your bookmark on an issue.",
        annotations(destructive_hint = false)
    )]
    async fn bookmark_issue(&self, Parameters(a): Parameters<ToggleArgs>) -> CallToolResult {
        reply(async {
            issues::set_issue_bookmark(
                self.pool(),
                a.issue.path()?,
                web::Json(issues::ToggleRequest {
                    enabled: Some(a.enabled),
                }),
                self.actor(),
            )
            .await
        })
        .await
    }

    #[tool(
        description = "Subscribe to or unsubscribe from an issue.",
        annotations(destructive_hint = false)
    )]
    async fn subscribe_issue(&self, Parameters(a): Parameters<ToggleArgs>) -> CallToolResult {
        reply(async {
            issues::set_issue_subscription(
                self.pool(),
                a.issue.path()?,
                web::Json(issues::ToggleRequest {
                    enabled: Some(a.enabled),
                }),
                self.actor(),
            )
            .await
        })
        .await
    }

    #[tool(
        description = "Mark an issue as seen by you.",
        annotations(destructive_hint = false)
    )]
    async fn mark_issue_seen(&self, Parameters(a): Parameters<IssueArgs>) -> CallToolResult {
        reply(async { issues::mark_issue_seen(self.pool(), a.path()?, self.actor()).await }).await
    }

    #[tool(
        description = "List user feedback reports attached to an issue.",
        annotations(read_only_hint = true)
    )]
    async fn list_user_reports(
        &self,
        Parameters(a): Parameters<OnIssue<issues::ListUserReportsQuery>>,
    ) -> CallToolResult {
        reply(async {
            issues::list_issue_user_reports(
                self.pool(),
                a.issue.path()?,
                web::Query(a.rest),
                self.actor(),
            )
            .await
        })
        .await
    }

    #[tool(
        description = "Attach a user feedback report to an issue.",
        annotations(destructive_hint = false)
    )]
    async fn submit_user_report(
        &self,
        Parameters(a): Parameters<OnIssue<issues::UserReportRequest>>,
    ) -> CallToolResult {
        reply(async {
            issues::create_issue_user_report(
                self.pool(),
                a.issue.path()?,
                web::Json(a.rest),
                self.actor(),
            )
            .await
        })
        .await
    }
}

fn status(status: &str) -> UpdateIssueState {
    UpdateIssueState {
        status: Some(status.into()),
        ..Default::default()
    }
}
