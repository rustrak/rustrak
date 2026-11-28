use std::io::{IsTerminal, Write};

use serde_json::{json, Value};

use crate::api::{paths, MetricDelta, Page, Project, ProjectStats, SessionSummary};
use crate::cli::ProjectCommand;
use crate::commands::Context;
use crate::error::{Error, Result};
use crate::output::{one_line, truncate, when};

pub async fn run(command: ProjectCommand, ctx: &Context) -> Result<()> {
    match command {
        ProjectCommand::List(args) => {
            let query = vec![
                ("page", args.page.to_string()),
                ("per_page", args.limit.to_string()),
                ("query", args.query.unwrap_or_default()),
                ("stats_period", args.period.unwrap_or_default()),
            ];
            let page: Page<Project> = ctx.client.get(paths::projects(), &query).await?;
            if ctx.output.is_json() {
                return ctx
                    .output
                    .json(&page.items.iter().map(project_json).collect::<Vec<_>>());
            }
            let mut table = ctx
                .output
                .table(&["ID", "SLUG", "NAME", "OPEN", "FATAL", "EVENTS", "PLATFORM"]);
            for p in &page.items {
                let stats = p.stats.as_ref();
                table.add_row([
                    p.id.to_string(),
                    truncate(&one_line(&p.slug), 24),
                    truncate(&one_line(&p.name), 32),
                    stats
                        .map(|s| s.open_issues.to_string())
                        .unwrap_or_else(missing),
                    stats
                        .map(|s| s.fatal_issues.to_string())
                        .unwrap_or_else(missing),
                    stats
                        .and_then(|s| s.events.as_ref())
                        .map(|m| m.current.to_string())
                        .unwrap_or_else(missing),
                    truncate(&one_line(p.platform.as_deref().unwrap_or("-")), 24),
                ]);
            }
            println!("{table}");
            ctx.output.line(format!(
                "{} of {} projects",
                page.items.len(),
                page.total_count
            ));
        }
        ProjectCommand::Get(args) => {
            let project = ctx.project(args.project_ref.as_deref()).await?;
            show_project(&project, ctx)?;
        }
        ProjectCommand::Dsn(args) => {
            let project = ctx.project(args.project_ref.as_deref()).await?;
            let dsn = project.dsn.ok_or_else(|| {
                Error::Invalid(
                    "DSN unavailable; a token with client-keys:read permission is required"
                        .to_string(),
                )
            })?;
            if ctx.json {
                ctx.output.json(&json!({"dsn": dsn}))?;
            } else {
                // Pipes must receive the DSN itself unless --json was requested.
                println!("{dsn}");
            }
        }
        ProjectCommand::Create(args) => {
            ctx.require_auth()?;
            let mut body = json!({"name": args.name});
            if let Some(platform) = args.platform {
                body["platform"] = json!(platform);
            }
            if let Some(slug) = args.slug {
                body["slug"] = json!(slug);
            }
            let project: Project = ctx.client.post(paths::projects(), &body).await?;
            show_project(&project, ctx)?;
            if args.r#use {
                let note = format!(
                    "Pass --project {} or export RUSTRAK_PROJECT={}",
                    project.id, project.id
                );
                if ctx.output.is_json() {
                    eprintln!("{note}");
                } else {
                    ctx.output.line(note);
                }
            }
        }
        ProjectCommand::Update(args) => {
            if args.name.is_none() && args.platform.is_none() {
                return Err(Error::Invalid(
                    "nothing to update; pass --name or --platform".to_string(),
                ));
            }
            ctx.require_auth()?;
            let id = ctx.project_id(args.project_ref.as_deref()).await?;
            let mut body = json!({});
            if let Some(name) = args.name {
                body["name"] = json!(name);
            }
            if let Some(platform) = args.platform {
                body["platform"] = json!(platform);
            }
            let project: Project = ctx.client.patch(&paths::project(id), &body).await?;
            show_project(&project, ctx)?;
        }
        ProjectCommand::Delete(args) => {
            ctx.require_auth()?;
            let project = ctx.project(args.project_ref.as_deref()).await?;
            if !args.yes
                && !confirm(&format!(
                    "Delete project {} ({})? [y/N] ",
                    one_line(&project.name),
                    one_line(&project.slug)
                ))?
            {
                return Err(Error::Invalid(
                    "deletion cancelled; pass --yes to confirm deletion without a prompt"
                        .to_string(),
                ));
            }
            ctx.client.delete::<()>(&paths::project(project.id)).await?;
            if ctx.output.is_json() {
                ctx.output.json(&json!({"deleted": project.id}))?;
            } else {
                ctx.output.line(format!(
                    "Deleted project {} ({}).",
                    one_line(&project.name),
                    project.id
                ));
            }
        }
        ProjectCommand::Stats(args) => {
            ctx.require_auth()?;
            let id = ctx.project_id(args.project_ref.as_deref()).await?;
            let query = stats_query(args.period, args.environment);
            let stats: ProjectStats = ctx.client.get(&paths::stats_summary(id), &query).await?;
            if ctx.output.is_json() {
                return ctx.output.json(&json!({"period_hours": stats.period_hours, "events": metric_json(stats.events.as_ref()), "new_issues": metric_json(stats.new_issues.as_ref()), "open_issues": stats.open_issues}));
            }
            ctx.output.line(format!(
                "Period: {}",
                stats
                    .period_hours
                    .map(|h| format!("{h}h"))
                    .unwrap_or_else(missing)
            ));
            ctx.output
                .line(format!("Events: {}", metric_text(stats.events.as_ref())));
            ctx.output.line(format!(
                "New issues: {}",
                stats
                    .new_issues
                    .as_ref()
                    .map(|m| m.current.to_string())
                    .unwrap_or_else(missing)
            ));
            ctx.output
                .line(format!("Open issues: {}", stats.open_issues));
        }
        ProjectCommand::Sessions(args) => {
            ctx.require_auth()?;
            let id = ctx.project_id(args.project_ref.as_deref()).await?;
            let stats: SessionSummary = ctx
                .client
                .get(
                    &paths::sessions_summary(id),
                    &stats_query(args.period, args.environment),
                )
                .await?;
            if ctx.output.is_json() {
                return ctx.output.json(&json!({"total": stats.total, "errored": stats.errored, "crashed": stats.crashed, "abnormal": stats.abnormal, "crash_free_sessions_rate": stats.crash_free_sessions_rate, "crash_free_users_rate": stats.crash_free_users_rate, "active_releases": stats.active_releases}));
            }
            ctx.output.line(format!(
                "Total: {}\nErrored: {}\nCrashed: {}\nAbnormal: {}",
                stats.total, stats.errored, stats.crashed, stats.abnormal
            ));
            ctx.output.line(format!(
                "Crash-free sessions: {}\nCrash-free users: {}",
                percentage(stats.crash_free_sessions_rate),
                percentage(stats.crash_free_users_rate)
            ));
        }
        ProjectCommand::Envs => {
            let id = ctx.project_id(None).await?;
            let environments: Vec<String> = ctx.client.get(&paths::environments(id), &[]).await?;
            if ctx.output.is_json() {
                return ctx.output.json(&environments);
            }
            for environment in environments {
                ctx.output.line(one_line(&environment));
            }
        }
        ProjectCommand::Use(args) => {
            let id = ctx.project_id(Some(&args.project)).await?;
            // Numeric references bypass lookup in project_id, so validate those too.
            let _: Project = ctx.client.get(&paths::project(id), &[]).await?;
            let guidance = format!("export RUSTRAK_PROJECT={id}");
            if ctx.output.is_json() {
                ctx.output
                    .json(&json!({"project_id": id, "instruction": guidance}))?;
            } else {
                ctx.output.line(guidance);
            }
        }
    }
    Ok(())
}

