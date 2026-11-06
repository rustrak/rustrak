use actix_web::{web, HttpResponse};
use serde::Deserialize;

use crate::auth::rbac::{Action, Resource};
use crate::auth::ApiActor;
use crate::db::DbPool;
use crate::error::AppResult;
#[cfg(feature = "openapi")]
use crate::models::monitor::{CheckInResponse, MonitorEnvironmentStats, MonitorResponse};
use crate::models::monitor::{CreateMonitor, UpdateMonitor};
use crate::pagination::OffsetPaginatedResponse;
use crate::services::monitor::MonitorService;

#[cfg(feature = "openapi")]
use utoipa::OpenApi;

/// Query parameters for listing a monitor's check-ins.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct ListCheckInsQuery {
    /// Page number (1-indexed, default: 1)
    #[serde(default = "default_page")]
    pub page: i64,
    /// Items per page (default: 50, max: 100)
    #[serde(default = "default_per_page")]
    pub per_page: i64,
    /// Only check-ins from this environment.
    pub environment: Option<String>,
}

/// Query parameters for a project's monitor stats.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct MonitorStatsQuery {
    /// The window (`24h`, `7d`, `30d`...); the last day if absent.
    pub period: Option<String>,
    /// Only this monitor.
    pub slug: Option<String>,
}

fn default_page() -> i64 {
    1
}

fn default_per_page() -> i64 {
    50
}

