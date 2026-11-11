use actix_web::{web, HttpResponse};

use crate::auth::ApiActor;
use crate::db::DbPool;
use crate::error::AppResult;
use crate::models::rbac::{Action, Resource};
use crate::pagination::OffsetPaginatedResponse;
use crate::routes::period::parse_period_hours;
use crate::services::scope::Scope;
use crate::services::session::SessionService;

#[cfg(feature = "openapi")]
use crate::models::session::{ReleaseHealthRow, SessionSummary, SessionTimeseriesPoint};
#[cfg(feature = "openapi")]
use utoipa::OpenApi;

/// Query params for the stats endpoint.
#[derive(serde::Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct StatsQuery {
    /// Time window in hours (default: 24).
    pub period: Option<String>,
    /// Scope to a single release (all environments). When omitted, every
    /// release in the project is returned.
    pub release: Option<String>,
    /// Page number (1-indexed, default: 1)
    #[serde(default = "crate::pagination::default_page")]
    #[cfg_attr(feature = "openapi", param(minimum = 1))]
    pub page: i64,
    /// Items per page (default: 20, max: 100)
    #[serde(default = "crate::pagination::default_per_page")]
    #[cfg_attr(feature = "openapi", param(minimum = 1, maximum = 100))]
    pub per_page: i64,
}

impl StatsQuery {
    pub fn period_hours(&self) -> Option<i64> {
        parse_period_hours(self.period.as_deref())
    }
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/sessions/stats",
    operation_id = "sessions.getStats",
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
pub async fn get_stats(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<StatsQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();

    actor.require(Resource::Sessions, Action::Read, Some(project_id))?;

    let period_hours = query.period_hours();
    let page = query.page.max(1);
    let per_page = crate::pagination::per_page(query.per_page)?;

    let (rows, total) = match query.release.as_deref() {
        Some(release) => {
            SessionService::release_health_for_release(
                pool.get_ref(),
                project_id,
                release,
                period_hours,
                page,
                per_page,
            )
            .await?
        }
        None => {
            SessionService::release_health(pool.get_ref(), project_id, period_hours, page, per_page)
                .await?
        }
    };

    Ok(HttpResponse::Ok().json(OffsetPaginatedResponse::new(rows, total, page, per_page)))
}

/// Query params for the summary endpoint.
#[derive(serde::Deserialize)]
pub struct SummaryQuery {
    /// Time window in hours (default: 24).
    pub period: Option<String>,
    /// Only sessions from this environment.
    pub environment: Option<String>,
    /// Only sessions from this release.
    pub release: Option<String>,
}

impl SummaryQuery {
    fn period_hours(&self) -> Option<i64> {
        parse_period_hours(self.period.as_deref())
    }
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/sessions/summary",
    operation_id = "sessions.getSummary",
    tag = "Sessions",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("period" = Option<String>, Query, description = "Time window, e.g. '24h', '7d' (default: 24h)"),
        ("environment" = Option<String>, Query, description = "Only sessions from this environment. Omit for every environment."),
        ("release" = Option<String>, Query, description = "Only sessions from this release. Omit for every release."),
    ),
    responses(
        (status = 200, description = "Project-wide session health summary", body = SessionSummary),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer_auth" = [])),
))]
pub async fn get_summary(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<SummaryQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();

    actor.require(Resource::Sessions, Action::Read, Some(project_id))?;

    let period_hours = query.period_hours();
    let summary = SessionService::project_summary(
        pool.get_ref(),
        project_id,
        period_hours,
        Scope {
            environment: query.environment.as_deref(),
            release: query.release.as_deref(),
        },
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
    /// Only sessions from this environment.
    pub environment: Option<String>,
    /// Only sessions from this release.
    pub release: Option<String>,
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
    operation_id = "sessions.getTimeseries",
    tag = "Sessions",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("period" = Option<String>, Query, description = "Time window, e.g. '24h', '7d' (default: 24h)"),
        ("interval" = Option<i64>, Query, description = "Bucket width in hours (default: 1, max: 24)"),
        ("environment" = Option<String>, Query, description = "Only sessions from this environment. Omit for every environment."),
        ("release" = Option<String>, Query, description = "Only sessions from this release. Omit for every release."),
    ),
    responses(
        (status = 200, description = "Time-bucketed session trend, project-wide", body = Vec<SessionTimeseriesPoint>),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer_auth" = [])),
))]
pub async fn get_timeseries(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<TimeseriesQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();

    actor.require(Resource::Sessions, Action::Read, Some(project_id))?;

    let points = SessionService::session_timeseries(
        pool.get_ref(),
        project_id,
        query.period_hours(),
        query.interval_hours(),
        Scope {
            environment: query.environment.as_deref(),
            release: query.release.as_deref(),
        },
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

pub fn configure_project(cfg: &mut web::ServiceConfig) {
    cfg.route("/sessions/stats", web::get().to(get_stats))
        .route("/sessions/summary", web::get().to(get_summary))
        .route("/sessions/timeseries", web::get().to(get_timeseries));
}
