//! Inspect a Sentry event and its original payload.

use crate::api::{paths, EventDetail};
use crate::cli::EventCommand;
use crate::commands::Context;
use crate::error::{Error, Result};

pub async fn run(command: EventCommand, ctx: &Context) -> Result<()> {
    ctx.require_auth()?;
    let EventCommand::Get(args) = command;
    if args.event.is_empty() {
        return Err(Error::Invalid("event id cannot be empty".to_string()));
    }
    let project = ctx.project_id(None).await?;
    let detail: EventDetail = ctx
        .client
        .get(&paths::event_detail(project, &args.event), &[])
        .await?;
    if ctx.output.is_json() {
        return ctx.output.json(&serde_json::json!({
            "event_id": detail.event_id, "title": detail.title,
            "level": detail.level, "platform": detail.platform,
            "release": detail.release, "environment": detail.environment,
            "timestamp": detail.timestamp, "sdk_name": detail.sdk_name,
            "sdk_version": detail.sdk_version, "ingested_at": detail.ingested_at,
            "data": detail.data,
        }));
    }
    if args.raw {
        return ctx.output.json(&detail.data);
    }
    println!("Event ID:    {}", detail.event_id);
    println!("Title:       {}", detail.title);
    println!("Level:       {}", detail.level);
    println!("Platform:    {}", detail.platform);
    println!("Release:     {}", detail.release);
    println!("Environment: {}", detail.environment);
    println!("Timestamp:   {}", detail.timestamp);
    println!("SDK:         {} {}", detail.sdk_name, detail.sdk_version);
    println!("Ingested at: {}", detail.ingested_at);
    println!("Data:");
    ctx.output.json(&detail.data)
}
