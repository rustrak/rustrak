//! Everything a command needs that is not its own flags: the resolved client,
//! the project it acts on, and the output format.
//!
//! A command returns [`Result<()>`]; the dispatcher builds this once, so no
//! handler reads the environment or parses a project reference twice.

pub mod alert;
pub mod auth;
pub mod config;
pub mod dispatch;
pub mod event;
pub mod issue;
pub mod log;
pub mod monitor;
pub mod perf;
pub mod project;
pub mod release;
pub mod send;
pub mod team;
pub mod token;
pub mod trace;

use crate::api::{self, Project};
use crate::cli::GlobalArgs;
use crate::client::Client;
// `config` names two modules here: this module's own `config` command and the
// crate's `config` file reader. The file reader is what a context needs.
use crate::config as app_config;
use crate::error::{Error, Result};
use crate::output::Output;

pub struct Context {
    pub client: Client,
    pub output: Output,
    pub url: String,
    /// The profile that supplied the URL and token, if one did.
    pub profile: Option<String>,
    /// The project id or slug named by `--project`, if any.
    pub project_hint: Option<String>,
    pub json: bool,
}

impl Context {
    pub fn build(global: &GlobalArgs) -> Result<Self> {
        let resolved = app_config::resolve(
            global.profile.as_deref(),
            global.url.as_deref(),
            global.token.as_deref(),
        )?;

        let client = Client::new(resolved.url.clone(), resolved.token.clone())?;

        Ok(Self {
            client,
            output: Output::resolve(global.json, global.table),
            url: resolved.url,
            profile: resolved.profile,
            project_hint: global.project.clone(),
            json: global.json,
        })
    }

    /// A client with no credential, for login and health.
    pub fn anonymous(&self) -> Result<Client> {
        Ok(Client::new(self.url.clone(), None)?.anonymous())
    }

    /// Fails with a pointer to `auth login` when no token is stored, so a
    /// first-time user is told what to run rather than shown a 401.
    pub fn require_auth(&self) -> Result<()> {
        if self.client.has_token() {
            Ok(())
        } else {
            Err(Error::Unauthenticated(format!(
                "not signed in to {}; run `rustrak auth login --url {}`",
                self.url, self.url
            )))
        }
    }

    /// The project id a command acts on: `--project`, the argument, or the
    /// project `project use` stored.
    pub async fn project_id(&self, explicit: Option<&str>) -> Result<i32> {
        let reference = explicit
            .map(str::to_string)
            .or_else(|| self.project_hint.clone())
            .or_else(stored_project)
            .ok_or_else(|| {
                Error::Invalid(
                    "no project given; pass one or set a default with `rustrak project use <id|slug>`"
                        .to_string(),
                )
            })?;

        if let Ok(id) = reference.parse::<i32>() {
            return Ok(id);
        }

        // A slug: find the project whose slug or name matches, so both of the
        // things the dashboard shows accept a reference.
        let page: api::Page<Project> = self
            .client
            .get(api::paths::projects(), &[("per_page", "100".to_string())])
            .await?;
        page.items
            .into_iter()
            .find(|p| {
                p.slug.eq_ignore_ascii_case(&reference) || p.name.eq_ignore_ascii_case(&reference)
            })
            .map(|p| p.id)
            .ok_or_else(|| Error::Invalid(format!("no project matches {reference:?}")))
    }

    /// The project resolved to its full record, for a command that prints it.
    pub async fn project(&self, explicit: Option<&str>) -> Result<Project> {
        let id = self.project_id(explicit).await?;
        self.client.get(&api::paths::project(id), &[]).await
    }
}

/// The project id `project use` wrote. Kept out of the config file's profile
/// so switching projects does not rewrite the token file.
fn stored_project() -> Option<String> {
    std::env::var("RUSTRAK_PROJECT")
        .ok()
        .filter(|p| !p.is_empty())
}
