//! `rustrak.toml`: the declarative bootstrap file, read once at startup from
//! the path in `RUSTRAK_CONFIG`.
//!
//! Parsing refuses everything no database could accept (a misspelled field,
//! two projects with one id, a malformed token) before a database is touched,
//! so a mistake stops the server with a message instead of half applying.
//! What depends on the database is checked when it is applied
//! ([`super::apply`]).

use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use slug::slugify;
use uuid::Uuid;

use crate::models::token::is_valid_token_format;
use crate::models::{Product, SELECTABLE_PLATFORMS};

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct FileError(String);

#[derive(Debug, Default)]
pub struct BootstrapFile {
    pub superuser: Option<SuperuserSpec>,
    pub projects: Vec<ProjectSpec>,
    pub tokens: Vec<TokenSpec>,
}

/// A project, identified by its `id`: the DSN is `<sentry_key>@<host>/<id>`,
/// so both are fixed here to make the DSN known before the server starts.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectSpec {
    pub id: i32,
    pub name: String,
    /// Omitted: derived from the name when created, and never changed after.
    pub slug: Option<String>,
    /// Omitted: a random one when created. Accepts the DSN's 32 hex
    /// characters or the hyphenated form.
    pub sentry_key: Option<Uuid>,
    pub platform: Option<String>,
    pub disabled_products: Option<Vec<Product>>,
}

/// An instance API token the file owns, identified by its `name`.
pub struct TokenSpec {
    pub name: String,
    /// The raw value: 40 lowercase hex characters.
    pub token: String,
    pub description: Option<String>,
}

pub struct SuperuserSpec {
    pub email: String,
    pub password: String,
}

const REDACTED: &str = "<redacted>";

impl fmt::Debug for TokenSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenSpec")
            .field("name", &self.name)
            .field("token", &REDACTED)
            .field("description", &self.description)
            .finish()
    }
}

impl fmt::Debug for SuperuserSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SuperuserSpec")
            .field("email", &self.email)
            .field("password", &REDACTED)
            .finish()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    #[serde(default)]
    bootstrap: RawBootstrap,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBootstrap {
    superuser: Option<RawSuperuser>,
    #[serde(default)]
    projects: Vec<ProjectSpec>,
    #[serde(default)]
    tokens: Vec<RawToken>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawToken {
    name: String,
    token: Option<String>,
    token_file: Option<PathBuf>,
    description: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSuperuser {
    email: String,
    password: Option<String>,
    password_file: Option<PathBuf>,
}

impl BootstrapFile {
    /// Reads and parses the file at `path`.
    pub fn load(path: &Path) -> Result<Self, FileError> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| FileError(format!("cannot read {}: {e}", path.display())))?;
        Self::parse(&text).map_err(|e| FileError(format!("{}: {e}", path.display())))
    }

    pub fn parse(text: &str) -> Result<Self, FileError> {
        let doc: Document = toml::from_str(text).map_err(|e| syntax_error(text, &e))?;
        let raw = doc.bootstrap;
        let file = Self {
            superuser: raw.superuser.map(resolve_superuser).transpose()?,
            projects: raw.projects,
            tokens: raw
                .tokens
                .into_iter()
                .map(resolve_token)
                .collect::<Result<_, _>>()?,
        };
        file.validate_projects()?;
        file.validate_tokens()?;
        Ok(file)
    }

    fn validate_projects(&self) -> Result<(), FileError> {
        let mut ids = HashSet::new();
        let mut keys = HashSet::new();
        let mut slugs = HashSet::new();
        let mut names = HashSet::new();
        for p in &self.projects {
            let at = |what: String| FileError(format!("bootstrap.projects id {}: {what}", p.id));
            if p.id <= 0 {
                return Err(at("id must be a positive integer".into()));
            }
            if !ids.insert(p.id) {
                return Err(at(format!("id {} is declared twice", p.id)));
            }
            let name = p.name.trim();
            if name.is_empty() {
                return Err(at("name cannot be empty".into()));
            }
            // The limit `ProjectService` enforces, in bytes like it.
            if name.len() > 255 {
                return Err(at("name cannot exceed 255 bytes".into()));
            }
            if !names.insert(name) {
                return Err(at(format!("name {name:?} is declared twice")));
            }
            if let Some(platform) = p.platform.as_deref() {
                if !SELECTABLE_PLATFORMS.contains(&platform) {
                    return Err(at(format!("platform {platform:?} is not a known platform")));
                }
            }
            if p.disabled_products
                .iter()
                .flatten()
                .any(|&off| off == Product::Issues)
            {
                return Err(at(
                    "issues cannot be in disabled_products: a project always keeps its errors"
                        .into(),
                ));
            }
            if let Some(key) = p.sentry_key {
                if !keys.insert(key) {
                    return Err(at(format!("sentry_key {} is declared twice", key.simple())));
                }
            }
            // Compared as stored: "App" and "app" are one slug.
            if let Some(slug) = p.slug.as_deref().map(slugify) {
                // Creation would derive one from the name, and every later
                // start would then try to set this one and fail.
                if slug.is_empty() {
                    return Err(at(
                        "slug has no letters or digits; set one, or leave slug out".into(),
                    ));
                }
                if !slugs.insert(slug.clone()) {
                    return Err(at(format!("slug {slug:?} is declared twice")));
                }
            }
        }
        Ok(())
    }

    fn validate_tokens(&self) -> Result<(), FileError> {
        let mut names = HashSet::new();
        let mut values = HashSet::new();
        for t in &self.tokens {
            if !names.insert(t.name.as_str()) {
                return Err(FileError(format!(
                    "bootstrap.tokens: name {:?} is declared twice",
                    t.name
                )));
            }
            // Never the value itself in the message: it is a credential.
            if !values.insert(t.token.as_str()) {
                return Err(FileError(format!(
                    "bootstrap.tokens {:?}: its token is the same as another token's",
                    t.name
                )));
            }
        }
        Ok(())
    }
}

