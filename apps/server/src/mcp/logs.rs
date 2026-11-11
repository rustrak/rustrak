use actix_web::web;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use super::{reply, InProject, RustrakMcp};
use crate::routes::logs;

#[tool_router(router = log_tools, vis = "pub(super)")]
impl RustrakMcp {
    #[tool(
        description = "List standalone logs for a Rustrak project (newest first by log timestamp), with offset pagination and optional filters. Each log includes its level, body (message), trace_id/span_id for correlation, severity number, and arbitrary attributes. Filter by level (trace/debug/info/warn/error/fatal), by trace_id to see all logs for one trace, by text in the message, or by a window like 24h.",
        annotations(read_only_hint = true)
    )]
    async fn list_logs(
        &self,
        Parameters(a): Parameters<InProject<logs::ListLogsQuery>>,
    ) -> CallToolResult {
        reply(async {
            logs::list_logs(
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