async fn require(
    _pool: &DbPool,
    actor: &ApiActor,
    project_id: i32,
    resource: Resource,
    action: Action,
) -> AppResult<()> {
    actor.require(resource, action, Some(project_id))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/monitors",
    operation_id = "monitors.list",
    tag = "Monitors",
    params(("project_id" = i32, Path, description = "Project ID")),
    responses(
        (status = 200, description = "Every cron monitor of the project, with its environments and recent check-ins", body = Vec<MonitorResponse>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/projects/{project_id}/monitors
pub async fn list_monitors(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();
    require(&pool, &actor, project_id, Resource::Monitors, Action::Read).await?;
    let monitors = MonitorService::list(&pool, project_id).await?;
    Ok(HttpResponse::Ok().json(monitors))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/monitor-stats",
    operation_id = "monitors.stats",
    tag = "Monitors",
    params(("project_id" = i32, Path, description = "Project ID"), MonitorStatsQuery),
    responses(
        (status = 200, description = "Each monitor environment's check-ins by status, in sixty buckets on one axis", body = Vec<MonitorEnvironmentStats>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/projects/{project_id}/monitor-stats
/// Counts the check-ins of a project's monitors by status over a window, bucket by bucket.
pub async fn get_monitor_stats(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<MonitorStatsQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();
    require(&pool, &actor, project_id, Resource::Monitors, Action::Read).await?;
    let period_hours =
        crate::routes::period::parse_period_hours(query.period.as_deref()).unwrap_or(24);
    let stats = MonitorService::stats(
        &pool,
        project_id,
        query.slug.as_deref().filter(|s| !s.is_empty()),
        period_hours,
    )
    .await?;
    Ok(HttpResponse::Ok().json(stats))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/projects/{project_id}/monitors",
    operation_id = "monitors.create",
    tag = "Monitors",
    params(("project_id" = i32, Path, description = "Project ID")),
    request_body = CreateMonitor,
    responses(
        (status = 201, description = "Created", body = MonitorResponse),
        (status = 400, description = "Invalid name or config", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 409, description = "Slug already taken", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// POST /api/projects/{project_id}/monitors
pub async fn create_monitor(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    body: web::Json<CreateMonitor>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();
    require(
        &pool,
        &actor,
        project_id,
        Resource::Monitors,
        Action::Create,
    )
    .await?;
    let monitor = MonitorService::create(&pool, project_id, body.into_inner()).await?;
    Ok(HttpResponse::Created().json(monitor))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/monitors/{slug}",
    operation_id = "monitors.get",
    tag = "Monitors",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("slug" = String, Path, description = "Monitor slug"),
    ),
    responses(
        (status = 200, description = "The monitor", body = MonitorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/projects/{project_id}/monitors/{slug}
pub async fn get_monitor(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, String)>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, slug) = path.into_inner();
    require(&pool, &actor, project_id, Resource::Monitors, Action::Read).await?;
    let monitor = MonitorService::get_by_slug(&pool, project_id, &slug).await?;
    Ok(HttpResponse::Ok().json(monitor))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    put,
    path = "/api/projects/{project_id}/monitors/{slug}",
    operation_id = "monitors.update",
    tag = "Monitors",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("slug" = String, Path, description = "Monitor slug"),
    ),
    request_body = UpdateMonitor,
    responses(
        (status = 200, description = "Updated", body = MonitorResponse),
        (status = 400, description = "Invalid name or config", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// PUT /api/projects/{project_id}/monitors/{slug}
pub async fn update_monitor(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, String)>,
    body: web::Json<UpdateMonitor>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, slug) = path.into_inner();
    require(
        &pool,
        &actor,
        project_id,
        Resource::Monitors,
        Action::Update,
    )
    .await?;
    let monitor = MonitorService::update(&pool, project_id, &slug, body.into_inner()).await?;
    Ok(HttpResponse::Ok().json(monitor))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/api/projects/{project_id}/monitors/{slug}",
    operation_id = "monitors.delete",
    tag = "Monitors",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("slug" = String, Path, description = "Monitor slug"),
    ),
    responses(
        (status = 204, description = "Deleted with its check-ins; its issues stay"),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// DELETE /api/projects/{project_id}/monitors/{slug}
pub async fn delete_monitor(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, String)>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, slug) = path.into_inner();
    require(
        &pool,
        &actor,
        project_id,
        Resource::Monitors,
        Action::Delete,
    )
    .await?;
    MonitorService::delete(&pool, project_id, &slug).await?;
    Ok(HttpResponse::NoContent().finish())
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/monitors/{slug}/checkins",
    operation_id = "monitors.listCheckins",
    tag = "Monitors",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("slug" = String, Path, description = "Monitor slug"),
        ListCheckInsQuery,
    ),
    responses(
        (status = 200, description = "Check-ins, newest first", body = inline(crate::pagination::OffsetPaginatedResponse<CheckInResponse>)),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/projects/{project_id}/monitors/{slug}/checkins
pub async fn list_checkins(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, String)>,
    query: web::Query<ListCheckInsQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, slug) = path.into_inner();
    require(&pool, &actor, project_id, Resource::Monitors, Action::Read).await?;
    let page = query.page.max(1);
    let per_page = query.per_page.clamp(1, 100);
    let (items, total) = MonitorService::list_checkins(
        &pool,
        project_id,
        &slug,
        query.environment.as_deref(),
        page,
        per_page,
    )
    .await?;
    Ok(HttpResponse::Ok().json(OffsetPaginatedResponse::new(items, total, page, per_page)))
}

#[cfg(feature = "openapi")]
#[derive(OpenApi)]
#[openapi(paths(
    list_monitors,
    create_monitor,
    get_monitor,
    update_monitor,
    delete_monitor,
    list_checkins,
    get_monitor_stats
))]
pub struct MonitorsApi;

/// Configure monitor routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/projects/{project_id}/monitors")
            .route("", web::get().to(list_monitors))
            .route("", web::post().to(create_monitor))
            .route("/{slug}", web::get().to(get_monitor))
            .route("/{slug}", web::put().to(update_monitor))
            .route("/{slug}", web::delete().to(delete_monitor))
            .route("/{slug}/checkins", web::get().to(list_checkins)),
    )
    .route(
        "/api/projects/{project_id}/monitor-stats",
        web::get().to(get_monitor_stats),
    );
}
