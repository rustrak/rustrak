use actix_web::{web, HttpResponse};
use uuid::Uuid;

use crate::auth::ApiActor;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::rbac::{Action, Resource};
#[cfg(feature = "openapi")]
use crate::models::IssueResponse;
use crate::models::{BulkDeleteIssues, BulkUpdateIssues, UpdateIssueState};
use crate::pagination::{
    IssueCategory, IssueFilter, IssueSort, OffsetPaginatedResponse, SortOrder,
};
use crate::services::github;
use crate::services::issue::RESOLVED_IN_NEXT_RELEASE;
use crate::services::scope::Scope;
use crate::services::{EventService, IssueService, IssueSocialService, ProjectService};
use serde::Deserialize;
use serde_json::json;

/// Body for creating an issue comment (note).
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CommentRequest {
    #[schemars(description = "Comment body")]
    pub text: String,
}

/// Body for toggling a per-user flag (bookmark/subscription).
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ToggleRequest {
    #[serde(default)]
    pub enabled: Option<bool>,
}

/// Body for submitting user feedback on an issue.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UserReportRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub comments: Option<String>,
    #[serde(default)]
    pub event_id: Option<Uuid>,
}

#[cfg(feature = "openapi")]
use utoipa::OpenApi;

/// Query parameters for listing issues (offset-based)
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct ListIssuesQuery {
    /// Page number (1-indexed, default: 1)
    #[serde(default = "crate::pagination::default_page")]
    #[cfg_attr(feature = "openapi", param(minimum = 1))]
    pub page: i64,

    /// Items per page (default: 20, max: 100)
    #[serde(default = "crate::pagination::default_per_page")]
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

    /// Issue category: error (default) or feedback
    #[serde(default)]
    pub category: IssueCategory,

    /// Free-text search across type, value, transaction, and culprit.
    #[serde(default)]
    pub q: Option<String>,

    /// Only issues seen in this environment, with that environment's first
    /// and last seen, event count, users and trend.
    #[serde(default)]
    pub environment: Option<String>,

    /// Only issues that had an event in this release.
    #[serde(default)]
    pub release: Option<String>,

    /// Only these levels, comma-separated: `fatal,error`.
    #[serde(default)]
    pub level: Option<String>,

    /// Only these priorities, comma-separated: `high,medium`.
    #[serde(default)]
    pub priority: Option<String>,

    /// `none` for unassigned issues, or a user id.
    #[serde(default)]
    pub assigned: Option<String>,

    /// Only issues first seen within this window: `24h`, `7d`.
    #[serde(default)]
    pub first_seen: Option<String>,

    /// Only issues last seen within this window: `24h`, `7d`.
    #[serde(default)]
    pub last_seen: Option<String>,

    /// Only issues with at least this many events.
    #[serde(default)]
    pub events_min: Option<i64>,

    /// Only issues with at most this many events.
    #[serde(default)]
    pub events_max: Option<i64>,
}

