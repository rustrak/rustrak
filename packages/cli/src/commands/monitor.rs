//! Inspect cron monitors and their check-ins.

use serde_json::{json, Value};

use crate::api::{paths, CheckIn, Monitor, Page};
use crate::cli::MonitorCommand;
use crate::commands::Context;
use crate::error::{Error, Result};
use crate::output::{relative, when};

pub async fn run(command: MonitorCommand, ctx: &Context) -> Result<()> {
    let project = ctx.project_id(None).await?;
    match command {
        MonitorCommand::List => {
            let path = paths::monitors(project);
            let value: Value = ctx.client.get(&path, &[]).await?;
            let monitors: Vec<Monitor> = serde_json::from_value(value.clone())
                .map_err(|source| Error::Decode { path, source })?;
            if ctx.output.is_json() {
                return ctx.output.json(&value);
            }
            if monitors.is_empty() {
                ctx.output.line("No monitors found.");
                return Ok(());
            }
            let mut table = ctx.output.table(&[
                "SLUG",
                "NAME",
                "MUTED",
                "ENVIRONMENTS",
                "LAST CHECKIN",
                "NEXT",
            ]);
            for monitor in monitors {
                let first = monitor.environments.first();
                table.add_row([
                    monitor.slug,
                    monitor.name,
                    if monitor.is_muted { "yes" } else { "no" }.into(),
                    monitor
                        .environments
                        .iter()
                        .map(|env| format!("{}: {}", env.environment, env.status))
                        .collect::<Vec<_>>()
                        .join(", "),
                    time(first.and_then(|env| env.last_checkin.as_deref())),
                    time(first.and_then(|env| env.next_checkin.as_deref())),
                ]);
            }
            println!("{table}");
        }
        MonitorCommand::Get(args) => {
            let path = paths::monitor(project, &args.slug);
            let value: Value = ctx.client.get(&path, &[]).await?;
            let monitor: Monitor = serde_json::from_value(value.clone())
                .map_err(|source| Error::Decode { path, source })?;
            if ctx.output.is_json() {
                return ctx.output.json(&value);
            }
            println!("Slug: {}", monitor.slug);
            println!("Name: {}", monitor.name);
            println!("Muted: {}", if monitor.is_muted { "yes" } else { "no" });
            println!("Created: {}", when(&monitor.created_at));
            println!(
                "Schedule: {}",
                monitor
                    .config
                    .schedule
                    .map(|schedule| schedule.to_string())
                    .unwrap_or_else(|| "-".into())
            );
            println!(
                "Timezone: {}",
                monitor.config.timezone.as_deref().unwrap_or("-")
            );
            for env in monitor.environments {
                println!("Environment: {}", env.environment);
                println!("  Status: {}", env.status);
                println!("  Last checkin: {}", time(env.last_checkin.as_deref()));
                println!("  Next checkin: {}", time(env.next_checkin.as_deref()));
            }
        }
        MonitorCommand::Checkins(args) => {
            if args.limit < 1 {
                return Err(Error::Invalid("limit must be positive".into()));
            }
            let mut query = vec![("page", "1".into()), ("per_page", args.limit.to_string())];
            if let Some(environment) = args.environment {
                query.push(("environment", environment));
            }
            let path = paths::monitor_checkins(project, &args.slug);
            let value: Value = ctx.client.get(&path, &query).await?;
            let page: Page<CheckIn> = serde_json::from_value(value.clone())
                .map_err(|source| Error::Decode { path, source })?;
            if ctx.output.is_json() {
                return ctx.output.json(&value);
            }
            let mut table = ctx
                .output
                .table(&["TIME", "ENVIRONMENT", "STATUS", "DURATION"]);
            for checkin in page.items {
                table.add_row([
                    relative(&checkin.date_added),
                    checkin.environment,
                    checkin.status,
                    checkin
                        .duration_ms
                        .map(|ms| format!("{ms} ms"))
                        .unwrap_or_else(|| "-".into()),
                ]);
            }
            println!("{table}");
        }
        MonitorCommand::Mute(args) => {
            ctx.require_auth()?;
            let path = paths::monitor(project, &args.slug);
            let value: Value = ctx
                .client
                .put(&path, &json!({"is_muted": !args.off}))
                .await?;
            let monitor: Monitor = serde_json::from_value(value.clone())
                .map_err(|source| Error::Decode { path, source })?;
            if ctx.output.is_json() {
                return ctx.output.json(&value);
            }
            ctx.output.line(format!(
                "{} monitor {:?}.",
                if monitor.is_muted { "Muted" } else { "Unmuted" },
                monitor.slug
            ));
        }
    }
    Ok(())
}

fn time(value: Option<&str>) -> String {
    value.map(relative).unwrap_or_else(|| "-".into())
}
