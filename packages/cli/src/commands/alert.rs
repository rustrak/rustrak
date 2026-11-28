//! Inspect a project's alert rules.

use serde_json::Value;

use crate::api::{paths, AlertRule};
use crate::cli::AlertCommand;
use crate::commands::Context;
use crate::error::{Error, Result};
use crate::output::relative;

pub async fn run(command: AlertCommand, ctx: &Context) -> Result<()> {
    match command {
        AlertCommand::List => {
            let project = ctx.project_id(None).await?;
            let path = paths::alert_rules(project);
            let value: Value = ctx.client.get(&path, &[]).await?;
            let rules: Vec<AlertRule> = serde_json::from_value(value.clone())
                .map_err(|source| Error::Decode { path, source })?;
            if ctx.output.is_json() {
                return ctx.output.json(&value);
            }
            let mut table = ctx.output.table(&[
                "ID",
                "NAME",
                "TYPE",
                "ENABLED",
                "COOLDOWN",
                "LAST FIRED",
                "CHANNELS",
            ]);
            for rule in rules {
                table.add_row([
                    rule.id.to_string(),
                    rule.name,
                    rule.alert_type,
                    if rule.is_enabled { "yes" } else { "no" }.into(),
                    format!("{}m", rule.cooldown_minutes),
                    rule.last_triggered_at
                        .as_deref()
                        .map(relative)
                        .unwrap_or_else(|| "never".into()),
                    rule.channels.len().to_string(),
                ]);
            }
            println!("{table}");
        }
    }
    Ok(())
}
