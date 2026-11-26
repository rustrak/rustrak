//! A plugin's HTTP routes. The host authenticates and resolves the project;
//! the plugin only sees the request it declared, without cookies or tokens.
//!
//! - `/api/projects/{project_id}/plugins/{id}/{path}`: project routes. A
//!   public one takes the project's DSN key (`?sentry_key=` or
//!   `X-Rustrak-Key`), a user one a session or token that can see the project.
//! - `/api/plugins/{id}/{path}`: instance routes, users only.

use std::time::Duration;

use actix_web::http::StatusCode;
use actix_web::{web, FromRequest, HttpRequest, HttpResponse};

use crate::auth::ApiActor;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::rbac::{plugin_key, Action, Resource, Scope};
use crate::plugins::host::{Installed, PluginHost, Source};
use crate::plugins::manifest::{Auth, Route};
use crate::plugins::registry::{Registry, RegistryListing, RegistryPlugin};
use crate::plugins::runtime::{CallError, Request};
use crate::services::ProjectService;

/// CPU budget of one request.
const BUDGET: Duration = Duration::from_secs(2);
/// Largest request body a plugin is handed.
const MAX_BODY: usize = 1024 * 1024;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/api/plugins", web::get().to(list_plugins))
        .service(
            web::resource("/api/settings/plugins")
                .app_data(web::PayloadConfig::new(
                    crate::plugins::package::MAX_PACKAGE_BYTES as usize,
                ))
                .route(web::post().to(install_plugin)),
        )
        // Before `{plugin}`, which would take "registry" for a plugin's id.
        .route("/api/settings/plugins/registry", web::get().to(registry))
        .route(
            "/api/settings/plugins/registry/{plugin}",
            web::post().to(install_from_registry),
        )
        .route(
            "/api/settings/plugins/registry/{plugin}/logo",
            web::get().to(registry_logo),
        )
        .route(
            "/api/settings/plugins/registry/{plugin}/readme",
            web::get().to(registry_readme),
        )
        .service(
            web::resource("/api/settings/plugins/{plugin}")
                .route(web::put().to(configure_plugin))
                .route(web::delete().to(uninstall_plugin)),
        )
        .service(
            web::resource("/api/settings/plugins/{plugin}/settings")
                .route(web::get().to(instance_settings))
                .route(web::put().to(set_instance_settings)),
        )
        .route(
            "/api/plugin-assets/{plugin}/{tag}/{file:.*}",
            web::get().to(plugin_asset),
        )
        .route("/public/{plugin}/{file:.*}", web::get().to(plugin_public))
        .service(
            web::resource("/api/plugins/{plugin}/{path:.*}")
                .app_data(web::PayloadConfig::new(MAX_BODY))
                .to(instance_route),
        );
}

