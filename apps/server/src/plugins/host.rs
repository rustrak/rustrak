//! The installed plugins: installing a package, loading them at boot, where
//! each one is active, and running one call on the blocking pool.
//!
//! On disk, under `RUSTRAK_PLUGINS_DIR`:
//!
//! ```text
//! <id>/<version>/plugin.rtp     the verified package, as installed
//! <id>/<version>/server-*.cwasm its compiled code, rebuilt when missing
//! <id>/data.sqlite              the plugin's own database, across versions
//! ```

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use semver::Version;
use sqlx::types::Json;
use tokio::sync::Semaphore;

use super::manifest::{Manifest, PermissionSpec, UiPage, UiWidget};
use super::package::Package;
use super::runtime::{CallError, Loaded, Request, Response, Runtime, SettingsSource};
use super::settings;
use super::store::PluginDb;
use crate::db::DbPool;
use crate::error::{AppError, AppResult, FieldError, FieldErrorCode};
use crate::models::rbac::{plugin_key, Action, Scope};

/// Calls one plugin runs at once; the rest wait their turn.
const CONCURRENT_CALLS: usize = 8;
/// A plugin's own logo, in its package and among its assets.
const LOGO: &str = "logo.svg";
const MAX_LOGO: usize = 256 * 1024;

/// How long a call waits for a turn before the request gets a 503.
const QUEUE_WAIT: Duration = Duration::from_secs(5);

pub struct Installed {
    pub manifest: Manifest,
    loaded: Loaded,
    turns: Semaphore,
    /// The dashboard bundle, `ui/` stripped from each path.
    ui_files: HashMap<String, Vec<u8>>,
    /// The package's hash, in the bundle's address: cached for good, the
    /// address has to change whenever the content does, same version or not.
    ui_tag: String,
    readme: BTreeMap<String, String>,
    messages: Map<String, Value>,
    /// Installed from the signed registry, rather than uploaded by hand.
    verified: bool,
    /// What it hands out to anyone (`public/` in its package): a script a
    /// website loads, say.
    public_files: HashMap<String, Vec<u8>>,
    /// The values set for its settings, read from the database at install
    /// and boot and written through: one process per database, so this is
    /// the truth and a call never waits on a query for its settings.
    stored: RwLock<StoredSettings>,
}

#[derive(Debug, Default)]
struct StoredSettings {
    instance: Map<String, Value>,
    projects: HashMap<i32, Map<String, Value>>,
}

/// One plugin's settings at one level, for the form Rustrak draws.
#[derive(Debug, serde::Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PluginSettingsView {
    /// Set at this level (the instance, or this project). No secrets.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub values: Map<String, Value>,
    /// What the plugin reads here: this level, then the instance, then the
    /// defaults. No secrets.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub resolved: Map<String, Value>,
    /// Whether each secret is set; their values never leave the server.
    pub secrets: BTreeMap<String, bool>,
    /// Required settings nothing sets yet: the plugin answers 503 until then.
    pub missing: Vec<String>,
}

impl Installed {
    /// A file it hands out to anyone, by its path under `public/`.
    pub fn public_file(&self, path: &str) -> Option<&[u8]> {
        self.public_files.get(path).map(Vec::as_slice)
    }

    /// Its settings as its code reads them: secrets included.
    pub fn settings(&self, project: Option<i32>) -> Map<String, Value> {
        let stored = self.stored.read().expect("settings lock");
        settings::resolve(
            &self.manifest.settings,
            &stored.instance,
            project.and_then(|p| stored.projects.get(&p)),
        )
    }

