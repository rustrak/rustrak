use actix_web::web;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use super::{reply, InProject, RustrakMcp};
use crate::routes::sessions;

#[tool_router(router = session_tools, vis = "pub(super)")]
impl RustrakMcp {
    #[tool(
        description = "Get per-release health stats for a Rustrak project: crash-free session rate, crash-free user rate, and error/crash/abnormal counts. Paginated, ordered by session volume.",
        annotations(read_only_hint = true)
    )]
    async fn get_release_health(
        &self,
        Parameters(a): Parameters<InProject<sessions::StatsQuery>>,
    ) -> CallToolResult {
        reply(async {
            sessions::get_stats(
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