pub fn configure_project(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::resource("/plugin-settings/{plugin}")
            .route(web::get().to(project_settings))
            .route(web::put().to(set_project_settings)),
    )
    .service(
        web::resource("/plugins/{plugin}/{path:.*}")
            .app_data(web::PayloadConfig::new(MAX_BODY))
            .to(project_route),
    );
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/plugins",
    operation_id = "plugins.list",
    tag = "plugins",
    responses(
        (status = 200, description = "Installed plugins", body = Vec<crate::plugins::host::PluginSummary>),
        (status = 401, description = "Not signed in", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Lists the installed plugins and what each adds to the dashboard.
pub async fn list_plugins(
    host: web::Data<PluginHost>,
    pool: web::Data<DbPool>,
    _actor: ApiActor,
) -> AppResult<HttpResponse> {
    let plugins = host.list(pool.get_ref()).await?;
    Ok(HttpResponse::Ok().json(plugins))
}

// Owners only until `plugins` joins the RBAC catalog.
fn require_owner(actor: &ApiActor) -> AppResult<()> {
    if actor.is_owner() {
        Ok(())
    } else {
        Err(AppError::Forbidden("Only an owner manages plugins".into()))
    }
}

/// A `.rtp` file's bytes. Opaque in the spec, like an attachment: the client
/// validates bodies with zod, which has no type for a binary one.
#[cfg(feature = "openapi")]
#[derive(utoipa::ToSchema)]
#[schema(value_type = Value)]
pub struct PluginPackage(#[allow(dead_code)] Vec<u8>);

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/settings/plugins",
    operation_id = "plugins.install",
    tag = "plugins",
    request_body(content = PluginPackage, content_type = "application/octet-stream", description = "The plugin package, a .rtp file"),
    responses(
        (status = 201, description = "Installed", body = crate::plugins::host::PluginSummary),
        (status = 400, description = "Not a valid package for this server", body = crate::error::ErrorResponse),
        (status = 403, description = "Not an owner", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Installs a plugin package, or replaces the installed version of it.
pub async fn install_plugin(
    host: web::Data<PluginHost>,
    pool: web::Data<DbPool>,
    actor: ApiActor,
    body: web::Bytes,
) -> AppResult<HttpResponse> {
    require_owner(&actor)?;
    let installed = host
        .install(pool.get_ref(), body.into(), Source::Upload)
        .await?;
    let summary = host
        .summary(pool.get_ref(), &installed.manifest.plugin.id)
        .await?;
    Ok(HttpResponse::Created().json(summary))
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ConfigurePlugin {
    /// Active in every project, those created later included.
    pub all_projects: bool,
    /// The projects it runs in one by one; left out, they stay as they are.
    #[cfg_attr(feature = "openapi", schema(nullable = false))]
    pub projects: Option<Vec<i32>>,
}

#[cfg_attr(feature = "openapi", utoipa::path(
    put,
    path = "/api/settings/plugins/{plugin}",
    operation_id = "plugins.configure",
    tag = "plugins",
    params(("plugin" = String, Path, description = "Plugin id")),
    request_body = ConfigurePlugin,
    responses(
        (status = 200, description = "Updated", body = crate::plugins::host::PluginSummary),
        (status = 403, description = "Not an owner", body = crate::error::ErrorResponse),
        (status = 404, description = "Not installed", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Chooses where an installed plugin runs.
pub async fn configure_plugin(
    host: web::Data<PluginHost>,
    pool: web::Data<DbPool>,
    actor: ApiActor,
    path: web::Path<String>,
    body: web::Json<ConfigurePlugin>,
) -> AppResult<HttpResponse> {
    require_owner(&actor)?;
    let id = path.into_inner();
    host.set_projects(
        pool.get_ref(),
        &id,
        body.all_projects,
        body.projects.as_deref(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(host.summary(pool.get_ref(), &id).await?))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/settings/plugins/registry",
    operation_id = "plugins.registry",
    tag = "plugins",
    responses(
        (status = 200, description = "The published plugins this Rustrak runs, or why there are none", body = RegistryListing),
        (status = 403, description = "Not an owner", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Lists the plugins the signed registry publishes, with the version this Rustrak runs.
pub async fn registry(
    host: web::Data<PluginHost>,
    registry: web::Data<Registry>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    require_owner(&actor)?;
    let listing = match registry.index().await {
        Ok(index) => {
            let installed = host.installed_versions();
            RegistryListing {
                available: true,
                error: None,
                plugins: index
                    .plugins
                    .into_iter()
                    .filter_map(|p| {
                        let version = p.compatible(host.host_version())?.version.to_string();
                        Some(RegistryPlugin {
                            logo: p
                                .logo
                                .as_ref()
                                .map(|_| format!("/api/settings/plugins/registry/{}/logo", p.id)),
                            installed_version: installed.get(&p.id).cloned(),
                            version,
                            id: p.id,
                            name: p.name,
                            description: p.description,
                            icon: p.icon,
                            author: p.author,
                        })
                    })
                    .collect(),
            }
        }
        Err(e) => RegistryListing {
            available: false,
            error: Some(e.to_string()),
            plugins: Vec::new(),
        },
    };
    Ok(HttpResponse::Ok().json(listing))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/settings/plugins/registry/{plugin}",
    operation_id = "plugins.installFromRegistry",
    tag = "plugins",
    params(("plugin" = String, Path, description = "Plugin id")),
    responses(
        (status = 201, description = "Installed, verified", body = crate::plugins::host::PluginSummary),
        (status = 400, description = "Not a package this Rustrak can run", body = crate::error::ErrorResponse),
        (status = 403, description = "Not an owner", body = crate::error::ErrorResponse),
        (status = 502, description = "The registry could not be read, or what it served does not verify", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Installs a published plugin, its newest version this Rustrak runs, verified against the signed index.
pub async fn install_from_registry(
    host: web::Data<PluginHost>,
    registry: web::Data<Registry>,
    pool: web::Data<DbPool>,
    actor: ApiActor,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    require_owner(&actor)?;
    let id = path.into_inner();
    let (rtp, listed) = registry
        .download(&id, host.host_version())
        .await
        .map_err(|e| AppError::BadGateway(e.to_string()))?;
    // Before installing: a package of another plugin would replace that one,
    // and one of another version would install as what the index did not list.
    // Unzipping up to MAX_PACKAGE_BYTES is CPU work: off the async worker.
    let (rtp, served) = web::block(move || {
        let read = crate::plugins::package::Package::read(&rtp);
        read.map(|p| (rtp, p.manifest.plugin))
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
    .map_err(|e| AppError::BadGateway(e.to_string()))?;
    if served.id != id || served.version != listed {
        return Err(AppError::BadGateway(format!(
            "the registry served {} {} for {id} {listed}",
            served.id, served.version
        )));
    }
    host.install(pool.get_ref(), rtp, Source::Registry).await?;
    Ok(HttpResponse::Created().json(host.summary(pool.get_ref(), &id).await?))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/settings/plugins/registry/{plugin}/readme",
    operation_id = "plugins.registryReadme",
    tag = "plugins",
    params(("plugin" = String, Path, description = "Plugin id")),
    responses(
        (status = 200, description = "Its README, Markdown, by language: English always", body = std::collections::BTreeMap<String, String>),
        (status = 403, description = "Not an owner", body = crate::error::ErrorResponse),
        (status = 502, description = "The registry could not be read, or what it served does not verify", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Reads a published plugin's README before it is installed, from the package the signed index lists.
pub async fn registry_readme(
    host: web::Data<PluginHost>,
    registry: web::Data<Registry>,
    actor: ApiActor,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    require_owner(&actor)?;
    let (rtp, _) = registry
        .download(&path.into_inner(), host.host_version())
        .await
        .map_err(|e| AppError::BadGateway(e.to_string()))?;
    // Unzipping up to MAX_PACKAGE_BYTES is CPU work: off the async worker.
    let readme = web::block(move || crate::plugins::package::Package::read(&rtp).map(|p| p.readme))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .map_err(|e| AppError::BadGateway(e.to_string()))?;
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "private, max-age=600"))
        .json(readme))
}

/// A published plugin's logo, checked against the signed index and served
/// as an inert image, before the plugin is installed.
async fn registry_logo(
    registry: web::Data<Registry>,
    actor: ApiActor,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    require_owner(&actor)?;
    let bytes = registry
        .logo(&path.into_inner())
        .await
        .map_err(|e| AppError::BadGateway(e.to_string()))?;
    Ok(HttpResponse::Ok()
        .content_type("image/svg+xml")
        .insert_header(("Cache-Control", "private, max-age=3600"))
        .insert_header(("X-Content-Type-Options", "nosniff"))
        .insert_header(inert("image/svg+xml"))
        .body(bytes))
}

/// Values for a plugin's settings: a value sets, `null` clears, a key left
/// out stays as it is.
#[derive(serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(value_type = Object))]
pub struct SettingsPatch(serde_json::Map<String, serde_json::Value>);

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/settings/plugins/{plugin}/settings",
    operation_id = "plugins.settings",
    tag = "plugins",
    params(("plugin" = String, Path, description = "Plugin id")),
    responses(
        (status = 200, description = "Its settings for the whole instance", body = crate::plugins::host::PluginSettingsView),
        (status = 401, description = "Not signed in", body = crate::error::ErrorResponse),
        (status = 404, description = "Not installed", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Reads a plugin's settings for the whole instance, without its secrets.
pub async fn instance_settings(
    host: web::Data<PluginHost>,
    _actor: ApiActor,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    Ok(HttpResponse::Ok().json(host.settings_view(&path.into_inner(), None)?))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    put,
    path = "/api/settings/plugins/{plugin}/settings",
    operation_id = "plugins.setSettings",
    tag = "plugins",
    params(("plugin" = String, Path, description = "Plugin id")),
    request_body = SettingsPatch,
    responses(
        (status = 200, description = "Saved", body = crate::plugins::host::PluginSettingsView),
        (status = 400, description = "A value its schema refuses", body = crate::error::ErrorResponse),
        (status = 403, description = "Not an owner", body = crate::error::ErrorResponse),
        (status = 404, description = "Not installed", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Writes a plugin's settings for the whole instance, checked against its schema.
pub async fn set_instance_settings(
    host: web::Data<PluginHost>,
    pool: web::Data<DbPool>,
    actor: ApiActor,
    path: web::Path<String>,
    body: web::Json<SettingsPatch>,
) -> AppResult<HttpResponse> {
    require_owner(&actor)?;
    let view = host
        .set_settings(
            pool.get_ref(),
            &path.into_inner(),
            None,
            body.into_inner().0,
        )
        .await?;
    Ok(HttpResponse::Ok().json(view))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/plugin-settings/{plugin}",
    operation_id = "plugins.projectSettings",
    tag = "plugins",
    params(
        ("project_id" = i32, Path, description = "Project id"),
        ("plugin" = String, Path, description = "Plugin id"),
    ),
    responses(
        (status = 200, description = "Its settings in this project", body = crate::plugins::host::PluginSettingsView),
        (status = 404, description = "Not installed, or no such project", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Reads a plugin's settings in one project, without its secrets.
pub async fn project_settings(
    host: web::Data<PluginHost>,
    actor: ApiActor,
    path: web::Path<(i32, String)>,
) -> AppResult<HttpResponse> {
    let (project_id, plugin) = path.into_inner();
    actor.require(Resource::Project, Action::Read, Some(project_id))?;
    Ok(HttpResponse::Ok().json(host.settings_view(&plugin, Some(project_id))?))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    put,
    path = "/api/projects/{project_id}/plugin-settings/{plugin}",
    operation_id = "plugins.setProjectSettings",
    tag = "plugins",
    params(
        ("project_id" = i32, Path, description = "Project id"),
        ("plugin" = String, Path, description = "Plugin id"),
    ),
    request_body = SettingsPatch,
    responses(
        (status = 200, description = "Saved", body = crate::plugins::host::PluginSettingsView),
        (status = 400, description = "A value its schema refuses", body = crate::error::ErrorResponse),
        (status = 403, description = "May not change this project", body = crate::error::ErrorResponse),
        (status = 404, description = "Not installed, or no such project", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Writes a plugin's settings in one project: only its project settings.
pub async fn set_project_settings(
    host: web::Data<PluginHost>,
    pool: web::Data<DbPool>,
    actor: ApiActor,
    path: web::Path<(i32, String)>,
    body: web::Json<SettingsPatch>,
) -> AppResult<HttpResponse> {
    let (project_id, plugin) = path.into_inner();
    actor.require(Resource::Project, Action::Update, Some(project_id))?;
    let view = host
        .set_settings(
            pool.get_ref(),
            &plugin,
            Some(project_id),
            body.into_inner().0,
        )
        .await?;
    Ok(HttpResponse::Ok().json(view))
}

#[derive(Debug, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct UninstallQuery {
    /// Delete the plugin's database too. Without it, reinstalling finds it.
    #[serde(default)]
    pub purge: bool,
}

#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/api/settings/plugins/{plugin}",
    operation_id = "plugins.uninstall",
    tag = "plugins",
    params(("plugin" = String, Path, description = "Plugin id"), UninstallQuery),
    responses(
        (status = 204, description = "Uninstalled"),
        (status = 403, description = "Not an owner", body = crate::error::ErrorResponse),
        (status = 404, description = "Not installed", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Uninstalls a plugin, keeping its data unless `purge`.
pub async fn uninstall_plugin(
    host: web::Data<PluginHost>,
    pool: web::Data<DbPool>,
    actor: ApiActor,
    path: web::Path<String>,
    query: web::Query<UninstallQuery>,
) -> AppResult<HttpResponse> {
    require_owner(&actor)?;
    host.uninstall(pool.get_ref(), &path.into_inner(), query.purge)
        .await?;
    Ok(HttpResponse::NoContent().finish())
}

/// A file of a plugin's dashboard bundle. The package's hash is in the path,
/// so it never changes under a URL and is cached for good.
async fn plugin_asset(
    host: web::Data<PluginHost>,
    path: web::Path<(String, String, String)>,
) -> AppResult<HttpResponse> {
    let (plugin, tag, file) = path.into_inner();
    let missing = || AppError::NotFound(format!("No such file in plugin {plugin}"));
    let installed = host.get(&plugin).ok_or_else(missing)?;
    let bytes = installed.ui_file(&tag, &file).ok_or_else(missing)?;
    let content_type = content_type(&file);
    Ok(HttpResponse::Ok()
        .content_type(content_type)
        .insert_header(("Cache-Control", "public, max-age=31536000, immutable"))
        .insert_header(("X-Content-Type-Options", "nosniff"))
        .insert_header(inert(content_type))
        .body(bytes.to_vec()))
}

fn content_type(file: &str) -> &'static str {
    match file.rsplit_once('.').map(|(_, ext)| ext) {
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json" | "map") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// A file a plugin hands out to anyone (`public/` in its package), like the
/// script a website loads. Its address is stable across updates, so it is
/// cached for minutes, not for good; any origin may load it.
async fn plugin_public(
    host: web::Data<PluginHost>,
    path: web::Path<(String, String)>,
) -> AppResult<HttpResponse> {
    let (plugin, file) = path.into_inner();
    let missing = || AppError::NotFound(format!("No such file in plugin {plugin}"));
    let installed = host.get(&plugin).ok_or_else(missing)?;
    let bytes = installed.public_file(&file).ok_or_else(missing)?;
    let content_type = content_type(&file);
    Ok(HttpResponse::Ok()
        .content_type(content_type)
        .insert_header(("Cache-Control", "public, max-age=300"))
        .insert_header(("Cross-Origin-Resource-Policy", "cross-origin"))
        .insert_header(("X-Content-Type-Options", "nosniff"))
        .insert_header(inert(content_type))
        .body(bytes.to_vec()))
}

/// An SVG a plugin ships is an image, never a page: opened on its own, its
/// scripts must not run with Rustrak's origin. Anything else keeps the
/// default.
fn inert(content_type: &str) -> (&'static str, &'static str) {
    if content_type == "image/svg+xml" {
        (
            "Content-Security-Policy",
            "default-src 'none'; style-src 'unsafe-inline'; sandbox",
        )
    } else {
        ("X-Frame-Options", "DENY")
    }
}

fn not_found(plugin: &str) -> AppError {
    AppError::NotFound(format!("Plugin route not found in {plugin}"))
}

/// The plugin and the route the request names, or a 404.
fn find(
    host: &PluginHost,
    req: &HttpRequest,
    plugin: &str,
    path: &str,
) -> AppResult<(std::sync::Arc<Installed>, Route)> {
    let installed = host.get(plugin).ok_or_else(|| not_found(plugin))?;
    let path = format!("/{path}");
    let route = installed
        .manifest
        .routes
        .iter()
        .find(|r| r.method.as_str() == req.method().as_str() && r.path == path)
        .cloned()
        .ok_or_else(|| not_found(plugin))?;
    Ok((installed, route))
}

async fn instance_route(
    req: HttpRequest,
    body: web::Bytes,
    host: web::Data<PluginHost>,
    path: web::Path<(String, String)>,
) -> AppResult<HttpResponse> {
    let (plugin, path) = path.into_inner();
    let (installed, route) = find(&host, &req, &plugin, &path)?;
    if route.scope != Scope::Instance || route.auth != Auth::User {
        return Err(not_found(&plugin));
    }
    let actor = ApiActor::extract(&req).await?;
    check_requires(&actor, &installed, &route, None)?;
    let call = request(&req, &route, body, None);
    respond(host.call_http(installed, call, BUDGET).await?)
}

async fn project_route(
    req: HttpRequest,
    body: web::Bytes,
    host: web::Data<PluginHost>,
    pool: web::Data<DbPool>,
    path: web::Path<(i32, String, String)>,
) -> AppResult<HttpResponse> {
    let (project_id, plugin, path) = path.into_inner();
    let (installed, route) = find(&host, &req, &plugin, &path)?;
    if route.scope != Scope::Project {
        return Err(not_found(&plugin));
    }
    match route.auth {
        Auth::Public => check_key(pool.get_ref(), &req, project_id).await?,
        Auth::User => {
            let actor = ApiActor::extract(&req).await?;
            // Seeing the project is the floor; a 404 hides that it exists.
            actor.require(Resource::Project, Action::Read, Some(project_id))?;
            check_requires(&actor, &installed, &route, Some(project_id))?;
        }
    }
    if !PluginHost::active_in(pool.get_ref(), &plugin, project_id).await? {
        return Err(not_found(&plugin));
    }
    let call = request(&req, &route, body, Some(project_id as u64));
    respond(host.call_http(installed, call, BUDGET).await?)
}

/// The route's `requires`, a permission its plugin declares, checked like
/// Rustrak's own: through the caller's roles, where the manifest scopes it.
fn check_requires(
    actor: &ApiActor,
    plugin: &Installed,
    route: &Route,
    project_id: Option<i32>,
) -> AppResult<()> {
    let Some(req) = &route.requires else {
        return Ok(());
    };
    let id = &plugin.manifest.plugin.id;
    let missing = || AppError::Forbidden(format!("Missing permission {id}:{req}"));
    let (resource, action) = req.split_once(':').ok_or_else(missing)?;
    let action = Action::parse(action).ok_or_else(missing)?;
    let scope = plugin
        .permission_scope(resource, action)
        .ok_or_else(missing)?;
    if actor
        .access
        .can_plugin(&plugin_key(id, resource), scope, action, project_id)
    {
        Ok(())
    } else {
        Err(missing())
    }
}

/// The DSN key of the project, the way Sentry SDKs prove they may send.
async fn check_key(pool: &DbPool, req: &HttpRequest, project_id: i32) -> AppResult<()> {
    let query_key = req.query_string().split('&').find_map(|pair| {
        pair.split_once('=')
            .filter(|(k, _)| *k == "sentry_key")
            .map(|(_, v)| v.to_string())
    });
    let header_key = req
        .headers()
        .get("x-rustrak-key")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let key: uuid::Uuid = query_key
        .or(header_key)
        .and_then(|k| k.parse().ok())
        .ok_or_else(|| AppError::Unauthorized("Missing or invalid project key".into()))?;
    let project = ProjectService::get_by_id(pool, project_id).await?;
    if project.sentry_key != key {
        return Err(AppError::Unauthorized("Invalid key for project".into()));
    }
    Ok(())
}

/// Headers a plugin never sees: credentials stay with the host.
const WITHHELD: &[&str] = &["cookie", "authorization", "x-rustrak-key"];

fn request(req: &HttpRequest, route: &Route, body: web::Bytes, project_id: Option<u64>) -> Request {
    let query = req
        .query_string()
        .split('&')
        .filter(|p| !p.starts_with("sentry_key="))
        .collect::<Vec<_>>()
        .join("&");
    Request {
        method: route.method.as_str().to_string(),
        path: route.path.clone(),
        query,
        headers: req
            .headers()
            .iter()
            .filter(|(name, _)| !WITHHELD.contains(&name.as_str()))
            .filter_map(|(name, value)| Some((name.to_string(), value.to_str().ok()?.to_string())))
            .collect(),
        body: body.to_vec(),
        project_id,
        client_ip: req
            .connection_info()
            .realip_remote_addr()
            .map(str::to_string),
    }
}

/// Headers a plugin may not set: the host owns sessions and framing.
const REFUSED: &[&str] = &[
    "set-cookie",
    "content-length",
    "transfer-encoding",
    "connection",
];

fn respond(
    result: Result<crate::plugins::runtime::Response, CallError>,
) -> AppResult<HttpResponse> {
    match result {
        Ok(r) => {
            let status =
                StatusCode::from_u16(r.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            let mut resp = HttpResponse::build(status);
            for (name, value) in &r.headers {
                if !REFUSED.contains(&name.to_ascii_lowercase().as_str()) {
                    resp.insert_header((name.as_str(), value.as_str()));
                }
            }
            Ok(resp.body(r.body))
        }
        Err(CallError::Timeout) => Ok(HttpResponse::GatewayTimeout().json(serde_json::json!({
            "detail": "The plugin did not answer in time"
        }))),
        Err(CallError::Trap(e)) => Err(AppError::Internal(format!("plugin failed: {e}"))),
    }
}
