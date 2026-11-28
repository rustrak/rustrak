//! List, mint, and revoke API tokens.

use std::io::{IsTerminal, Write};

use chrono::{Duration, Utc};
use serde_json::json;

use crate::api::{paths, AuthToken, CreatedToken};
use crate::cli::{TokenCommand, TokenCreateArgs, TokenRevokeArgs};
use crate::commands::Context;
use crate::error::{Error, Result};
use crate::output::{relative, when};

pub async fn run(command: TokenCommand, ctx: &Context) -> Result<()> {
    ctx.require_auth()?;
    match command {
        TokenCommand::List => list(ctx).await,
        TokenCommand::Create(args) => create(args, ctx).await,
        TokenCommand::Revoke(args) => revoke(args, ctx).await,
    }
}

async fn list(ctx: &Context) -> Result<()> {
    let body: serde_json::Value = ctx.client.get(paths::tokens(), &[]).await?;
    let items = body.get("items").filter(|v| v.is_array()).unwrap_or(&body);
    if ctx.output.is_json() || !items.is_array() {
        return ctx.output.json(items);
    }
    let tokens: Vec<AuthToken> =
        serde_json::from_value(items.clone()).map_err(|source| Error::Decode {
            path: paths::tokens().to_string(),
            source,
        })?;
    let mut table = ctx.output.table(&[
        "ID",
        "PREFIX",
        "DESCRIPTION",
        "CREATED",
        "LAST USED",
        "EXPIRES",
    ]);
    for token in tokens {
        table.add_row([
            token.id.to_string(),
            token.token_prefix,
            token.description.unwrap_or_default(),
            when(&token.created_at),
            token
                .last_used_at
                .as_deref()
                .map(relative)
                .unwrap_or_else(|| "never".to_string()),
            token
                .expires_at
                .as_deref()
                .map(when)
                .unwrap_or_else(|| "never".to_string()),
        ]);
    }
    println!("{table}");
    Ok(())
}

async fn create(args: TokenCreateArgs, ctx: &Context) -> Result<()> {
    let mut body =
        json!({ "description": args.description.unwrap_or_else(|| "rustrak CLI".to_string()) });
    if let Some(weeks) = args.weeks {
        let expires = (weeks > 0)
            .then(|| Duration::try_weeks(weeks))
            .flatten()
            .and_then(|duration| Utc::now().checked_add_signed(duration))
            .ok_or_else(|| {
                Error::Invalid(
                    "--weeks must be positive and within the supported date range".to_string(),
                )
            })?;
        body["expires_at"] = json!(expires.to_rfc3339());
    }
    let created: CreatedToken = ctx.client.post(paths::tokens(), &body).await.map_err(|error| match error {
        Error::Api { status: 403, kind, message, fields } => Error::Api {
            status: 403,
            kind,
            message: format!("{message}\nAPI tokens can only be minted from a signed-in session. Use `rustrak auth login`, or mint a token in the dashboard (Settings -> API tokens) and store it with `rustrak config set --token <TOKEN>`."),
            fields,
        },
        other => other,
    })?;
    if ctx.output.is_json() {
        eprintln!("Save this token now; it cannot be shown again.");
        return ctx.output.json(&json!({
            "id": created.id,
            "token": created.token,
            "description": created.description,
            "created_at": created.created_at,
            "expires_at": created.expires_at,
        }));
    }
    ctx.output.line(format!("Token: {}", created.token));
    ctx.output
        .line("Save this token now; it cannot be shown again.");
    Ok(())
}

async fn revoke(args: TokenRevokeArgs, ctx: &Context) -> Result<()> {
    if !args.yes && !confirm(args.id)? {
        return Err(Error::Invalid(
            "token revocation cancelled; pass --yes to confirm without a prompt".to_string(),
        ));
    }
    ctx.client
        .delete::<serde_json::Value>(&paths::token(args.id))
        .await?;
    if ctx.output.is_json() {
        return ctx.output.json(&json!({ "revoked": args.id }));
    }
    ctx.output.line(format!("Revoked token {}.", args.id));
    Ok(())
}

fn confirm(id: i32) -> Result<bool> {
    if !std::io::stdin().is_terminal() {
        return Ok(false);
    }
    eprint!("Revoke token {id}? [y/N] ");
    std::io::stderr().flush()?;
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}
