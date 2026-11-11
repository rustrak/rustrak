use actix_web::web;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::schemars::{self, JsonSchema};
use rmcp::{tool, tool_router};
use serde::Deserialize;

use super::{done, reply, InProject, ProjectArgs, RustrakMcp};
use crate::models::monitor::{CreateMonitor, UpdateMonitor};
use crate::routes::monitors;

#[derive(Deserialize, JsonSchema)]
pub struct MonitorArgs {
    /// Project ID
    project_id: i32,
    /// Monitor slug
    slug: String,
}

impl MonitorArgs {
    fn path(&self) -> web::Path<(i32, String)> {
        web::Path::from((self.project_id, self.slug.clone()))
    }
}

/// A monitor and the handler's own query or body for it.
#[derive(Deserialize, JsonSchema)]
pub struct OnMonitor<T> {
    #[serde(flatten)]
    monitor: MonitorArgs,
    #[serde(flatten)]
    rest: T,
}

#[tool_router(router = monitor_tools, vis = "pub(super)")]
impl RustrakMcp {
    #[tool(
        description = "List the cron monitors of a project (Sentry Crons): each scheduled job with its schedule, config, and per-environment status (active = waiting for a first ok run, ok, error = incident open) with its recent check-ins.",
        annotations(read_only_hint = true)
    )]
    async fn list_monitors(&self, Parameters(a): Parameters<ProjectArgs>) -> CallToolResult {
        reply(monitors::list_monitors(
            self.pool(),
            web::Path::from(a.project_id),
            self.actor(),
        ))
        .await
    }

    #[tool(
        description = "Get one cron monitor by slug: its config, environments, last and next expected check-in, and recent check-ins.",
        annotations(read_only_hint = true)
    )]
    async fn get_monitor(&self, Parameters(a): Parameters<MonitorArgs>) -> CallToolResult {
        reply(monitors::get_monitor(self.pool(), a.path(), self.actor())).await
    }

    #[tool(
        description = "List the check-ins of a cron monitor, newest first. status is ok, error, in_progress, missed, timeout or unknown; missed and timeout are set by Rustrak when a run did not report on time.",
        annotations(read_only_hint = true)
    )]
    async fn list_monitor_checkins(
        &self,
        Parameters(a): Parameters<OnMonitor<monitors::ListCheckInsQuery>>,
    ) -> CallToolResult {
        reply(async {
            monitors::list_checkins(
                self.pool(),
                a.monitor.path(),
                web::Query(a.rest),
                self.actor(),
            )
            .await
        })
        .await
    }

    #[tool(
        description = "Create a cron monitor. Its slug (derived from the name when omitted) is what the job checks in with. Most monitors are instead created by the job itself, by sending a monitor_config with its first check-in.",
        annotations(destructive_hint = false)
    )]
    async fn create_monitor(
        &self,
        Parameters(a): Parameters<InProject<CreateMonitor>>,
    ) -> CallToolResult {
        reply(async {
            monitors::create_monitor(
                self.pool(),
                web::Path::from(a.project_id),
                Ok(web::Json(a.rest)),
                self.actor(),
            )
            .await
        })
        .await
    }

    #[tool(
        description = "Rename, reconfigure, mute or unmute a cron monitor. Config fields left out keep their value. A muted monitor still records check-ins but opens no issue and sends no alert.",
        annotations(destructive_hint = false)
    )]
    async fn update_monitor(
        &self,
        Parameters(a): Parameters<OnMonitor<UpdateMonitor>>,
    ) -> CallToolResult {
        reply(async {
            monitors::update_monitor(
                self.pool(),
                a.monitor.path(),
                web::Json(a.rest),
                self.actor(),
            )
            .await
        })
        .await
    }

    #[tool(
        description = "Permanently delete a cron monitor and its check-ins. The issues it opened stay. A job that keeps checking in with a monitor_config creates it again.",
        annotations(destructive_hint = true)
    )]
    async fn delete_monitor(&self, Parameters(a): Parameters<MonitorArgs>) -> CallToolResult {
        done(
            monitors::delete_monitor(self.pool(), a.path(), self.actor()),
            format!("Monitor {} deleted.", a.slug),
        )
        .await
    }
}