impl ListIssuesQuery {
    /// The table's narrowing filters, read out of the query string. A value
    /// that does not parse narrows nothing rather than failing the list.
    pub fn issue_filters(&self) -> crate::services::issue::IssueFilters {
        let list = |raw: &Option<String>| -> Vec<String> {
            raw.as_deref()
                .unwrap_or("")
                .split(',')
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_string)
                .collect()
        };
        let period = |raw: &Option<String>| {
            raw.as_deref()
                .and_then(|p| crate::routes::period::parse_period_hours(Some(p)))
        };
        crate::services::issue::IssueFilters {
            levels: list(&self.level),
            priorities: list(&self.priority),
            assigned: match self.assigned.as_deref() {
                Some("none") => Some(crate::services::issue::Assignee::Nobody),
                Some(id) => id.parse().ok().map(crate::services::issue::Assignee::User),
                None => None,
            },
            first_seen_hours: period(&self.first_seen),
            last_seen_hours: period(&self.last_seen),
            events_min: self.events_min,
            events_max: self.events_max,
        }
    }
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/issues",
    operation_id = "issues.list",
    tag = "Issues",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ListIssuesQuery,
    ),
    responses(
        (status = 200, description = "List of issues", body = inline(crate::pagination::OffsetPaginatedResponse<IssueResponse>)),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Project not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Lists issues for a project with offset-based pagination
pub async fn list_issues(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<ListIssuesQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();

    actor.require(Resource::Issues, Action::Read, Some(project_id))?;

    // Verify project exists and get slug for response
    let project = ProjectService::get_by_id(pool.get_ref(), project_id).await?;

    // Execute paginated query with offset
    let (issues, total_count) = IssueService::list_offset(
        pool.get_ref(),
        project_id,
        query.sort,
        query.order,
        query.filter,
        query.category,
        query.page,
        query.per_page,
        query.q.as_deref(),
        Scope {
            environment: query.environment.as_deref(),
            release: query.release.as_deref(),
        },
        &query.issue_filters(),
    )
    .await?;

    // Bulk-compute per-issue user_count/trend for this page in one request
    // (avoids the list UI firing one aggregates/stats call per visible row).
    let issue_ids: Vec<Uuid> = issues.iter().map(|i| i.id).collect();
    let list_stats = IssueService::list_stats(
        pool.get_ref(),
        &issue_ids,
        Scope {
            environment: query.environment.as_deref(),
            release: query.release.as_deref(),
        },
    )
    .await?;

    // Build responses
    let responses: Vec<_> = issues
        .iter()
        .map(|i| {
            let mut response = i.to_response(&project.slug);
            if let Some(stats) = list_stats.get(&i.id) {
                response.user_count = Some(stats.user_count);
                response.trend = Some(stats.trend.clone());
            }
            response
        })
        .collect();

    Ok(HttpResponse::Ok().json(OffsetPaginatedResponse::new(
        responses,
        total_count,
        query.page,
        query.per_page,
    )))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/environments",
    operation_id = "environments.list",
    tag = "Issues",
    params(("project_id" = i32, Path, description = "Project ID")),
    responses(
        (status = 200, description = "Environments the project's issues have been seen in", body = Vec<String>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Project not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Lists the environments a project's issues have been seen in, by name.
pub async fn list_environments(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();
    actor.require(Resource::Issues, Action::Read, Some(project_id))?;
    ProjectService::get_by_id(pool.get_ref(), project_id).await?;
    let environments = IssueService::environments(pool.get_ref(), project_id).await?;
    Ok(HttpResponse::Ok().json(environments))
}

/// Query of the feedback list.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct ListFeedbackQuery {
    /// Page number (1-indexed, default: 1)
    #[serde(default = "crate::pagination::default_page")]
    pub page: i64,
    /// Items per page (default: 25, max: 100)
    #[serde(default = "default_feedback_per_page")]
    pub per_page: i64,
    /// open (the inbox, default), resolved, muted (spam) or all
    #[serde(default)]
    pub filter: crate::pagination::IssueFilter,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct ListUserReportsQuery {
    /// Page number (1-indexed, default: 1)
    #[serde(default = "crate::pagination::default_page")]
    pub page: i64,
    /// Items per page (default: 25, max: 100)
    #[serde(default = "default_feedback_per_page")]
    pub per_page: i64,
}

fn default_feedback_per_page() -> i64 {
    25
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/feedback",
    operation_id = "feedback.list",
    tag = "Issues",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ListFeedbackQuery,
    ),
    responses(
        (status = 200, description = "User feedback, newest first", body = inline(crate::pagination::OffsetPaginatedResponse<crate::models::FeedbackResponse>)),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Project not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Lists user feedback with what each user submitted. Every feedback is an
/// issue of category `feedback`; spam is a muted one.
pub async fn list_feedback(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<ListFeedbackQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();
    actor.require(Resource::Issues, Action::Read, Some(project_id))?;
    ProjectService::get_by_id(pool.get_ref(), project_id).await?;

    let (issues, total_count) = IssueService::list_offset(
        pool.get_ref(),
        project_id,
        crate::pagination::IssueSort::LastSeen,
        crate::pagination::SortOrder::Desc,
        query.filter,
        crate::pagination::IssueCategory::Feedback,
        query.page,
        query.per_page,
        None,
        Scope::default(),
        &crate::services::issue::IssueFilters::default(),
    )
    .await?;
    let ids: Vec<Uuid> = issues.iter().map(|i| i.id).collect();
    let contexts = IssueService::feedback_contexts(pool.get_ref(), &ids).await?;
    let mut attachments =
        crate::services::AttachmentService::list_for_issues(pool.get_ref(), &ids).await?;
    let items: Vec<_> = issues
        .iter()
        .map(|i| {
            crate::models::FeedbackResponse::new(
                i,
                contexts.get(&i.id),
                attachments.remove(&i.id).unwrap_or_default(),
            )
        })
        .collect();

    Ok(HttpResponse::Ok().json(OffsetPaginatedResponse::new(
        items,
        total_count,
        query.page,
        query.per_page,
    )))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/issues/{issue_id}",
    operation_id = "issues.get",
    tag = "Issues",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("issue_id" = uuid::Uuid, Path, description = "Issue ID"),
    ),
    responses(
        (status = 200, description = "Issue details", body = IssueResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Gets a single issue by ID
pub async fn get_issue(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid)>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, issue_id) = path.into_inner();

    actor.require(Resource::Issues, Action::Read, Some(project_id))?;

    // Verify project exists and get slug
    let project = ProjectService::get_by_id(pool.get_ref(), project_id).await?;

    let issue = IssueService::get_in_project(pool.get_ref(), project_id, issue_id).await?;

    // Enrich with per-user and aggregate fields.
    let mut response = issue.to_response(&project.slug);
    response.user_report_count =
        Some(IssueSocialService::user_report_count(pool.get_ref(), issue_id).await?);
    if let Some(user_id) = actor.user_id() {
        response.is_bookmarked =
            Some(IssueSocialService::is_bookmarked(pool.get_ref(), issue_id, user_id).await?);
        response.is_subscribed =
            Some(IssueSocialService::is_subscribed(pool.get_ref(), issue_id, user_id).await?);
        response.has_seen =
            Some(IssueSocialService::has_seen(pool.get_ref(), issue_id, user_id).await?);
    }

    Ok(HttpResponse::Ok().json(response))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    patch,
    path = "/api/projects/{project_id}/issues/{issue_id}",
    operation_id = "issues.update",
    tag = "Issues",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("issue_id" = uuid::Uuid, Path, description = "Issue ID"),
    ),
    request_body = UpdateIssueState,
    responses(
        (status = 200, description = "Issue updated", body = IssueResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Updates issue state (resolve, mute, etc.)
pub async fn update_issue(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid)>,
    body: web::Json<UpdateIssueState>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, issue_id) = path.into_inner();

    actor.require(Resource::Issues, Action::Update, Some(project_id))?;

    // Verify project exists and get slug
    let project = ProjectService::get_by_id(pool.get_ref(), project_id).await?;

    let issue = IssueService::get_in_project(pool.get_ref(), project_id, issue_id).await?;

    // Resolving in the next release suppresses regressions until a new
    // deploy is recorded.
    let mut updated = issue;
    let status = match body.status.as_deref() {
        Some(RESOLVED_IN_NEXT_RELEASE) => Some((RESOLVED_IN_NEXT_RELEASE, None)),
        _ => match body.resolved_status()? {
            Some(status) => Some((status, body.validated_substatus()?)),
            None => None,
        },
    };
    if let Some((status, substatus)) = status {
        updated = IssueService::change_status(
            pool.get_ref(),
            issue_id,
            status,
            substatus,
            actor.user_id(),
        )
        .await?;
    }
    if let Some(priority) = body.priority.as_deref() {
        updated = IssueService::set_priority(pool.get_ref(), issue_id, priority).await?;
    }
    if body.assigned_to.is_some() || body.assignee_type.is_some() {
        updated = IssueService::assign(
            pool.get_ref(),
            issue_id,
            body.assigned_to.flatten(),
            body.assignee_type.as_ref().and_then(|t| t.as_deref()),
        )
        .await?;
        github::sync::spawn(
            "assignee sync",
            github::sync::mirror_assignee(pool.get_ref().clone(), issue_id, None),
        );
    }

    Ok(HttpResponse::Ok().json(updated.to_response(&project.slug)))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/api/projects/{project_id}/issues/{issue_id}",
    operation_id = "issues.delete",
    tag = "Issues",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("issue_id" = uuid::Uuid, Path, description = "Issue ID"),
    ),
    responses(
        (status = 204, description = "Issue deleted"),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Hard-deletes an issue and all associated events
pub async fn delete_issue(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid)>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, issue_id) = path.into_inner();

    actor.require(Resource::Issues, Action::Delete, Some(project_id))?;

    IssueService::get_in_project(pool.get_ref(), project_id, issue_id).await?;

    IssueService::delete(pool.get_ref(), issue_id).await?;

    Ok(HttpResponse::NoContent().finish())
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/issues/{issue_id}/hashes",
    operation_id = "issues.getHashes",
    tag = "Issues",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("issue_id" = uuid::Uuid, Path, description = "Issue ID"),
    ),
    responses(
        (status = 200, description = "Grouping hashes mapped to the issue", body = Vec<crate::models::grouping::Grouping>),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Lists the grouping hashes that map to an issue.
pub async fn get_issue_hashes(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid)>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, issue_id) = path.into_inner();
    actor.require(Resource::Issues, Action::Read, Some(project_id))?;

    IssueService::get_in_project(pool.get_ref(), project_id, issue_id).await?;

    let hashes = IssueService::list_hashes(pool.get_ref(), issue_id).await?;
    Ok(HttpResponse::Ok().json(hashes))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/issues/{issue_id}/tags/{key}",
    operation_id = "issues.getTagValues",
    tag = "Issues",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("issue_id" = uuid::Uuid, Path, description = "Issue ID"),
        ("key" = String, Path, description = "Tag key"),
    ),
    responses(
        (status = 200, description = "Distinct values for the tag key, one entry per value (Sentry-compatible: a bare list, not wrapped)", body = Vec<crate::services::issue::IssueTagValue>),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Lists the distinct values (with counts) for a tag key across the issue.
pub async fn get_issue_tag_values(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid, String)>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, issue_id, key) = path.into_inner();
    actor.require(Resource::Issues, Action::Read, Some(project_id))?;

    IssueService::get_in_project(pool.get_ref(), project_id, issue_id).await?;

    let values = IssueService::tag_values(pool.get_ref(), issue_id, &key).await?;
    Ok(HttpResponse::Ok().json(values))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/issues/{issue_id}/aggregates",
    operation_id = "issues.getAggregates",
    tag = "Issues",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("issue_id" = uuid::Uuid, Path, description = "Issue ID"),
    ),
    responses(
        (status = 200, description = "Unique user count and top tags", body = crate::services::issue::IssueAggregates),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Returns the unique user count and top tags for the issue.
pub async fn get_issue_aggregates(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid)>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, issue_id) = path.into_inner();
    actor.require(Resource::Issues, Action::Read, Some(project_id))?;

    IssueService::get_in_project(pool.get_ref(), project_id, issue_id).await?;

    let aggregates = IssueService::aggregates(pool.get_ref(), issue_id).await?;
    Ok(HttpResponse::Ok().json(aggregates))
}

/// Sets the status or priority of many issues at once, by id.
#[cfg_attr(feature = "openapi", utoipa::path(
    put,
    path = "/api/projects/{project_id}/issues",
    operation_id = "issues.bulkUpdate",
    tag = "Issues",
    params(("project_id" = i32, Path, description = "Project ID")),
    request_body = crate::models::BulkUpdateIssues,
    responses(
        (status = 200, description = "Number of issues updated", body = resp::BulkUpdateResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
pub async fn bulk_update_issues(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    body: AppResult<web::Json<BulkUpdateIssues>>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();
    actor.require(Resource::Issues, Action::Update, Some(project_id))?;
    let body = body?;
    body.validate_size()?;

    let updated = IssueService::bulk_update(
        pool.get_ref(),
        project_id,
        &body.ids,
        body.status.as_deref(),
        body.priority.as_deref(),
        actor.user_id(),
    )
    .await?;

    Ok(HttpResponse::Ok().json(json!({ "updated": updated })))
}

/// Deletes many issues at once, selected by id or by filter.
#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/api/projects/{project_id}/issues",
    operation_id = "issues.bulkDelete",
    tag = "Issues",
    params(("project_id" = i32, Path, description = "Project ID")),
    request_body = crate::models::BulkDeleteIssues,
    responses(
        (status = 200, description = "Number of issues deleted", body = resp::BulkDeleteResponse),
        (status = 400, description = "Neither or both of ids and filter given", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
pub async fn bulk_delete_issues(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    body: Result<web::Json<BulkDeleteIssues>, actix_web::Error>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let body = body.map_err(|error| {
        if error.as_response_error().status_code() == actix_web::http::StatusCode::PAYLOAD_TOO_LARGE {
            AppError::PayloadTooLarge("Bulk delete request is too large".to_string())
        } else {
            AppError::Validation(
                "Specify exactly one of ids or filter (open, resolved, muted, all), with no extra fields"
                    .to_string(),
            )
        }
    });
    delete_issues(pool, path.into_inner(), body, actor).await
}

/// [`bulk_delete_issues`] past the extractor, whose `actix_web::Error` is not
/// `Send`: what the MCP tool calls.
pub async fn delete_issues(
    pool: web::Data<DbPool>,
    project_id: i32,
    body: AppResult<web::Json<BulkDeleteIssues>>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    actor.require(Resource::Issues, Action::Delete, Some(project_id))?;

    let body = body?;
    body.validate_size()?;

    let deleted = match &*body {
        BulkDeleteIssues::Ids { ids } => {
            IssueService::bulk_delete(pool.get_ref(), project_id, ids).await?
        }
        BulkDeleteIssues::Filter { filter } => {
            IssueService::delete_by_filter(pool.get_ref(), project_id, *filter).await?
        }
    };
    Ok(HttpResponse::Ok().json(json!({ "deleted": deleted })))
}

fn require_user(actor: &ApiActor) -> AppResult<i32> {
    actor
        .user_id()
        .ok_or_else(|| AppError::Validation("This action requires a user session".to_string()))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/issues/{issue_id}/stats",
    operation_id = "issues.getStats",
    tag = "Issues",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("issue_id" = uuid::Uuid, Path, description = "Issue ID"),
        ("window" = Option<crate::models::issue::IssueStatsWindow>, Query, description = "Time window (default: 24h)"),
    ),
    responses(
        (status = 200, description = "Zero-filled event-count timeseries", body = resp::IssueStatsResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Zero-filled event-count timeseries.
pub async fn get_issue_stats(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid)>,
    query: web::Query<std::collections::HashMap<String, String>>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, issue_id) = path.into_inner();
    actor.require(Resource::Issues, Action::Read, Some(project_id))?;
    IssueService::get_in_project(pool.get_ref(), project_id, issue_id).await?;

    // window=24h → 24 hourly buckets; window=30d → 30 daily buckets.
    let (bucket_secs, buckets) = match query.get("window").map(String::as_str) {
        Some("30d") => (86_400, 30),
        _ => (3_600, 24),
    };
    let series = IssueService::stats(pool.get_ref(), issue_id, bucket_secs, buckets).await?;
    let points: Vec<_> = series
        .into_iter()
        .map(|(ts, count)| json!([ts, count]))
        .collect();
    Ok(HttpResponse::Ok().json(json!({ "data": points })))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/issues/{issue_id}/activity",
    operation_id = "issues.getActivity",
    tag = "Issues",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("issue_id" = uuid::Uuid, Path, description = "Issue ID"),
    ),
    responses(
        (status = 200, description = "Activity log (status changes, comments, …)", body = Vec<crate::services::issue_social::ActivityEntry>),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Chronological activity log (incl. comments).
pub async fn get_issue_activity(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid)>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, issue_id) = path.into_inner();
    actor.require(Resource::Issues, Action::Read, Some(project_id))?;
    IssueService::get_in_project(pool.get_ref(), project_id, issue_id).await?;
    let activity = IssueSocialService::list_activity(pool.get_ref(), issue_id).await?;
    Ok(HttpResponse::Ok().json(activity))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/projects/{project_id}/issues/{issue_id}/comments",
    operation_id = "issues.createComment",
    tag = "Issues",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("issue_id" = uuid::Uuid, Path, description = "Issue ID"),
    ),
    request_body = CommentRequest,
    responses(
        (status = 201, description = "Comment added", body = crate::services::issue_social::ActivityEntry),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Add a note.
pub async fn create_issue_comment(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid)>,
    body: AppResult<web::Json<CommentRequest>>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, issue_id) = path.into_inner();
    actor.require(Resource::Issues, Action::Update, Some(project_id))?;
    let body = body?;
    IssueService::get_in_project(pool.get_ref(), project_id, issue_id).await?;
    let entry =
        IssueSocialService::add_comment(pool.get_ref(), issue_id, actor.user_id(), &body.text)
            .await?;
    github::sync::spawn(
        "comment sync",
        github::sync::mirror_comment(
            pool.get_ref().clone(),
            issue_id,
            actor.user_id(),
            body.text.clone(),
        ),
    );
    Ok(HttpResponse::Created().json(entry))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    put,
    path = "/api/projects/{project_id}/issues/{issue_id}/bookmark",
    operation_id = "issues.setBookmark",
    tag = "Issues",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("issue_id" = uuid::Uuid, Path, description = "Issue ID"),
    ),
    request_body = ToggleRequest,
    responses(
        (status = 200, description = "Updated bookmark state", body = resp::BookmarkResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Set/clear the per-user bookmark.
pub async fn set_issue_bookmark(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid)>,
    body: web::Json<ToggleRequest>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, issue_id) = path.into_inner();
    actor.require(Resource::Issues, Action::Read, Some(project_id))?;
    IssueService::get_in_project(pool.get_ref(), project_id, issue_id).await?;
    let user_id = require_user(&actor)?;
    let enabled = body.enabled.unwrap_or(true);
    IssueSocialService::set_bookmark(pool.get_ref(), issue_id, user_id, enabled).await?;
    Ok(HttpResponse::Ok().json(json!({ "is_bookmarked": enabled })))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    put,
    path = "/api/projects/{project_id}/issues/{issue_id}/subscription",
    operation_id = "issues.setSubscription",
    tag = "Issues",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("issue_id" = uuid::Uuid, Path, description = "Issue ID"),
    ),
    request_body = ToggleRequest,
    responses(
        (status = 200, description = "Updated subscription state", body = resp::SubscriptionResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Set/clear the per-user subscription.
pub async fn set_issue_subscription(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid)>,
    body: web::Json<ToggleRequest>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, issue_id) = path.into_inner();
    actor.require(Resource::Issues, Action::Read, Some(project_id))?;
    IssueService::get_in_project(pool.get_ref(), project_id, issue_id).await?;
    let user_id = require_user(&actor)?;
    let enabled = body.enabled.unwrap_or(true);
    IssueSocialService::set_subscription(pool.get_ref(), issue_id, user_id, enabled, "manual")
        .await?;
    Ok(HttpResponse::Ok().json(json!({ "is_subscribed": enabled })))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/projects/{project_id}/issues/{issue_id}/seen",
    operation_id = "issues.markSeen",
    tag = "Issues",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("issue_id" = uuid::Uuid, Path, description = "Issue ID"),
    ),
    responses(
        (status = 200, description = "Issue marked as seen", body = resp::SeenResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Mark the issue as seen by the current user.
pub async fn mark_issue_seen(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid)>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, issue_id) = path.into_inner();
    actor.require(Resource::Issues, Action::Read, Some(project_id))?;
    IssueService::get_in_project(pool.get_ref(), project_id, issue_id).await?;
    let user_id = require_user(&actor)?;
    IssueSocialService::mark_seen(pool.get_ref(), issue_id, user_id).await?;
    Ok(HttpResponse::Ok().json(json!({ "has_seen": true })))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/issues/{issue_id}/user-reports",
    operation_id = "issues.listUserReports",
    tag = "Issues",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("issue_id" = uuid::Uuid, Path, description = "Issue ID"),
        ListUserReportsQuery,
    ),
    responses(
        (status = 200, description = "User feedback reports for the issue, newest first", body = inline(crate::pagination::OffsetPaginatedResponse<crate::services::issue_social::UserReport>)),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// List user feedback for an issue.
pub async fn list_issue_user_reports(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid)>,
    query: web::Query<ListUserReportsQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, issue_id) = path.into_inner();
    actor.require(Resource::Issues, Action::Read, Some(project_id))?;
    IssueService::get_in_project(pool.get_ref(), project_id, issue_id).await?;
    let page = query.page.max(1);
    let per_page = crate::pagination::per_page(query.per_page)?;
    let reports =
        IssueSocialService::list_user_reports_page(pool.get_ref(), issue_id, page, per_page)
            .await?;
    let total_count = IssueSocialService::user_report_count(pool.get_ref(), issue_id).await?;
    Ok(HttpResponse::Ok().json(OffsetPaginatedResponse::new(
        reports,
        total_count,
        page,
        per_page,
    )))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/projects/{project_id}/issues/{issue_id}/user-reports",
    operation_id = "issues.createUserReport",
    tag = "Issues",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("issue_id" = uuid::Uuid, Path, description = "Issue ID"),
    ),
    request_body = UserReportRequest,
    responses(
        (status = 201, description = "User feedback attached", body = crate::services::issue_social::UserReport),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Attach user feedback to an issue.
pub async fn create_issue_user_report(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid)>,
    body: web::Json<UserReportRequest>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, issue_id) = path.into_inner();
    actor.require(Resource::Issues, Action::Update, Some(project_id))?;
    IssueService::get_in_project(pool.get_ref(), project_id, issue_id).await?;

    // If an event_id is supplied, verify it actually belongs to this issue so
    // the report can't be linked to an event from another issue/project.
    if let Some(event_id) = body.event_id {
        let event = EventService::get_by_id(pool.get_ref(), event_id).await?;
        if event.issue_id != Some(issue_id) {
            return Err(AppError::Validation(format!(
                "Event {} does not belong to issue {}",
                event_id, issue_id
            )));
        }
    }

    let report = IssueSocialService::create_user_report(
        pool.get_ref(),
        project_id,
        Some(issue_id),
        body.event_id,
        body.name.as_deref().unwrap_or_default(),
        body.email.as_deref().unwrap_or_default(),
        body.comments.as_deref().unwrap_or_default(),
    )
    .await?;
    Ok(HttpResponse::Created().json(report))
}

/// OpenAPI-only response schemas for endpoints that return ad-hoc JSON objects.
#[cfg(feature = "openapi")]
pub mod resp {
    use utoipa::ToSchema;

    #[derive(ToSchema)]
    pub struct IssueStatsResponse {
        /// Each point is `[bucketStartUnix, count]`.
        pub data: Vec<Vec<i64>>,
    }
    #[derive(ToSchema)]
    pub struct BulkUpdateResponse {
        pub updated: i64,
    }
    #[derive(ToSchema)]
    pub struct BulkDeleteResponse {
        pub deleted: i64,
    }
    #[derive(ToSchema)]
    pub struct BookmarkResponse {
        pub is_bookmarked: bool,
    }
    #[derive(ToSchema)]
    pub struct SubscriptionResponse {
        pub is_subscribed: bool,
    }
    #[derive(ToSchema)]
    pub struct SeenResponse {
        pub has_seen: bool,
    }
}

#[cfg(feature = "openapi")]
#[derive(OpenApi)]
#[openapi(
    paths(
        list_issues,
        get_issue,
        update_issue,
        delete_issue,
        get_issue_hashes,
        get_issue_tag_values,
        get_issue_aggregates,
        get_issue_stats,
        get_issue_activity,
        create_issue_comment,
        set_issue_bookmark,
        set_issue_subscription,
        mark_issue_seen,
        list_issue_user_reports,
        create_issue_user_report,
        bulk_update_issues,
        bulk_delete_issues,
    ),
    components(schemas(
        crate::models::IssueResponse,
        crate::models::UpdateIssueState,
        crate::models::BulkUpdateIssues,
        crate::models::BulkDeleteIssues,
        crate::models::grouping::Grouping,
        crate::services::issue::IssueAggregates,
        crate::services::issue::TagSummary,
        crate::services::issue::TagValueCount,
        crate::services::issue::IssueTagValue,
        crate::services::issue_social::ActivityEntry,
        crate::services::issue_social::UserReport,
        CommentRequest,
        ToggleRequest,
        UserReportRequest,
        resp::IssueStatsResponse,
        resp::BulkUpdateResponse,
        resp::BulkDeleteResponse,
        resp::BookmarkResponse,
        resp::SubscriptionResponse,
        resp::SeenResponse,
    ))
)]
pub struct IssuesApi;