    fn view(&self, project: Option<i32>) -> PluginSettingsView {
        let specs = &self.manifest.settings;
        let stored = self.stored.read().expect("settings lock");
        let own = match project {
            None => stored.instance.clone(),
            Some(p) => stored.projects.get(&p).cloned().unwrap_or_default(),
        };
        let resolved = settings::resolve(
            specs,
            &stored.instance,
            project.and_then(|p| stored.projects.get(&p)),
        );
        PluginSettingsView {
            values: settings::public(specs, own),
            missing: settings::missing(specs, &resolved),
            resolved: settings::public(specs, resolved),
            secrets: specs
                .iter()
                .filter(|s| s.kind == settings::SettingType::Secret)
                .map(|s| (s.key.clone(), stored.instance.contains_key(&s.key)))
                .collect(),
        }
    }

    /// The scope of one of its permissions, if it declares it with `action`.
    pub fn permission_scope(&self, resource: &str, action: Action) -> Option<Scope> {
        self.manifest
            .permissions
            .iter()
            .find(|p| p.resource == resource && p.actions.contains(&action))
            .map(|p| p.scope)
    }

    /// A file of the plugin's dashboard bundle.
    pub fn ui_file(&self, tag: &str, path: &str) -> Option<&[u8]> {
        (tag == self.ui_tag)
            .then(|| self.ui_files.get(path).map(Vec::as_slice))
            .flatten()
    }
}

/// What the dashboard needs to know about one installed plugin.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PluginSummary {
    pub id: String,
    pub name: String,
    pub version: String,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub description: Option<String>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub author: Option<String>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub repository: Option<String>,
    /// A `@rustrak/ui` icon name.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub icon: Option<String>,
    /// Its own logo (`logo.svg` in its package), shown instead of `icon`.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub logo: Option<String>,
    /// Its page in the catalog, Markdown, by language: English always.
    pub readme: BTreeMap<String, String>,
    /// Whether it keeps its own database.
    pub database: bool,
    /// The permissions it adds to roles.
    pub permissions: Vec<PermissionSpec>,
    /// Active in every project, those created later included.
    pub all_projects: bool,
    /// Projects it was turned on in one by one.
    pub projects: Vec<i32>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub ui: Option<PluginUi>,
    /// Installed from the signed registry; `false` for one uploaded by hand.
    pub verified: bool,
    /// What its settings form is drawn from.
    pub settings: Vec<settings::SettingSpec>,
    /// Its texts, by language (`messages/<locale>.json`): they label its
    /// settings and permissions, and its pages.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub messages: Map<String, Value>,
}

#[derive(Debug, serde::Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PluginUi {
    /// The bundle to load, a script that registers the plugin.
    pub entry: String,
    /// Its stylesheet, when it has one.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub styles: Option<String>,
    pub pages: Vec<UiPage>,
    pub widgets: Vec<UiWidget>,
}

pub struct PluginHost {
    runtime: Arc<Runtime>,
    dir: PathBuf,
    host_version: Version,
    installed: RwLock<HashMap<String, Arc<Installed>>>,
}

impl PluginHost {
    pub fn new(dir: PathBuf) -> std::io::Result<Self> {
        let runtime =
            Runtime::new().map_err(|e| std::io::Error::other(format!("plugin runtime: {e:#}")))?;
        Ok(Self {
            runtime,
            dir,
            host_version: super::host_version(),
            installed: RwLock::default(),
        })
    }

    pub fn get(&self, id: &str) -> Option<Arc<Installed>> {
        self.installed
            .read()
            .expect("plugins lock")
            .get(id)
            .cloned()
    }

