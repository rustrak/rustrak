use actix_web::web;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use super::{reply, InProject, RustrakMcp};
use crate::routes::spans;

#[tool_router(router = span_tools, vis = "pub(super)")]
impl RustrakMcp {
    #[tool(
        description = "List spans for a Rustrak project (newest first), with offset pagination and optional filters. Covers spans regardless of origin — standalone \"span\" envelope items and spans extracted from transactions both share this table. Filter by operation_type (agent/tool/handoff/ai_client) to find AI Agent Monitoring spans specifically.",
        annotations(read_only_hint = true)
    )]
    async fn list_spans(
        &self,
        Parameters(a): Parameters<InProject<spans::ListSpansQuery>>,
    ) -> CallToolResult {
        reply(async {
            spans::list_spans(
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
