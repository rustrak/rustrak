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
use crate::models::rbac::{Action, Resource};
#[cfg(feature = "openapi")]
use crate::models::{
    AgentConversation, AgentConversationSummary, AgentDurationPoint, AgentModelRow, AgentSummary,
    AgentTimeseriesPoint, AgentToolRow, AgentTraceSummary, GenAiBreakdownRow,
};
use crate::pagination::OffsetPaginatedResponse;
use crate::services::span::{AgentFilters, SpanService};

#[cfg(feature = "openapi")]
use utoipa::OpenApi;

async fn require_view_access(_pool: &DbPool, project_id: i32, actor: &ApiActor) -> AppResult<()> {
    actor.require(Resource::Performance, Action::Read, Some(project_id))
}

/// Query parameters for AI Agent Monitoring time-series widgets (Agent Runs,
/// Estimated Cost, Duration).
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct AgentTimeseriesQuery {
    /// Lookback window in hours (default: all time, no filter).
    pub period_hours: Option<i64>,

    /// Restrict to spans reporting this environment (default: all).
    pub environment: Option<String>,

    /// Bucket width in hours (default: 1).
    #[serde(default = "default_interval_hours")]
    #[cfg_attr(feature = "openapi", param(minimum = 1))]
    pub interval_hours: i64,
}

fn default_interval_hours() -> i64 {
    1
}

/// Query parameters for AI Agent Monitoring breakdown widgets (LLM Calls by
/// Model, Tokens Used by Model, Tool Calls by Tool).
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct AgentBreakdownQuery {
    /// Lookback window in hours (default: all time, no filter).
    pub period_hours: Option<i64>,

    /// Restrict to spans reporting this environment (default: all).
    pub environment: Option<String>,

    /// Max rows returned (default: 3, matching Sentry's own widget cap).
    #[serde(default = "default_breakdown_limit")]
    #[cfg_attr(feature = "openapi", param(minimum = 1, maximum = 100))]
    pub limit: i64,
}

fn default_breakdown_limit() -> i64 {
    3
}

/// Query parameters for the agents-dashboard aggregates that return a whole
/// table rather than a top-N slice: summary, models, tools/stats.
///
/// Deliberately not `AgentBreakdownQuery`: that one carries `limit`, which
/// these endpoints have no way to honour, and reusing it published a
/// documented parameter they silently ignored.
#[derive(Debug, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct AgentWindowQuery {
    /// Lookback window in hours (default: all time, no filter).
    pub period_hours: Option<i64>,

    /// Restrict to spans reporting this environment (default: all).
    pub environment: Option<String>,
}

/// Query parameters for the AI Agent Monitoring Traces table (offset-based).
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct AgentTracesQuery {
    /// Lookback window in hours (default: all time, no filter). The dashboard
    /// applies one window to every widget, and this table is one of them.
    pub period_hours: Option<i64>,

    /// Restrict to spans reporting this environment (default: all).
    pub environment: Option<String>,

    /// Page number (1-indexed, default: 1)
    #[serde(default = "crate::pagination::default_page")]
    #[cfg_attr(feature = "openapi", param(minimum = 1))]
    pub page: i64,

    /// Items per page (default: 20, max: 100)
    #[serde(default = "crate::pagination::default_per_page")]
    #[cfg_attr(feature = "openapi", param(minimum = 1, maximum = 100))]
    pub per_page: i64,

    /// Only the traces where this agent ran.
    pub agent: Option<String>,

    /// Only the traces that called this model (response model, else request).
    pub model: Option<String>,

    /// Only the traces that used this tool.
    pub tool: Option<String>,

    /// Only the traces with a failing AI span.
    #[serde(default)]
    pub errors: bool,

    /// How to rank them; newest first by default.
    #[serde(default)]
    #[cfg_attr(feature = "openapi", param(inline))]
    pub sort: crate::services::span::AgentTraceSort,
}

