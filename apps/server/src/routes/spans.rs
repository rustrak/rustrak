use actix_web::{web, HttpResponse};

use crate::auth::ApiActor;
use crate::db::DbPool;
use crate::error::AppResult;
use crate::models::rbac::{Action, Resource};
#[cfg(feature = "openapi")]
use crate::models::{SpanDetailResponse, SpanResponse};
use crate::pagination::OffsetPaginatedResponse;
use crate::services::span::{SpanFilters, SpanService};
use uuid::Uuid;

#[cfg(feature = "openapi")]
use utoipa::OpenApi;

/// Query parameters for listing spans (offset-based)
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct ListSpansQuery {
    /// Page number (1-indexed, default: 1)
    #[serde(default = "crate::pagination::default_page")]
    #[cfg_attr(feature = "openapi", param(minimum = 1))]
    pub page: i64,

    /// Items per page (default: 20, max: 100)
    #[serde(default = "crate::pagination::default_per_page")]
    #[cfg_attr(feature = "openapi", param(minimum = 1, maximum = 100))]
    pub per_page: i64,

    /// Filter by span operation, e.g. `http.client`.
    pub op: Option<String>,

    /// Filter by span status.
    pub status: Option<String>,

    /// Filter by trace id — matches spans regardless of origin (standalone
    /// or extracted from a transaction), since both share this table.
    pub trace_id: Option<String>,

    /// Filter by gen_ai.operation.type
    /// (`agent`/`tool`/`handoff`/`ai_client`/`other`).
    pub operation_type: Option<String>,
}

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
    let per_page = crate::pagination::per_page(query.per_page)?;

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

pub fn configure_project(cfg: &mut web::ServiceConfig) {
    cfg.route("/spans", web::get().to(list_spans))
        .route("/spans/{span_id}", web::get().to(get_span));
}
