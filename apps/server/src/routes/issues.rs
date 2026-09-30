use actix_web::{web, HttpResponse};
use uuid::Uuid;

use crate::auth::ApiActor;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
#[cfg(feature = "openapi")]
use crate::models::IssueResponse;
use crate::models::{BulkDeleteIssues, BulkUpdateIssues, UpdateIssueState};
use crate::pagination::{ListIssuesQuery, OffsetPaginatedResponse};
use crate::services::access::{self, Action};
use crate::services::{EventService, IssueService, IssueSocialService, ProjectService};
use serde::Deserialize;
use serde_json::json;

/// Body for creating an issue comment (note).
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CommentRequest {
    pub text: String,
}

/// Body for toggling a per-user flag (bookmark/subscription).
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ToggleRequest {
    #[serde(default)]
    pub enabled: Option<bool>,
}

/// Body for submitting user feedback on an issue.
#[derive(Debug, Deserialize)]
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

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/issues",
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
/// GET /api/projects/{project_id}/issues
/// Lists issues for a project with offset-based pagination
pub async fn list_issues(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<ListIssuesQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();

    access::require(
        pool.get_ref(),
        actor.is_admin(),
        actor.user_id(),
        project_id,
        Action::ViewProject,
    )
    .await?;

    // Verify project exists and get slug for response
    let project = ProjectService::get_by_id(pool.get_ref(), project_id).await?;

    // Execute paginated query with offset
    let (issues, total_count) = IssueService::list_offset(
        pool.get_ref(),
        project_id,
        query.sort,
        query.order,
        query.filter,
        query.page,
        query.per_page,
        query.q.as_deref(),
        query.environment.as_deref().filter(|s| !s.is_empty()),
    )
    .await?;

    // Bulk-compute per-issue user_count/trend for this page in one request
    // (avoids the list UI firing one aggregates/stats call per visible row).
    let issue_ids: Vec<Uuid> = issues.iter().map(|i| i.id).collect();
    let list_stats = IssueService::list_stats(
        pool.get_ref(),
        &issue_ids,
        query.environment.as_deref().filter(|s| !s.is_empty()),
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
                if let Some(count) = stats.event_count {
                    response.event_count = count as i32;
                }
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
    path = "/api/projects/{project_id}/issues/{issue_id}",
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
/// GET /api/projects/{project_id}/issues/{issue_id}
/// Gets a single issue by ID
pub async fn get_issue(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid)>,
    query: web::Query<std::collections::HashMap<String, String>>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, issue_id) = path.into_inner();

    access::require(
        pool.get_ref(),
        actor.is_admin(),
        actor.user_id(),
        project_id,
        Action::ViewProject,
    )
    .await?;

    // Verify project exists and get slug
    let project = ProjectService::get_by_id(pool.get_ref(), project_id).await?;

    // Get issue and verify it belongs to the project
    let issue = IssueService::get_by_id(pool.get_ref(), issue_id).await?;

    if issue.project_id != project_id {
        return Err(AppError::NotFound(format!("Issue {} not found", issue_id)));
    }

    // Enrich with per-user and aggregate fields.
    let mut response = issue.to_response(&project.slug);
    if let Some(environment) = query.get("environment").filter(|s| !s.is_empty()) {
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM events WHERE issue_id = $1 AND environment = $2")
                .bind(issue_id)
                .bind(environment)
                .fetch_one(pool.get_ref())
                .await?;
        response.event_count = count.0 as i32;
    }
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
/// PATCH /api/projects/{project_id}/issues/{issue_id}
/// Updates issue state (resolve, mute, etc.)
pub async fn update_issue(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid)>,
    body: web::Json<UpdateIssueState>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, issue_id) = path.into_inner();

    access::require(
        pool.get_ref(),
        actor.is_admin(),
        actor.user_id(),
        project_id,
        Action::MutateIssue,
    )
    .await?;

    // Verify project exists and get slug
    let project = ProjectService::get_by_id(pool.get_ref(), project_id).await?;

    // Verify issue belongs to the project
    let issue = IssueService::get_by_id(pool.get_ref(), issue_id).await?;
    if issue.project_id != project_id {
        return Err(AppError::NotFound(format!("Issue {} not found", issue_id)));
    }

    // Apply state changes. `status` (canonical) and is_resolved/is_muted
    // (deprecated) are unified via `resolved_status()`.
    let mut updated = issue;
    // Special case: resolve in the next release (suppresses regression until a
    // new deploy is recorded).
    if body.status.as_deref() == Some("resolvedInNextRelease") {
        updated = IssueService::resolve_in_next_release(pool.get_ref(), issue_id).await?;
        IssueSocialService::record_status_change(
            pool.get_ref(),
            issue_id,
            actor.user_id(),
            "resolvedInNextRelease",
        )
        .await?;
    } else if let Some(status) = body.resolved_status(&updated.status)? {
        let substatus = body.validated_substatus()?;
        updated = IssueService::set_status(pool.get_ref(), issue_id, status, substatus).await?;
        IssueSocialService::record_status_change(pool.get_ref(), issue_id, actor.user_id(), status)
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
    }

    Ok(HttpResponse::Ok().json(updated.to_response(&project.slug)))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/api/projects/{project_id}/issues/{issue_id}",
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
/// DELETE /api/projects/{project_id}/issues/{issue_id}
/// Hard-deletes an issue and all associated events
pub async fn delete_issue(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid)>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, issue_id) = path.into_inner();

    access::require(
        pool.get_ref(),
        actor.is_admin(),
        actor.user_id(),
        project_id,
        Action::MutateIssue,
    )
    .await?;

    // Verify issue belongs to the project before deleting
    let issue = IssueService::get_by_id(pool.get_ref(), issue_id).await?;
    if issue.project_id != project_id {
        return Err(AppError::NotFound(format!("Issue {} not found", issue_id)));
    }

    IssueService::delete(pool.get_ref(), issue_id).await?;

    Ok(HttpResponse::NoContent().finish())
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/issues/{issue_id}/hashes",
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
/// GET /api/projects/{project_id}/issues/{issue_id}/hashes
/// Lists the grouping hashes that map to an issue.
pub async fn get_issue_hashes(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid)>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, issue_id) = path.into_inner();
    access::require(
        pool.get_ref(),
        actor.is_admin(),
        actor.user_id(),
        project_id,
        Action::ViewProject,
    )
    .await?;

    let issue = IssueService::get_by_id(pool.get_ref(), issue_id).await?;
    if issue.project_id != project_id {
        return Err(AppError::NotFound(format!("Issue {} not found", issue_id)));
    }

    let hashes = IssueService::list_hashes(pool.get_ref(), issue_id).await?;
    Ok(HttpResponse::Ok().json(hashes))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/issues/{issue_id}/tags/{key}",
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
/// GET /api/projects/{project_id}/issues/{issue_id}/tags/{key}
/// Lists the distinct values (with counts) for a tag key across the issue.
pub async fn get_issue_tag_values(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid, String)>,
    query: web::Query<std::collections::HashMap<String, String>>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, issue_id, key) = path.into_inner();
    access::require(
        pool.get_ref(),
        actor.is_admin(),
        actor.user_id(),
        project_id,
        Action::ViewProject,
    )
    .await?;

    let issue = IssueService::get_by_id(pool.get_ref(), issue_id).await?;
    if issue.project_id != project_id {
        return Err(AppError::NotFound(format!("Issue {} not found", issue_id)));
    }

    let values = IssueService::tag_values(
        pool.get_ref(),
        issue_id,
        &key,
        query
            .get("environment")
            .map(String::as_str)
            .filter(|s| !s.is_empty()),
    )
    .await?;
    Ok(HttpResponse::Ok().json(values))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/issues/{issue_id}/aggregates",
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
/// GET /api/projects/{project_id}/issues/{issue_id}/aggregates
/// Returns the unique user count and top tags for the issue.
pub async fn get_issue_aggregates(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid)>,
    query: web::Query<std::collections::HashMap<String, String>>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, issue_id) = path.into_inner();
    access::require(
        pool.get_ref(),
        actor.is_admin(),
        actor.user_id(),
        project_id,
        Action::ViewProject,
    )
    .await?;

    let issue = IssueService::get_by_id(pool.get_ref(), issue_id).await?;
    if issue.project_id != project_id {
        return Err(AppError::NotFound(format!("Issue {} not found", issue_id)));
    }

    let aggregates = IssueService::aggregates(
        pool.get_ref(),
        issue_id,
        query
            .get("environment")
            .map(String::as_str)
            .filter(|s| !s.is_empty()),
    )
    .await?;
    Ok(HttpResponse::Ok().json(aggregates))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    put,
    path = "/api/projects/{project_id}/issues",
    tag = "Issues",
    params(("project_id" = i32, Path, description = "Project ID")),
    request_body = crate::models::BulkUpdateIssues,
    responses(
        (status = 200, description = "Number of issues updated", body = resp::BulkUpdateResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// PUT /api/projects/{project_id}/issues  (bulk mutate)
pub async fn bulk_update_issues(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    body: web::Json<BulkUpdateIssues>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();
    access::require(
        pool.get_ref(),
        actor.is_admin(),
        actor.user_id(),
        project_id,
        Action::MutateIssue,
    )
    .await?;
    body.validate_size()?;

    // Status and priority are applied in one transaction so a request that sets
    // both never leaves issues with the new status but the old priority (or
    // vice versa) if one of the two updates fails partway through.
    // Write-first (UPDATE opens the tx) — deferred BEGIN deliberate, see db::begin_write.
    let mut tx = pool.get_ref().begin().await?;

    let mut updated = 0u64;
    if let Some(status) = body.status.as_deref() {
        updated = if status == "resolvedInNextRelease" {
            IssueService::bulk_resolve_in_next_release(&mut tx, project_id, &body.ids).await?
        } else {
            IssueService::bulk_set_status(&mut tx, project_id, &body.ids, status).await?
        };
    }
    if let Some(priority) = body.priority.as_deref() {
        let priority_updated =
            IssueService::bulk_set_priority(&mut tx, project_id, &body.ids, priority).await?;
        if body.status.is_none() {
            updated = priority_updated;
        }
    }

    tx.commit().await?;

    Ok(HttpResponse::Ok().json(json!({ "updated": updated })))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/api/projects/{project_id}/issues",
    tag = "Issues",
    params(("project_id" = i32, Path, description = "Project ID")),
    request_body = crate::models::BulkDeleteIssues,
    responses(
        (status = 200, description = "Number of issues deleted", body = resp::BulkDeleteResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// DELETE /api/projects/{project_id}/issues  (bulk delete)
pub async fn bulk_delete_issues(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    body: web::Json<BulkDeleteIssues>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();
    access::require(
        pool.get_ref(),
        actor.is_admin(),
        actor.user_id(),
        project_id,
        Action::MutateIssue,
    )
    .await?;
    body.validate_size()?;

    let deleted = IssueService::bulk_delete(pool.get_ref(), project_id, &body.ids).await?;
    Ok(HttpResponse::Ok().json(json!({ "deleted": deleted })))
}

/// Loads an issue and verifies it belongs to the project (helper for sub-routes).
async fn require_issue_in_project(pool: &DbPool, project_id: i32, issue_id: Uuid) -> AppResult<()> {
    let issue = IssueService::get_by_id(pool, issue_id).await?;
    if issue.project_id != project_id {
        return Err(AppError::NotFound(format!("Issue {} not found", issue_id)));
    }
    Ok(())
}

fn require_user(actor: &ApiActor) -> AppResult<i32> {
    actor
        .user_id()
        .ok_or_else(|| AppError::Validation("This action requires a user session".to_string()))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/issues/{issue_id}/stats",
    tag = "Issues",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("issue_id" = uuid::Uuid, Path, description = "Issue ID"),
        ("window" = Option<String>, Query, description = "Time window: 24h (default) or 30d"),
    ),
    responses(
        (status = 200, description = "Zero-filled event-count timeseries", body = resp::IssueStatsResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /{issue_id}/stats?window=24h|30d — zero-filled event-count timeseries.
pub async fn get_issue_stats(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid)>,
    query: web::Query<std::collections::HashMap<String, String>>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, issue_id) = path.into_inner();
    access::require(
        pool.get_ref(),
        actor.is_admin(),
        actor.user_id(),
        project_id,
        Action::ViewProject,
    )
    .await?;
    require_issue_in_project(pool.get_ref(), project_id, issue_id).await?;

    // window=24h → 24 hourly buckets; window=30d → 30 daily buckets.
    let (bucket_secs, buckets) = match query.get("window").map(String::as_str) {
        Some("30d") => (86_400, 30),
        _ => (3_600, 24),
    };
    let series = IssueService::stats(
        pool.get_ref(),
        issue_id,
        bucket_secs,
        buckets,
        query
            .get("environment")
            .map(String::as_str)
            .filter(|s| !s.is_empty()),
    )
    .await?;
    let points: Vec<_> = series
        .into_iter()
        .map(|(ts, count)| json!([ts, count]))
        .collect();
    Ok(HttpResponse::Ok().json(json!({ "data": points })))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/issues/{issue_id}/activity",
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
/// GET /{issue_id}/activity — chronological activity log (incl. comments).
pub async fn get_issue_activity(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid)>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, issue_id) = path.into_inner();
    access::require(
        pool.get_ref(),
        actor.is_admin(),
        actor.user_id(),
        project_id,
        Action::ViewProject,
    )
    .await?;
    require_issue_in_project(pool.get_ref(), project_id, issue_id).await?;
    let activity = IssueSocialService::list_activity(pool.get_ref(), issue_id).await?;
    Ok(HttpResponse::Ok().json(activity))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/projects/{project_id}/issues/{issue_id}/comments",
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
/// POST /{issue_id}/comments — add a note.
pub async fn create_issue_comment(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid)>,
    body: web::Json<CommentRequest>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, issue_id) = path.into_inner();
    access::require(
        pool.get_ref(),
        actor.is_admin(),
        actor.user_id(),
        project_id,
        Action::MutateIssue,
    )
    .await?;
    require_issue_in_project(pool.get_ref(), project_id, issue_id).await?;
    let entry =
        IssueSocialService::add_comment(pool.get_ref(), issue_id, actor.user_id(), &body.text)
            .await?;
    Ok(HttpResponse::Created().json(entry))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    put,
    path = "/api/projects/{project_id}/issues/{issue_id}/bookmark",
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
/// PUT /{issue_id}/bookmark — set/clear the per-user bookmark.
pub async fn set_issue_bookmark(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid)>,
    body: web::Json<ToggleRequest>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, issue_id) = path.into_inner();
    access::require(
        pool.get_ref(),
        actor.is_admin(),
        actor.user_id(),
        project_id,
        Action::ViewProject,
    )
    .await?;
    require_issue_in_project(pool.get_ref(), project_id, issue_id).await?;
    let user_id = require_user(&actor)?;
    let enabled = body.enabled.unwrap_or(true);
    IssueSocialService::set_bookmark(pool.get_ref(), issue_id, user_id, enabled).await?;
    Ok(HttpResponse::Ok().json(json!({ "is_bookmarked": enabled })))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    put,
    path = "/api/projects/{project_id}/issues/{issue_id}/subscription",
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
/// PUT /{issue_id}/subscription — set/clear the per-user subscription.
pub async fn set_issue_subscription(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid)>,
    body: web::Json<ToggleRequest>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, issue_id) = path.into_inner();
    access::require(
        pool.get_ref(),
        actor.is_admin(),
        actor.user_id(),
        project_id,
        Action::ViewProject,
    )
    .await?;
    require_issue_in_project(pool.get_ref(), project_id, issue_id).await?;
    let user_id = require_user(&actor)?;
    let enabled = body.enabled.unwrap_or(true);
    IssueSocialService::set_subscription(pool.get_ref(), issue_id, user_id, enabled, "manual")
        .await?;
    Ok(HttpResponse::Ok().json(json!({ "is_subscribed": enabled })))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/projects/{project_id}/issues/{issue_id}/seen",
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
/// POST /{issue_id}/seen — mark the issue as seen by the current user.
pub async fn mark_issue_seen(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid)>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, issue_id) = path.into_inner();
    access::require(
        pool.get_ref(),
        actor.is_admin(),
        actor.user_id(),
        project_id,
        Action::ViewProject,
    )
    .await?;
    require_issue_in_project(pool.get_ref(), project_id, issue_id).await?;
    let user_id = require_user(&actor)?;
    IssueSocialService::mark_seen(pool.get_ref(), issue_id, user_id).await?;
    Ok(HttpResponse::Ok().json(json!({ "has_seen": true })))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/issues/{issue_id}/user-reports",
    tag = "Issues",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("issue_id" = uuid::Uuid, Path, description = "Issue ID"),
    ),
    responses(
        (status = 200, description = "User feedback reports for the issue", body = Vec<crate::services::issue_social::UserReport>),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /{issue_id}/user-reports — list user feedback for an issue.
pub async fn list_issue_user_reports(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid)>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, issue_id) = path.into_inner();
    access::require(
        pool.get_ref(),
        actor.is_admin(),
        actor.user_id(),
        project_id,
        Action::ViewProject,
    )
    .await?;
    require_issue_in_project(pool.get_ref(), project_id, issue_id).await?;
    let reports = IssueSocialService::list_user_reports(pool.get_ref(), issue_id).await?;
    Ok(HttpResponse::Ok().json(reports))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/projects/{project_id}/issues/{issue_id}/user-reports",
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
/// POST /{issue_id}/user-reports — attach user feedback to an issue.
pub async fn create_issue_user_report(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid)>,
    body: web::Json<UserReportRequest>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, issue_id) = path.into_inner();
    access::require(
        pool.get_ref(),
        actor.is_admin(),
        actor.user_id(),
        project_id,
        Action::MutateIssue,
    )
    .await?;
    require_issue_in_project(pool.get_ref(), project_id, issue_id).await?;

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
#[allow(dead_code)]
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

