use actix_web::web;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use super::{reply, InProject, RustrakMcp};
use crate::routes::stats;

#[tool_router(router = stats_tools, vis = "pub(super)")]
impl RustrakMcp {
    #[tool(
        description = "Get error-event volume over time for a Rustrak project, bucketed by severity (fatal, error, warning, info). Use this to see when a project started breaking, spot spikes, and compare severity mix across a window. Unlike get_issue_stats, which covers a single issue, this spans the whole project. Buckets are zero-filled, so a quiet bucket comes back with total 0 rather than being absent.",
        annotations(read_only_hint = true)
    )]
    async fn get_error_volume(
        &self,
        Parameters(a): Parameters<InProject<stats::EventTimeseriesQuery>>,
    ) -> CallToolResult {
        reply(async {
            stats::get_event_timeseries(
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
        description = "Get headline counters for a Rustrak project over a time window: error events, new issues, and currently open issues. Each windowed counter also carries the same figure for the window immediately before it, so you can state whether things are getting better or worse rather than just quoting a number. `previous` is null for all-time requests, where there is no earlier window to compare against.",
        annotations(read_only_hint = true)
    )]
    async fn get_project_stats(
        &self,
        Parameters(a): Parameters<InProject<stats::StatsSummaryQuery>>,
    ) -> CallToolResult {
        reply(async {
            stats::get_stats_summary(
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
