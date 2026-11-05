use actix_web::web;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::schemars::{self, JsonSchema};
use rmcp::{tool, tool_router};
use serde::{Deserialize, Serialize};

use super::{args, refuse, reply, NoArgs, RustrakMcp};
use crate::routes::storage;

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct CleanupArgs {
    /// Delete data older than this many days
    older_than_days: i64,
    /// Scope to one project (omit for all projects)
    #[serde(skip_serializing_if = "Option::is_none")]
    project_id: Option<i32>,
    /// Include error events (and the issues they empty). Defaults to true.
    #[serde(skip_serializing_if = "Option::is_none")]
    include_events: Option<bool>,
    /// Include transactions and their cascaded spans. Defaults to true.
    #[serde(skip_serializing_if = "Option::is_none")]
    include_transactions: Option<bool>,
    /// Include logs. Defaults to true.
    #[serde(skip_serializing_if = "Option::is_none")]
    include_logs: Option<bool>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ExecuteCleanupArgs {
    #[serde(flatten)]
    cleanup: CleanupArgs,
    /// Must be true to run this destructive cleanup
    confirm: Option<bool>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ConfirmArgs {
    /// Must be true to run this destructive cleanup
    confirm: Option<bool>,
}

/// Days a kind of data is kept; null keeps it forever (1-36500).
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct WindowsArg {
    /// Error events, and the issues they empty
    events_days: Option<i64>,
    /// Transactions with their spans
    transactions_days: Option<i64>,
    /// Logs
    logs_days: Option<i64>,
}

impl WindowsArg {
    fn deletes(&self) -> bool {
        self.events_days.is_some() || self.transactions_days.is_some() || self.logs_days.is_some()
    }
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct OverrideArg {
    project_id: i32,
    #[serde(flatten)]
    windows: WindowsArg,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct RetentionArgs {
    /// How long every project keeps each kind; null is forever
    defaults: WindowsArg,
    /// Projects kept for other windows; each replaces the defaults whole
    overrides: Vec<OverrideArg>,
    /// How often it runs: daily or weekly
    frequency: String,
    /// Hour it runs, UTC (0-23)
    hour: u8,
    /// Day of a weekly run, 0 = Sunday (0-6)
    weekday: u8,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct UpdateRetentionArgs {
    #[serde(flatten)]
    settings: RetentionArgs,
    /// Must be true to save settings that delete anything
    confirm: Option<bool>,
}

#[tool_router(router = storage_tools, vis = "pub(super)")]
impl RustrakMcp {
    #[tool(
        description = "Get the instance-wide Rustrak storage summary (needs storage:read): total database size, row counts for events, transactions, spans and logs, and the exact source-map weight. Use this to see what is consuming storage before cleaning up.",
        annotations(read_only_hint = true)
    )]
    async fn get_storage_summary(&self, Parameters(_): Parameters<NoArgs>) -> CallToolResult {
        reply(async { storage::get_summary(self.pool(), self.cleanup_job()?, self.actor()).await })
            .await
    }

    #[tool(
        description = "Get the per-project Rustrak storage breakdown (needs storage:read): event/transaction/span/source-map counts and estimated bytes for every project. Use this to find which project is accumulating the most data.",
        annotations(read_only_hint = true)
    )]
    async fn get_storage_by_project(&self, Parameters(_): Parameters<NoArgs>) -> CallToolResult {
        reply(async { storage::get_projects(self.pool(), self.cleanup_job()?, self.actor()).await })
            .await
    }

    #[tool(
        description = "Dry-run a Rustrak retention cleanup (needs storage:read): report how many events, transactions, spans, logs and issues would be removed if data older than `older_than_days` were deleted, optionally scoped to one project and to specific data categories. Mutates nothing — always run this before execute_storage_cleanup.",
        annotations(read_only_hint = true)
    )]
    async fn preview_storage_cleanup(
        &self,
        Parameters(a): Parameters<CleanupArgs>,
    ) -> CallToolResult {
        reply(async {
            storage::preview_cleanup(self.pool(), self.actor(), web::Json(args(&a)?)).await
        })
        .await
    }

    #[tool(
        description = "DESTRUCTIVE (needs storage:delete): permanently delete Rustrak data older than `older_than_days` (optionally scoped to one project and to specific data categories) and remove the issues it leaves with zero events. This cannot be undone. You MUST set confirm=true to proceed; without it the tool refuses and returns an error. Always run preview_storage_cleanup first and show the user the counts before confirming. The cleanup runs in the background: this returns the started job at once, and get_storage_cleanup_status reports its progress and final counts. Only one cleanup runs at a time; starting another while one runs is refused with a conflict.",
        annotations(destructive_hint = true)
    )]
    async fn execute_storage_cleanup(
        &self,
        Parameters(a): Parameters<ExecuteCleanupArgs>,
    ) -> CallToolResult {
        if a.confirm != Some(true) {
            return refuse("execute_storage_cleanup is destructive and was not confirmed. Run preview_storage_cleanup first, then call again with confirm=true to proceed.");
        }
        reply(async {
            storage::execute_cleanup(
                self.pool(),
                self.cleanup_job()?,
                self.actor(),
                web::Json(args(&a.cleanup)?),
            )
            .await
        })
        .await
    }

    #[tool(
        description = "Get the progress of the running Rustrak storage cleanup, or the outcome of the last one (needs storage:read): its state (idle, running, completed, failed), the rows removed so far, and when it started and finished. Use it after execute_storage_cleanup to tell the user when the deletion is done.",
        annotations(read_only_hint = true)
    )]
    async fn get_storage_cleanup_status(
        &self,
        Parameters(_): Parameters<NoArgs>,
    ) -> CallToolResult {
        reply(async { storage::get_cleanup_status(self.cleanup_job()?, self.actor()).await }).await
    }

    #[tool(
        description = "Dry-run a Rustrak source-map garbage collection (needs storage:read): report how many orphaned source-map files would be removed and how many bytes reclaimed, without deleting anything. Mutates nothing — always run this before gc_storage_source_maps.",
        annotations(read_only_hint = true)
    )]
    async fn preview_storage_source_maps_gc(
        &self,
        Parameters(_): Parameters<NoArgs>,
    ) -> CallToolResult {
        reply(storage::preview_source_map_gc(self.pool(), self.actor())).await
    }

    #[tool(
        description = "DESTRUCTIVE (needs storage:delete): permanently remove orphaned Rustrak source-map files no longer referenced by any upload (e.g. left behind by deleted projects), from the database and disk. This cannot be undone. You MUST set confirm=true to proceed; without it the tool refuses and returns an error. Always run preview_storage_source_maps_gc first and show the user the counts before confirming.",
        annotations(destructive_hint = true)
    )]
    async fn gc_storage_source_maps(
        &self,
        Parameters(a): Parameters<ConfirmArgs>,
    ) -> CallToolResult {
        if a.confirm != Some(true) {
            return refuse("gc_storage_source_maps is destructive and was not confirmed. Run preview_storage_source_maps_gc first, then call again with confirm=true to proceed.");
        }
        reply(async {
            storage::gc_source_maps(
                self.pool(),
                self.cleanup_job()?,
                self.sourcemap_store()?,
                self.actor(),
            )
            .await
        })
        .await
    }

    #[tool(
        description = "Get the storage retention (needs storage:read): how many days each kind of data (error events, transactions with their spans, logs) is kept by default and per project (null keeps it forever), when the cleanup that enforces it runs (daily or weekly at an hour, UTC), when it runs next and how its last run went.",
        annotations(read_only_hint = true)
    )]
    async fn get_retention(&self, Parameters(_): Parameters<NoArgs>) -> CallToolResult {
        reply(async {
            storage::get_retention(self.pool(), self.cleanup_job()?, self.actor()).await
        })
        .await
    }

    #[tool(
        description = "Dry-run retention settings (needs storage:read): the rows enforcing them now would permanently delete, each project against its own windows. Saves and deletes nothing. Run it before update_retention and show the user the counts.",
        annotations(read_only_hint = true)
    )]
    async fn preview_retention(&self, Parameters(a): Parameters<RetentionArgs>) -> CallToolResult {
        reply(async {
            storage::preview_retention(self.pool(), self.actor(), web::Json(args(&a)?)).await
        })
        .await
    }

    #[tool(
        description = "DESTRUCTIVE (needs storage:delete): enforce the saved retention now instead of waiting for the schedule. Permanently deletes every project's data older than its retention window, and the source maps no upload uses. You MUST set confirm=true to proceed; without it the tool refuses. Run preview_retention with the saved settings first and show the user what goes. It runs in the background: this returns the started job, and get_storage_cleanup_status reports its progress.",
        annotations(destructive_hint = true)
    )]
    async fn run_retention(&self, Parameters(a): Parameters<ConfirmArgs>) -> CallToolResult {
        if a.confirm != Some(true) {
            return refuse("run_retention is destructive and was not confirmed. Run preview_retention, show the user what goes, then call again with confirm=true to proceed.");
        }
        reply(async {
            storage::run_retention(
                self.pool(),
                self.cleanup_job()?,
                self.sourcemap_store()?,
                self.actor(),
            )
            .await
        })
        .await
    }

    #[tool(
        description = "Replace the storage retention settings (needs storage:update). Every field is sent. DESTRUCTIVE whenever any window is set: the cleanup then permanently deletes data older than it on every run. You MUST set confirm=true to save such settings; without it the tool refuses. Keeping everything forever needs no confirmation. Run preview_retention first and show the user what goes.",
        annotations(destructive_hint = true)
    )]
    async fn update_retention(
        &self,
        Parameters(a): Parameters<UpdateRetentionArgs>,
    ) -> CallToolResult {
        let deletes = a.settings.defaults.deletes()
            || a.settings.overrides.iter().any(|o| o.windows.deletes());
        if deletes && a.confirm != Some(true) {
            return refuse("update_retention with a retention window schedules permanent deletion and was not confirmed. Run preview_retention, show the user what goes, then call again with confirm=true to proceed.");
        }
        reply(async {
            storage::update_retention(
                self.pool(),
                self.cleanup_job()?,
                self.actor(),
                web::Json(args(&a.settings)?),
            )
            .await
        })
        .await
    }
}
