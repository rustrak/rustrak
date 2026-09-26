use actix_web::{web, HttpResponse};
use uuid::Uuid;

use crate::auth::ApiActor;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
#[cfg(feature = "openapi")]
use crate::models::{EventDetailResponse, EventResponse};
use crate::pagination::{EventCursor, ListEventsQuery, PaginatedResponse, PAGE_SIZE};
use crate::services::access::{self, Action};
use crate::services::{EventService, IssueService};

#[cfg(feature = "openapi")]
use utoipa::OpenApi;

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/issues/{issue_id}/events",
    tag = "Events",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("issue_id" = uuid::Uuid, Path, description = "Issue ID"),
        ListEventsQuery,
    ),
    responses(
        (status = 200, description = "Paginated event list", body = inline(crate::pagination::PaginatedResponse<EventResponse>)),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/projects/{project_id}/issues/{issue_id}/events
/// Lists events for an issue with cursor-based pagination
pub async fn list_events(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid)>,
    query: web::Query<ListEventsQuery>,
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

    // Verify issue exists and belongs to the project
    let issue = IssueService::get_by_id(pool.get_ref(), issue_id).await?;
    if issue.project_id != project_id {
        return Err(AppError::NotFound(format!("Issue {} not found", issue_id)));
    }

    // Parse cursor if provided
    let cursor = query
        .cursor
        .as_ref()
        .map(|c| EventCursor::decode(c))
        .transpose()?;

    // Execute paginated query
    let (events, has_more) = EventService::list_paginated(
        pool.get_ref(),
        issue_id,
        query.order,
        cursor.as_ref(),
        PAGE_SIZE,
    )
    .await?;

    // Build responses (without full data field)
    let responses: Vec<_> = events.iter().map(|e| e.to_response()).collect();

    // Build next cursor if there are more results
    let next_cursor = if has_more {
        events
            .last()
            .map(|last| EventCursor::new(query.order.as_str(), last.timestamp, last.id).encode())
            .transpose()?
    } else {
        None
    };

    Ok(HttpResponse::Ok().json(PaginatedResponse::new(responses, next_cursor, has_more)))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/issues/{issue_id}/events/{event_id}",
    tag = "Events",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("issue_id" = uuid::Uuid, Path, description = "Issue ID"),
        ("event_id" = uuid::Uuid, Path, description = "Event ID"),
    ),
    responses(
        (status = 200, description = "Full event detail", body = EventDetailResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/projects/{project_id}/issues/{issue_id}/events/{event_id}
/// Gets a single event with full data
pub async fn get_event(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid, Uuid)>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, issue_id, event_id) = path.into_inner();

    access::require(
        pool.get_ref(),
        actor.is_admin(),
        actor.user_id(),
        project_id,
        Action::ViewProject,
    )
    .await?;

    // Verify issue exists and belongs to the project
    let issue = IssueService::get_by_id(pool.get_ref(), issue_id).await?;
    if issue.project_id != project_id {
        return Err(AppError::NotFound(format!("Issue {} not found", issue_id)));
    }

    // Get event and verify it belongs to the issue
    let event = EventService::get_by_id(pool.get_ref(), event_id).await?;
    if event.issue_id != Some(issue_id) {
        return Err(AppError::NotFound(format!("Event {} not found", event_id)));
    }

    // Return full detail response (includes data field)
    Ok(HttpResponse::Ok().json(event.to_detail_response()))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/events/sentry/{event_id}",
    tag = "Events",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("event_id" = uuid::Uuid, Path, description = "Client-supplied Sentry event ID (compact or hyphenated UUID)"),
    ),
    responses(
        (status = 200, description = "Full event detail, including its issue ID", body = EventDetailResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Event or accessible project not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Resolve the ID returned by a Sentry SDK without already knowing the issue.
pub async fn get_event_by_sentry_id(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid)>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, event_id) = path.into_inner();
    // Apply the same Viewer boundary as issue-scoped detail before reading data.
    access::require(
        pool.get_ref(),
        actor.is_admin(),
        actor.user_id(),
        project_id,
        Action::ViewProject,
    )
    .await?;
    // The existing unique (project_id, event_id) index also serves this lookup.
    let event = EventService::get_by_event_id(pool.get_ref(), project_id, event_id).await?;
    Ok(HttpResponse::Ok().json(event.to_detail_response()))
}

#[cfg(feature = "openapi")]
#[derive(OpenApi)]
#[openapi(
    paths(list_events, get_event, get_event_by_sentry_id),
    components(schemas(crate::models::EventResponse, crate::models::EventDetailResponse,))
)]
pub struct EventsApi;

/// Configure event routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route(
        "/api/projects/{project_id}/events/sentry/{event_id}",
        web::get().to(get_event_by_sentry_id),
    );
    cfg.service(
        web::scope("/api/projects/{project_id}/issues/{issue_id}/events")
            .route("", web::get().to(list_events))
            .route("/{event_id}", web::get().to(get_event)),
    );
}
