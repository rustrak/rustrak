use actix_web::{web, HttpResponse};

use crate::auth::rbac::{Action, Resource};
use crate::auth::ApiActor;
use crate::db::DbPool;
use crate::error::AppResult;
#[cfg(feature = "openapi")]
use crate::models::{SpanDetailResponse, SpanResponse};
use crate::pagination::{ListSpansQuery, OffsetPaginatedResponse};
use crate::services::span::{SpanFilters, SpanService};
use uuid::Uuid;

#[cfg(feature = "openapi")]
use utoipa::OpenApi;

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/spans",
    operation_id = "spans.list",
    tag = "Spans",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ListSpansQuery,
    ),
    responses(
        (status = 200, description = "Paginated span list", body = inline(crate::pagination::OffsetPaginatedResponse<SpanResponse>)),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/projects/{project_id}/spans
/// Lists spans for a project with offset-based pagination (newest by
/// start_timestamp first), optionally filtered by op/status/trace_id.
/// Returns spans regardless of origin — standalone or extracted from a
/// transaction — since both are stored in the same table.
pub async fn list_spans(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<ListSpansQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();

    actor.require(Resource::Performance, Action::Read, Some(project_id))?;

    let page = query.page.max(1);
    let per_page = query.per_page.clamp(1, 100);

    let filters = SpanFilters {
        op: query.op.clone(),
        status: query.status.clone(),
        trace_id: query.trace_id.clone(),
        operation_type: query.operation_type.clone(),
    };

    let (spans, total_count) =
        SpanService::list_offset(pool.get_ref(), project_id, page, per_page, &filters).await?;

    Ok(HttpResponse::Ok().json(OffsetPaginatedResponse::new(
        spans,
        total_count,
        page,
        per_page,
    )))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/spans/{span_id}",
    operation_id = "spans.get",
    tag = "Spans",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("span_id" = Uuid, Path, description = "Span row ID (the `id` field of a span list item, not the SDK's 16-hex span_id)"),
    ),
    responses(
        (status = 200, description = "Span with its full attribute bag", body = SpanDetailResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/projects/{project_id}/spans/{span_id}
/// Returns one span together with its attributes and tags — the `gen_ai.*`
/// payload (prompts, responses, tool arguments and results) that the list
/// response deliberately omits, since `spans.data` is untrimmed and a whole
/// trace's worth of it would dwarf the waterfall it is drawn from.
pub async fn get_span(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid)>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, span_id) = path.into_inner();

    actor.require(Resource::Performance, Action::Read, Some(project_id))?;

    let span = SpanService::get_by_id(pool.get_ref(), project_id, span_id).await?;

    Ok(HttpResponse::Ok().json(span))
}

#[cfg(feature = "openapi")]
#[derive(OpenApi)]
#[openapi(paths(list_spans, get_span))]
pub struct SpansApi;

/// Configure span routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/projects/{project_id}/spans")
            .route("", web::get().to(list_spans))
            .route("/{span_id}", web::get().to(get_span)),
    );
}
