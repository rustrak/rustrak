use actix_web::{web, HttpResponse};

use crate::auth::ApiActor;
use crate::db::DbPool;
use crate::error::AppResult;
use crate::models::rbac::{Action, Resource};
use crate::routes::period::parse_period_hours;
use crate::services::scope::Scope;
use crate::services::stats::StatsService;

#[cfg(feature = "openapi")]
use crate::models::stats::{EventTimeseriesPoint, MetricDelta, ProjectStatsSummary};
#[cfg(feature = "openapi")]
use utoipa::OpenApi;

/// Query params for the event timeseries endpoint.
#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct EventTimeseriesQuery {
    /// Time window, e.g. "24h" or "7d". Omit for all time.
    pub period: Option<String>,
    /// Bucket width in hours (default: 1, clamped to 1-24).
    pub interval: Option<i64>,
    /// Only events from this environment.
    pub environment: Option<String>,
    /// Only events from this release.
    pub release: Option<String>,
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
    operation_id = "stats.getEventTimeseries",
    tag = "Stats",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("period" = Option<String>, Query, description = "Time window, e.g. '24h', '7d'. Omit for all time."),
        ("interval" = Option<i64>, Query, description = "Bucket width in hours (default: 1, max: 24)"),
        ("environment" = Option<String>, Query, description = "Only events from this environment. Omit for every environment."),
        ("release" = Option<String>, Query, description = "Only events from this release. Omit for every release."),
    ),
    responses(
        (status = 200, description = "Time-bucketed error-event volume by severity", body = Vec<EventTimeseriesPoint>),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer_auth" = [])),
))]
pub async fn get_event_timeseries(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<EventTimeseriesQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();

    actor.require(Resource::Issues, Action::Read, Some(project_id))?;

    let points = StatsService::event_timeseries(
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

/// Query params for the project stats summary endpoint.
#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct StatsSummaryQuery {
    /// Time window, e.g. "24h" or "7d". Omit for all time (no comparison).
    pub period: Option<String>,
    /// Only events, and issues seen, in this environment.
    pub environment: Option<String>,
    /// Only events, and issues seen, in this release.
    pub release: Option<String>,
}

impl StatsSummaryQuery {
    fn period_hours(&self) -> Option<i64> {
        parse_period_hours(self.period.as_deref())
    }
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/stats/summary",
    operation_id = "stats.getSummary",
    tag = "Stats",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("period" = Option<String>, Query, description = "Time window, e.g. '24h', '7d'. Omit for all time."),
        ("environment" = Option<String>, Query, description = "Only events, and issues seen, in this environment. Omit for every environment."),
        ("release" = Option<String>, Query, description = "Only events, and issues seen, in this release. Omit for every release."),
    ),
    responses(
        (status = 200, description = "Project counters with previous-period comparison", body = ProjectStatsSummary),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer_auth" = [])),
))]
pub async fn get_stats_summary(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<StatsSummaryQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();

    actor.require(Resource::Issues, Action::Read, Some(project_id))?;

    let summary = StatsService::project_summary(
        pool.get_ref(),
        project_id,
        query.period_hours(),
        Scope {
            environment: query.environment.as_deref(),
            release: query.release.as_deref(),
        },
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

///
/// `/events/stats` sits under its own `/events` scope rather than the existing
/// one in `routes::events`, which is nested under an issue
/// (`/issues/{issue_id}/events`) and so cannot host a project-level path.
pub fn configure_project(cfg: &mut web::ServiceConfig) {
    cfg.route("/events/stats", web::get().to(get_event_timeseries))
        .route("/stats/summary", web::get().to(get_stats_summary));
}