impl AgentTracesQuery {
    pub fn trace_filters(&self) -> crate::services::span::AgentTraceFilters {
        let set = |v: &Option<String>| v.clone().filter(|s| !s.is_empty());
        crate::services::span::AgentTraceFilters {
            agent: set(&self.agent),
            model: set(&self.model),
            tool: set(&self.tool),
            errors: self.errors,
            sort: self.sort,
        }
    }
}

/// Query parameters for the agent conversations list (offset-based).
#[derive(Debug, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct AgentConversationsQuery {
    /// Lookback window in hours (default: all time, no filter).
    pub period_hours: Option<i64>,

    /// Restrict to spans reporting this environment (default: all).
    pub environment: Option<String>,

    /// Page number (1-indexed, default: 1)
    #[serde(default = "crate::pagination::default_page")]
    #[cfg_attr(feature = "openapi", param(minimum = 1))]
    pub page: i64,

    /// Items per page (default: 20, max: 100)
    #[serde(default = "crate::pagination::default_per_page")]
    #[cfg_attr(feature = "openapi", param(minimum = 1, maximum = 100))]
    pub per_page: i64,
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/agents/runs",
    operation_id = "agents.runs",
    tag = "Agents",
    params(("project_id" = i32, Path, description = "Project ID"), AgentTimeseriesQuery),
    responses(
        (status = 200, description = "Agent runs over time", body = Vec<AgentTimeseriesPoint>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
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
            environment: query.environment.clone(),
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
    operation_id = "agents.duration",
    tag = "Agents",
    params(("project_id" = i32, Path, description = "Project ID"), AgentTimeseriesQuery),
    responses(
        (status = 200, description = "Avg/p95 duration over time for agent runs and LLM calls", body = Vec<AgentDurationPoint>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
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
            environment: query.environment.clone(),
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
    operation_id = "agents.modelsCalls",
    tag = "Agents",
    params(("project_id" = i32, Path, description = "Project ID"), AgentBreakdownQuery),
    responses(
        (status = 200, description = "Top LLM call counts by response model", body = Vec<GenAiBreakdownRow>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
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
            environment: query.environment.clone(),
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
    operation_id = "agents.modelsTokens",
    tag = "Agents",
    params(("project_id" = i32, Path, description = "Project ID"), AgentBreakdownQuery),
    responses(
        (status = 200, description = "Top total tokens used by response model", body = Vec<GenAiBreakdownRow>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
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
            environment: query.environment.clone(),
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
    operation_id = "agents.tools",
    tag = "Agents",
    params(("project_id" = i32, Path, description = "Project ID"), AgentBreakdownQuery),
    responses(
        (status = 200, description = "Top tool call counts by tool name", body = Vec<GenAiBreakdownRow>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
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
            environment: query.environment.clone(),
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
    operation_id = "agents.traces",
    tag = "Agents",
    params(("project_id" = i32, Path, description = "Project ID"), AgentTracesQuery),
    responses(
        (status = 200, description = "Paginated agent traces", body = inline(crate::pagination::OffsetPaginatedResponse<AgentTraceSummary>)),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
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
    let per_page = crate::pagination::per_page(query.per_page)?;

    let filters = AgentFilters {
        environment: query.environment.clone(),
    };
    let (traces, total_count) = SpanService::agent_traces(
        pool.get_ref(),
        project_id,
        page,
        per_page,
        query.period_hours,
        &filters,
        &query.trace_filters(),
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
    operation_id = "agents.summary",
    tag = "Agents",
    params(("project_id" = i32, Path, description = "Project ID"), AgentWindowQuery),
    responses(
        (status = 200, description = "Headline totals for the selected window", body = AgentSummary),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
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
        environment: query.environment.clone(),
    };
    let summary =
        SpanService::agent_summary(pool.get_ref(), project_id, query.period_hours, &filters)
            .await?;

    Ok(HttpResponse::Ok().json(summary))
}

/// Lists the AI models a project's agents called, with requests, tokens and latency.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/agents/models",
    operation_id = "agents.modelsTable",
    tag = "Agents",
    params(("project_id" = i32, Path, description = "Project ID"), AgentWindowQuery),
    responses(
        (status = 200, description = "Per-model volume, failures, latency and token split", body = Vec<AgentModelRow>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
pub async fn agent_models_table(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<AgentWindowQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();
    require_view_access(pool.get_ref(), project_id, &actor).await?;

    let filters = AgentFilters {
        environment: query.environment.clone(),
    };
    let rows =
        SpanService::models_table(pool.get_ref(), project_id, query.period_hours, &filters).await?;

    Ok(HttpResponse::Ok().json(rows))
}

/// Lists the tools a project's agents called, with calls, errors and latency.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/agents/tools/stats",
    operation_id = "agents.toolsTable",
    tag = "Agents",
    params(("project_id" = i32, Path, description = "Project ID"), AgentWindowQuery),
    responses(
        (status = 200, description = "Per-tool call volume, failures and latency", body = Vec<AgentToolRow>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
pub async fn agent_tools_table(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<AgentWindowQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();
    require_view_access(pool.get_ref(), project_id, &actor).await?;

    let filters = AgentFilters {
        environment: query.environment.clone(),
    };
    let rows =
        SpanService::tools_table(pool.get_ref(), project_id, query.period_hours, &filters).await?;

    Ok(HttpResponse::Ok().json(rows))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/agents/environments",
    operation_id = "agents.environments",
    tag = "Agents",
    params(("project_id" = i32, Path, description = "Project ID")),
    responses(
        (status = 200, description = "Environments present in this project's AI spans", body = Vec<String>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
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

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/agents/conversations",
    operation_id = "agents.conversations",
    tag = "Agents",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        AgentConversationsQuery,
    ),
    responses(
        (status = 200, description = "Paginated agent conversations", body = inline(crate::pagination::OffsetPaginatedResponse<AgentConversationSummary>)),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Lists conversations -- traces sharing a `gen_ai.conversation.id` -- the most recently active first.
pub async fn agent_conversations(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<AgentConversationsQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();
    require_view_access(pool.get_ref(), project_id, &actor).await?;

    let page = query.page.max(1);
    let per_page = crate::pagination::per_page(query.per_page)?;
    let filters = AgentFilters {
        environment: query.environment.clone(),
    };
    let (conversations, total_count) = SpanService::agent_conversations(
        pool.get_ref(),
        project_id,
        page,
        per_page,
        query.period_hours,
        &filters,
    )
    .await?;

    Ok(HttpResponse::Ok().json(OffsetPaginatedResponse::new(
        conversations,
        total_count,
        page,
        per_page,
    )))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/agents/conversations/{conversation_id}",
    operation_id = "agents.conversation",
    tag = "Agents",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("conversation_id" = String, Path, description = "The gen_ai.conversation.id"),
    ),
    responses(
        (status = 200, description = "The conversation's AI spans across its turns", body = AgentConversation),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Returns every AI span of a conversation's turns, oldest first, with its input and output.
pub async fn agent_conversation(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, String)>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, conversation_id) = path.into_inner();
    require_view_access(pool.get_ref(), project_id, &actor).await?;
    let conversation =
        SpanService::agent_conversation(pool.get_ref(), project_id, &conversation_id).await?;
    Ok(HttpResponse::Ok().json(conversation))
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
    agent_conversations,
    agent_conversation,
))]
pub struct AgentsApi;

pub fn configure_project(cfg: &mut web::ServiceConfig) {
    cfg.route("/agents/runs", web::get().to(agent_runs))
        .route("/agents/duration", web::get().to(agent_duration))
        .route("/agents/models/calls", web::get().to(agent_models_calls))
        .route("/agents/models/tokens", web::get().to(agent_models_tokens))
        .route("/agents/tools", web::get().to(agent_tools))
        .route("/agents/tools/stats", web::get().to(agent_tools_table))
        .route("/agents/traces", web::get().to(agent_traces))
        .route("/agents/summary", web::get().to(agent_summary))
        .route("/agents/models", web::get().to(agent_models_table))
        .route("/agents/environments", web::get().to(agent_environments))
        .route("/agents/conversations", web::get().to(agent_conversations))
        .route(
            "/agents/conversations/{conversation_id}",
            web::get().to(agent_conversation),
        );
}
