use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use super::{reply, NoArgs, RustrakMcp};
use crate::routes::health;

#[tool_router(router = health_tools, vis = "pub(super)")]
impl RustrakMcp {
    #[tool(
        description = "Get the Rustrak server version.",
        annotations(read_only_hint = true)
    )]
    async fn get_server_version(&self, Parameters(_): Parameters<NoArgs>) -> CallToolResult {
        reply(async { Ok(health::version(self.actor()).await) }).await
    }
}
