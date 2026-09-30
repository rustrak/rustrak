//! AI Agent Monitoring dashboard API (story-ai-agent-monitoring.md, GH #180).
//!
//! Powers the 6 dashboard widgets: Agent Runs, Duration, LLM Calls by
//! Model, Tokens Used by Model, Tool Calls by Tool, Traces.
//!
//! Deliberately no cost/spend widget: an accurate per-model pricing table
//! across dozens of fast-changing models is more ongoing maintenance
//! (would need a release every time a provider updates pricing) than a
//! self-hosted, single-maintainer project can promise. Token counts (exact,
//! straight from the SDK) are shown instead.

use actix_web::{web, HttpResponse};

use crate::auth::ApiActor;
use crate::db::DbPool;
use crate::error::AppResult;
#[cfg(feature = "openapi")]
use crate::models::{
    AgentDurationPoint, AgentModelRow, AgentSummary, AgentTimeseriesPoint, AgentToolRow,
    AgentTraceSummary, GenAiBreakdownRow,
};
use crate::pagination::{
    AgentBreakdownQuery, AgentTimeseriesQuery, AgentTracesQuery, AgentWindowQuery,
    OffsetPaginatedResponse,
};
use crate::services::access::{self, Action};
use crate::services::span::{AgentFilters, SpanService};

#[cfg(feature = "openapi")]
use utoipa::OpenApi;

