use actix_web::web;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use super::{reply, InProject, RustrakMcp};
use crate::routes::agents;

#[tool_router(router = agent_tools, vis = "pub(super)")]
impl RustrakMcp {
    #[tool(
        description = "Get a time-bucketed count of AI agent runs (gen_ai.operation.type:agent spans) for a Rustrak project — the \"Agent Runs\" AI Agent Monitoring widget.",
        annotations(read_only_hint = true)
    )]
    async fn get_agent_runs(
        &self,
        Parameters(a): Parameters<InProject<agents::AgentTimeseriesQuery>>,
    ) -> CallToolResult {
        reply(async {
            agents::agent_runs(
                self.pool(),
                web::Path::from(a.project_id),
                web::Query(a.rest),
                self.actor(),
            )
            .await
        })
        .await
    }

    #[tool(
        description = "Get time-bucketed avg/p95 duration for agent runs and LLM calls (gen_ai.operation.type:agent/ai_client spans) for a Rustrak project — the \"Duration\" AI Agent Monitoring widget.",
        annotations(read_only_hint = true)
    )]
    async fn get_agent_duration(
        &self,
        Parameters(a): Parameters<InProject<agents::AgentTimeseriesQuery>>,
    ) -> CallToolResult {
        reply(async {
            agents::agent_duration(
                self.pool(),
                web::Path::from(a.project_id),
                web::Query(a.rest),
                self.actor(),
            )
            .await
        })
        .await
    }

    #[tool(
        description = "Get the top LLM models by call count (gen_ai.operation.type:ai_client spans, grouped by response model) for a Rustrak project — the \"LLM Calls by Model\" AI Agent Monitoring widget.",
        annotations(read_only_hint = true)
    )]
    async fn get_agent_models_by_calls(
        &self,
        Parameters(a): Parameters<InProject<agents::AgentBreakdownQuery>>,
    ) -> CallToolResult {
        reply(async {
            agents::agent_models_calls(
                self.pool(),
                web::Path::from(a.project_id),
                web::Query(a.rest),
                self.actor(),
            )
            .await
        })
        .await
    }

    #[tool(
        description = "Get the top LLM models by total tokens used (gen_ai.operation.type:ai_client spans, grouped by response model) for a Rustrak project — the \"Tokens Used by Model\" AI Agent Monitoring widget.",
        annotations(read_only_hint = true)
    )]
    async fn get_agent_models_by_tokens(
        &self,
        Parameters(a): Parameters<InProject<agents::AgentBreakdownQuery>>,
    ) -> CallToolResult {
        reply(async {
            agents::agent_models_tokens(
                self.pool(),
                web::Path::from(a.project_id),
                web::Query(a.rest),
                self.actor(),
            )
            .await
        })
        .await
    }

    #[tool(
        description = "Get the top tools by call count (gen_ai.operation.type:tool spans, grouped by tool name) for a Rustrak project — the \"Tool Calls by Tool\" AI Agent Monitoring widget.",
        annotations(read_only_hint = true)
    )]
    async fn get_agent_tools(
        &self,
        Parameters(a): Parameters<InProject<agents::AgentBreakdownQuery>>,
    ) -> CallToolResult {
        reply(async {
            agents::agent_tools(
                self.pool(),
                web::Path::from(a.project_id),
                web::Query(a.rest),
                self.actor(),
            )
            .await
        })
        .await
    }

    #[tool(
        description = "List AI agent traces for a Rustrak project — one row per trace_id with its agents, duration, total tokens, LLM and tool call counts and error count, aggregated across every AI span in that trace (standalone or transaction-embedded). Offset-paginated, newest trace first unless sorted by another total. Narrow to the traces of one agent, model or tool, or to failing ones. This is the AI Agent Monitoring \"Traces\" table — use list_spans with trace_id to drill into one trace's individual spans.",
        annotations(read_only_hint = true)
    )]
    async fn list_agent_traces(
        &self,
        Parameters(a): Parameters<InProject<agents::AgentTracesQuery>>,
    ) -> CallToolResult {
        reply(async {
            agents::agent_traces(
                self.pool(),
                web::Path::from(a.project_id),
                web::Query(a.rest),
                self.actor(),
            )
            .await
        })
        .await
    }
}
