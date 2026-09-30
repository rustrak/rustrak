use actix_web::{web, HttpResponse};
use uuid::Uuid;

use crate::auth::ApiActor;
use crate::db::DbPool;
use crate::error::AppResult;
#[cfg(feature = "openapi")]
use crate::models::{TransactionDetailResponse, TransactionResponse};
use crate::pagination::{ListTransactionsQuery, OffsetPaginatedResponse};
use crate::services::access::{self, Action};
use crate::services::TransactionService;

#[cfg(feature = "openapi")]
use utoipa::OpenApi;

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/transactions",
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
/// GET /api/projects/{project_id}/transactions
/// Lists transactions for a project with offset-based pagination (newest first).
pub async fn list_transactions(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<ListTransactionsQuery>,
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

    let page = query.page.max(1);
    let per_page = query.per_page.clamp(1, 100);

    let filters = crate::services::TransactionFilters {
        name: query.name.clone(),
        op: query.op.clone(),
        status: query.status.clone(),
        environment: query.environment.clone().filter(|name| !name.is_empty()),
        release: query.release.clone(),
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
/// GET /api/projects/{project_id}/transactions/{transaction_id}
/// Returns a single transaction with its full Sentry payload (spans, contexts,
/// measurements, tags) for the performance detail view.
pub async fn get_transaction(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid)>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, transaction_id) = path.into_inner();

    access::require(
        pool.get_ref(),
        actor.is_admin(),
        actor.user_id(),
        project_id,
        Action::ViewProject,
    )
    .await?;

    let transaction =
        TransactionService::get_by_id(pool.get_ref(), project_id, transaction_id).await?;

    Ok(HttpResponse::Ok().json(transaction))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/transactions/{transaction_id}/spans",
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
/// GET /api/projects/{project_id}/transactions/{transaction_id}/spans
/// Returns the indexed spans extracted from a transaction, ordered for the
/// waterfall view (by start_timestamp ascending).
pub async fn get_transaction_spans(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid)>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, transaction_id) = path.into_inner();

    access::require(
        pool.get_ref(),
        actor.is_admin(),
        actor.user_id(),
        project_id,
        Action::ViewProject,
    )
    .await?;

    let spans = TransactionService::list_spans(pool.get_ref(), project_id, transaction_id).await?;

    Ok(HttpResponse::Ok().json(spans))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/transactions/stats",
    tag = "Transactions",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        crate::pagination::TransactionStatsQuery,
    ),
    responses(
        (status = 200, description = "Paginated aggregate latency/throughput/failure stats per (transaction, op)", body = inline(crate::pagination::OffsetPaginatedResponse<crate::models::TransactionStatsResponse>)),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/projects/{project_id}/transactions/stats
/// Returns paginated aggregate performance stats (count, p50/p95/p99 latency,
/// failure rate) grouped by transaction name + op, most frequent first.
pub async fn get_transaction_stats(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<crate::pagination::TransactionStatsQuery>,
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

    let page = query.page.max(1);
    let per_page = query.per_page.clamp(1, 100);

    let (stats, total) = TransactionService::stats(
        pool.get_ref(),
        project_id,
        page,
        per_page,
        query.environment.as_deref().filter(|s| !s.is_empty()),
    )
    .await?;

    Ok(HttpResponse::Ok().json(OffsetPaginatedResponse::new(stats, total, page, per_page)))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/transactions/stats/group",
    tag = "Transactions",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        crate::pagination::TransactionStatGroupQuery,
    ),
    responses(
        (status = 200, description = "Aggregate stats for one (transaction, op) group", body = crate::models::TransactionStatsResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Group not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/projects/{project_id}/transactions/stats/group?name=&op=
/// Returns the aggregate stats for a single transaction group — a direct lookup
/// so the summary header works regardless of how many groups the project has.
pub async fn get_transaction_stat_group(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<crate::pagination::TransactionStatGroupQuery>,
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

    let stat = TransactionService::stats_for_group(
        pool.get_ref(),
        project_id,
        &query.name,
        query.op.as_deref(),
        query.environment.as_deref().filter(|s| !s.is_empty()),
    )
    .await?
    .ok_or_else(|| {
        crate::error::AppError::NotFound(format!("No stats for transaction '{}'", query.name))
    })?;

    Ok(HttpResponse::Ok().json(stat))
}

#[cfg(feature = "openapi")]
#[derive(OpenApi)]
#[openapi(
    paths(
        list_transactions,
        get_transaction,
        get_transaction_spans,
        get_transaction_stats,
        get_transaction_stat_group
    ),
    components(schemas(
        TransactionResponse,
        TransactionDetailResponse,
        crate::models::SpanResponse,
        crate::models::TransactionStatsResponse,
    ))
)]
pub struct TransactionsApi;

/// Configure transaction routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/projects/{project_id}/transactions")
            .route("", web::get().to(list_transactions))
            // `/stats*` must precede `/{transaction_id}` so they aren't captured as ids.
            .route("/stats", web::get().to(get_transaction_stats))
            .route("/stats/group", web::get().to(get_transaction_stat_group))
            .route("/{transaction_id}", web::get().to(get_transaction))
            .route(
                "/{transaction_id}/spans",
                web::get().to(get_transaction_spans),
            ),
    );
}
