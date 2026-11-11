use actix_web::web;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::schemars::{self, JsonSchema};
use rmcp::{tool, tool_router};
use serde::Deserialize;

use super::{reply, uuid, InProject, RustrakMcp};
use crate::error::AppResult;
use crate::routes::transactions;

#[derive(Deserialize, JsonSchema)]
pub struct TransactionArgs {
    /// Project ID
    project_id: i32,
    /// Transaction ID (UUID)
    transaction_id: String,
}

impl TransactionArgs {
    fn path(&self) -> AppResult<web::Path<(i32, uuid::Uuid)>> {
        Ok(web::Path::from((
            self.project_id,
            uuid("transaction_id", &self.transaction_id)?,
        )))
    }
}

#[tool_router(router = transaction_tools, vis = "pub(super)")]
impl RustrakMcp {
    #[tool(
        description = "List performance transactions for a Rustrak project (newest first), with offset pagination and optional filters. Each transaction includes its name, duration in milliseconds, platform, environment, and release. Use name+op to list the samples of a single grouped transaction (see get_transaction_stats).",
        annotations(read_only_hint = true)
    )]
    async fn list_transactions(
        &self,
        Parameters(a): Parameters<InProject<transactions::ListTransactionsQuery>>,
    ) -> CallToolResult {
        reply(async {
            transactions::list_transactions(
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
        description = "Get a single Rustrak transaction by ID with its full Sentry payload: the span list (waterfall), trace context, measurements (web vitals), tags, request and user. Use the transaction id from list_transactions.",
        annotations(read_only_hint = true)
    )]
    async fn get_transaction(&self, Parameters(a): Parameters<TransactionArgs>) -> CallToolResult {
        reply(async { transactions::get_transaction(self.pool(), a.path()?, self.actor()).await })
            .await
    }

    #[tool(
        description = "Get aggregate performance stats for a Rustrak project, grouped by transaction name + operation: throughput (count), p50/p95/p99 latency, and failure rate. Offset-paginated, most frequent first. This is the performance overview — use it to find slow or failing transactions, then list_transactions with name+op to inspect samples.",
        annotations(read_only_hint = true)
    )]
    async fn get_transaction_stats(
        &self,
        Parameters(a): Parameters<InProject<transactions::TransactionStatsQuery>>,
    ) -> CallToolResult {
        reply(async {
            transactions::get_transaction_stats(
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
        description = "Get the indexed spans extracted from a Rustrak transaction, in waterfall order. Each span carries op, description, status, duration and self (exclusive) time — use it to find the slowest operations within a trace.",
        annotations(read_only_hint = true)
    )]
    async fn get_transaction_spans(
        &self,
        Parameters(a): Parameters<TransactionArgs>,
    ) -> CallToolResult {
        reply(async {
            transactions::get_transaction_spans(self.pool(), a.path()?, self.actor()).await
        })
        .await
    }
}