    /// Verifies, migrates, compiles and records a package. Nothing changes
    /// unless every step succeeds; installing an id again replaces it.
    pub async fn install(
        &self,
        pool: &DbPool,
        rtp: Vec<u8>,
        source: Source,
    ) -> AppResult<Arc<Installed>> {
        let installed = {
            let runtime = self.runtime.clone();
            let dir = self.dir.clone();
            let host_version = self.host_version.clone();
            tokio::task::spawn_blocking(move || {
                prepare(&runtime, &dir, &host_version, &rtp, source, true)
            })
            .await
            .map_err(|e| AppError::Internal(format!("plugin install: {e}")))??
        };
        sqlx::query(
            "INSERT INTO plugins (id, version, source) VALUES ($1, $2, $3) \
             ON CONFLICT (id) DO UPDATE SET version = EXCLUDED.version, \
             source = EXCLUDED.source, state = 'enabled', updated_at = CURRENT_TIMESTAMP",
        )
        .bind(&installed.manifest.plugin.id)
        .bind(installed.manifest.plugin.version.to_string())
        .bind(source.as_str())
        .execute(pool)
        .await?;
        let id = installed.manifest.plugin.id.clone();
        let version = installed.manifest.plugin.version.to_string();
        let installed = self.activate(pool, installed).await?;
        // The version it replaced, if any: its data stays.
        remove_entries(&self.dir.join(&id), |name| {
            name == version || name.starts_with(DB_FILE)
        })?;
        Ok(installed)
    }

    /// Reads a prepared plugin's settings and makes it answer.
    async fn activate(&self, pool: &DbPool, installed: Installed) -> AppResult<Arc<Installed>> {
        let id = installed.manifest.plugin.id.clone();
        *installed.stored.write().expect("settings lock") = load_settings(pool, &id).await?;
        let installed = Arc::new(installed);
        self.installed
            .write()
            .expect("plugins lock")
            .insert(id, installed.clone());
        Ok(installed)
    }

    /// Loads every enabled plugin. One that fails to load is logged and left
    /// out: a broken plugin never stops the server from starting.
    pub async fn boot(&self, pool: &DbPool) -> AppResult<()> {
        let rows: Vec<(String, String, String)> =
            sqlx::query_as("SELECT id, version, source FROM plugins WHERE state = 'enabled'")
                .fetch_all(pool)
                .await?;
        for (id, version, source) in rows {
            let runtime = self.runtime.clone();
            let dir = self.dir.clone();
            let host_version = self.host_version.clone();
            let path = package_path(&dir, &id, &version);
            let source = Source::parse(&source);
            let loaded = tokio::task::spawn_blocking(move || {
                let rtp = std::fs::read(&path)
                    .map_err(|e| AppError::Internal(format!("{}: {e}", path.display())))?;
                prepare(&runtime, &dir, &host_version, &rtp, source, false)
            })
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
            match loaded {
                Ok(installed) => {
                    log::info!("plugin {id} {version} loaded");
                    if let Err(e) = self.activate(pool, installed).await {
                        log::error!("plugin {id} {version} was not activated: {e}");
                    }
                }
                Err(e) => log::error!("plugin {id} {version} was not loaded: {e}"),
            }
        }
        Ok(())
    }

    /// Every installed plugin, as the dashboard sees it.
    pub async fn list(&self, pool: &DbPool) -> AppResult<Vec<PluginSummary>> {
        let rows: Vec<(String, bool)> =
            sqlx::query_as("SELECT id, all_projects FROM plugins ORDER BY id")
                .fetch_all(pool)
                .await?;
        let picked: Vec<(String, i32)> =
            sqlx::query_as("SELECT plugin_id, project_id FROM plugin_projects")
                .fetch_all(pool)
                .await?;
        Ok(rows
            .into_iter()
            .filter_map(|(id, all_projects)| {
                let installed = self.get(&id)?;
                let info = &installed.manifest.plugin;
                let tag = &installed.ui_tag;
                let ui = installed
                    .manifest
                    .ui
                    .as_ref()
                    .filter(|ui| !ui.is_empty())
                    .map(|ui| PluginUi {
                        entry: asset_url(&id, tag, "index.js"),
                        styles: installed
                            .ui_files
                            .contains_key("index.css")
                            .then(|| asset_url(&id, tag, "index.css")),
                        pages: ui.pages.clone(),
                        widgets: ui.widgets.clone(),
                    });
                let manifest = &installed.manifest;
                Some(PluginSummary {
                    name: info.name.clone(),
                    version: info.version.to_string(),
                    description: info.description.clone(),
                    author: info.author.clone(),
                    repository: info.repository.clone(),
                    icon: info.icon.clone(),
                    logo: installed
                        .ui_files
                        .contains_key(LOGO)
                        .then(|| asset_url(&id, tag, LOGO)),
                    readme: installed.readme.clone(),
                    verified: installed.verified,
                    settings: installed.manifest.settings.clone(),
                    messages: installed.messages.clone(),
                    database: manifest.capabilities.sql,
                    permissions: manifest.permissions.clone(),
                    all_projects,
                    projects: picked
                        .iter()
                        .filter(|(p, _)| *p == id)
                        .map(|(_, project)| *project)
                        .collect(),
                    ui,
                    id,
                })
            })
            .collect())
    }

