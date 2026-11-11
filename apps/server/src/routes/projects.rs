use actix_web::{web, HttpResponse};

use crate::auth::ApiActor;
use crate::config::Config;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::rbac::{Action, ProjectScope, Resource};
use crate::models::{CreateProject, ProjectResponse, UpdateProject};
use crate::pagination::{OffsetPaginatedResponse, SortOrder};
use crate::routes::period::parse_period_hours;
use crate::services::stats::StatsService;
use crate::services::{ProjectService, RoleService};

#[cfg(feature = "openapi")]
use utoipa::OpenApi;

/// Seeing a project takes any permission on it; its ingest key takes this one.
fn keys_if_held(actor: &ApiActor, mut response: ProjectResponse) -> ProjectResponse {
    if !actor
        .access
        .can(Resource::ClientKeys, Action::Read, Some(response.id))
    {
        response.sentry_key = None;
        response.dsn = None;
    }
    response
}

/// Query parameters for listing projects (offset-based)
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct ListProjectsQuery {
    /// Page number (1-indexed, default: 1)
    #[serde(default = "crate::pagination::default_page")]
    #[cfg_attr(feature = "openapi", param(minimum = 1))]
    pub page: i64,

    /// Items per page (default: 20, max: 100)
    #[serde(default = "crate::pagination::default_per_page")]
    #[cfg_attr(feature = "openapi", param(minimum = 1, maximum = 100))]
    pub per_page: i64,

    /// Sort order direction (default: desc = newest first)
    #[serde(default)]
    pub order: SortOrder,

    /// Window for the per-project stats on each row (e.g. `24h`, `7d`).
    ///
    /// Omitted means the caller does not want stats at all, and the extra
    /// aggregate queries are skipped entirely — this is what keeps the plain
    /// `GET /api/projects` that SDK tooling and the project picker use as
    /// cheap as it was before the list table grew a sparkline. Sentry's own
    /// projects endpoint gates its `stats` field on `?statsPeriod=` the same
    /// way.
    #[serde(default)]
    pub stats_period: Option<String>,
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects",
    operation_id = "projects.list",
    tag = "Projects",
    params(ListProjectsQuery),
    responses(
        (status = 200, description = "List of projects", body = inline(crate::pagination::OffsetPaginatedResponse<ProjectResponse>)),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// List projects with pagination.
///
/// Only the projects the caller holds a permission on.
pub async fn list_projects(
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    query: web::Query<ListProjectsQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let per_page = crate::pagination::per_page(query.per_page)?;
    let (projects, total_count) = match actor.visible_projects() {
        ProjectScope::All => {
            ProjectService::list_offset(pool.get_ref(), query.order, query.page, per_page).await?
        }
        ProjectScope::Only(ids) => {
            ProjectService::list_offset_for_ids(
                pool.get_ref(),
                &ids,
                query.order,
                query.page,
                per_page,
            )
            .await?
        }
    };

    let base_url = config.base_url();
    let mut responses: Vec<_> = projects
        .iter()
        .map(|p| keys_if_held(&actor, p.to_response(&base_url)))
        .collect();

    // Stats are attached to the page that was already fetched, so the extra
    // work scales with `per_page` (capped at 100) and not with the number of
    // projects on the instance.
    if let Some(period_hours) = parse_period_hours(query.stats_period.as_deref()) {
        let ids: Vec<i32> = responses
            .iter()
            .map(|p| p.id)
            .filter(|&id| actor.access.can(Resource::Issues, Action::Read, Some(id)))
            .collect();
        let mut stats = StatsService::list_stats(pool.get_ref(), &ids, period_hours).await?;
        for response in &mut responses {
            response.stats = stats.remove(&response.id);
        }
    }

    Ok(HttpResponse::Ok().json(OffsetPaginatedResponse::new(
        responses,
        total_count,
        query.page,
        per_page,
    )))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{id}",
    operation_id = "projects.get",
    tag = "Projects",
    params(("id" = i32, Path, description = "Project ID")),
    responses(
        (status = 200, description = "Project details", body = ProjectResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Get a project by ID
pub async fn get_project(
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    path: web::Path<i32>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    actor.require(Resource::Project, Action::Read, Some(id))?;

    let project = ProjectService::get_by_id(pool.get_ref(), id).await?;
    let base_url = config.base_url();

    Ok(HttpResponse::Ok().json(keys_if_held(&actor, project.to_response(&base_url))))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/projects",
    operation_id = "projects.create",
    tag = "Projects",
    request_body = CreateProject,
    responses(
        (status = 201, description = "Project created", body = ProjectResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Missing projects:create, or no permission on projects", body = crate::error::ErrorResponse),
        (status = 409, description = "Conflict", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Create a new project
///
/// Needs `projects:create` and a permission on at least one project, because
/// the creator's own roles extend to the new one. A token limited to chosen
/// projects can create one, but its list does not grow to reach it.
pub async fn create_project(
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    body: AppResult<web::Json<CreateProject>>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    actor.require(Resource::Projects, Action::Create, None)?;
    let body = body?;
    // Their own roles are what will reach the project: with none that reach
    // projects, they could never see what they created.
    if actor.visible_projects() == ProjectScope::Only(vec![]) {
        return Err(AppError::Forbidden(
            "Creating a project also requires a project permission".to_string(),
        ));
    }
    let project = ProjectService::create(pool.get_ref(), body.into_inner()).await?;

    // The creator's own roles extend to it, unless they already reach every
    // project, so it grants nothing new.
    if let (Some(uid), ProjectScope::Only(_)) = (actor.user_id(), actor.visible_projects()) {
        RoleService::extend_to_project(pool.get_ref(), uid, project.id).await?;
    }

    let base_url = config.base_url();
    let mut response = project.to_response(&base_url);
    // `actor` predates the new project, so ask what holds on it.
    if !actor
        .access
        .can_on_new_project(Resource::ClientKeys, Action::Read, project.id)
    {
        response.sentry_key = None;
        response.dsn = None;
    }

    Ok(HttpResponse::Created().json(response))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    patch,
    path = "/api/projects/{id}",
    operation_id = "projects.update",
    tag = "Projects",
    params(("id" = i32, Path, description = "Project ID")),
    request_body = UpdateProject,
    responses(
        (status = 200, description = "Project updated", body = ProjectResponse),
        (status = 400, description = "Invalid value, such as a limit above the server's", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
        (status = 409, description = "Conflict", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Update a project
pub async fn update_project(
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    path: web::Path<i32>,
    body: web::Json<UpdateProject>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    actor.require(Resource::Project, Action::Update, Some(id))?;
    // A project's limit can only tighten the operator's: one above it would
    // be stored and silently never apply.
    let limits = &config.rate_limit;
    for (field, limit, max) in [
        (
            "rate_limit_per_minute",
            body.rate_limit_per_minute,
            limits.max_events_per_project_per_minute,
        ),
        (
            "rate_limit_per_hour",
            body.rate_limit_per_hour,
            limits.max_events_per_project_per_hour,
        ),
    ] {
        if matches!(limit, Some(Some(value)) if value > max) {
            return Err(
                AppError::Validation(format!("{field} cannot exceed the server's {max}"))
                    .with_field(field, crate::error::FieldErrorCode::Invalid),
            );
        }
    }

    let project = ProjectService::update(pool.get_ref(), id, body.into_inner()).await?;
    let base_url = config.base_url();

    Ok(HttpResponse::Ok().json(keys_if_held(&actor, project.to_response(&base_url))))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/api/projects/{id}",
    operation_id = "projects.delete",
    tag = "Projects",
    params(("id" = i32, Path, description = "Project ID")),
    responses(
        (status = 204, description = "Project deleted"),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Delete a project
pub async fn delete_project(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    actor.require(Resource::Project, Action::Delete, Some(id))?;

    ProjectService::delete(pool.get_ref(), id).await?;

    Ok(HttpResponse::NoContent().finish())
}

/// The server's per-project event limits (`MAX_EVENTS_PER_PROJECT_*`).
#[derive(Debug, serde::Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RateLimitsResponse {
    pub project_per_minute: i64,
    pub project_per_hour: i64,
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/rate-limits",
    operation_id = "projects.getRateLimits",
    tag = "Projects",
    responses(
        (status = 200, description = "The server's per-project limits", body = RateLimitsResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// What a project without limits of its own follows
pub async fn get_rate_limits(
    config: web::Data<Config>,
    _actor: ApiActor,
) -> AppResult<HttpResponse> {
    Ok(HttpResponse::Ok().json(RateLimitsResponse {
        project_per_minute: config.rate_limit.max_events_per_project_per_minute,
        project_per_hour: config.rate_limit.max_events_per_project_per_hour,
    }))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/api/rate-limits", web::get().to(get_rate_limits))
        .route("/api/projects", web::get().to(list_projects))
        .route("/api/projects", web::post().to(create_project));
}

pub fn configure_project(cfg: &mut web::ServiceConfig) {
    cfg.route("", web::get().to(get_project))
        .route("", web::patch().to(update_project))
        .route("", web::delete().to(delete_project));
}

#[cfg(feature = "openapi")]
#[derive(OpenApi)]
#[openapi(
    paths(
        list_projects,
        get_project,
        create_project,
        update_project,
        delete_project,
        get_rate_limits
    ),
    components(schemas(
        crate::models::ProjectResponse,
        RateLimitsResponse,
        crate::models::CreateProject,
        crate::models::UpdateProject,
        crate::error::ErrorResponse,
        crate::error::ErrorDetail,
    ))
)]
pub struct ProjectsApi;
