//! List, inspect, and follow project logs.

use std::collections::HashSet;
use std::time::Duration;

use crate::api::{paths, LogEntry, Page};
use crate::cli::{LogCommand, LogListArgs, LogTailArgs};
use crate::commands::Context;
use crate::error::{Error, Result};
use crate::output::{one_line, relative, truncate};

pub async fn run(command: LogCommand, ctx: &Context) -> Result<()> {
    ctx.require_auth()?;
    let project = ctx.project_id(None).await?;
    match command {
        LogCommand::List(args) => list(args, project, ctx).await,
        LogCommand::Tail(args) => tail(args, project, ctx).await,
        LogCommand::Get(args) => {
            if args.log.is_empty() {
                return Err(Error::Invalid("log id cannot be empty".to_string()));
            }
            let entry: LogEntry = ctx
                .client
                .get(&format!("/api/projects/{project}/logs/{}", args.log), &[])
                .await?;
            if ctx.output.is_json() {
                return ctx.output.json(&log_json(&entry));
            }
            println!("ID:          {}", entry.id);
            println!("Level:       {}", entry.level);
            println!("Timestamp:   {}", entry.timestamp);
            println!(
                "Environment: {}",
                entry.environment.as_deref().unwrap_or("-")
            );
            println!("Trace ID:    {}", entry.trace_id.as_deref().unwrap_or("-"));
            println!("Body:        {}", entry.body);
            println!("Attributes:");
            ctx.output.json(&entry.attributes)
        }
    }
}

async fn list(args: LogListArgs, project: i32, ctx: &Context) -> Result<()> {
    if args.page < 1 || args.limit < 1 {
        return Err(Error::Invalid(
            "page and limit must be positive".to_string(),
        ));
    }
    let query = vec![
        ("page", args.page.to_string()),
        ("per_page", args.limit.to_string()),
        ("level", args.level.unwrap_or_default()),
        ("trace_id", args.trace_id.unwrap_or_default()),
        ("environment", args.environment.unwrap_or_default()),
        ("query", args.query.unwrap_or_default()),
        ("period", args.period.unwrap_or_default()),
    ];
    let page: Page<LogEntry> = ctx.client.get(&paths::logs(project), &query).await?;
    if ctx.output.is_json() {
        return ctx.output.json(&serde_json::json!({
            "items": page.items.iter().map(log_json).collect::<Vec<_>>(),
            "total_count": page.total_count, "page": page.page,
            "per_page": page.per_page, "total_pages": page.total_pages,
            "total_capped": page.total_capped,
        }));
    }
    let mut table = ctx.output.table(&["TIME", "LEVEL", "ENV", "TRACE", "BODY"]);
    for entry in &page.items {
        table.add_row([
            relative(&entry.timestamp),
            entry.level.clone(),
            entry.environment.clone().unwrap_or_else(|| "-".to_string()),
            entry
                .trace_id
                .as_ref()
                .map(|id| id.chars().take(8).collect())
                .unwrap_or_else(|| "-".to_string()),
            truncate(&one_line(&entry.body), 100),
        ]);
    }
    println!("{table}");
    ctx.output.line(format!(
        "{} logs shown, {} total (page {}).",
        page.items.len(),
        page.total_count,
        args.page
    ));
    Ok(())
}

async fn tail(args: LogTailArgs, project: i32, ctx: &Context) -> Result<()> {
    if args.interval == 0 {
        return Err(Error::Invalid("interval must be positive".to_string()));
    }
    let query = vec![
        ("per_page", "50".to_string()),
        ("period", args.period),
        ("level", args.level.unwrap_or_default()),
        ("environment", args.environment.unwrap_or_default()),
        ("query", args.query.unwrap_or_default()),
    ];
    let path = paths::logs(project);
    let mut seen = HashSet::new();
    let interrupt = tokio::signal::ctrl_c();
    tokio::pin!(interrupt);
    loop {
        let page: Page<LogEntry> = tokio::select! {
            result = &mut interrupt => { result?; break; }
            result = ctx.client.get(&path, &query) => result?,
        };
        // The API returns newest first; reverse each batch for stream order.
        for entry in page.items.into_iter().rev() {
            if !seen.insert(entry.id.clone()) {
                continue;
            }
            if ctx.output.is_json() {
                println!("{}", log_json(&entry));
            } else {
                let time = chrono::DateTime::parse_from_rfc3339(&entry.timestamp)
                    .map(|time| {
                        time.with_timezone(&chrono::Local)
                            .format("%H:%M:%S")
                            .to_string()
                    })
                    .unwrap_or_else(|_| entry.timestamp.clone());
                println!(
                    "{time} {} [{}] {}",
                    entry.level,
                    entry.environment.as_deref().unwrap_or("-"),
                    one_line(&entry.body)
                );
            }
        }
        tokio::select! {
            result = &mut interrupt => { result?; break; }
            _ = tokio::time::sleep(Duration::from_secs(args.interval)) => {}
        }
    }
    println!();
    Ok(())
}

// API records are deserialize-only; keep the output shape identical here.
fn log_json(entry: &LogEntry) -> serde_json::Value {
    serde_json::json!({
        "id": entry.id, "level": entry.level, "body": entry.body,
        "timestamp": entry.timestamp, "environment": entry.environment,
        "trace_id": entry.trace_id, "attributes": entry.attributes,
    })
}
