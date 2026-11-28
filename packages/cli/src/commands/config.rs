//! Read and edit the local config file.

use crate::cli::{ConfigCommand, ConfigRemoveArgs, ConfigSetArgs, ConfigShowArgs, ConfigUseArgs};
use crate::commands::Context;
use crate::config::{self, ConfigFile, Profile};
use crate::error::{Error, Result};

pub fn run(command: ConfigCommand, ctx: &Context) -> Result<()> {
    match command {
        ConfigCommand::List => list(ctx),
        ConfigCommand::Show(args) => show(args, ctx),
        ConfigCommand::Use(args) => r#use(args, ctx),
        ConfigCommand::Path => path(ctx),
        ConfigCommand::Set(args) => set(args, ctx),
        ConfigCommand::Remove(args) => remove(args, ctx),
    }
}

fn list(ctx: &Context) -> Result<()> {
    let file = ConfigFile::load()?;

    if ctx.output.is_json() {
        let profiles: Vec<_> = file
            .profiles
            .iter()
            .map(|(name, profile)| {
                serde_json::json!({
                    "name": name,
                    "url": profile.url,
                    "current": file.current.as_deref() == Some(name.as_str()),
                    "has_token": profile.token.is_some(),
                })
            })
            .collect();
        return ctx.output.json(&profiles);
    }

    if file.profiles.is_empty() {
        println!("No profiles. Run `rustrak auth login --url <instance>`.");
        return Ok(());
    }

    let mut table = ctx.output.table(&["", "NAME", "URL", "TOKEN"]);
    for (name, profile) in &file.profiles {
        let marker = if file.current.as_deref() == Some(name.as_str()) {
            "*"
        } else {
            ""
        };
        let token = if profile.token.is_some() { "yes" } else { "no" };
        table.add_row([marker, name, &profile.url, token]);
    }
    println!("{table}");
    Ok(())
}

fn show(args: ConfigShowArgs, ctx: &Context) -> Result<()> {
    let file = ConfigFile::load()?;
    let name = args
        .name
        .or_else(|| file.current.clone())
        .ok_or_else(|| Error::Config("no profile is selected".to_string()))?;
    let profile = file
        .profiles
        .get(&name)
        .ok_or_else(|| Error::Config(format!("no profile named {name:?}")))?;

    if ctx.output.is_json() {
        return ctx.output.json(&serde_json::json!({
            "name": name,
            "url": profile.url,
            "token": profile.token.as_deref().map(mask),
            "current": file.current.as_deref() == Some(name.as_str()),
        }));
    }

    println!("Name:  {name}");
    println!("URL:   {}", profile.url);
    match &profile.token {
        Some(token) => println!("Token: {}", mask(token)),
        None => println!("Token: <none>"),
    }
    Ok(())
}

fn r#use(args: ConfigUseArgs, ctx: &Context) -> Result<()> {
    let mut file = ConfigFile::load()?;
    if !file.profiles.contains_key(&args.name) {
        return Err(Error::Config(format!(
            "no profile named {:?}; run `rustrak config list`",
            args.name
        )));
    }
    file.current = Some(args.name.clone());
    file.save()?;

    if ctx.output.is_json() {
        return ctx
            .output
            .json(&serde_json::json!({ "current": args.name }));
    }
    println!("Now using profile {:?}.", args.name);
    Ok(())
}

fn path(ctx: &Context) -> Result<()> {
    let path = ConfigFile::path()?;
    if ctx.output.is_json() {
        return ctx.output.json(&serde_json::json!({ "path": path }));
    }
    println!("{}", path.display());
    Ok(())
}

fn set(args: ConfigSetArgs, ctx: &Context) -> Result<()> {
    if args.url.is_none() && args.token.is_none() {
        return Err(Error::Invalid(
            "nothing to set; pass --url or --token".to_string(),
        ));
    }

    let mut file = ConfigFile::load()?;
    let name = args
        .profile_name
        .or_else(|| file.current.clone())
        .unwrap_or_else(|| "default".to_string());

    let mut profile = file.profiles.get(&name).cloned().unwrap_or_default();
    if let Some(url) = args.url {
        profile.url = config::normalise_url(&url);
    }
    if let Some(token) = args.token {
        profile.token = Some(token);
    }
    file.upsert(&name, profile);
    file.save()?;

    if ctx.output.is_json() {
        return ctx.output.json(&serde_json::json!({ "profile": name }));
    }
    println!("Updated profile {name:?}.");
    Ok(())
}

fn remove(args: ConfigRemoveArgs, ctx: &Context) -> Result<()> {
    let mut file = ConfigFile::load()?;
    if file.profiles.remove(&args.name).is_none() {
        return Err(Error::Config(format!("no profile named {:?}", args.name)));
    }
    if file.current.as_deref() == Some(args.name.as_str()) {
        file.current = None;
    }
    file.save()?;

    if ctx.output.is_json() {
        return ctx
            .output
            .json(&serde_json::json!({ "removed": args.name }));
    }
    println!("Removed profile {:?}.", args.name);
    Ok(())
}

/// `abcd…wxyz`: enough to recognise a token in a list, not enough to use one.
fn mask(token: &str) -> String {
    if token.len() <= 12 {
        return "*".repeat(token.len());
    }
    format!("{}…{}", &token[..4], &token[token.len() - 4..])
}

/// A profile for a URL that has none yet, so `auth login` can create one.
pub fn profile_for(url: &str) -> Profile {
    Profile {
        url: config::normalise_url(url),
        token: None,
        email: None,
    }
}

#[cfg(test)]
mod tests {
    use super::mask;

    #[test]
    fn a_token_is_masked_at_both_ends() {
        let masked = mask("0123456789abcdef0123456789abcdef01234567");
        assert!(masked.starts_with("0123"));
        assert!(masked.ends_with("4567"));
        assert!(masked.contains('…'));
        assert!(!masked.contains("89abcdef"));
    }

    #[test]
    fn a_short_value_is_fully_masked() {
        assert_eq!(mask("short"), "*****");
    }
}
