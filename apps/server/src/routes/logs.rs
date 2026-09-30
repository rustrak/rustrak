use actix_web::{web, HttpResponse};

use crate::auth::ApiActor;
use crate::db::DbPool;
use crate::error::AppResult;
#[cfg(feature = "openapi")]
use crate::models::LogResponse;
use crate::pagination::{ListLogsQuery, OffsetPaginatedResponse};
use crate::services::access::{self, Action};
use crate::services::log::{LogFilters, LogService};

#[cfg(feature = "openapi")]
use utoipa::OpenApi;

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/logs",
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
/// log timestamp), optionally filtered by level/trace_id.
pub async fn list_logs(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<ListLogsQuery>,
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

    let page = query.page.max(1);
    let per_page = query.per_page.clamp(1, 100);

    let filters = LogFilters {
        level: query.level.clone(),
        trace_id: query.trace_id.clone(),
        environment: query.environment.clone().filter(|s| !s.is_empty()),
    };

    let (logs, total_count) =
        LogService::list_offset(pool.get_ref(), project_id, page, per_page, &filters).await?;

    Ok(HttpResponse::Ok().json(OffsetPaginatedResponse::new(
        logs,
        total_count,
        page,
        per_page,
    )))
}

#[cfg(feature = "openapi")]
#[derive(OpenApi)]
#[openapi(paths(list_logs))]
pub struct LogsApi;

/// Configure log routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(web::scope("/api/projects/{project_id}/logs").route("", web::get().to(list_logs)));
}