async fn require_view_access(pool: &DbPool, project_id: i32, actor: &ApiActor) -> AppResult<()> {
    access::require(
        pool,
        actor.is_admin(),
        actor.user_id(),
        project_id,
        Action::ViewProject,
    )
    .await
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/agents/runs",
    tag = "Agents",
    params(("project_id" = i32, Path, description = "Project ID"), AgentTimeseriesQuery),
    responses(
        (status = 200, description = "Agent runs over time", body = Vec<AgentTimeseriesPoint>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/projects/{project_id}/agents/runs
/// Time-bucketed count of agent-run spans (`gen_ai.operation.type:agent`).
pub async fn agent_runs(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<AgentTimeseriesQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();
    require_view_access(pool.get_ref(), project_id, &actor).await?;

    let points = SpanService::agent_runs_timeseries(
        pool.get_ref(),
        project_id,
        &AgentFilters {
            environment: query.environment.clone().filter(|name| !name.is_empty()),
        },
        query.period_hours,
        query.interval_hours,
    )
    .await?;

    Ok(HttpResponse::Ok().json(points))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/agents/duration",
    tag = "Agents",
    params(("project_id" = i32, Path, description = "Project ID"), AgentTimeseriesQuery),
    responses(
        (status = 200, description = "Avg/p95 duration over time for agent runs and LLM calls", body = Vec<AgentDurationPoint>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/projects/{project_id}/agents/duration
/// Time-bucketed avg/p95 duration for `agent`/`ai_client` spans.
pub async fn agent_duration(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<AgentTimeseriesQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();
    require_view_access(pool.get_ref(), project_id, &actor).await?;

    let points = SpanService::agent_duration_timeseries(
        pool.get_ref(),
        project_id,
        &AgentFilters {
            environment: query.environment.clone().filter(|name| !name.is_empty()),
        },
        query.period_hours,
        query.interval_hours,
    )
    .await?;

    Ok(HttpResponse::Ok().json(points))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/agents/models/calls",
    tag = "Agents",
    params(("project_id" = i32, Path, description = "Project ID"), AgentBreakdownQuery),
    responses(
        (status = 200, description = "Top LLM call counts by response model", body = Vec<GenAiBreakdownRow>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/projects/{project_id}/agents/models/calls
/// Top models by LLM call count (`gen_ai.operation.type:ai_client`).
pub async fn agent_models_calls(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<AgentBreakdownQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();
    require_view_access(pool.get_ref(), project_id, &actor).await?;

    let rows = SpanService::llm_calls_by_model(
        pool.get_ref(),
        project_id,
        &AgentFilters {
            environment: query.environment.clone().filter(|name| !name.is_empty()),
        },
        query.period_hours,
        query.limit,
    )
    .await?;

    Ok(HttpResponse::Ok().json(rows))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/agents/models/tokens",
    tag = "Agents",
    params(("project_id" = i32, Path, description = "Project ID"), AgentBreakdownQuery),
    responses(
        (status = 200, description = "Top total tokens used by response model", body = Vec<GenAiBreakdownRow>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/projects/{project_id}/agents/models/tokens
/// Top models by total tokens used (`gen_ai.operation.type:ai_client`).
pub async fn agent_models_tokens(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<AgentBreakdownQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();
    require_view_access(pool.get_ref(), project_id, &actor).await?;

    let rows = SpanService::tokens_by_model(
        pool.get_ref(),
        project_id,
        &AgentFilters {
            environment: query.environment.clone().filter(|name| !name.is_empty()),
        },
        query.period_hours,
        query.limit,
    )
    .await?;

    Ok(HttpResponse::Ok().json(rows))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/agents/tools",
    tag = "Agents",
    params(("project_id" = i32, Path, description = "Project ID"), AgentBreakdownQuery),
    responses(
        (status = 200, description = "Top tool call counts by tool name", body = Vec<GenAiBreakdownRow>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/projects/{project_id}/agents/tools
/// Top tools by call count (`gen_ai.operation.type:tool`).
pub async fn agent_tools(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<AgentBreakdownQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();
    require_view_access(pool.get_ref(), project_id, &actor).await?;

    let rows = SpanService::tool_calls_by_tool(
        pool.get_ref(),
        project_id,
        &AgentFilters {
            environment: query.environment.clone().filter(|name| !name.is_empty()),
        },
        query.period_hours,
        query.limit,
    )
    .await?;

    Ok(HttpResponse::Ok().json(rows))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/agents/traces",
    tag = "Agents",
    params(("project_id" = i32, Path, description = "Project ID"), AgentTracesQuery),
    responses(
        (status = 200, description = "Paginated agent traces", body = inline(crate::pagination::OffsetPaginatedResponse<AgentTraceSummary>)),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/projects/{project_id}/agents/traces
/// Paginated per-trace_id aggregate (duration, tokens, tool usage) across
/// all AI spans sharing that trace, regardless of origin.
pub async fn agent_traces(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<AgentTracesQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();
    require_view_access(pool.get_ref(), project_id, &actor).await?;

    let page = query.page.max(1);
    let per_page = query.per_page.clamp(1, 100);

    let filters = AgentFilters {
        environment: query.environment.clone().filter(|name| !name.is_empty()),
    };
    let (traces, total_count) = SpanService::agent_traces(
        pool.get_ref(),
        project_id,
        page,
        per_page,
        query.period_hours,
        &filters,
    )
    .await?;

    Ok(HttpResponse::Ok().json(OffsetPaginatedResponse::new(
        traces,
        total_count,
        page,
        per_page,
    )))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/agents/summary",
    tag = "Agents",
    params(("project_id" = i32, Path, description = "Project ID"), AgentWindowQuery),
    responses(
        (status = 200, description = "Headline totals for the selected window", body = AgentSummary),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/projects/{project_id}/agents/summary
/// Agent runs, LLM calls, tool calls, errors, tokens and latency over the
/// selected window — the numbers the six charts cannot state outright.
pub async fn agent_summary(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<AgentWindowQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();
    require_view_access(pool.get_ref(), project_id, &actor).await?;

    let filters = AgentFilters {
        environment: query.environment.clone().filter(|name| !name.is_empty()),
    };
    let summary =
        SpanService::agent_summary(pool.get_ref(), project_id, query.period_hours, &filters)
            .await?;

    Ok(HttpResponse::Ok().json(summary))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/agents/models",
    tag = "Agents",
    params(("project_id" = i32, Path, description = "Project ID"), AgentWindowQuery),
    responses(
        (status = 200, description = "Per-model volume, failures, latency and token split", body = Vec<AgentModelRow>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/projects/{project_id}/agents/models
pub async fn agent_models_table(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<AgentWindowQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();
    require_view_access(pool.get_ref(), project_id, &actor).await?;

    let filters = AgentFilters {
        environment: query.environment.clone().filter(|name| !name.is_empty()),
    };
    let rows =
        SpanService::models_table(pool.get_ref(), project_id, query.period_hours, &filters).await?;

    Ok(HttpResponse::Ok().json(rows))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/agents/tools/stats",
    tag = "Agents",
    params(("project_id" = i32, Path, description = "Project ID"), AgentWindowQuery),
    responses(
        (status = 200, description = "Per-tool call volume, failures and latency", body = Vec<AgentToolRow>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/projects/{project_id}/agents/tools/stats
pub async fn agent_tools_table(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<AgentWindowQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();
    require_view_access(pool.get_ref(), project_id, &actor).await?;

    let filters = AgentFilters {
        environment: query.environment.clone().filter(|name| !name.is_empty()),
    };
    let rows =
        SpanService::tools_table(pool.get_ref(), project_id, query.period_hours, &filters).await?;

    Ok(HttpResponse::Ok().json(rows))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/agents/environments",
    tag = "Agents",
    params(("project_id" = i32, Path, description = "Project ID")),
    responses(
        (status = 200, description = "Environments present in this project's AI spans", body = Vec<String>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/projects/{project_id}/agents/environments
/// The options the environment filter offers, read from the data rather than
/// hardcoded — an installation may name its environments anything.
pub async fn agent_environments(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();
    require_view_access(pool.get_ref(), project_id, &actor).await?;

    let envs = SpanService::agent_environments(pool.get_ref(), project_id).await?;

    Ok(HttpResponse::Ok().json(envs))
}

#[cfg(feature = "openapi")]
#[derive(OpenApi)]
#[openapi(paths(
    agent_runs,
    agent_duration,
    agent_models_calls,
    agent_models_tokens,
    agent_tools,
    agent_traces,
    agent_summary,
    agent_models_table,
    agent_tools_table,
    agent_environments,
))]
pub struct AgentsApi;

/// Configure AI Agent Monitoring dashboard routes.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/projects/{project_id}/agents")
            .route("/runs", web::get().to(agent_runs))
            .route("/duration", web::get().to(agent_duration))
            .route("/models/calls", web::get().to(agent_models_calls))
            .route("/models/tokens", web::get().to(agent_models_tokens))
            .route("/tools", web::get().to(agent_tools))
            .route("/tools/stats", web::get().to(agent_tools_table))
            .route("/traces", web::get().to(agent_traces))
            .route("/summary", web::get().to(agent_summary))
            .route("/models", web::get().to(agent_models_table))
            .route("/environments", web::get().to(agent_environments)),
    );
}
