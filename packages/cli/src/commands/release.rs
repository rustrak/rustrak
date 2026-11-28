//! Register releases and inspect their health.

use std::io::{IsTerminal, Write};

use serde_json::{json, Value};

use crate::api::{paths, Page, Release};
use crate::cli::ReleaseCommand;
use crate::commands::Context;
use crate::error::{Error, Result};
use crate::output::{relative, when};

pub async fn run(command: ReleaseCommand, ctx: &Context) -> Result<()> {
    let project = ctx.project_id(None).await?;
    match command {
        ReleaseCommand::List(args) => {
            if args.page < 1 || args.limit < 1 {
                return Err(Error::Invalid("page and limit must be positive".into()));
            }
            let path = paths::releases(project);
            let mut query = vec![
                ("page", args.page.to_string()),
                ("per_page", args.limit.to_string()),
            ];
            for (name, value) in [
                ("environment", args.environment),
                ("period", args.period),
                ("query", args.query),
            ] {
                if let Some(value) = value {
                    query.push((name, value));
                }
            }
            let value: Value = ctx.client.get(&path, &query).await?;
            let page: Page<Release> = serde_json::from_value(value.clone())
                .map_err(|source| Error::Decode { path, source })?;
            if ctx.output.is_json() {
                return ctx.output.json(&value);
            }
            let mut table = ctx.output.table(&[
                "VERSION",
                "RELEASED",
                "EVENTS",
                "NEW ISSUES",
                "SESSIONS",
                "CRASH-FREE",
                "ADOPTION",
                "ENVS",
            ]);
            for release in &page.items {
                table.add_row([
                    release.version.clone(),
                    relative(
                        release
                            .date_released
                            .as_deref()
                            .unwrap_or(&release.date_created),
                    ),
                    release.event_count.to_string(),
                    release.new_issues.to_string(),
                    release.sessions.to_string(),
                    percent(release.crash_free_sessions, 1),
                    percent(release.adoption, 0),
                    release.environments.join(", "),
                ]);
            }
            println!("{table}");
            ctx.output.line(format!(
                "{} releases shown, {} total (page {}).",
                page.items.len(),
                page.total_count,
                args.page
            ));
        }
        ReleaseCommand::Get(args) => {
            let value: Value = ctx
                .client
                .get(&paths::release(project, &args.release_version), &[])
                .await?;
            if ctx.output.is_json() {
                return ctx.output.json(&value);
            }
            let Ok(release) = serde_json::from_value::<Release>(value.clone()) else {
                return ctx.output.json(&value);
            };
            println!("Version: {}", release.version);
            println!("Registered: {}", release.registered);
            println!("Created: {}", when(&release.date_created));
            println!(
                "Released: {}",
                release
                    .date_released
                    .as_deref()
                    .map(when)
                    .unwrap_or_else(|| "-".into())
            );
            println!("Ref: {}", release.git_ref.as_deref().unwrap_or("-"));
            println!("URL: {}", release.url.as_deref().unwrap_or("-"));
            println!("Events: {}", release.event_count);
            println!("New issues: {}", release.new_issues);
            println!("Sessions: {}", release.sessions);
            println!("Crashed: {}", release.crashed);
            println!(
                "Crash-free sessions: {}",
                percent(release.crash_free_sessions, 1)
            );
            println!("Crash-free users: {}", percent(release.crash_free_users, 1));
            println!("Adoption: {}", percent(release.adoption, 0));
            println!("Environments: {}", release.environments.join(", "));
        }
        ReleaseCommand::Create(args) => {
            ctx.require_auth()?;
            let mut body = json!({"version": args.release_version});
            if let Some(git_ref) = args.r#ref {
                body["ref"] = json!(git_ref);
            }
            if let Some(url) = args.url {
                body["url"] = json!(url);
            }
            let path = paths::releases(project);
            let value: Value = ctx.client.post(&path, &body).await?;
            let release: Release = serde_json::from_value(value.clone())
                .map_err(|source| Error::Decode { path, source })?;
            if ctx.output.is_json() {
                return ctx.output.json(&value);
            }
            ctx.output
                .line(format!("Registered release {:?}.", release.version));
        }
        ReleaseCommand::Delete(args) => {
            ctx.require_auth()?;
            if !args.yes && !confirm(&format!("Delete release {:?}?", args.release_version))? {
                return Err(Error::Invalid(
                    "release not deleted; pass --yes to confirm deletion".into(),
                ));
            }
            let _: Value = ctx
                .client
                .delete(&paths::release(project, &args.release_version))
                .await?;
            if ctx.output.is_json() {
                return ctx.output.json(&json!({"deleted": args.release_version}));
            }
            ctx.output
                .line(format!("Deleted release {:?}.", args.release_version));
        }
    }
    Ok(())
}

fn percent(value: Option<f64>, precision: usize) -> String {
    value
        .map(|value| format!("{value:.precision$}%"))
        .unwrap_or_else(|| "-".into())
}

fn confirm(q: &str) -> Result<bool> {
    let stdin = std::io::stdin();
    if !stdin.is_terminal() {
        return Ok(false);
    }
    eprint!("{q} [y/N] ");
    std::io::stderr().flush()?;
    let mut answer = String::new();
    stdin.read_line(&mut answer)?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}