/// Configure issue routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/projects/{project_id}/issues")
            .route("", web::get().to(list_issues))
            .route("", web::put().to(bulk_update_issues))
            .route("", web::delete().to(bulk_delete_issues))
            .route("/{issue_id}/hashes", web::get().to(get_issue_hashes))
            .route(
                "/{issue_id}/aggregates",
                web::get().to(get_issue_aggregates),
            )
            .route(
                "/{issue_id}/tags/{key}",
                web::get().to(get_issue_tag_values),
            )
            .route("/{issue_id}/stats", web::get().to(get_issue_stats))
            .route("/{issue_id}/activity", web::get().to(get_issue_activity))
            .route("/{issue_id}/comments", web::post().to(create_issue_comment))
            .route("/{issue_id}/bookmark", web::put().to(set_issue_bookmark))
            .route(
                "/{issue_id}/subscription",
                web::put().to(set_issue_subscription),
            )
            .route("/{issue_id}/seen", web::post().to(mark_issue_seen))
            .route(
                "/{issue_id}/user-reports",
                web::get().to(list_issue_user_reports),
            )
            .route(
                "/{issue_id}/user-reports",
                web::post().to(create_issue_user_report),
            )
            .route("/{issue_id}", web::get().to(get_issue))
            .route("/{issue_id}", web::patch().to(update_issue))
            .route("/{issue_id}", web::put().to(update_issue))
            .route("/{issue_id}", web::delete().to(delete_issue)),
    );
}
