use actix_web::{web, HttpResponse};

use crate::auth::ApiActor;
use crate::db::DbPool;
use crate::error::AppResult;
use crate::routes::period::parse_period_hours;
use crate::services::access::{self, Action};
use crate::services::stats::StatsService;

#[cfg(feature = "openapi")]
use crate::models::stats::{EventTimeseriesPoint, MetricDelta, ProjectStatsSummary};
#[cfg(feature = "openapi")]
use utoipa::OpenApi;

/// Query params for the event timeseries endpoint.
#[derive(serde::Deserialize)]
pub struct EventTimeseriesQuery {
    /// Time window, e.g. "24h" or "7d". Omit for all time.
    pub period: Option<String>,
    /// Bucket width in hours (default: 1, clamped to 1-24).
    pub interval: Option<i64>,
    pub environment: Option<String>,
}

impl EventTimeseriesQuery {
    fn period_hours(&self) -> Option<i64> {
        parse_period_hours(self.period.as_deref())
    }

    fn interval_hours(&self) -> i64 {
        self.interval.unwrap_or(1).clamp(1, 24)
    }
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/events/stats",
    tag = "Stats",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("period" = Option<String>, Query, description = "Time window, e.g. '24h', '7d'. Omit for all time."),
        ("interval" = Option<i64>, Query, description = "Bucket width in hours (default: 1, max: 24)"),
    ),
    responses(
        (status = 200, description = "Time-bucketed error-event volume by severity", body = Vec<EventTimeseriesPoint>),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/projects/{project_id}/events/stats
pub async fn get_event_timeseries(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<EventTimeseriesQuery>,
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

    let points = StatsService::event_timeseries(
        pool.get_ref(),
        project_id,
        query.period_hours(),
        query.interval_hours(),
        query.environment.as_deref().filter(|s| !s.is_empty()),
    )
    .await?;

    Ok(HttpResponse::Ok().json(points))
}

/// Query params for the project stats summary endpoint.
#[derive(serde::Deserialize)]
pub struct StatsSummaryQuery {
    /// Time window, e.g. "24h" or "7d". Omit for all time (no comparison).
    pub period: Option<String>,
    pub environment: Option<String>,
}

impl StatsSummaryQuery {
    fn period_hours(&self) -> Option<i64> {
        parse_period_hours(self.period.as_deref())
    }
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/stats/summary",
    tag = "Stats",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("period" = Option<String>, Query, description = "Time window, e.g. '24h', '7d'. Omit for all time."),
    ),
    responses(
        (status = 200, description = "Project counters with previous-period comparison", body = ProjectStatsSummary),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/projects/{project_id}/stats/summary
pub async fn get_stats_summary(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<StatsSummaryQuery>,
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

    let summary = StatsService::project_summary(
        pool.get_ref(),
        project_id,
        query.period_hours(),
        query.environment.as_deref().filter(|s| !s.is_empty()),
    )
    .await?;

    Ok(HttpResponse::Ok().json(summary))
}

#[cfg(feature = "openapi")]
#[derive(OpenApi)]
#[openapi(
    paths(get_event_timeseries, get_stats_summary),
    components(schemas(EventTimeseriesPoint, MetricDelta, ProjectStatsSummary))
)]
pub struct StatsApi;

/// Configure project stats routes.
///
/// `/events/stats` sits under its own `/events` scope rather than the existing
/// one in `routes::events`, which is nested under an issue
/// (`/issues/{issue_id}/events`) and so cannot host a project-level path.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/projects/{project_id}/events")
            .route("/stats", web::get().to(get_event_timeseries)),
    )
    .service(
        web::scope("/api/projects/{project_id}/stats")
            .route("/summary", web::get().to(get_stats_summary)),
    );
}
