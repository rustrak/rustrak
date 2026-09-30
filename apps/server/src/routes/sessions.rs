use actix_web::{web, HttpResponse};

use crate::auth::ApiActor;
use crate::db::DbPool;
use crate::error::AppResult;
use crate::pagination::OffsetPaginatedResponse;
use crate::routes::period::parse_period_hours;
use crate::services::access::{self, Action};
use crate::services::session::SessionService;

#[cfg(feature = "openapi")]
use crate::models::session::{ReleaseHealthRow, SessionSummary, SessionTimeseriesPoint};
#[cfg(feature = "openapi")]
use utoipa::OpenApi;

/// Query params for the stats endpoint.
#[derive(serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct StatsQuery {
    /// Time window in hours (default: 24).
    pub period: Option<String>,
    /// Scope to a single release (all environments). When omitted, every
    /// release in the project is returned.
    pub release: Option<String>,
    pub environment: Option<String>,
    /// Page number (1-indexed, default: 1)
    #[serde(default = "default_page")]
    #[cfg_attr(feature = "openapi", param(minimum = 1))]
    pub page: i64,
    /// Items per page (default: 20, max: 100)
    #[serde(default = "default_per_page")]
    #[cfg_attr(feature = "openapi", param(minimum = 1, maximum = 100))]
    pub per_page: i64,
}

fn default_page() -> i64 {
    1
}

fn default_per_page() -> i64 {
    crate::pagination::PAGE_SIZE
}

impl StatsQuery {
    pub fn period_hours(&self) -> Option<i64> {
        parse_period_hours(self.period.as_deref())
    }
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/sessions/stats",
    tag = "Sessions",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        StatsQuery,
    ),
    responses(
        (status = 200, description = "Paginated per-release health stats", body = inline(crate::pagination::OffsetPaginatedResponse<ReleaseHealthRow>)),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/projects/{project_id}/sessions/stats
pub async fn get_stats(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<StatsQuery>,
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

    let period_hours = query.period_hours();
    let page = query.page.max(1);
    let per_page = query.per_page.clamp(1, 100);

    let (rows, total) = match query.release.as_deref() {
        Some(release) => {
            SessionService::release_health_for_release(
                pool.get_ref(),
                project_id,
                release,
                period_hours,
                page,
                per_page,
                query.environment.as_deref().filter(|s| !s.is_empty()),
            )
            .await?
        }
        None => {
            SessionService::release_health(
                pool.get_ref(),
                project_id,
                period_hours,
                page,
                per_page,
                query.environment.as_deref().filter(|s| !s.is_empty()),
            )
            .await?
        }
    };

    Ok(HttpResponse::Ok().json(OffsetPaginatedResponse::new(rows, total, page, per_page)))
}

/// Query params for the summary endpoint. Only `period` — unlike `StatsQuery`,
/// this endpoint has no release scoping, so it doesn't accept `release` at all
/// rather than silently ignoring it.
#[derive(serde::Deserialize)]
pub struct SummaryQuery {
    /// Time window in hours (default: 24).
    pub period: Option<String>,
    pub environment: Option<String>,
}

impl SummaryQuery {
    fn period_hours(&self) -> Option<i64> {
        parse_period_hours(self.period.as_deref())
    }
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/sessions/summary",
    tag = "Sessions",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("period" = Option<String>, Query, description = "Time window, e.g. '24h', '7d' (default: 24h)"),
    ),
    responses(
        (status = 200, description = "Project-wide session health summary", body = SessionSummary),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/projects/{project_id}/sessions/summary
pub async fn get_summary(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<SummaryQuery>,
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

    let period_hours = query.period_hours();
    let summary = SessionService::project_summary(
        pool.get_ref(),
        project_id,
        period_hours,
        query.environment.as_deref().filter(|s| !s.is_empty()),
    )
    .await?;

    Ok(HttpResponse::Ok().json(summary))
}

/// Query params for the timeseries endpoint.
#[derive(serde::Deserialize)]
pub struct TimeseriesQuery {
    /// Time window in hours (default: 24).
    pub period: Option<String>,
    /// Bucket width in hours (default: 1, clamped to 1-24).
    pub interval: Option<i64>,
    pub environment: Option<String>,
}

impl TimeseriesQuery {
    fn period_hours(&self) -> Option<i64> {
        parse_period_hours(self.period.as_deref())
    }

    fn interval_hours(&self) -> i64 {
        self.interval.unwrap_or(1).clamp(1, 24)
    }
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/sessions/timeseries",
    tag = "Sessions",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("period" = Option<String>, Query, description = "Time window, e.g. '24h', '7d' (default: 24h)"),
        ("interval" = Option<i64>, Query, description = "Bucket width in hours (default: 1, max: 24)"),
    ),
    responses(
        (status = 200, description = "Time-bucketed session trend, project-wide", body = Vec<SessionTimeseriesPoint>),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/projects/{project_id}/sessions/timeseries
pub async fn get_timeseries(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<TimeseriesQuery>,
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

    let points = SessionService::session_timeseries(
        pool.get_ref(),
        project_id,
        query.period_hours(),
        query.interval_hours(),
        query.environment.as_deref().filter(|s| !s.is_empty()),
    )
    .await?;

    Ok(HttpResponse::Ok().json(points))
}

#[cfg(feature = "openapi")]
#[derive(OpenApi)]
#[openapi(
    paths(get_stats, get_summary, get_timeseries),
    components(schemas(ReleaseHealthRow, SessionSummary, SessionTimeseriesPoint))
)]
pub struct SessionsApi;

/// Configure session routes.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/projects/{project_id}/sessions")
            .route("/stats", web::get().to(get_stats))
            .route("/summary", web::get().to(get_summary))
            .route("/timeseries", web::get().to(get_timeseries)),
    );
}
