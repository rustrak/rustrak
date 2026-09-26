use actix_web::{web, HttpResponse};

use crate::auth::ApiActor;
use crate::config::Config;
use crate::db::DbPool;
use crate::error::AppResult;
#[cfg(feature = "openapi")]
use crate::models::ProjectResponse;
use crate::models::{CreateProject, ProjectRole, UpdateProject};
use crate::pagination::{ListProjectsQuery, OffsetPaginatedResponse};
use crate::routes::period::parse_period_hours;
use crate::services::access::{self, Action};
use crate::services::stats::StatsService;
use crate::services::{ProjectMemberService, ProjectService};

#[cfg(feature = "openapi")]
use utoipa::OpenApi;

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects",
    tag = "Projects",
    params(ListProjectsQuery),
    responses(
        (status = 200, description = "List of projects", body = inline(crate::pagination::OffsetPaginatedResponse<ProjectResponse>)),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/projects - List projects with pagination
///
/// Admins see every project. Non-admins see only the projects they belong to.
pub async fn list_projects(
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    query: web::Query<ListProjectsQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (projects, total_count) = if actor.is_admin() {
        ProjectService::list_offset(pool.get_ref(), query.order, query.page, query.per_page).await?
    } else {
        let uid = actor
            .user_id()
            .ok_or_else(|| crate::error::AppError::Unauthorized("Not authenticated".to_string()))?;
        let ids = ProjectMemberService::accessible_project_ids(pool.get_ref(), uid).await?;
        ProjectService::list_offset_for_ids(
            pool.get_ref(),
            &ids,
            query.order,
            query.page,
            query.per_page,
        )
        .await?
    };

    let base_url = build_base_url(&config);
    let mut responses: Vec<_> = projects.iter().map(|p| p.to_response(&base_url)).collect();

    // Stats are attached to the page that was already fetched, so the extra
    // work scales with `per_page` (capped at 100) and not with the number of
    // projects on the instance.
    if let Some(period_hours) = parse_period_hours(query.stats_period.as_deref()) {
        let ids: Vec<i32> = responses.iter().map(|p| p.id).collect();
        let mut stats = StatsService::list_stats(pool.get_ref(), &ids, period_hours).await?;
        for response in &mut responses {
            response.stats = stats.remove(&response.id);
        }
    }

    Ok(HttpResponse::Ok().json(OffsetPaginatedResponse::new(
        responses,
        total_count,
        query.page,
        query.per_page,
    )))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{id}",
    tag = "Projects",
    params(("id" = i32, Path, description = "Project ID")),
    responses(
        (status = 200, description = "Project details", body = ProjectResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/projects/{id} - Get a project by ID
pub async fn get_project(
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    path: web::Path<i32>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    access::require(
        pool.get_ref(),
        actor.is_admin(),
        actor.user_id(),
        id,
        Action::ViewProject,
    )
    .await?;

    let project = ProjectService::get_by_id(pool.get_ref(), id).await?;
    let base_url = build_base_url(&config);

    Ok(HttpResponse::Ok().json(project.to_response(&base_url)))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/projects",
    tag = "Projects",
    request_body = CreateProject,
    responses(
        (status = 201, description = "Project created", body = ProjectResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 409, description = "Conflict", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// POST /api/projects - Create a new project
///
/// Any authenticated user may create a project. A non-admin creator is
/// automatically granted the project `admin` role so they can access it.
pub async fn create_project(
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    body: web::Json<CreateProject>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project = ProjectService::create(pool.get_ref(), body.into_inner()).await?;

    if !actor.is_admin() {
        if let Some(uid) = actor.user_id() {
            ProjectMemberService::upsert(pool.get_ref(), project.id, uid, ProjectRole::Admin)
                .await?;
        }
    }

    let base_url = build_base_url(&config);

    Ok(HttpResponse::Created().json(project.to_response(&base_url)))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    patch,
    path = "/api/projects/{id}",
    tag = "Projects",
    params(("id" = i32, Path, description = "Project ID")),
    request_body = UpdateProject,
    responses(
        (status = 200, description = "Project updated", body = ProjectResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
        (status = 409, description = "Conflict", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// PATCH /api/projects/{id} - Update a project
pub async fn update_project(
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    path: web::Path<i32>,
    body: web::Json<UpdateProject>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    access::require(
        pool.get_ref(),
        actor.is_admin(),
        actor.user_id(),
        id,
        Action::UpdateProject,
    )
    .await?;

    let project = ProjectService::update(pool.get_ref(), id, body.into_inner()).await?;
    let base_url = build_base_url(&config);

    Ok(HttpResponse::Ok().json(project.to_response(&base_url)))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/api/projects/{id}",
    tag = "Projects",
    params(("id" = i32, Path, description = "Project ID")),
    responses(
        (status = 204, description = "Project deleted"),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// DELETE /api/projects/{id} - Delete a project
pub async fn delete_project(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    access::require(
        pool.get_ref(),
        actor.is_admin(),
        actor.user_id(),
        id,
        Action::DeleteProject,
    )
    .await?;

    ProjectService::delete(pool.get_ref(), id).await?;

    Ok(HttpResponse::NoContent().finish())
}

/// Configure project routes
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
    tag = "Projects",
    responses(
        (status = 200, description = "The server's per-project limits", body = RateLimitsResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/rate-limits - What a project without limits of its own follows
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
    cfg.route("/api/rate-limits", web::get().to(get_rate_limits));
    cfg.service(
        web::scope("/api/projects")
            .route("", web::get().to(list_projects))
            .route("", web::post().to(create_project))
            .route("/{id}", web::get().to(get_project))
            .route("/{id}", web::patch().to(update_project))
            .route("/{id}", web::delete().to(delete_project)),
    );
}

/// Build base URL from config
fn build_base_url(config: &Config) -> String {
    config
        .public_url
        .clone()
        .unwrap_or_else(|| format!("http://{}:{}", config.host, config.port))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{DashboardConfig, DatabaseConfig, RateLimitConfig, SecurityConfig};
    use std::time::Duration;

    fn make_config(public_url: Option<String>, host: &str, port: u16) -> Config {
        Config {
            host: host.to_string(),
            port,
            public_url,
            database: DatabaseConfig {
                url: "postgres://test:test@localhost/test".to_string(),
                max_connections: 10,
                min_connections: 1,
                acquire_timeout: Duration::from_secs(5),
                idle_timeout: Duration::from_secs(600),
                max_lifetime: Duration::from_secs(1800),
            },
            rate_limit: RateLimitConfig {
                max_events_per_minute: 1000,
                max_events_per_hour: 10000,
                max_events_per_project_per_minute: 500,
                max_events_per_project_per_hour: 5000,
            },
            security: SecurityConfig {
                ssl_proxy: false,
                session_secret_key: None,
            },
            ingest_dir: None,
            sourcemap_storage_path: "/tmp/test_sourcemaps".to_string(),
            sourcemap_cache_bytes: 64 * 1024 * 1024,
            max_chunk_size_bytes: 10 * 1024 * 1024,
            session_flush_interval_secs: 30,
            session_cardinality_cap: 10_000,
            dashboard: DashboardConfig {
                dir: "./static".to_string(),
                enabled: true,
                url: None,
            },
            telemetry: crate::config::TelemetryConfig {
                enabled: false,
                do_not_track: false,
            },
        }
    }

    #[test]
    fn test_build_base_url_uses_public_url_when_set() {
        let config = make_config(Some("https://api.example.com".to_string()), "0.0.0.0", 8080);
        let result = build_base_url(&config);
        assert_eq!(result, "https://api.example.com");
    }

    #[test]
    fn test_build_base_url_fallback_when_no_public_url() {
        let config = make_config(None, "127.0.0.1", 9090);
        let result = build_base_url(&config);
        assert_eq!(result, "http://127.0.0.1:9090");
    }
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
