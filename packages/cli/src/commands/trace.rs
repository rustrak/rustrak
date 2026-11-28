//! Agent activity, traces, and call statistics.

use crate::api::{paths, AgentModel, AgentSummary, AgentTool, AgentTrace, Page};
use crate::cli::TraceCommand;
use crate::commands::Context;
use crate::error::{Error, Result};
use crate::output::{one_line, relative};
use serde_json::json;

pub async fn run(command: TraceCommand, ctx: &Context) -> Result<()> {
    ctx.require_auth()?;
    let project = ctx.project_id(None).await?;
    let (hours, environment) = match &command {
        TraceCommand::Summary(a) | TraceCommand::Models(a) | TraceCommand::Tools(a) => {
            (a.hours, a.environment.clone())
        }
        TraceCommand::List(a) => (a.hours, a.environment.clone()),
    };
    if hours.is_some_and(|h| h <= 0) {
        return Err(Error::Invalid("hours must be positive".to_string()));
    }
    let mut query = vec![
        (
            "period_hours",
            hours.map(|h| h.to_string()).unwrap_or_default(),
        ),
        ("environment", environment.unwrap_or_default()),
    ];
    match command {
        TraceCommand::Summary(_) => {
            let s: AgentSummary = ctx
                .client
                .get(&paths::agents_summary(project), &query)
                .await?;
            if ctx.output.is_json() {
                return ctx.output.json(&json!({
                    "agent_runs": s.agent_runs, "llm_calls": s.llm_calls,
                    "tool_calls": s.tool_calls, "error_count": s.error_count,
                    "total_tokens": s.total_tokens, "avg_duration_ms": s.avg_duration_ms,
                    "p95_duration_ms": s.p95_duration_ms
                }));
            }
            let mut table = ctx.output.table(&["STAT", "VALUE"]);
            for (label, value) in [
                ("agent_runs", s.agent_runs.to_string()),
                ("llm_calls", s.llm_calls.to_string()),
                ("tool_calls", s.tool_calls.to_string()),
                ("error_count", s.error_count.to_string()),
                ("total_tokens", thousands(s.total_tokens)),
                ("avg_duration_ms", format!("{:.1} ms", s.avg_duration_ms)),
                ("p95_duration_ms", format!("{:.1} ms", s.p95_duration_ms)),
            ] {
                table.add_row([label.to_string(), value]);
            }
            println!("{table}");
        }
        TraceCommand::List(a) => {
            if a.page <= 0 || a.limit <= 0 {
                return Err(Error::Invalid(
                    "page and limit must be positive".to_string(),
                ));
            }
            query.extend([
                ("page", a.page.to_string()),
                ("per_page", a.limit.to_string()),
                ("agent", a.agent.unwrap_or_default()),
                ("model", a.model.unwrap_or_default()),
            ]);
            if a.errors {
                query.push(("errors", "true".to_string()));
            }
            let page: Page<AgentTrace> = ctx
                .client
                .get(&paths::agents_traces(project), &query)
                .await?;
            if ctx.output.is_json() {
                let items: Vec<_> = page.items.iter().map(|t| json!({
                    "trace_id": t.trace_id, "agent_names": t.agent_names,
                    "duration_ms": t.duration_ms, "total_tokens": t.total_tokens,
                    "tool_call_count": t.tool_call_count, "llm_call_count": t.llm_call_count,
                    "error_count": t.error_count, "started_at": t.started_at
                })).collect();
                return ctx.output.json(&json!({
                    "items": items, "total_count": page.total_count, "page": page.page,
                    "per_page": page.per_page, "total_pages": page.total_pages,
                    "total_capped": page.total_capped
                }));
            }
            let mut table = ctx.output.table(&[
                "TRACE", "AGENTS", "DURATION", "LLM", "TOOLS", "ERRORS", "TOKENS", "STARTED",
            ]);
            for t in &page.items {
                table.add_row([
                    t.trace_id.chars().take(8).collect::<String>(),
                    one_line(&t.agent_names.join(", ")),
                    t.duration_ms
                        .map(|v| format!("{v:.1} ms"))
                        .unwrap_or_else(|| "-".into()),
                    t.llm_call_count.to_string(),
                    t.tool_call_count.to_string(),
                    t.error_count.to_string(),
                    thousands(t.total_tokens),
                    relative(&t.started_at),
                ]);
            }
            println!("{table}");
            ctx.output.line(format!(
                "Showing {} of {} traces (page {}).",
                page.items.len(),
                page.total_count,
                a.page
            ));
        }
        TraceCommand::Models(_) => {
            let models: Vec<AgentModel> = ctx
                .client
                .get(&paths::agents_models(project), &query)
                .await?;
            if ctx.output.is_json() {
                return ctx.output.json(&models.iter().map(|m| json!({
                    "model": m.model, "requests": m.requests, "errors": m.errors,
                    "avg_ms": m.avg_ms, "p95_ms": m.p95_ms, "input_tokens": m.input_tokens,
                    "output_tokens": m.output_tokens, "total_tokens": m.total_tokens
                })).collect::<Vec<_>>());
            }
            let mut table = ctx.output.table(&[
                "MODEL", "REQUESTS", "ERRORS", "AVG MS", "P95 MS", "INPUT", "OUTPUT", "TOTAL",
            ]);
            for m in models {
                table.add_row([
                    one_line(&m.model),
                    m.requests.to_string(),
                    m.errors.to_string(),
                    format!("{:.1}", m.avg_ms),
                    format!("{:.1}", m.p95_ms),
                    thousands(m.input_tokens),
                    thousands(m.output_tokens),
                    thousands(m.total_tokens),
                ]);
            }
            println!("{table}");
        }
        TraceCommand::Tools(_) => {
            let tools: Vec<AgentTool> = ctx
                .client
                .get(&paths::agents_tools(project), &query)
                .await?;
            if ctx.output.is_json() {
                return ctx.output.json(&tools.iter().map(|t| json!({
                    "tool": t.tool, "calls": t.calls, "errors": t.errors, "avg_ms": t.avg_ms, "p95_ms": t.p95_ms
                })).collect::<Vec<_>>());
            }
            let mut table = ctx
                .output
                .table(&["TOOL", "CALLS", "ERRORS", "AVG MS", "P95 MS"]);
            for t in tools {
                table.add_row([
                    one_line(&t.tool),
                    t.calls.to_string(),
                    t.errors.to_string(),
                    format!("{:.1}", t.avg_ms),
                    format!("{:.1}", t.p95_ms),
                ]);
            }
            println!("{table}");
        }
    }
    Ok(())
}

fn thousands(n: f64) -> String {
    let digits = format!("{:.0}", n.abs());
    let mut result = String::new();
    if n.is_sign_negative() && n.round() != 0.0 {
        result.push('-');
    }
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(c);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::thousands;

    #[test]
    fn formats_token_counts() {
        for (n, expected) in [
            (0.0, "0"),
            (999.0, "999"),
            (1000.0, "1,000"),
            (12345.0, "12,345"),
            (1234567.0, "1,234,567"),
            (-1234.0, "-1,234"),
            (999.9, "1,000"),
        ] {
            assert_eq!(thousands(n), expected);
        }
    }
}
