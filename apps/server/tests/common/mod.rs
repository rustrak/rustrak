//! Common test utilities and helpers
//!
//! This module provides shared functionality for all tests.

#![allow(unused_imports, dead_code)]

pub mod db;
pub mod fixtures;
pub mod telemetry;

pub use db::TestDb;
pub use fixtures::{create_envelope, create_envelope_no_length, EventBuilder, StackFrame};

use actix_web::body::MessageBody;
use actix_web::dev::{Service, ServiceResponse};
use rustrak::app::AppState;
use rustrak::config::{Config, RateLimitConfig, TelemetryConfig};
use rustrak::db::DbPool;
use rustrak::digest::processors::{ErrorProcessor, ProcessorCtx, Processors};
use rustrak::digest::session_aggregator::SessionAggregator;
use rustrak::error::AppResult;
use rustrak::ingest::EventMetadata;
use rustrak::models::{CreateAuthToken, CreateProject, Project};
use rustrak::services::sourcemap::{SourceMapEntry, SourceMapProvider};
use rustrak::services::{AuthTokenService, ProjectService};
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

/// The quota every test server runs with unless a test sets its own.
pub fn rate_limit() -> RateLimitConfig {
    RateLimitConfig {
        max_events_per_minute: 1000,
        max_events_per_hour: 10000,
        max_events_per_project_per_minute: 500,
        max_events_per_project_per_hour: 5000,
    }
}

/// The configuration of a test server: production defaults, a local bind
/// address, telemetry off and no dashboard, whatever the working tree holds.
pub fn config() -> Config {
    let mut config = Config {
        host: "127.0.0.1".to_string(),
        port: 0,
        rate_limit: rate_limit(),
        sourcemap_storage_path: "/tmp/test_sourcemaps".to_string(),
        telemetry: TelemetryConfig {
            enabled: false,
            do_not_track: false,
        },
        ..Config::default()
    };
    config.dashboard.enabled = false;
    config
}

/// [`config`] with the digest reading and writing events in `ingest_dir`.
pub fn config_with_ingest_dir(ingest_dir: &Path) -> Config {
    Config {
        ingest_dir: ingest_dir.to_path_buf(),
        ..config()
    }
}

/// The server's shared state over a test database.
pub async fn state(pool: &DbPool, config: Config) -> AppState {
    AppState::build(config, pool.clone())
        .await
        .expect("Failed to build the app state")
}

/// A test app's responses with their body boxed, the type test helpers name.
pub struct BoxedBody<S> {
    service: S,
}

impl<S, B> Service<actix_http::Request> for BoxedBody<S>
where
    S: Service<actix_http::Request, Response = ServiceResponse<B>, Error = actix_web::Error>,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse;
    type Error = actix_web::Error;
    type Future = futures_util::future::MapOk<S::Future, fn(ServiceResponse<B>) -> ServiceResponse>;

    actix_web::dev::forward_ready!(service);

    fn call(&self, req: actix_http::Request) -> Self::Future {
        use futures_util::TryFutureExt;
        self.service
            .call(req)
            .map_ok(ServiceResponse::map_into_boxed_body as fn(_) -> _)
    }
}

/// The production app over `state`, ready for `test::call_service`.
pub async fn app_with(
    state: &AppState,
) -> impl Service<actix_http::Request, Response = ServiceResponse, Error = actix_web::Error> {
    BoxedBody {
        service: actix_web::test::init_service(rustrak::app::app(state)).await,
    }
}

/// The production app over a test database, with [`config`].
pub async fn app(
    pool: &DbPool,
) -> impl Service<actix_http::Request, Response = ServiceResponse, Error = actix_web::Error> {
    app_with_config(pool, config()).await
}

/// The production app over a test database, with `config`.
pub async fn app_with_config(
    pool: &DbPool,
    config: Config,
) -> impl Service<actix_http::Request, Response = ServiceResponse, Error = actix_web::Error> {
    app_with(&state(pool, config).await).await
}

/// The digest pipeline on its own, writing events under `ingest_dir`.
pub fn processors(pool: &DbPool, ingest_dir: &Path) -> Processors {
    processors_with_config(pool, &config_with_ingest_dir(ingest_dir))
}

pub fn processors_with_config(pool: &DbPool, config: &Config) -> Processors {
    Processors::new(
        config,
        null_sourcemap_provider(),
        SessionAggregator::new(pool.clone(), 30, 10_000),
    )
}

