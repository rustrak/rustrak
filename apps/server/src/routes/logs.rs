use actix_web::{web, HttpResponse};

use crate::auth::ApiActor;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::rbac::{Action, Resource};
#[cfg(feature = "openapi")]
use crate::models::{LogResponse, LogVolumePoint};
use crate::pagination::OffsetPaginatedResponse;
use crate::services::log::LogService;

#[cfg(feature = "openapi")]
use utoipa::OpenApi;

/// Query parameters for listing logs (offset-based)
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct ListLogsQuery {
    /// Page number (1-indexed, default: 1)
    #[serde(default = "crate::pagination::default_page")]
    #[cfg_attr(feature = "openapi", param(minimum = 1))]
    pub page: i64,

    /// Items per page (default: 20, max: 100)
    #[serde(default = "crate::pagination::default_per_page")]
    #[cfg_attr(feature = "openapi", param(minimum = 1, maximum = 100))]
    pub per_page: i64,

    /// Filter by log level (trace/debug/info/warn/error/fatal).
    pub level: Option<String>,

    /// Filter by trace id.
    pub trace_id: Option<String>,

    /// Only the logs whose message contains this, ignoring case.
    pub query: Option<String>,

    /// Only this window (`24h`, `7d`, `14d`...); all time if absent.
    pub period: Option<String>,
}

impl ListLogsQuery {
    pub fn filters(&self) -> crate::services::log::LogFilters {
        crate::services::log::LogFilters {
            level: self.level.clone().filter(|l| !l.is_empty()),
            trace_id: self.trace_id.clone().filter(|t| !t.is_empty()),
            query: self.query.clone(),
            period_hours: crate::routes::period::parse_period_hours(self.period.as_deref()),
        }
    }
}

/// Query parameters for the log volume: the list's filters, no page.
#[derive(Debug, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct LogVolumeQuery {
    /// Filter by log level (trace/debug/info/warn/error/fatal).
    pub level: Option<String>,

    /// Filter by trace id.
    pub trace_id: Option<String>,

    /// Only the logs whose message contains this, ignoring case.
    pub query: Option<String>,

    /// Only this window (`24h`, `7d`, `14d`...); since the oldest log if absent.
    pub period: Option<String>,
}

impl LogVolumeQuery {
    pub fn filters(&self) -> crate::services::log::LogFilters {
        crate::services::log::LogFilters {
            level: self.level.clone().filter(|l| !l.is_empty()),
            trace_id: self.trace_id.clone().filter(|t| !t.is_empty()),
            query: self.query.clone(),
            period_hours: crate::routes::period::parse_period_hours(self.period.as_deref()),
        }
    }
}

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
    let per_page = crate::pagination::per_page(query.per_page)?;

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

pub fn configure_project(cfg: &mut web::ServiceConfig) {
    cfg.route("/logs", web::get().to(list_logs))
        .route("/logs/volume", web::get().to(get_log_volume))
        .route("/logs/{log_id}", web::get().to(get_log));
}
