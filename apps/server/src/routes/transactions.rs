use actix_web::{web, HttpResponse};
use uuid::Uuid;

use crate::auth::ApiActor;
use crate::db::DbPool;
use crate::error::AppResult;
use crate::models::rbac::{Action, Resource};
#[cfg(feature = "openapi")]
use crate::models::{TransactionDetailResponse, TransactionResponse};
use crate::pagination::OffsetPaginatedResponse;
use crate::services::TransactionService;

#[cfg(feature = "openapi")]
use utoipa::OpenApi;

/// Query parameters for listing transactions (offset-based)
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct ListTransactionsQuery {
    /// Page number (1-indexed, default: 1)
    #[serde(default = "crate::pagination::default_page")]
    #[cfg_attr(feature = "openapi", param(minimum = 1))]
    pub page: i64,

    /// Items per page (default: 20, max: 100)
    #[serde(default = "crate::pagination::default_per_page")]
    #[cfg_attr(feature = "openapi", param(minimum = 1, maximum = 100))]
    pub per_page: i64,

    /// Filter by exact transaction name (lists one group's samples).
    pub name: Option<String>,

    /// Filter by trace operation (contexts.trace.op), e.g. `http.server`.
    pub op: Option<String>,

    /// Filter by trace status (contexts.trace.status), e.g. `ok`.
    pub status: Option<String>,

    /// Filter by environment.
    pub environment: Option<String>,

    /// Filter by release.
    pub release: Option<String>,

    /// Only this window (`24h`, `7d`, `14d`...); all time if absent.
    pub period: Option<String>,

    /// `slowest` for the longest first; newest first otherwise.
    #[cfg_attr(feature = "openapi", param(inline))]
    pub sort: Option<TransactionListSort>,
}

/// The order of a transaction list.
#[derive(Debug, Clone, Copy, serde::Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum TransactionListSort {
    Recent,
    Slowest,
}

/// Query parameters for the transaction stats overview (offset-based)
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct TransactionStatsQuery {
    /// Page number (1-indexed, default: 1)
    #[serde(default = "crate::pagination::default_page")]
    #[cfg_attr(feature = "openapi", param(minimum = 1))]
    pub page: i64,

    /// Items per page (default: 20, max: 100)
    #[serde(default = "crate::pagination::default_per_page")]
    #[cfg_attr(feature = "openapi", param(minimum = 1, maximum = 100))]
    pub per_page: i64,

    /// Only this environment.
    pub environment: Option<String>,

    /// Only this window (`24h`, `7d`, `14d`...); all time if absent.
    pub period: Option<String>,

    /// Only the endpoints whose name contains this, ignoring case.
    pub query: Option<String>,

    /// How to rank them; most time spent in all by default.
    #[serde(default)]
    #[cfg_attr(feature = "openapi", param(inline))]
    pub sort: crate::services::transaction::StatsSort,
}

impl TransactionStatsQuery {
    pub fn filter(&self) -> crate::services::transaction::StatsFilter {
        crate::services::transaction::StatsFilter {
            environment: self.environment.clone().filter(|e| !e.is_empty()),
            period_hours: crate::routes::period::parse_period_hours(self.period.as_deref()),
            query: self.query.clone(),
            sort: self.sort,
        }
    }
}

/// Query parameters for a single transaction group's aggregate stats.
#[derive(Debug, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct TransactionStatGroupQuery {
    /// Exact transaction name of the group.
    pub name: String,
    /// Trace operation of the group (omit for groups with no op).
    pub op: Option<String>,
    /// Only this environment.
    pub environment: Option<String>,
    /// Only this window (`24h`, `7d`, `14d`...); all time if absent.
    pub period: Option<String>,
}

