//! Admin-only Storage API: usage visibility + retention/cleanup.
//!
//! Every endpoint is gated to global admins — usage exposes the whole instance
//! and cleanup is destructive, so neither is project-scoped RBAC territory.

use std::sync::Arc;

use actix_web::{web, HttpResponse};

use crate::auth::ApiActor;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::{AutoCleanupResponse, AutoCleanupRun, AutoCleanupSettings, CleanupRequest};
#[cfg(feature = "openapi")]
use crate::models::{
    CleanupCounts, CleanupState, CleanupStatus, ProjectStorage, SourceMapGcResult,
    SourceMapStorage, StorageSummary,
};
use crate::scheduler::engine::{JobRunStatus, Scheduler};
use crate::scheduler::jobs::StorageCleanupJob;
use crate::services::sourcemap_store::SourceMapStore;
use crate::services::{CleanupJob, StorageService};

#[cfg(feature = "openapi")]
use utoipa::OpenApi;

/// Ensures the actor is a global admin, else 403.
fn require_admin(actor: &ApiActor) -> AppResult<()> {
    if actor.is_admin() {
        Ok(())
    } else {
        Err(AppError::Forbidden("Admin privileges required".to_string()))
    }
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/storage/summary",
    tag = "Storage",
    responses(
        (status = 200, description = "Instance-wide storage summary", body = StorageSummary),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/storage/summary — row counts, DB size, and source-map weight.
pub async fn get_summary(pool: web::Data<DbPool>, actor: ApiActor) -> AppResult<HttpResponse> {
    require_admin(&actor)?;
    let summary = StorageService::global_summary(pool.get_ref()).await?;
    Ok(HttpResponse::Ok().json(summary))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/storage/projects",
    tag = "Storage",
    responses(
        (status = 200, description = "Per-project storage breakdown", body = inline(Vec<ProjectStorage>)),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/storage/projects — per-project counts and estimated weight.
pub async fn get_projects(pool: web::Data<DbPool>, actor: ApiActor) -> AppResult<HttpResponse> {
    require_admin(&actor)?;
    let rows = StorageService::by_project(pool.get_ref()).await?;
    Ok(HttpResponse::Ok().json(rows))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/storage/cleanup/preview",
    tag = "Storage",
    request_body = CleanupRequest,
    responses(
        (status = 200, description = "Dry-run: rows a cleanup would remove", body = CleanupCounts),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// POST /api/storage/cleanup/preview — dry-run count, mutates nothing.
pub async fn preview_cleanup(
    pool: web::Data<DbPool>,
    actor: ApiActor,
    body: web::Json<CleanupRequest>,
) -> AppResult<HttpResponse> {
    require_admin(&actor)?;
    let counts = StorageService::preview_cleanup(
        pool.get_ref(),
        body.older_than_days,
        body.project_id,
        body.filter(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(counts))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/storage/cleanup",
    tag = "Storage",
    request_body = CleanupRequest,
    responses(
        (status = 202, description = "Cleanup started in the background; poll its status", body = CleanupStatus),
        (status = 400, description = "Invalid retention window", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
        (status = 409, description = "A cleanup is already running", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// POST /api/storage/cleanup — starts deleting old data and the issues it
/// empties. Returns at once: the purge runs detached, in batches, and
/// `GET /api/storage/cleanup/status` reports its progress.
pub async fn execute_cleanup(
    pool: web::Data<DbPool>,
    job: web::Data<CleanupJob>,
    actor: ApiActor,
    body: web::Json<CleanupRequest>,
) -> AppResult<HttpResponse> {
    require_admin(&actor)?;
    let status = job.start(
        pool.get_ref().clone(),
        body.older_than_days,
        body.project_id,
        body.filter(),
    )?;
    Ok(HttpResponse::Accepted().json(status))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/storage/cleanup/status",
    tag = "Storage",
    responses(
        (status = 200, description = "The current or last cleanup and the rows it removed", body = CleanupStatus),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/storage/cleanup/status — progress of the running cleanup, or the
/// outcome of the last one.
pub async fn get_cleanup_status(
    job: web::Data<CleanupJob>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    require_admin(&actor)?;
    Ok(HttpResponse::Ok().json(job.status()))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/storage/source-maps/gc/preview",
    tag = "Storage",
    responses(
        (status = 200, description = "Dry-run: orphaned source maps a GC would remove", body = SourceMapGcResult),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// POST /api/storage/source-maps/gc/preview — dry-run orphan count, deletes nothing.
pub async fn preview_source_map_gc(
    pool: web::Data<DbPool>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    require_admin(&actor)?;
    let result = StorageService::preview_source_map_gc(pool.get_ref()).await?;
    Ok(HttpResponse::Ok().json(result))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/storage/source-maps/gc",
    tag = "Storage",
    responses(
        (status = 200, description = "Orphaned source maps removed", body = SourceMapGcResult),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// POST /api/storage/source-maps/gc — delete orphaned source maps (DB + disk).
pub async fn gc_source_maps(
    pool: web::Data<DbPool>,
    store: web::Data<Arc<dyn SourceMapStore>>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    require_admin(&actor)?;
    let result = StorageService::gc_source_maps(pool.get_ref(), store.get_ref().as_ref()).await?;
    Ok(HttpResponse::Ok().json(result))
}

#[cfg(feature = "openapi")]
#[derive(OpenApi)]
#[openapi(
    paths(
        get_summary,
        get_projects,
        preview_cleanup,
        execute_cleanup,
        get_cleanup_status,
        preview_source_map_gc,
        gc_source_maps,
        get_auto_cleanup,
        update_auto_cleanup
    ),
    components(schemas(
        AutoCleanupSettings,
        AutoCleanupResponse,
        AutoCleanupRun,
        StorageSummary,
        SourceMapStorage,
        ProjectStorage,
        CleanupCounts,
        CleanupRequest,
        CleanupState,
        CleanupStatus,
        SourceMapGcResult,
    ))
)]
pub struct StorageApi;

/// Configure storage routes.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/storage/auto-cleanup",
    tag = "Storage",
    responses(
        (status = 200, description = "Automatic cleanup settings, next run and last run", body = AutoCleanupResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Admin privileges required", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/storage/auto-cleanup
pub async fn get_auto_cleanup(pool: web::Data<DbPool>, actor: ApiActor) -> AppResult<HttpResponse> {
    require_admin(&actor)?;
    Ok(HttpResponse::Ok().json(auto_cleanup_response(pool.get_ref()).await?))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    put,
    path = "/api/storage/auto-cleanup",
    tag = "Storage",
    request_body = AutoCleanupSettings,
    responses(
        (status = 200, description = "Saved; the new schedule applies within a minute", body = AutoCleanupResponse),
        (status = 400, description = "Settings that cannot run", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Admin privileges required", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// PUT /api/storage/auto-cleanup
pub async fn update_auto_cleanup(
    pool: web::Data<DbPool>,
    actor: ApiActor,
    body: web::Json<AutoCleanupSettings>,
) -> AppResult<HttpResponse> {
    require_admin(&actor)?;
    let settings = body.into_inner();
    settings.validate()?;
    Scheduler::update_settings(
        pool.get_ref(),
        StorageCleanupJob::KEY,
        serde_json::to_value(&settings).unwrap_or_default(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(auto_cleanup_response(pool.get_ref()).await?))
}

async fn auto_cleanup_response(pool: &DbPool) -> AppResult<AutoCleanupResponse> {
    let state = match Scheduler::job(pool, StorageCleanupJob::KEY).await {
        Ok(state) => Some(state),
        Err(AppError::NotFound(_)) => None,
        Err(e) => return Err(e),
    };
    let Some(state) = state else {
        return Ok(AutoCleanupResponse {
            settings: AutoCleanupSettings::default(),
            next_run_at: None,
            last_run: None,
        });
    };
    let last_run = state.last_started_at.map(|started_at| {
        let finished = state.last_finished_at.filter(|f| *f >= started_at);
        AutoCleanupRun {
            started_at,
            finished_at: finished,
            status: match (finished, state.last_status) {
                (None, _) => "running",
                (Some(_), Some(JobRunStatus::Ok)) => "ok",
                (Some(_), _) => "error",
            }
            .to_string(),
            removed: finished
                .and(state.last_result.clone())
                .and_then(|r| serde_json::from_value(r).ok()),
            error: finished.and(state.last_error.clone()),
        }
    });
    Ok(AutoCleanupResponse {
        settings: StorageCleanupJob::settings(&state.settings),
        next_run_at: state.next_run_at,
        last_run,
    })
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/storage")
            .route("/summary", web::get().to(get_summary))
            .route("/projects", web::get().to(get_projects))
            .route("/cleanup/preview", web::post().to(preview_cleanup))
            .route("/cleanup", web::post().to(execute_cleanup))
            .route("/cleanup/status", web::get().to(get_cleanup_status))
            .route("/auto-cleanup", web::get().to(get_auto_cleanup))
            .route("/auto-cleanup", web::put().to(update_auto_cleanup))
            .route(
                "/source-maps/gc/preview",
                web::post().to(preview_source_map_gc),
            )
            .route("/source-maps/gc", web::post().to(gc_source_maps)),
    );
}