    /// One installed plugin, as the dashboard sees it.
    pub async fn summary(&self, pool: &DbPool, id: &str) -> AppResult<PluginSummary> {
        self.list(pool)
            .await?
            .into_iter()
            .find(|p| p.id == id)
            .ok_or_else(|| not_found(id))
    }

    /// Makes a plugin active in every project, or in the ones it was turned
    /// on in; `projects` replaces that list, `None` keeps it.
    pub async fn set_projects(
        &self,
        pool: &DbPool,
        id: &str,
        all: bool,
        projects: Option<&[i32]>,
    ) -> AppResult<()> {
        let mut tx = crate::db::begin_write(pool).await?;
        let done = sqlx::query("UPDATE plugins SET all_projects = $1 WHERE id = $2")
            .bind(all)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        if done.rows_affected() == 0 {
            return Err(not_found(id));
        }
        if let Some(projects) = projects {
            sqlx::query("DELETE FROM plugin_projects WHERE plugin_id = $1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            for project in projects {
                sqlx::query(
                    "INSERT INTO plugin_projects (plugin_id, project_id) \
                     SELECT $1, id FROM projects WHERE id = $2",
                )
                .bind(id)
                .bind(project)
                .execute(&mut *tx)
                .await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }

    /// Removes a plugin: it stops answering at once. Its database stays for a
    /// reinstall unless `purge`.
    pub async fn uninstall(&self, pool: &DbPool, id: &str, purge: bool) -> AppResult<()> {
        let removed = self.installed.write().expect("plugins lock").remove(id);
        let mut tx = crate::db::begin_write(pool).await?;
        // Without purge the row stays, marked: its settings are its data,
        // like its database, and a reinstall finds them.
        let gone = if purge {
            sqlx::query("DELETE FROM plugins WHERE id = $1")
        } else {
            sqlx::query("UPDATE plugins SET state = 'uninstalled', all_projects = FALSE WHERE id = $1 AND state <> 'uninstalled'")
        }
        .bind(id)
        .execute(&mut *tx)
        .await?;
        if removed.is_none() && gone.rows_affected() == 0 {
            return Err(not_found(id));
        }
        sqlx::query("DELETE FROM plugin_projects WHERE plugin_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        if purge {
            // The roles granting its permissions forget them too.
            sqlx::query("DELETE FROM role_permissions WHERE resource LIKE $1")
                .bind(format!("{id}:%"))
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        drop(removed);
        let root = self.dir.join(id);
        remove_entries(&root, |name| !purge && name.starts_with(DB_FILE))?;
        if purge && root.exists() {
            std::fs::remove_dir(&root)
                .map_err(|e| AppError::Internal(format!("{}: {e}", root.display())))?;
        }
        Ok(())
    }

    /// Whether a plugin runs in a project.
    pub async fn active_in(pool: &DbPool, id: &str, project_id: i32) -> AppResult<bool> {
        let active: Option<bool> = sqlx::query_scalar(
            "SELECT p.all_projects OR pp.project_id IS NOT NULL FROM plugins p \
             LEFT JOIN plugin_projects pp ON pp.plugin_id = p.id AND pp.project_id = $2 \
             WHERE p.id = $1 AND p.state = 'enabled'",
        )
        .bind(id)
        .bind(project_id)
        .fetch_optional(pool)
        .await?;
        Ok(active.unwrap_or(false))
    }

    /// The version installed of each plugin, by id.
    pub fn installed_versions(&self) -> HashMap<String, String> {
        self.installed
            .read()
            .expect("plugins lock")
            .iter()
            .map(|(id, p)| (id.clone(), p.manifest.plugin.version.to_string()))
            .collect()
    }

    /// The Rustrak version plugins are checked against.
    pub fn host_version(&self) -> &Version {
        &self.host_version
    }

    /// Every installed plugin's permissions, as `<plugin>:<resource>`.
    pub fn permission_catalog(&self) -> Vec<CatalogPluginPermission> {
        let installed = self.installed.read().expect("plugins lock");
        let mut all: Vec<_> = installed
            .values()
            .flat_map(|p| {
                let id = p.manifest.plugin.id.clone();
                let name = p.manifest.plugin.name.clone();
                // `permissions.<resource>.label` (and `.description`) in each
                // of its languages: Roles shows them without loading its UI.
                let text = |resource: &str, part: &str| -> BTreeMap<String, String> {
                    p.messages
                        .iter()
                        .filter_map(|(locale, m)| {
                            let s = m.pointer(&format!("/permissions/{resource}/{part}"))?;
                            Some((locale.clone(), s.as_str()?.to_string()))
                        })
                        .collect()
                };
                p.manifest
                    .permissions
                    .iter()
                    .map(|perm| CatalogPluginPermission {
                        key: plugin_key(&id, &perm.resource),
                        plugin: id.clone(),
                        plugin_name: name.clone(),
                        scope: perm.scope,
                        actions: perm.actions.clone(),
                        label: text(&perm.resource, "label"),
                        description: text(&perm.resource, "description"),
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        all.sort_by(|a, b| a.key.cmp(&b.key));
        all
    }

    /// The scope of a plugin's permission, if an installed plugin declares
    /// it with that action.
    pub fn permission_scope(&self, key: &str, action: Action) -> Option<Scope> {
        let (id, resource) = key.split_once(':')?;
        self.get(id)?.permission_scope(resource, action)
    }

    /// Each installed plugin's data, for the Storage page: its file, its
    /// quota, and its history against the window in `retention` (days by
    /// plugin id).
    pub async fn storage(
        &self,
        retention: &BTreeMap<String, u32>,
    ) -> AppResult<Vec<PluginStorage>> {
        let plugins: Vec<_> = {
            let installed = self.installed.read().expect("plugins lock");
            let mut all: Vec<_> = installed.values().cloned().collect();
            all.sort_by(|a, b| a.manifest.plugin.id.cmp(&b.manifest.plugin.id));
            all
        };
        let dir = self.dir.clone();
        let retention = retention.clone();
        tokio::task::spawn_blocking(move || {
            plugins
                .iter()
                .map(|p| {
                    let id = p.manifest.plugin.id.clone();
                    let days = retention.get(&id).copied();
                    let history = match p.loaded.db() {
                        Some(db) => db
                            .history(days.map(cutoff))
                            .map_err(|e| AppError::Internal(format!("plugin {id}: {e}")))?,
                        None => Vec::new(),
                    };
                    let file = db_path(&dir, &id);
                    let bytes = ["", "-wal"]
                        .iter()
                        .filter_map(|suffix| {
                            std::fs::metadata(format!("{}{suffix}", file.display())).ok()
                        })
                        .map(|m| m.len())
                        .sum();
                    Ok(PluginStorage {
                        name: p.manifest.plugin.name.clone(),
                        bytes,
                        quota_bytes: u64::from(p.manifest.capabilities.storage_mb) * 1024 * 1024,
                        retention_days: days,
                        history,
                        id,
                    })
                })
                .collect()
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }

    /// The history rows of each plugin older than its window: what clearing
    /// would delete.
    pub async fn expired_rows(&self, windows: &BTreeMap<String, u32>) -> AppResult<i64> {
        Ok(self
            .storage(windows)
            .await?
            .iter()
            .filter(|p| windows.contains_key(&p.id))
            .flat_map(|p| p.history.iter().map(|h| h.expired))
            .sum())
    }

    /// Clears each plugin's history older than its window; the rows deleted.
    pub async fn forget_expired(&self, retention: &BTreeMap<String, u32>) -> AppResult<i64> {
        let targets: Vec<_> = retention
            .iter()
            .filter_map(|(id, days)| Some((self.get(id)?, *days)))
            .collect();
        tokio::task::spawn_blocking(move || {
            let mut removed = 0;
            for (plugin, days) in targets {
                let Some(db) = plugin.loaded.db() else {
                    continue;
                };
                removed += db.forget_before(cutoff(days)).map_err(|e| {
                    AppError::Internal(format!("plugin {}: {e}", plugin.manifest.plugin.id))
                })? as i64;
            }
            Ok(removed)
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    }

    /// One plugin's settings at the instance (`None`) or one project.
    pub fn settings_view(&self, id: &str, project: Option<i32>) -> AppResult<PluginSettingsView> {
        Ok(self.require(id)?.view(project))
    }

    fn require(&self, id: &str) -> AppResult<Arc<Installed>> {
        self.get(id).ok_or_else(|| not_found(id))
    }

    /// Writes `patch` into the instance's (`None`) or one project's values,
    /// checked against the plugin's schema: a value sets, `null` clears.
    pub async fn set_settings(
        &self,
        pool: &DbPool,
        id: &str,
        project: Option<i32>,
        patch: Map<String, Value>,
    ) -> AppResult<PluginSettingsView> {
        let plugin = self.require(id)?;
        let level = if project.is_some() {
            Scope::Project
        } else {
            Scope::Instance
        };
        let mut values = {
            let stored = plugin.stored.read().expect("settings lock");
            match project {
                None => stored.instance.clone(),
                Some(p) => stored.projects.get(&p).cloned().unwrap_or_default(),
            }
        };
        settings::apply(&plugin.manifest.settings, level, &mut values, patch).map_err(
            |errors| {
                let (field, why) = &errors[0];
                AppError::Validation(format!("{field} {why}")).with_fields(
                    errors
                        .iter()
                        .map(|(f, _)| FieldError::new(f.clone(), FieldErrorCode::Invalid))
                        .collect(),
                )
            },
        )?;
        match project {
            None => {
                let done = sqlx::query("UPDATE plugins SET settings = $1 WHERE id = $2")
                    .bind(Json(&values))
                    .bind(id)
                    .execute(pool)
                    .await?;
                if done.rows_affected() == 0 {
                    return Err(not_found(id));
                }
            }
            Some(p) => {
                sqlx::query(
                    "INSERT INTO plugin_project_settings (plugin_id, project_id, settings) \
                     VALUES ($1, $2, $3) ON CONFLICT (plugin_id, project_id) \
                     DO UPDATE SET settings = EXCLUDED.settings",
                )
                .bind(id)
                .bind(p)
                .bind(Json(&values))
                .execute(pool)
                .await?;
            }
        }
        {
            let mut stored = plugin.stored.write().expect("settings lock");
            match project {
                None => stored.instance = values,
                Some(p) => {
                    stored.projects.insert(p, values);
                }
            }
        }
        Ok(plugin.view(project))
    }

    /// One HTTP request to a plugin, off the async workers.
    pub async fn call_http(
        &self,
        plugin: Arc<Installed>,
        req: Request,
        budget: Duration,
    ) -> AppResult<Result<Response, CallError>> {
        let _turn = tokio::time::timeout(QUEUE_WAIT, plugin.turns.acquire())
            .await
            .map_err(|_| {
                AppError::Unavailable(format!("plugin {} is busy", plugin.manifest.plugin.id))
            })?
            .map_err(|e| AppError::Internal(e.to_string()))?;
        let runtime = self.runtime.clone();
        let call = plugin.clone();
        let settings = settings_source(plugin.clone());
        tokio::task::spawn_blocking(move || {
            runtime.handle_http(&call.loaded, req, budget, settings)
        })
        .await
        .map_err(|e| AppError::Internal(format!("plugin call: {e}")))
    }
}

/// Now minus `days`, in unix seconds.
fn cutoff(days: u32) -> i64 {
    chrono::Utc::now().timestamp() - i64::from(days) * 86_400
}

/// One plugin's data, in the Storage page.
#[derive(Debug, serde::Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PluginStorage {
    pub id: String,
    pub name: String,
    /// Its SQLite file, write-ahead log included.
    pub bytes: u64,
    pub quota_bytes: u64,
    /// The admin's window for its history; `null` keeps everything.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub retention_days: Option<u32>,
    /// Its tables with `created_at`, the ones retention clears.
    pub history: Vec<super::store::HistoryTable>,
}

/// One permission a plugin adds, in the catalog Roles is built from.
#[derive(Debug, serde::Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CatalogPluginPermission {
    /// `<plugin>:<resource>`, as roles store it.
    pub key: String,
    pub plugin: String,
    pub plugin_name: String,
    pub scope: Scope,
    pub actions: Vec<Action>,
    /// By language.
    pub label: BTreeMap<String, String>,
    /// By language.
    pub description: BTreeMap<String, String>,
}

/// What the plugin's `settings` interface answers: its values as JSON.
fn settings_source(plugin: Arc<Installed>) -> SettingsSource {
    Arc::new(move |project: Option<u64>| {
        let project = project.and_then(|p| i32::try_from(p).ok());
        Value::Object(plugin.settings(project)).to_string()
    })
}

/// The values set for a plugin's settings, from the database.
async fn load_settings(pool: &DbPool, id: &str) -> AppResult<StoredSettings> {
    let instance: Option<(Json<Map<String, Value>>,)> =
        sqlx::query_as("SELECT settings FROM plugins WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    let projects: Vec<(i32, Json<Map<String, Value>>)> = sqlx::query_as(
        "SELECT project_id, settings FROM plugin_project_settings WHERE plugin_id = $1",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    Ok(StoredSettings {
        instance: instance.map(|(v,)| v.0).unwrap_or_default(),
        projects: projects.into_iter().map(|(p, v)| (p, v.0)).collect(),
    })
}

fn package_path(dir: &Path, id: &str, version: &str) -> PathBuf {
    dir.join(id).join(version).join("plugin.rtp")
}

/// Everything installing and booting share, off the async workers: verify
/// the package, check this host can run it, open and migrate its database,
/// compile it. With `write`, the package is also stored for the next boot.
fn prepare(
    runtime: &Runtime,
    dir: &Path,
    host_version: &Version,
    rtp: &[u8],
    source: Source,
    write: bool,
) -> AppResult<Installed> {
    let package = Package::read(rtp).map_err(|e| AppError::Validation(e.to_string()))?;
    let mut manifest = package.manifest;
    manifest
        .supports_host(host_version)
        .map_err(|e| AppError::Validation(e.to_string()))?;
    let id = manifest.plugin.id.clone();
    let version = manifest.plugin.version.to_string();

    // Everything that can refuse the package, before anything is written.
    let mut messages = Map::new();
    for (path, bytes) in &package.files {
        let Some(file) = path.strip_prefix("messages/") else {
            continue;
        };
        let Some(locale) = file.strip_suffix(".json") else {
            return Err(AppError::Validation(format!(
                "{path}: messages are JSON files"
            )));
        };
        let texts: Value = serde_json::from_slice(bytes)
            .map_err(|e| AppError::Validation(format!("{path} is not valid JSON: {e}")))?;
        messages.insert(locale.to_string(), texts);
    }
    if package
        .files
        .get(LOGO)
        .is_some_and(|logo| logo.len() > MAX_LOGO)
    {
        return Err(AppError::Validation(format!(
            "plugin {id}: logo.svg is over {} KB",
            MAX_LOGO / 1024
        )));
    }

    // Its routes and database come from the code, read without running it.
    let code = super::inspect::inspect(&package.wasm)
        .map_err(|e| AppError::Validation(format!("plugin {id}: {e}")))?;
    manifest
        .read_code(code)
        .map_err(|e| AppError::Validation(format!("plugin {id}: {e}")))?;

    let version_dir = dir.join(&id).join(&version);
    std::fs::create_dir_all(&version_dir)
        .map_err(|e| AppError::Internal(format!("{}: {e}", version_dir.display())))?;

    let db = if manifest.capabilities.sql {
        let db = PluginDb::open(&db_path(dir, &id), manifest.capabilities.storage_mb)
            .map_err(|e| AppError::Internal(format!("plugin {id} database: {e}")))?;
        Some(Arc::new(db))
    } else {
        None
    };

    // Compile first: a component that does not load must not leave its
    // migrations applied.
    let loaded = runtime
        .load(&id, &package.wasm, &version_dir, db.clone())
        .map_err(|e| AppError::Validation(format!("plugin {id} does not load: {e:#}")))?;

    if let Some(db) = &db {
        db.migrate(&package.migrations)
            .map_err(|e| AppError::Validation(format!("plugin {id}: {e}")))?;
    }

    if write {
        let path = package_path(dir, &id, &version);
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, rtp)
            .and_then(|()| std::fs::rename(&tmp, &path))
            .map_err(|e| AppError::Internal(format!("{}: {e}", path.display())))?;
    }
    let mut ui_files: HashMap<String, Vec<u8>> = package
        .files
        .iter()
        .filter_map(|(path, bytes)| Some((path.strip_prefix("ui/")?.to_string(), bytes.clone())))
        .collect();
    // Served beside its UI, under the same cached address.
    if let Some(logo) = package.files.get(LOGO) {
        ui_files.insert(LOGO.to_string(), logo.clone());
    }
    Ok(Installed {
        manifest,
        loaded,
        turns: Semaphore::new(CONCURRENT_CALLS),
        ui_files,
        ui_tag: hex::encode(&Sha256::digest(rtp)[..8]),
        readme: package.readme,
        messages,
        verified: source == Source::Registry,
        public_files: package
            .files
            .iter()
            .filter_map(|(path, bytes)| {
                Some((path.strip_prefix("public/")?.to_string(), bytes.clone()))
            })
            .collect(),
        stored: RwLock::default(),
    })
}

/// Where a package came from: the signed registry is what "verified" means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Upload,
    Registry,
}

impl Source {
    fn as_str(self) -> &'static str {
        match self {
            Source::Upload => "upload",
            Source::Registry => "registry",
        }
    }

    fn parse(s: &str) -> Self {
        if s == "registry" {
            Source::Registry
        } else {
            Source::Upload
        }
    }
}

const DB_FILE: &str = "data.sqlite";

/// Removes what is in `root` but what `keep` names; nothing when it is absent.
fn remove_entries(root: &Path, keep: impl Fn(&str) -> bool) -> AppResult<()> {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(AppError::Internal(format!("{}: {e}", root.display()))),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if keep(&entry.file_name().to_string_lossy()) {
            continue;
        }
        let removed = if path.is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        removed.map_err(|e| AppError::Internal(format!("{}: {e}", path.display())))?;
    }
    Ok(())
}

/// A plugin's own database, kept across versions.
fn db_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(id).join(DB_FILE)
}

/// Where the dashboard loads one of a plugin's assets, under the package's
/// hash so a new package is a new address. Served by `routes::plugins`.
fn asset_url(id: &str, tag: &str, file: &str) -> String {
    format!("/api/plugin-assets/{id}/{tag}/{file}")
}

fn not_found(id: &str) -> AppError {
    AppError::NotFound(format!("Plugin {id} not found"))
}