fn confirm(q: &str) -> Result<bool> {
    let stdin = std::io::stdin();
    if !stdin.is_terminal() {
        return Ok(false);
    }
    let mut stderr = std::io::stderr().lock();
    write!(stderr, "{q}")?;
    stderr.flush()?;
    let mut answer = String::new();
    stdin.read_line(&mut answer)?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

fn project_json(p: &Project) -> Value {
    json!({"id": p.id, "slug": p.slug, "name": p.name, "platform": p.platform, "dsn": p.dsn,
        "created_at": p.created_at, "stored_event_count": p.stored_event_count, "digested_event_count": p.digested_event_count,
        "stats": p.stats.as_ref().map(|s| json!({"open_issues": s.open_issues, "fatal_issues": s.fatal_issues, "events": metric_json(s.events.as_ref()), "new_issues": metric_json(s.new_issues.as_ref())}))})
}

fn show_project(p: &Project, ctx: &Context) -> Result<()> {
    if ctx.output.is_json() {
        return ctx.output.json(&project_json(p));
    }
    ctx.output.line(format!("ID: {}\nSlug: {}\nName: {}\nPlatform: {}\nDSN: {}\nCreated: {}\nStored events: {}\nDigested events: {}", p.id, one_line(&p.slug), one_line(&p.name), p.platform.as_deref().unwrap_or("-"), p.dsn.as_deref().unwrap_or("-"), when(&p.created_at), p.stored_event_count, p.digested_event_count));
    Ok(())
}

fn stats_query(period: Option<String>, environment: Option<String>) -> Vec<(&'static str, String)> {
    vec![
        ("period", period.unwrap_or_default()),
        ("environment", environment.unwrap_or_default()),
    ]
}

fn metric_json(metric: Option<&MetricDelta>) -> Value {
    metric
        .map(|m| json!({"current": m.current, "previous": m.previous}))
        .unwrap_or(Value::Null)
}

fn metric_text(metric: Option<&MetricDelta>) -> String {
    match metric {
        Some(m) => match m.previous {
            Some(previous) => format!(
                "{} ({:+} vs previous {})",
                m.current,
                i128::from(m.current) - i128::from(previous),
                previous
            ),
            None => m.current.to_string(),
        },
        None => missing(),
    }
}

fn percentage(rate: Option<f64>) -> String {
    rate.map(|r| format!("{:.2}%", r * 100.0))
        .unwrap_or_else(missing)
}

fn missing() -> String {
    "-".to_string()
}