/// A legacy API token, which holds every permission.
pub async fn create_test_token(pool: &DbPool) -> String {
    AuthTokenService::create(
        pool,
        CreateAuthToken {
            description: Some("Test token".to_string()),
            ..Default::default()
        },
    )
    .await
    .expect("Failed to create test token")
    .token
}

pub async fn create_test_project(pool: &DbPool, name: &str) -> Project {
    ProjectService::create(
        pool,
        CreateProject {
            name: name.to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .expect("Failed to create test project")
}

/// A project's id and DSN key, what an ingest request needs.
pub async fn create_ingest_project(pool: &DbPool, name: &str) -> (i32, String) {
    let project = create_test_project(pool, name).await;
    (project.id, project.sentry_key.to_string())
}

/// Drives the [`ErrorProcessor`] over an event already written to the temp
/// store.
pub async fn process_error_event(
    pool: &DbPool,
    metadata: &EventMetadata,
    ingest_dir: &Path,
    rate_limit_config: &RateLimitConfig,
    sourcemap_provider: Arc<dyn SourceMapProvider>,
) -> AppResult<()> {
    let config = Config {
        rate_limit: rate_limit_config.clone(),
        ..config_with_ingest_dir(ingest_dir)
    };
    let processor = ErrorProcessor::new(&config, sourcemap_provider);
    processor.digest(pool, metadata).await.map(|_| ())
}

/// Writes an event to the temp store the way ingest does, as a pending record
/// of `project_id`.
pub async fn store_event(
    ingest_dir: &Path,
    project_id: i32,
    event_id: &str,
    event_bytes: &[u8],
) -> AppResult<std::path::PathBuf> {
    rustrak::ingest::store_event_with_metadata(
        ingest_dir,
        event_id,
        event_bytes,
        &EventMetadata {
            event_id: event_id.to_string(),
            project_id,
            ingested_at: chrono::Utc::now(),
            remote_addr: None,
        },
    )
    .await
}

/// No-op SourceMapProvider for tests that don't exercise source map features.
pub struct NullSourceMapProvider;

#[async_trait::async_trait]
impl SourceMapProvider for NullSourceMapProvider {
    async fn fetch_sourcemap(
        &self,
        _project_id: i32,
        _debug_id: &str,
        _file_type: &str,
    ) -> AppResult<Option<SourceMapEntry>> {
        Ok(None)
    }
}

pub fn null_sourcemap_provider() -> Arc<dyn SourceMapProvider> {
    Arc::new(NullSourceMapProvider)
}

/// The role named `name`, created on first use: the install ships none, so the
/// suite keeps the sets existing memberships migrate to, plus a workspace admin.
pub async fn ensure_role(pool: &DbPool, name: &str) -> i32 {
    use rustrak::models::rbac::{Action, Resource, Scope};
    use rustrak::models::Permission;
    use rustrak::services::RoleService;

    if let Some(role) = RoleService::list(pool)
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.name == name)
    {
        return role.id;
    }
    let keep: fn(Resource, Action) -> bool = match name {
        "Workspace Admin" => |r, _| r.scope() == Scope::Instance,
        "Project Admin" => {
            |r, a| r.scope() == Scope::Project || (r, a) == (Resource::ApiTokens, Action::Create)
        }
        "Editor" => |r, a| {
            (r, a) == (Resource::ApiTokens, Action::Create)
                || (r.scope() == Scope::Project
                    && !matches!(
                        (r, a),
                        (Resource::Project, Action::Delete)
                            | (Resource::ProjectMembers, Action::Update)
                    ))
        },
        "Viewer" => |r, a| {
            (r, a) == (Resource::ApiTokens, Action::Create)
                || (r.scope() == Scope::Project && a == Action::Read)
        },
        other => panic!("no such test role {other}"),
    };
    let permissions = Resource::ALL
        .iter()
        .flat_map(|r| r.actions().iter().map(move |a| (*r, *a)))
        .filter(|&(r, a)| keep(r, a))
        .map(|(resource, action)| Permission { resource, action })
        .collect();
    RoleService::create(pool, name, "", permissions)
        .await
        .unwrap()
        .id
}

/// Gives a user a named test role on one project.
pub async fn grant_project_role(
    pool: &DbPool,
    project_id: i32,
    user_id: i32,
    role: &str,
) -> AppResult<()> {
    let id = ensure_role(pool, role).await;
    rustrak::services::RoleService::assign(pool, user_id, id, Some(project_id)).await
}

pub async fn seed_user(
    pool: &DbPool,
    email: &str,
    role: rustrak::models::UserRole,
) -> rustrak::models::User {
    rustrak::services::UsersService::create_user(
        pool,
        &rustrak::models::CreateUserRequest {
            email: email.to_string(),
            password: "password123".to_string(),
        },
        role,
    )
    .await
    .expect("seed user")
}

/// An API token acting as `user_id`.
pub async fn token_for(pool: &DbPool, user_id: i32) -> String {
    AuthTokenService::create_for_user(pool, CreateAuthToken::default(), Some(user_id))
        .await
        .expect("seed token")
        .token
}

/// Signs in with `email` and `password123` (the [`seed_user`] password) and
/// returns the session cookie.
pub async fn login<S>(app: &S, email: &str) -> actix_web::cookie::Cookie<'static>
where
    S: Service<actix_http::Request, Response = ServiceResponse, Error = actix_web::Error>,
{
    let req = actix_web::test::TestRequest::post()
        .uri("/auth/login")
        .set_json(serde_json::json!({ "email": email, "password": "password123" }))
        .to_request();
    let resp = actix_web::test::call_service(app, req).await;
    assert_eq!(resp.status(), 200, "login as {email}");
    resp.response()
        .cookies()
        .next()
        .expect("session cookie")
        .into_owned()
}

/// Polls `check` until it returns `Some`, for work a background task finishes
/// later. Panics after 10 s.
pub async fn eventually<T, F, Fut>(mut check: F) -> T
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Option<T>>,
{
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if let Some(value) = check().await {
            return value;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "condition not met within 10 s"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

/// Whether anything is still waiting in an ingest directory: the digest
/// removes an event's file whether it stores or drops it.
pub fn has_files(dir: &std::path::Path) -> bool {
    std::fs::read_dir(dir).is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            let path = entry.path();
            if path.is_dir() {
                has_files(&path)
            } else {
                true
            }
        })
    })
}