impl TransactionStatGroupQuery {
    pub fn filter(&self) -> crate::services::transaction::StatsFilter {
        crate::services::transaction::StatsFilter {
            environment: self.environment.clone().filter(|e| !e.is_empty()),
            period_hours: crate::routes::period::parse_period_hours(self.period.as_deref()),
            ..Default::default()
        }
    }
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/transactions",
    operation_id = "transactions.list",
    tag = "Transactions",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ListTransactionsQuery,
    ),
    responses(
        (status = 200, description = "Paginated transaction list", body = inline(crate::pagination::OffsetPaginatedResponse<TransactionResponse>)),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Lists transactions for a project with offset-based pagination (newest first).
pub async fn list_transactions(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<ListTransactionsQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();

    actor.require(Resource::Performance, Action::Read, Some(project_id))?;

    let page = query.page.max(1);
    let per_page = crate::pagination::per_page(query.per_page)?;

    let filters = crate::services::TransactionFilters {
        name: query.name.clone(),
        op: query.op.clone(),
        status: query.status.clone(),
        environment: query.environment.clone(),
        release: query.release.clone(),
        period_hours: crate::routes::period::parse_period_hours(query.period.as_deref()),
        slowest: query.sort == Some(TransactionListSort::Slowest),
    };

    let (transactions, total_count) =
        TransactionService::list_offset(pool.get_ref(), project_id, page, per_page, &filters)
            .await?;

    Ok(HttpResponse::Ok().json(OffsetPaginatedResponse::new(
        transactions,
        total_count,
        page,
        per_page,
    )))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/transactions/{transaction_id}",
    operation_id = "transactions.get",
    tag = "Transactions",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("transaction_id" = uuid::Uuid, Path, description = "Transaction ID"),
    ),
    responses(
        (status = 200, description = "Full transaction detail", body = TransactionDetailResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Returns a single transaction with its full Sentry payload (spans, contexts,
/// measurements, tags) for the performance detail view.
pub async fn get_transaction(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid)>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, transaction_id) = path.into_inner();

    actor.require(Resource::Performance, Action::Read, Some(project_id))?;

    let transaction =
        TransactionService::get_by_id(pool.get_ref(), project_id, transaction_id).await?;

    Ok(HttpResponse::Ok().json(transaction))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/transactions/{transaction_id}/spans",
    operation_id = "transactions.getSpans",
    tag = "Transactions",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("transaction_id" = uuid::Uuid, Path, description = "Transaction ID"),
    ),
    responses(
        (status = 200, description = "Indexed spans for the transaction (waterfall order)", body = inline(Vec<crate::models::SpanResponse>)),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Returns the indexed spans extracted from a transaction, ordered for the
/// waterfall view (by start_timestamp ascending).
pub async fn get_transaction_spans(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid)>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, transaction_id) = path.into_inner();

    actor.require(Resource::Performance, Action::Read, Some(project_id))?;

    let spans = TransactionService::list_spans(pool.get_ref(), project_id, transaction_id).await?;

    Ok(HttpResponse::Ok().json(spans))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/transactions/stats",
    operation_id = "transactions.getStats",
    tag = "Transactions",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        TransactionStatsQuery,
    ),
    responses(
        (status = 200, description = "Paginated aggregate latency/throughput/failure stats per (transaction, op)", body = inline(crate::pagination::OffsetPaginatedResponse<crate::models::TransactionStatsResponse>)),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Returns paginated aggregate performance stats (count, time spent, p50/p95/p99
/// latency, failure rate) grouped by transaction name + op, the most time spent
/// first unless `sort` asks otherwise.
pub async fn get_transaction_stats(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<TransactionStatsQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();

    actor.require(Resource::Performance, Action::Read, Some(project_id))?;

    let page = query.page.max(1);
    let per_page = crate::pagination::per_page(query.per_page)?;

    let (stats, total) =
        TransactionService::stats(pool.get_ref(), project_id, page, per_page, &query.filter())
            .await?;

    Ok(HttpResponse::Ok().json(OffsetPaginatedResponse::new(stats, total, page, per_page)))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/transactions/stats/group",
    operation_id = "transactions.getStatGroup",
    tag = "Transactions",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        TransactionStatGroupQuery,
    ),
    responses(
        (status = 200, description = "Aggregate stats for one (transaction, op) group", body = crate::models::TransactionStatsResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Group not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Returns the aggregate stats for a single transaction group — a direct lookup
/// so the summary header works regardless of how many groups the project has.
pub async fn get_transaction_stat_group(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<TransactionStatGroupQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();

    actor.require(Resource::Performance, Action::Read, Some(project_id))?;

    let stat = TransactionService::stats_for_group(
        pool.get_ref(),
        project_id,
        &query.name,
        query.op.as_deref(),
        &query.filter(),
    )
    .await?
    .ok_or_else(|| {
        crate::error::AppError::NotFound(format!("No stats for transaction '{}'", query.name))
    })?;

    Ok(HttpResponse::Ok().json(stat))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/transactions/stats/group/timeseries",
    operation_id = "transactions.getStatGroupTimeseries",
    tag = "Transactions",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        TransactionStatGroupQuery,
    ),
    responses(
        (status = 200, description = "The endpoint's runs, failures, p50 and p95 per bucket, oldest first", body = Vec<crate::models::TransactionTimeseriesPoint>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Returns one endpoint's trend over the window, every bucket present:
/// how often it ran and failed, and its p50 and p95. Without a window, back
/// to its oldest call and never less than the last 24 hours, which is also
/// what an endpoint with no calls gets.
pub async fn get_transaction_stat_group_timeseries(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<TransactionStatGroupQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();
    actor.require(Resource::Performance, Action::Read, Some(project_id))?;

    let filter = query.filter();
    let points = TransactionService::group_timeseries(
        pool.get_ref(),
        project_id,
        &query.name,
        query.op.as_deref(),
        filter.environment.as_deref(),
        filter.period_hours,
    )
    .await?;
    Ok(HttpResponse::Ok().json(points))
}

#[cfg(feature = "openapi")]
#[derive(OpenApi)]
#[openapi(
    paths(
        list_transactions,
        get_transaction,
        get_transaction_spans,
        get_transaction_stats,
        get_transaction_stat_group,
        get_transaction_stat_group_timeseries
    ),
    components(schemas(
        TransactionResponse,
        TransactionDetailResponse,
        crate::models::SpanResponse,
        crate::models::TransactionStatsResponse,
        crate::models::TransactionTimeseriesPoint,
    ))
)]
pub struct TransactionsApi;

pub fn configure_project(cfg: &mut web::ServiceConfig) {
    cfg.route("/transactions", web::get().to(list_transactions))
        // `/stats` must precede `/{transaction_id}` so it isn't captured as an id.
        .route("/transactions/stats", web::get().to(get_transaction_stats))
        .route(
            "/transactions/stats/group",
            web::get().to(get_transaction_stat_group),
        )
        .route(
            "/transactions/stats/group/timeseries",
            web::get().to(get_transaction_stat_group_timeseries),
        )
        .route(
            "/transactions/{transaction_id}",
            web::get().to(get_transaction),
        )
        .route(
            "/transactions/{transaction_id}/spans",
            web::get().to(get_transaction_spans),
        );
}
