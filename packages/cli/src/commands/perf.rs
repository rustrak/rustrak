//! Transaction latency and failure statistics.

use crate::api::{paths, Page, Transaction, TransactionStat};
use crate::cli::PerfCommand;
use crate::commands::Context;
use crate::error::{Error, Result};
use crate::output::{one_line, relative, truncate};
use serde_json::json;

pub async fn run(command: PerfCommand, ctx: &Context) -> Result<()> {
    ctx.require_auth()?;
    let project = ctx.project_id(None).await?;
    match command {
        PerfCommand::List(a) => {
            if a.page <= 0 || a.limit <= 0 {
                return Err(Error::Invalid(
                    "page and limit must be positive".to_string(),
                ));
            }
            let query = [
                ("page", a.page.to_string()),
                ("per_page", a.limit.to_string()),
                ("name", a.name.unwrap_or_default()),
                ("op", a.op.unwrap_or_default()),
                ("environment", a.environment.unwrap_or_default()),
                ("period", a.period.unwrap_or_default()),
            ];
            let page: Page<Transaction> = ctx
                .client
                .get(&paths::transactions(project), &query)
                .await?;
            if ctx.output.is_json() {
                let items: Vec<_> = page.items.iter().map(|t| json!({
                    "id": t.id, "event_id": t.event_id, "transaction_name": t.transaction_name,
                    "timestamp": t.timestamp, "duration_ms": t.duration_ms, "platform": t.platform,
                    "environment": t.environment, "release": t.release, "status": t.status
                })).collect();
                return ctx.output.json(&json!({
                    "items": items, "total_count": page.total_count, "page": page.page,
                    "per_page": page.per_page, "total_pages": page.total_pages,
                    "total_capped": page.total_capped
                }));
            }
            let mut table =
                ctx.output
                    .table(&["TIME", "DURATION", "STATUS", "ENV", "TRANSACTION NAME"]);
            for t in &page.items {
                table.add_row([
                    relative(&t.timestamp),
                    t.duration_ms
                        .map(|v| format!("{v:.1} ms"))
                        .unwrap_or_else(|| "-".into()),
                    one_line(t.status.as_deref().unwrap_or("-")),
                    one_line(&t.environment),
                    truncate(&one_line(&t.transaction_name), 60),
                ]);
            }
            println!("{table}");
            if page.total_capped {
                ctx.output.line(format!(
                    "showing first {} (total capped); {} rows on page {}.",
                    page.total_count,
                    page.items.len(),
                    a.page
                ));
            } else {
                ctx.output.line(format!(
                    "Showing {} of {} transactions (page {}).",
                    page.items.len(),
                    page.total_count,
                    a.page
                ));
            }
        }
        PerfCommand::Stats(a) => {
            if a.limit <= 0 {
                return Err(Error::Invalid("limit must be positive".to_string()));
            }
            let query = [
                ("page", "1".to_string()),
                ("per_page", a.limit.to_string()),
                ("environment", a.environment.unwrap_or_default()),
                ("period", a.period.unwrap_or_default()),
            ];
            let mut page: Page<TransactionStat> = ctx
                .client
                .get(&paths::transaction_stats(project), &query)
                .await?;
            page.items.sort_by(|a, b| b.p95_ms.total_cmp(&a.p95_ms));
            if ctx.output.is_json() {
                let items: Vec<_> = page
                    .items
                    .iter()
                    .map(|t| {
                        json!({
                            "transaction_name": t.transaction_name, "op": t.op, "count": t.count,
                            "p50_ms": t.p50_ms, "p95_ms": t.p95_ms, "p99_ms": t.p99_ms,
                            "avg_ms": t.avg_ms, "failure_rate": t.failure_rate,
                            "time_share": t.time_share, "last_seen": t.last_seen
                        })
                    })
                    .collect();
                return ctx.output.json(&json!({
                    "items": items, "total_count": page.total_count, "page": page.page,
                    "per_page": page.per_page, "total_pages": page.total_pages,
                    "total_capped": page.total_capped
                }));
            }
            let mut table = ctx.output.table(&[
                "NAME",
                "OP",
                "COUNT",
                "P50",
                "P95",
                "P99",
                "FAIL%",
                "SHARE",
                "LAST SEEN",
            ]);
            for t in &page.items {
                table.add_row([
                    one_line(&t.transaction_name),
                    one_line(t.op.as_deref().unwrap_or("-")),
                    t.count.to_string(),
                    format!("{:.1} ms", t.p50_ms),
                    format!("{:.1} ms", t.p95_ms),
                    format!("{:.1} ms", t.p99_ms),
                    format!("{:.1}%", t.failure_rate * 100.0),
                    format!("{:.1}%", t.time_share * 100.0),
                    relative(&t.last_seen),
                ]);
            }
            println!("{table}");
        }
    }
    Ok(())
}
