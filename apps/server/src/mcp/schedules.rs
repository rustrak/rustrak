use actix_web::web;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use super::{reply, RustrakMcp};
use crate::routes::schedules;

#[tool_router(router = schedule_tools, vis = "pub(super)")]
impl RustrakMcp {
    #[tool(
        description = "Validate a schedule (crontab or interval) and list its next five runs, using the parser Rustrak runs it with. Use it before saving a cron monitor or the automatic cleanup schedule.",
        annotations(read_only_hint = true)
    )]
    async fn preview_schedule(
        &self,
        Parameters(a): Parameters<schedules::PreviewSchedule>,
    ) -> CallToolResult {
        reply(schedules::preview_schedule(Ok(web::Json(a)), self.actor())).await
    }
}