static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Exclusive use of the process environment: sets and removes variables, and
/// puts every one it touched back when dropped, even when the test panics.
pub struct EnvGuard {
    saved: Vec<(String, Option<String>)>,
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl EnvGuard {
    pub fn new() -> Self {
        Self {
            saved: Vec::new(),
            _lock: ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner()),
        }
    }

    fn save(&mut self, name: &str) {
        if !self.saved.iter().any(|(n, _)| n == name) {
            self.saved
                .push((name.to_string(), std::env::var(name).ok()));
        }
    }

    pub fn set(&mut self, name: &str, value: &str) -> &mut Self {
        self.save(name);
        std::env::set_var(name, value);
        self
    }

    pub fn remove(&mut self, name: &str) -> &mut Self {
        self.save(name);
        std::env::remove_var(name);
        self
    }

    pub fn set_opt(&mut self, name: &str, value: Option<&str>) -> &mut Self {
        match value {
            Some(v) => self.set(name, v),
            None => self.remove(name),
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (name, value) in self.saved.drain(..).rev() {
            match value {
                Some(v) => std::env::set_var(&name, v),
                None => std::env::remove_var(&name),
            }
        }
    }
}

/// Turns `/mcp` on or off.
pub fn set_mcp(token: &str, enabled: bool) -> actix_web::test::TestRequest {
    actix_web::test::TestRequest::put()
        .uri("/api/settings/mcp")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(serde_json::json!({ "enabled": enabled }))
}

/// One MCP `tools/call` request.
pub fn call_tool(
    token: Option<&str>,
    name: &str,
    arguments: serde_json::Value,
) -> actix_web::test::TestRequest {
    let mut req = actix_web::test::TestRequest::post()
        .uri("/mcp")
        .insert_header(("Content-Type", "application/json"))
        .insert_header(("Accept", "application/json, text/event-stream"))
        .insert_header(("MCP-Protocol-Version", "2026-07-28"))
        .insert_header(("Mcp-Method", "tools/call"))
        .insert_header(("Mcp-Name", name))
        .set_payload(
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {
                    "name": name,
                    "arguments": arguments,
                    "_meta": {
                        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                        "io.modelcontextprotocol/clientInfo": { "name": "test", "version": "1.0" },
                        "io.modelcontextprotocol/clientCapabilities": {}
                    }
                }
            })
            .to_string(),
        );
    if let Some(token) = token {
        req = req.insert_header(("Authorization", format!("Bearer {token}")));
    }
    req
}
