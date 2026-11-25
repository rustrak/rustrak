//! `rustrak-plugin.toml`: what a plugin declares about itself.
//!
//! Parsing refuses anything the host could not honour (a misspelled field, a
//! route without a leading slash, a requirement on a permission the plugin
//! never declares) before a byte of the plugin runs.

use std::collections::HashSet;

use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};

pub use crate::models::rbac::{is_slug, Action, Scope};

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct ManifestError(String);

fn fail<T>(msg: impl Into<String>) -> Result<T, ManifestError> {
    Err(ManifestError(msg.into()))
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub plugin: PluginInfo,
    #[serde(default)]
    pub capabilities: Capabilities,
    /// Not in the TOML: read from the code ([`Manifest::read_code`]).
    #[serde(skip)]
    pub routes: Vec<Route>,
    #[serde(default)]
    pub permissions: Vec<PermissionSpec>,
    /// What the admin can change; Rustrak draws the form.
    #[serde(default)]
    pub settings: Vec<super::settings::SettingSpec>,
    #[serde(default)]
    pub ui: Option<Ui>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: Version,
    pub description: Option<String>,
    /// The Rustrak versions this plugin runs on.
    pub rustrak: VersionReq,
    /// The `rustrak:plugin` world it implements.
    pub world: Version,
    pub repository: Option<String>,
    pub author: Option<String>,
    /// A `@rustrak/ui` icon name, for its card in Settings › Plugins.
    pub icon: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
    /// Not in the TOML: whether the code imports its database.
    #[serde(skip)]
    pub sql: bool,
    #[serde(default = "default_storage_mb")]
    pub storage_mb: u32,
}

impl Default for Capabilities {
    fn default() -> Self {
        Self {
            sql: false,
            storage_mb: default_storage_mb(),
        }
    }
}

fn default_storage_mb() -> u32 {
    512
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Auth {
    /// Anyone; a project is found by its public key.
    Public,
    /// A signed-in user or an API token.
    User,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    pub method: Method,
    pub path: String,
    pub auth: Auth,
    pub scope: Scope,
    /// One of the plugin's own permissions, `resource:action`.
    pub requires: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PermissionSpec {
    pub resource: String,
    pub scope: Scope,
    pub actions: Vec<Action>,
}

/// Places in the dashboard's own pages a plugin's widgets draw into.
pub const ZONES: &[&str] = &[
    "issue.details.after",
    "project.overview.after",
    // In the plugin's own settings, under each project it runs in.
    "plugin.setup",
];

/// What the plugin adds to the dashboard; generated from its `ui/` folder.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ui {
    #[serde(default)]
    pub pages: Vec<UiPage>,
    #[serde(default)]
    pub widgets: Vec<UiWidget>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UiPage {
    /// Under the plugin's own URL: "" is its first page, "funnels/:funnelId" a deeper one.
    pub path: String,
    pub scope: Scope,
    /// Shown in the sidebar of its scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidebar: Option<Sidebar>,
    /// One of the plugin's permissions, `resource:action`: the page, and its
    /// place in the sidebar, are only for whoever holds it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Sidebar {
    /// A key of the plugin's messages.
    pub label: String,
    /// A `@rustrak/ui` icon name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rank: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UiWidget {
    pub zone: String,
}

impl Ui {
    pub fn is_empty(&self) -> bool {
        self.pages.is_empty() && self.widgets.is_empty()
    }
}

/// A page's path segment: lowercase words, or `:name` for a parameter.
fn is_segment(seg: &str) -> bool {
    match seg.strip_prefix(':') {
        Some(name) => {
            name.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
                && name.chars().all(|c| c.is_ascii_alphanumeric())
        }
        None => {
            !seg.is_empty()
                && seg
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        }
    }
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self, ManifestError> {
        let manifest: Manifest = toml::from_str(text).map_err(|e| ManifestError(e.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Whether this host's version is in the plugin's `rustrak` range.
    pub fn supports_host(&self, host: &Version) -> Result<(), ManifestError> {
        if self.plugin.rustrak.matches(host) {
            return Ok(());
        }
        fail(format!(
            "{} {} needs Rustrak {}, this is {host}",
            self.plugin.id, self.plugin.version, self.plugin.rustrak
        ))
    }

    /// Takes what the code declares, checking its routes against the rest.
    pub fn read_code(&mut self, code: super::inspect::Code) -> Result<(), ManifestError> {
        let mut routes = HashSet::new();
        for route in &code.routes {
            let key = format!("{} {}", route.method.as_str(), route.path);
            if !route.path.starts_with('/') || route.path.contains(['{', '}', '?', '#']) {
                return fail(format!(
                    "route {key}: a path starts with / and has no placeholders"
                ));
            }
            if !routes.insert(key.clone()) {
                return fail(format!("route {key} is declared twice"));
            }
            if let Some(req) = &route.requires {
                self.check_requirement(req)
                    .map_err(|e| ManifestError(format!("route {key}: {e}")))?;
            }
        }
        self.routes = code.routes;
        self.capabilities.sql = code.sql;
        Ok(())
    }

    fn validate(&self) -> Result<(), ManifestError> {
        let p = &self.plugin;
        if !is_slug(&p.id) {
            return fail(format!(
                "plugin.id {:?} must be 2 to 32 lowercase letters, digits or dashes, starting with a letter",
                p.id
            ));
        }
        if !super::WORLDS.contains(&p.world.to_string().as_str()) {
            return fail(format!(
                "plugin.world {} is not implemented here (supported: {})",
                p.world,
                super::WORLDS.join(", ")
            ));
        }

        super::settings::validate(&self.settings).or_else(|e| fail(format!("settings: {e}")))?;

        let mut resources = HashSet::new();
        for perm in &self.permissions {
            if !is_slug(&perm.resource) {
                return fail(format!(
                    "permissions.resource {:?} is not a slug",
                    perm.resource
                ));
            }
            if perm.actions.is_empty() {
                return fail(format!("permissions {:?} lists no actions", perm.resource));
            }
            if !resources.insert(perm.resource.as_str()) {
                return fail(format!("permissions {:?} is declared twice", perm.resource));
            }
        }

        if let Some(ui) = &self.ui {
            if let Some(w) = ui
                .widgets
                .iter()
                .find(|w| !ZONES.contains(&w.zone.as_str()))
            {
                return fail(format!(
                    "ui.widgets zone {:?} is not one of {}",
                    w.zone,
                    ZONES.join(", ")
                ));
            }
            let mut pages = HashSet::new();
            for page in &ui.pages {
                let ok = page.path.is_empty() || page.path.split('/').all(is_segment);
                if !ok || !pages.insert((page.path.as_str(), page.scope)) {
                    return fail(format!("ui.pages {:?}: a unique lowercase path", page.path));
                }
                if let Some(req) = &page.requires {
                    self.check_requirement(req)?;
                }
            }
        }

        Ok(())
    }

    /// `resource:action` must name one of this plugin's permissions.
    fn check_requirement(&self, req: &str) -> Result<(), ManifestError> {
        let declared = req.split_once(':').is_some_and(|(resource, action)| {
            self.permissions.iter().any(|p| {
                p.resource == resource
                    && Action::parse(action).is_some_and(|a| p.actions.contains(&a))
            })
        });
        if declared {
            Ok(())
        } else {
            fail(format!(
                "requires {req} is not among the plugin's [[permissions]]"
            ))
        }
    }
}
