//! Sign in, sign out, and report who the stored token belongs to.
//!
//! The API's rule shapes this whole module: `POST /api/tokens` refuses a caller
//! that authenticated with a token ("API tokens can only be created from a
//! signed-in session"). So `auth login` cannot just ask for a token — it signs
//! in with the password, keeps the session cookie, and trades *that* for a
//! long-lived token. The token then replaces the password everywhere else.
//!
//! Two consequences the code has to respect:
//! - the login client needs a cookie jar, so it is built by [`Client::new`] with
//!   `reqwest`'s `cookies` feature rather than the plain one;
//! - the session is short-lived and single-use here, so it is never stored. Only
//!   the minted token is.

use rpassword::prompt_password;

use crate::api::{self, AuthResponse, CreatedToken, CurrentUser, MyAccess};
use crate::cli::{AuthCommand, LoginArgs, LogoutArgs};
use crate::client::Client;
use crate::commands::Context;
use crate::config::{self, ConfigFile, Profile};
use crate::error::{Error, Result};

pub async fn run(command: AuthCommand, ctx: &Context) -> Result<()> {
    match command {
        AuthCommand::Login(args) => login(args, ctx).await,
        AuthCommand::Status => status(ctx).await,
        AuthCommand::Whoami => whoami(ctx).await,
        AuthCommand::Logout(args) => logout(args, ctx).await,
    }
}

async fn login(args: LoginArgs, ctx: &Context) -> Result<()> {
    let url = config::normalise_url(&args.url.clone().unwrap_or_else(|| ctx.url.clone()));

    // A token handed straight in: no password, no session, nothing to mint.
    if let Some(token) = args.token.clone() {
        let client = Client::new(url.clone(), Some(token.clone()))?;
        // There is no bearer endpoint that names its own user (`/auth/me` is
        // session-only), so a token login can only prove the token *works*.
        let access = verify(&client).await?;
        store(&args.profile_name, &url, &token, None)?;
        report_token_login(ctx, &args.profile_name, &url, &access);
        return Ok(());
    }

    let (token, user) = mint_token(&url, &args).await?;

    // Verify what was minted before it is trusted: a wrong port or a proxy
    // would otherwise store a token that fails on the next command.
    let client = Client::new(url.clone(), Some(token.clone()))?;
    let access = verify(&client).await?;

    store(&args.profile_name, &url, &token, Some(&user.email))?;
    report_signed_in(ctx, &args.profile_name, &url, &user, &access);
    Ok(())
}

/// Signs in and trades the session for a token, returning the token and the
/// identity the login answered with.
async fn mint_token(url: &str, args: &LoginArgs) -> Result<(String, CurrentUser)> {
    let email = match &args.email {
        Some(email) => email.clone(),
        None => prompt("Email: ")?,
    };

    let password = match &args.password {
        Some(password) => password.clone(),
        None => prompt_password("Password: ")
            .map_err(|e| Error::Invalid(format!("could not read the password: {e}")))?,
    };

    if email.trim().is_empty() {
        return Err(Error::Invalid("an email is required".to_string()));
    }
    if password.is_empty() {
        return Err(Error::Invalid("a password is required".to_string()));
    }

    // The cookie jar is the whole point: the token endpoint below authorizes
    // from the session this login sets.
    let client = Client::new(url.to_string(), None)?;

    let response = client
        .post::<_, AuthResponse>(
            api::paths::login(),
            &serde_json::json!({ "email": email.trim(), "password": password }),
        )
        .await
        .map_err(|e| match e {
            Error::Api { status: 401, .. } => {
                Error::Unauthenticated("invalid email or password".to_string())
            }
            Error::Api {
                status: 403,
                message,
                ..
            } => Error::Unauthenticated(message),
            other => other,
        })?;

    let description = args
        .description
        .clone()
        .unwrap_or_else(|| format!("rustrak CLI on {}", hostname()));

    let body = serde_json::json!({ "description": description });
    let created: CreatedToken = client.post(api::paths::tokens(), &body).await.map_err(|e| {
        match e {
            // The documented refusal when a server has password logins off but
            // somehow accepted one, or a proxy rewrote the request.
            Error::Api { status: 403, message, .. } => Error::Unauthenticated(format!(
                "{message}\nMint a token in the dashboard and run `rustrak auth login --token <TOKEN>`"
            )),
            other => other,
        }
    })?;

    Ok((created.token, response.user))
}

/// Confirms a token works, over the one access endpoint that accepts a bearer
/// credential. `/auth/me` does not: it resolves the session cookie only, so a
/// token-authorized CLI would get a 401 from it every time.
async fn verify(client: &Client) -> Result<MyAccess> {
    client.get(api::paths::access_me(), &[]).await
}

fn store(profile_name: &str, url: &str, token: &str, email: Option<&str>) -> Result<()> {
    let mut file = ConfigFile::load()?;
    file.upsert(
        profile_name,
        Profile {
            url: url.to_string(),
            token: Some(token.to_string()),
            email: email.map(str::to_string),
        },
    );
    file.save()
}

fn report_signed_in(
    ctx: &Context,
    profile_name: &str,
    url: &str,
    user: &CurrentUser,
    access: &MyAccess,
) {
    if ctx.output.is_json() {
        let _ = ctx.output.json(&serde_json::json!({
            "profile": profile_name,
            "url": url,
            "user": {
                "id": user.id,
                "email": user.email,
                "role": user.role,
                "is_admin": user.is_admin,
            },
            "is_owner": access.is_owner,
        }));
        return;
    }
    println!("Signed in to {url} as {} ({})", user.email, user.role);
    println!("Stored as profile {profile_name:?}.");
}

