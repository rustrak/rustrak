//! List the instance's team members.

use crate::api::{paths, TeamMember};
use crate::cli::TeamCommand;
use crate::commands::Context;
use crate::error::{Error, Result};
use crate::output::relative;

pub async fn run(command: TeamCommand, ctx: &Context) -> Result<()> {
    match command {
        TeamCommand::List => {
            ctx.require_auth()?;
            // Keep the raw body for JSON and for servers that wrap their lists.
            let body: serde_json::Value = ctx.client.get(paths::team(), &[]).await?;
            let items = body.get("items").filter(|v| v.is_array()).unwrap_or(&body);
            if ctx.output.is_json() || !items.is_array() {
                return ctx.output.json(items);
            }
            let members: Vec<TeamMember> =
                serde_json::from_value(items.clone()).map_err(|source| Error::Decode {
                    path: paths::team().to_string(),
                    source,
                })?;
            let mut table = ctx
                .output
                .table(&["ID", "EMAIL", "ROLE", "ACTIVE", "LAST LOGIN"]);
            for member in members {
                table.add_row([
                    member.id.to_string(),
                    member.email,
                    member.role,
                    if member.is_active { "yes" } else { "no" }.to_string(),
                    member
                        .last_login
                        .as_deref()
                        .map(relative)
                        .unwrap_or_else(|| "never".to_string()),
                ]);
            }
            println!("{table}");
            Ok(())
        }
    }
}
