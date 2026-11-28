//! Turns the parsed command into a call on one module's `run`, and nothing
//! else: no business logic lives here.

use crate::cli::{ApiArgs, Cli, Command, CompletionArgs, SendArgs};
use crate::commands::{self, Context};
use crate::error::Result;

pub async fn dispatch(cli: Cli) -> Result<()> {
    let ctx = Context::build(&cli.global)?;

    match cli.command {
        Command::Auth(command) => commands::auth::run(command, &ctx).await,
        Command::Config(command) => commands::config::run(command, &ctx),
        Command::Project(command) => commands::project::run(command, &ctx).await,
        Command::Issue(command) => commands::issue::run(command, &ctx).await,
        Command::Event(command) => commands::event::run(command, &ctx).await,
        Command::Log(command) => commands::log::run(command, &ctx).await,
        Command::Trace(command) => commands::trace::run(command, &ctx).await,
        Command::Perf(command) => commands::perf::run(command, &ctx).await,
        Command::Release(command) => commands::release::run(command, &ctx).await,
        Command::Monitor(command) => commands::monitor::run(command, &ctx).await,
        Command::Alert(command) => commands::alert::run(command, &ctx).await,
        Command::Team(command) => commands::team::run(command, &ctx).await,
        Command::Token(command) => commands::token::run(command, &ctx).await,
        Command::Send(args) => commands::send::run(args, &ctx).await,
        Command::Api(args) => api_call(args, &ctx).await,
        Command::Version => instance_version(&ctx).await,
        Command::Health => health(&ctx).await,
        Command::Completion(args) => completion(args),
    }
}

/// `rustrak api`: an escape hatch for an endpoint no command wraps yet, so the
/// CLI is never the reason someone reaches for `curl` and a hand-built token.
async fn api_call(args: ApiArgs, ctx: &Context) -> Result<()> {
    ctx.require_auth()?;

    let path = if args.path.starts_with("http") || args.path.starts_with('/') {
        args.path.clone()
    } else {
        format!("/api/{}", args.path)
    };

    let body: Option<serde_json::Value> = match &args.body {
        Some(raw) => Some(
            serde_json::from_str(raw)
                .map_err(|e| crate::Error::Invalid(format!("--body is not JSON: {e}")))?,
        ),
        None => None,
    };
    let body = body.unwrap_or(serde_json::Value::Null);

    let value: serde_json::Value = match args.method {
        crate::cli::Method::Get => ctx.client.get(&path, &[]).await?,
        crate::cli::Method::Post => ctx.client.post(&path, &body).await?,
        crate::cli::Method::Put => ctx.client.put(&path, &body).await?,
        crate::cli::Method::Patch => ctx.client.patch(&path, &body).await?,
        crate::cli::Method::Delete => ctx.client.delete_with(&path, &body).await?,
    };

    ctx.output.json(&value)
}

async fn instance_version(ctx: &Context) -> Result<()> {
    let version: crate::api::VersionInfo =
        ctx.client.get(crate::api::paths::version(), &[]).await?;
    if ctx.output.is_json() {
        return ctx.output.json(&version);
    }
    println!("{}", version.version);
    Ok(())
}

async fn health(ctx: &Context) -> Result<()> {
    // Health takes no credential, but the client carries one harmlessly; the
    // point is that it works before `auth login`.
    let body = ctx.client.get_text(crate::api::paths::health()).await?;
    if ctx.output.is_json() {
        println!("{}", body.trim());
        return Ok(());
    }
    println!("{} is up.", ctx.url);
    Ok(())
}

fn completion(args: CompletionArgs) -> Result<()> {
    use clap::CommandFactory;
    let mut command = Cli::command();
    let name = command.get_name().to_string();
    clap_complete::generate(args.shell, &mut command, name, &mut std::io::stdout());
    Ok(())
}

/// Kept so the unused-import lint does not fire on the module re-export while
/// `send` is still a single function.
#[allow(dead_code)]
fn _assert_send_answer(_: SendArgs) {}
