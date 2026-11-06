//! Common test utilities and helpers
//!
//! This module provides shared functionality for all tests.

#![allow(unused_imports, dead_code)]

pub mod db;
pub mod fixtures;
pub mod telemetry;

pub use db::TestDb;
pub use fixtures::{create_envelope, create_envelope_no_length, EventBuilder, StackFrame};

use rustrak::config::RateLimitConfig;
use rustrak::db::DbPool;
use rustrak::digest::processors::{ErrorProcessor, Processor, ProcessorCtx};
use rustrak::error::AppResult;
use rustrak::ingest::EventMetadata;
use rustrak::services::sourcemap::{SourceMapEntry, SourceMapProvider};
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

/// Drives the [`ErrorProcessor`] (the production processor pattern) over an
/// event already written to the temp store. Replaces the removed
/// `worker::process_event` free function: tests exercise the exact code path
/// production uses, without rebuilding the [`ProcessorCtx`] at every call site.
pub async fn process_error_event(
    pool: &DbPool,
    metadata: &EventMetadata,
    ingest_dir: &Path,
    rate_limit_config: &RateLimitConfig,
    sourcemap_provider: Arc<dyn SourceMapProvider>,
) -> AppResult<()> {
    let processor = ErrorProcessor::new(
        ingest_dir.to_path_buf(),
        rate_limit_config.clone(),
        sourcemap_provider,
    );
    let ctx = ProcessorCtx {
        pool: pool.clone(),
        project_id: metadata.project_id,
        event_id: Uuid::parse_str(&metadata.event_id).unwrap_or_else(|_| Uuid::nil()),
        ingested_at: metadata.ingested_at,
        remote_addr: metadata.remote_addr.clone(),
        keep_ai_spans: true,
    };
    processor.process(metadata.clone(), &ctx).await
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
    use rustrak::auth::rbac::{Action, Resource, Scope};
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
