use actix_web::{web, HttpResponse};

use crate::auth::rbac::{Action, Resource};
use crate::auth::ApiActor;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
#[cfg(feature = "openapi")]
use crate::models::{LogResponse, LogVolumePoint};
use crate::pagination::{ListLogsQuery, LogVolumeQuery, OffsetPaginatedResponse};
use crate::services::log::LogService;

#[cfg(feature = "openapi")]
use utoipa::OpenApi;

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/logs",
    operation_id = "logs.list",
    tag = "Logs",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ListLogsQuery,
    ),
    responses(
        (status = 200, description = "Paginated log list", body = inline(crate::pagination::OffsetPaginatedResponse<LogResponse>)),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/projects/{project_id}/logs
/// Lists logs for a project with offset-based pagination (newest first by
/// log timestamp), optionally filtered by level, trace id, message and window.
pub async fn list_logs(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<ListLogsQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();

    actor.require(Resource::Logs, Action::Read, Some(project_id))?;

    let page = query.page.max(1);
    let per_page = query.per_page.clamp(1, 100);

    let filters = query.filters();

    let (logs, total_count) =
        LogService::list_offset(pool.get_ref(), project_id, page, per_page, &filters).await?;

    Ok(HttpResponse::Ok().json(OffsetPaginatedResponse::new(
        logs,
        total_count,
        page,
        per_page,
    )))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/logs/volume",
    operation_id = "logs.volume",
    tag = "Logs",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        LogVolumeQuery,
    ),
    responses(
        (status = 200, description = "Log counts per level, bucket by bucket", body = Vec<LogVolumePoint>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/projects/{project_id}/logs/volume
/// Counts a project's logs per level over time, with the list's filters.
pub async fn get_log_volume(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<LogVolumeQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();

    actor.require(Resource::Logs, Action::Read, Some(project_id))?;

    let points = LogService::volume(pool.get_ref(), project_id, &query.filters()).await?;
    Ok(HttpResponse::Ok().json(points))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/logs/{log_id}",
    operation_id = "logs.get",
    tag = "Logs",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("log_id" = uuid::Uuid, Path, description = "Log ID"),
    ),
    responses(
        (status = 200, description = "The log", body = LogResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/projects/{project_id}/logs/{log_id}
/// Returns one of a project's logs, wherever it falls in the list.
pub async fn get_log(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, uuid::Uuid)>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, log_id) = path.into_inner();

    actor.require(Resource::Logs, Action::Read, Some(project_id))?;

    let log = LogService::get(pool.get_ref(), project_id, log_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Log {log_id}")))?;
    Ok(HttpResponse::Ok().json(log))
}

#[cfg(feature = "openapi")]
#[derive(OpenApi)]
#[openapi(paths(list_logs, get_log_volume, get_log))]
pub struct LogsApi;

/// Configure log routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/projects/{project_id}/logs")
            .route("", web::get().to(list_logs))
            .route("/volume", web::get().to(get_log_volume))
            .route("/{log_id}", web::get().to(get_log)),
    );
}
