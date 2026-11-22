use actix_web::{web, HttpResponse};

use crate::auth::ApiActor;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::rbac::{Action, Resource};
use crate::pagination::OffsetPaginatedResponse;
use crate::services::web_vitals::{Breakdown, Vital, WebVitalsFilter, WebVitalsService};

/// How many causes and samples a vital's panel lists.
const LISTED: i64 = 10;

/// Which vitals a Web Vitals read is over.
#[derive(Debug, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct WebVitalsQuery {
    /// Only this environment.
    pub environment: Option<String>,
    /// Only this browser, as the summary's `browsers` names it.
    pub browser: Option<String>,
    /// Only phones (`mobile`) or only computers (`desktop`).
    pub device: Option<String>,
    /// Only this page.
    pub page: Option<String>,
    /// Only this vital: the pages ranked by it, its causes, its samples.
    pub vital: Option<Vital>,
    /// Only this window (`24h`, `7d`, `14d`...): 90 days at most, and 90 days
    /// if absent.
    pub period: Option<String>,
}

impl WebVitalsQuery {
    fn filter(&self) -> AppResult<WebVitalsFilter> {
        let given = |value: &Option<String>| value.clone().filter(|v| !v.is_empty());
        let device = given(&self.device);
        if device
            .as_deref()
            .is_some_and(|d| !matches!(d, "mobile" | "desktop"))
        {
            return Err(AppError::Validation(
                "device must be mobile or desktop".to_string(),
            ));
        }
        Ok(WebVitalsFilter {
            environment: given(&self.environment),
            browser: given(&self.browser),
            device,
            page: given(&self.page),
            vital: self.vital,
            period_hours: crate::routes::period::window_hours(self.period.as_deref()),
        })
    }
}