pub fn configure_project(cfg: &mut web::ServiceConfig) {
    cfg.route("/environments", web::get().to(list_environments))
        .route("/feedback", web::get().to(list_feedback))
        .route("/issues", web::get().to(list_issues))
        .route("/issues", web::put().to(bulk_update_issues))
        .route("/issues", web::delete().to(bulk_delete_issues))
        .route("/issues/{issue_id}/hashes", web::get().to(get_issue_hashes))
        .route(
            "/issues/{issue_id}/aggregates",
            web::get().to(get_issue_aggregates),
        )
        .route(
            "/issues/{issue_id}/tags/{key}",
            web::get().to(get_issue_tag_values),
        )
        .route("/issues/{issue_id}/stats", web::get().to(get_issue_stats))
        .route(
            "/issues/{issue_id}/activity",
            web::get().to(get_issue_activity),
        )
        .route(
            "/issues/{issue_id}/comments",
            web::post().to(create_issue_comment),
        )
        .route(
            "/issues/{issue_id}/bookmark",
            web::put().to(set_issue_bookmark),
        )
        .route(
            "/issues/{issue_id}/subscription",
            web::put().to(set_issue_subscription),
        )
        .route("/issues/{issue_id}/seen", web::post().to(mark_issue_seen))
        .route(
            "/issues/{issue_id}/user-reports",
            web::get().to(list_issue_user_reports),
        )
        .route(
            "/issues/{issue_id}/user-reports",
            web::post().to(create_issue_user_report),
        )
        .route("/issues/{issue_id}", web::get().to(get_issue))
        .route("/issues/{issue_id}", web::patch().to(update_issue))
        .route("/issues/{issue_id}", web::put().to(update_issue))
        .route("/issues/{issue_id}", web::delete().to(delete_issue));
}