fn report_token_login(ctx: &Context, profile_name: &str, url: &str, access: &MyAccess) {
    if ctx.output.is_json() {
        let _ = ctx.output.json(&serde_json::json!({
            "profile": profile_name,
            "url": url,
            "authenticated": true,
            "is_owner": access.is_owner,
        }));
        return;
    }
    println!("Token accepted by {url}.");
    println!("Stored as profile {profile_name:?}.");
}

async fn status(ctx: &Context) -> Result<()> {
    let selected = ctx.profile.clone();
    let has_token = ctx.client.has_token();

    // The email comes from the profile: the API reports no identity on the
    // bearer path, so asking it would only ever answer "not authenticated".
    let email = ConfigFile::load()?
        .select(selected.as_deref())
        .ok()
        .flatten()
        .and_then(|(_, profile)| profile.email);

    let access = if has_token {
        verify(&ctx.client).await.ok()
    } else {
        None
    };

    if ctx.output.is_json() {
        return ctx.output.json(&serde_json::json!({
            "url": ctx.url,
            "profile": selected,
            "authenticated": access.is_some(),
            "email": email,
            "is_owner": access.as_ref().map(|a| a.is_owner),
        }));
    }

    println!("URL:      {}", ctx.url);
    if let Some(name) = &selected {
        println!("Profile:  {name}");
    }
    match (&email, &access) {
        (Some(email), Some(access)) => {
            let role = if access.is_owner { "owner" } else { "member" };
            println!("User:     {email} ({role})");
        }
        (None, Some(access)) => {
            let role = if access.is_owner { "owner" } else { "member" };
            println!("User:     <token login> ({role})");
        }
        (_, None) if has_token => println!("Token:    <rejected>"),
        _ => println!("Token:    <not signed in>"),
    }
    Ok(())
}

async fn whoami(ctx: &Context) -> Result<()> {
    ctx.require_auth()?;
    let access = verify(&ctx.client).await?;

    let email = ConfigFile::load()?
        .select(ctx.profile.as_deref())
        .ok()
        .flatten()
        .and_then(|(_, profile)| profile.email);

    if ctx.output.is_json() {
        return ctx.output.json(&serde_json::json!({
            "email": email,
            "is_owner": access.is_owner,
            "instance_permissions": access.instance.len(),
            "projects": access.projects.len(),
        }));
    }

    match &email {
        Some(email) => println!("{email}"),
        None => println!("<token login: no email recorded>"),
    }
    if access.is_owner {
        println!("Access: owner (everything)");
    } else {
        println!("Instance permissions: {}", access.instance.len());
        println!("Projects reachable:   {}", access.projects.len());
    }
    Ok(())
}

async fn logout(args: LogoutArgs, ctx: &Context) -> Result<()> {
    let mut file = ConfigFile::load()?;
    let name = args
        .profile_name
        .clone()
        .or_else(|| file.current.clone())
        .ok_or_else(|| Error::Config("no profile is selected".to_string()))?;

    let Some(profile) = file.profiles.get(&name).cloned() else {
        return Err(Error::Config(format!("no profile named {name:?}")));
    };

    // Revoking on the server is optional because it needs the token to still
    // work, and a user signing out of a dead instance still wants the file
    // cleaned. It is destructive and server-visible, so it is asked about.
    if args.revoke {
        if let Some(token) = &profile.token {
            let client = Client::new(profile.url.clone(), Some(token.clone()))?;
            if let Ok(tokens) = client
                .get::<Vec<api::AuthToken>>(api::paths::tokens(), &[])
                .await
            {
                // The list is masked, so match by prefix: the token's own id
                // is not otherwise known to the CLI.
                let prefix: String = token.chars().take(8).collect();
                if let Some(found) = tokens.iter().find(|t| t.token_prefix == prefix) {
                    if args.yes || confirm(&format!("Revoke token {} on the server?", found.id))? {
                        let _ = client
                            .delete::<serde_json::Value>(&api::paths::token(found.id))
                            .await;
                    }
                }
            }
        }
    }

    if let Some(stored) = file.profiles.get_mut(&name) {
        stored.token = None;
    }
    file.save()?;

    if ctx.output.is_json() {
        return ctx
            .output
            .json(&serde_json::json!({ "profile": name, "signed_out": true }));
    }
    println!("Signed out of profile {name:?}.");
    Ok(())
}

/// A terminal prompt. Reads a line from stdin; empty stdin is an error rather
/// than an empty value, so `rustrak auth login` in a pipeline says so.
fn prompt(label: &str) -> Result<String> {
    use std::io::Write;
    print!("{label}");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    if line.is_empty() {
        return Err(Error::Invalid(format!("no value for {label:?}")));
    }
    Ok(line.trim().to_string())
}

/// A yes/no question. `--yes` skips it; a non-terminal stdin answers yes so a
/// script does not hang.
fn confirm(question: &str) -> Result<bool> {
    use std::io::IsTerminal;
    use std::io::Write;
    if !std::io::stdin().is_terminal() {
        return Ok(true);
    }
    print!("{question} [y/N] ");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(matches!(
        line.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

fn hostname() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "a terminal".to_string())
}