/// [`WebVitalsQuery`] for one page of pages.
#[derive(Debug, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct WebVitalsPagesQuery {
    /// Page number (1-indexed, default: 1)
    #[serde(default = "crate::pagination::default_page")]
    #[cfg_attr(feature = "openapi", param(minimum = 1))]
    pub page: i64,
    /// Items per page (default: 20, max: 100)
    #[serde(default = "crate::pagination::default_per_page")]
    #[cfg_attr(feature = "openapi", param(minimum = 1, maximum = 100))]
    pub per_page: i64,
    pub environment: Option<String>,
    pub browser: Option<String>,
    pub device: Option<String>,
    /// Rank the pages by this vital's score alone.
    pub vital: Option<Vital>,
    pub period: Option<String>,
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/web-vitals",
    operation_id = "webVitals.getSummary",
    tag = "Web Vitals",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        WebVitalsQuery,
    ),
    responses(
        (status = 200, description = "The performance score and each vital's value, score and ratings", body = crate::models::web_vital::WebVitalsSummary),
        (status = 400, description = "An unknown device", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Returns the project's performance score over the window, or one page's,
/// with each vital's 75th percentile and the spread around it, average
/// score and how many of its measurements rate good, needing improvement
/// and poor.
pub async fn get_web_vitals_summary(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<WebVitalsQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();
    actor.require(Resource::Performance, Action::Read, Some(project_id))?;
    let summary = WebVitalsService::summary(pool.get_ref(), project_id, &query.filter()?).await?;
    Ok(HttpResponse::Ok().json(summary))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/web-vitals/pages",
    operation_id = "webVitals.listPages",
    tag = "Web Vitals",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        WebVitalsPagesQuery,
    ),
    responses(
        (status = 200, description = "Pages by the score they cost the project, the most first", body = inline(crate::pagination::OffsetPaginatedResponse<crate::models::web_vital::WebVitalsPage>)),
        (status = 400, description = "An unknown device", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Returns the pages with web vitals in the window, the one whose
/// improvement would raise the project's score the most first, or its
/// score for one vital when `vital` names it.
pub async fn list_web_vitals_pages(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<WebVitalsPagesQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();
    actor.require(Resource::Performance, Action::Read, Some(project_id))?;
    let page = query.page.max(1);
    let per_page = crate::pagination::per_page(query.per_page)?;
    let filter = WebVitalsQuery {
        environment: query.environment.clone(),
        browser: query.browser.clone(),
        device: query.device.clone(),
        page: None,
        vital: query.vital,
        period: query.period.clone(),
    }
    .filter()?;
    let (pages, total) = WebVitalsService::pages(
        pool.get_ref(),
        project_id,
        &filter,
        per_page,
        (page - 1) * per_page,
    )
    .await?;
    Ok(HttpResponse::Ok().json(OffsetPaginatedResponse::new(pages, total, page, per_page)))
}

/// [`WebVitalsQuery`] for one vital broken down by a dimension.
#[derive(Debug, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct WebVitalsBreakdownQuery {
    /// The vital to break down.
    pub vital: Option<Vital>,
    /// `device` or `browser`.
    #[cfg_attr(feature = "openapi", param(value_type = Option<Breakdown>))]
    pub by: Option<String>,
    pub environment: Option<String>,
    pub browser: Option<String>,
    pub device: Option<String>,
    pub page: Option<String>,
    pub period: Option<String>,
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/web-vitals/breakdown",
    operation_id = "webVitals.getBreakdown",
    tag = "Web Vitals",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        WebVitalsBreakdownQuery,
    ),
    responses(
        (status = 200, description = "The vital on each device or browser, the most measured first", body = Vec<crate::models::web_vital::WebVitalsBreakdownRow>),
        (status = 400, description = "No vital, an unknown dimension or an unknown device", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Returns one vital on each device or on each browser: where it is good
/// and where it is not, the most measured first.
pub async fn get_web_vitals_breakdown(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<WebVitalsBreakdownQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();
    actor.require(Resource::Performance, Action::Read, Some(project_id))?;
    let vital = query
        .vital
        .ok_or_else(|| AppError::Validation("vital is required".to_string()))?;
    let by = match query.by.as_deref() {
        Some("device") => Breakdown::Device,
        Some("browser") => Breakdown::Browser,
        _ => {
            return Err(AppError::Validation(
                "by must be device or browser".to_string(),
            ))
        }
    };
    let filter = WebVitalsQuery {
        environment: query.environment.clone(),
        browser: query.browser.clone(),
        device: query.device.clone(),
        page: query.page.clone(),
        vital: None,
        period: query.period.clone(),
    }
    .filter()?;
    let rows = WebVitalsService::breakdown(pool.get_ref(), project_id, &filter, vital, by).await?;
    Ok(HttpResponse::Ok().json(rows))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/web-vitals/targets",
    operation_id = "webVitals.listTargets",
    tag = "Web Vitals",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        WebVitalsQuery,
    ),
    responses(
        (status = 200, description = "The elements the vitals were about, the one losing the most score first", body = Vec<crate::models::web_vital::WebVitalTarget>),
        (status = 400, description = "An unknown device", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Returns what the window's vitals were about: an LCP's element and the
/// resource it loaded, a layout shift's element, an interaction's target.
/// The one losing the most score comes first, ten at most.
pub async fn list_web_vitals_targets(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<WebVitalsQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();
    actor.require(Resource::Performance, Action::Read, Some(project_id))?;
    let targets = WebVitalsService::targets(
        pool.get_ref(),
        project_id,
        &query.filter()?,
        LISTED as usize,
    )
    .await?;
    Ok(HttpResponse::Ok().json(targets))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/web-vitals/samples",
    operation_id = "webVitals.listSamples",
    tag = "Web Vitals",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        WebVitalsQuery,
    ),
    responses(
        (status = 200, description = "The worst measurements, the highest first", body = Vec<crate::models::web_vital::WebVitalSample>),
        (status = 400, description = "An unknown device", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Returns the window's worst measurements, ten at most, with where each
/// happened: the page and its URL, the element, the browser, the device,
/// the network and the page load's transaction.
pub async fn list_web_vitals_samples(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<WebVitalsQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();
    actor.require(Resource::Performance, Action::Read, Some(project_id))?;
    let samples =
        WebVitalsService::samples(pool.get_ref(), project_id, &query.filter()?, LISTED).await?;
    Ok(HttpResponse::Ok().json(samples))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/web-vitals/timeseries",
    operation_id = "webVitals.getTimeseries",
    tag = "Web Vitals",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        WebVitalsQuery,
    ),
    responses(
        (status = 200, description = "How the measurements rate per bucket, oldest first", body = Vec<crate::models::web_vital::WebVitalsPoint>),
        (status = 400, description = "An unknown device", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Returns how the window's measurements rate bucket by bucket, every
/// bucket present: how many were good, needed improvement and were poor.
/// One vital's when `vital` names it, as a trend is read.
pub async fn get_web_vitals_timeseries(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<WebVitalsQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();
    actor.require(Resource::Performance, Action::Read, Some(project_id))?;
    let points = WebVitalsService::timeseries(pool.get_ref(), project_id, &query.filter()?).await?;
    Ok(HttpResponse::Ok().json(points))
}

pub fn configure_project(cfg: &mut web::ServiceConfig) {
    cfg.route("/web-vitals", web::get().to(get_web_vitals_summary))
        .route("/web-vitals/pages", web::get().to(list_web_vitals_pages))
        .route(
            "/web-vitals/targets",
            web::get().to(list_web_vitals_targets),
        )
        .route(
            "/web-vitals/breakdown",
            web::get().to(get_web_vitals_breakdown),
        )
        .route(
            "/web-vitals/samples",
            web::get().to(list_web_vitals_samples),
        )
        .route(
            "/web-vitals/timeseries",
            web::get().to(get_web_vitals_timeseries),
        );
}
