use actix_web::{web, HttpResponse};
use uuid::Uuid;

use crate::auth::rbac::{Action, Resource};
use crate::auth::ApiActor;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
#[cfg(feature = "openapi")]
use crate::models::{EventDetailResponse, EventResponse};
use crate::pagination::{EventCursor, ListEventsQuery, PaginatedResponse, PAGE_SIZE};
use crate::services::attachment::image_type;
use crate::services::{AttachmentService, EventService, IssueService};

#[cfg(feature = "openapi")]
use utoipa::OpenApi;

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/issues/{issue_id}/events",
    operation_id = "events.list",
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

    actor.require(Resource::Issues, Action::Read, Some(project_id))?;

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
        query.environment.as_deref(),
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
    operation_id = "events.get",
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

    actor.require(Resource::Issues, Action::Read, Some(project_id))?;

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
    operation_id = "events.getBySentryId",
    tag = "Events",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("event_id" = String, Path, description = "Client-supplied Sentry event ID: 32 hex digits, compact or hyphenated"),
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
    actor.require(Resource::Issues, Action::Read, Some(project_id))?;
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

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/issues/{issue_id}/events/{event_id}/attachments",
    operation_id = "events.listAttachments",
    tag = "Events",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("issue_id" = uuid::Uuid, Path, description = "Issue ID"),
        ("event_id" = uuid::Uuid, Path, description = "Event ID"),
    ),
    responses(
        (status = 200, description = "The files sent with the event, without their bytes", body = Vec<crate::services::attachment::AttachmentInfo>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/projects/{project_id}/issues/{issue_id}/events/{event_id}/attachments
/// Lists the files sent with an event, such as a feedback's screenshot.
pub async fn list_event_attachments(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid, Uuid)>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, issue_id, event_id) = path.into_inner();
    actor.require(Resource::Issues, Action::Read, Some(project_id))?;
    let event = EventService::get_by_id(pool.get_ref(), event_id).await?;
    if event.project_id != project_id || event.issue_id != Some(issue_id) {
        return Err(AppError::NotFound(format!("Event {} not found", event_id)));
    }
    let attachments =
        AttachmentService::list_for_event(pool.get_ref(), project_id, event_id).await?;
    Ok(HttpResponse::Ok().json(attachments))
}

/// The raw bytes of an attachment. Untyped in the spec: the generated
/// client validates bodies with zod, which has no type for a binary one.
#[cfg(feature = "openapi")]
#[derive(utoipa::ToSchema)]
#[schema(value_type = Value)]
pub struct AttachmentBytes(#[allow(dead_code)] Vec<u8>);

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/attachments/{attachment_id}",
    operation_id = "events.downloadAttachment",
    tag = "Events",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("attachment_id" = uuid::Uuid, Path, description = "Attachment ID"),
    ),
    responses(
        (status = 200, description = "The file: inline when its bytes are a PNG, JPEG, GIF or WebP image, a download otherwise", body = AttachmentBytes, content_type = "application/octet-stream"),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/projects/{project_id}/attachments/{attachment_id}
/// Downloads an attachment. Only a real image is served inline; anything
/// else, whatever type it claims, is a download that never runs.
pub async fn download_attachment(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid)>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, attachment_id) = path.into_inner();
    actor.require(Resource::Issues, Action::Read, Some(project_id))?;
    let (info, data) = AttachmentService::get(pool.get_ref(), project_id, attachment_id).await?;
    use actix_web::http::header::{
        Charset, ContentDisposition, DispositionParam, DispositionType, ExtendedValue,
    };
    // The sender names the file: the plain name keeps to characters a header
    // cannot misread, and `filename*` (RFC 6266) carries the original,
    // percent-encoded.
    let filename: String = info
        .filename
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '-' | '_' => c,
            _ => '_',
        })
        .collect();
    let (content_type, disposition) = match image_type(&data) {
        Some(image) => (image, DispositionType::Inline),
        None => ("application/octet-stream", DispositionType::Attachment),
    };
    let mut parameters = vec![DispositionParam::Filename(filename.clone())];
    if filename != info.filename {
        parameters.push(DispositionParam::FilenameExt(ExtendedValue {
            charset: Charset::Ext("UTF-8".to_string()),
            language_tag: None,
            value: info.filename.into_bytes(),
        }));
    }
    Ok(HttpResponse::Ok()
        .content_type(content_type)
        .insert_header(ContentDisposition {
            disposition,
            parameters,
        })
        .insert_header(("X-Content-Type-Options", "nosniff"))
        .insert_header(("Content-Security-Policy", "sandbox"))
        .insert_header(("Cache-Control", "private, max-age=3600"))
        .body(data))
}

/// Configure event routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route(
        "/api/projects/{project_id}/events/sentry/{event_id}",
        web::get().to(get_event_by_sentry_id),
    );
    cfg.route(
        "/api/projects/{project_id}/attachments/{attachment_id}",
        web::get().to(download_attachment),
    );
    cfg.service(
        web::scope("/api/projects/{project_id}/issues/{issue_id}/events")
            .route("", web::get().to(list_events))
            .route("/{event_id}", web::get().to(get_event))
            .route(
                "/{event_id}/attachments",
                web::get().to(list_event_attachments),
            ),
    );
}