/// `toml`'s own message quotes the offending line, which can be the one
/// holding a token or a password, and a type or value mismatch names the
/// value itself (``invalid type: integer `123`, expected a string``). This
/// keeps the line number, the key on it and what was expected: never the
/// value.
fn syntax_error(text: &str, e: &toml::de::Error) -> FileError {
    let message = e.message().trim_end();
    let reason = match message.split_once(", expected ") {
        Some((found, expected))
            if found.starts_with("invalid type") || found.starts_with("invalid value") =>
        {
            format!("expected {expected}")
        }
        _ => message.to_string(),
    };
    let Some(span) = e.span() else {
        return FileError(reason.to_string());
    };
    let start = span.start.min(text.len());
    let line = text[..start].matches('\n').count() + 1;
    let key = text
        .lines()
        .nth(line - 1)
        .and_then(|l| l.split_once('='))
        .map(|(k, _)| k.trim())
        .filter(|k| !k.is_empty());
    match key {
        Some(key) => FileError(format!("line {line} ({key}): {reason}")),
        None => FileError(format!("line {line}: {reason}")),
    }
}

fn resolve_token(raw: RawToken) -> Result<TokenSpec, FileError> {
    let name = raw.name.trim().to_string();
    if name.is_empty() {
        return Err(FileError(
            "bootstrap.tokens: name cannot be empty".to_string(),
        ));
    }
    let at = format!("bootstrap.tokens {name:?}");
    let token = secret(raw.token, raw.token_file, "token", &at)?;
    if !is_valid_token_format(&token) {
        return Err(FileError(format!(
            "{at}: token must be 40 lowercase hex characters"
        )));
    }
    Ok(TokenSpec {
        name,
        token,
        description: raw.description,
    })
}

fn resolve_superuser(raw: RawSuperuser) -> Result<SuperuserSpec, FileError> {
    let at = "bootstrap.superuser";
    let password = secret(raw.password, raw.password_file, "password", at)?;
    if password.is_empty() {
        return Err(FileError(format!("{at}: password cannot be empty")));
    }
    Ok(SuperuserSpec {
        email: raw.email,
        password,
    })
}

/// A secret given inline as `<field>` or as a path in `<field>_file`, the way
/// Docker and Kubernetes mount secrets. Exactly one of the two.
fn secret(
    inline: Option<String>,
    file: Option<PathBuf>,
    field: &str,
    at: &str,
) -> Result<String, FileError> {
    match (inline, file) {
        (Some(value), None) => Ok(value),
        (None, Some(path)) => std::fs::read_to_string(&path)
            // A mounted secret usually ends with a newline nobody meant.
            .map(|s| s.trim_end_matches(['\r', '\n']).to_string())
            .map_err(|e| {
                FileError(format!(
                    "{at}: cannot read {field}_file {}: {e}",
                    path.display()
                ))
            }),
        (Some(_), Some(_)) => Err(FileError(format!(
            "{at}: set {field} or {field}_file, not both"
        ))),
        (None, None) => Err(FileError(format!(
            "{at}: {field} or {field}_file is required"
        ))),
    }
}
