//! Admin-only Storage API: usage visibility + retention/cleanup.
//!
//! Every endpoint is gated to global admins — usage exposes the whole instance
//! and cleanup is destructive, so neither is project-scoped RBAC territory.

use std::sync::Arc;

use actix_web::{web, HttpResponse};

use crate::auth::ApiActor;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::rbac::{Action, Resource};
#[cfg(feature = "openapi")]
use crate::models::{
    CleanupCounts, CleanupStatus, Compaction, ProjectStorage, SourceMapGcResult, SourceMapStorage,
    StorageSummary,
};
use crate::models::{
    CleanupRequest, CleanupState, RetentionResponse, RetentionRun, RetentionSettings,
};
use crate::scheduler::engine::{JobRunStatus, Scheduler};
use crate::scheduler::jobs::StorageCleanupJob;
use crate::services::sourcemap_store::SourceMapStore;
use crate::services::{CleanupJob, StorageService};

#[cfg(feature = "openapi")]
use utoipa::OpenApi;

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/storage/summary",
    operation_id = "storage.getSummary",
    tag = "Storage",
    responses(
        (status = 200, description = "Instance-wide storage summary", body = StorageSummary),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Row counts, DB size, and source-map weight.
pub async fn get_summary(
    pool: web::Data<DbPool>,
    job: web::Data<CleanupJob>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    actor.require(Resource::Storage, Action::Read, None)?;
    let snapshot = job.snapshot(pool.get_ref()).await?;
    Ok(HttpResponse::Ok().json(&snapshot.summary))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/storage/projects",
    operation_id = "storage.getProjects",
    tag = "Storage",
    responses(
        (status = 200, description = "Per-project storage breakdown", body = inline(Vec<ProjectStorage>)),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Per-project counts and estimated weight.
pub async fn get_projects(
    pool: web::Data<DbPool>,
    job: web::Data<CleanupJob>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    actor.require(Resource::Storage, Action::Read, None)?;
    let snapshot = job.snapshot(pool.get_ref()).await?;
    Ok(HttpResponse::Ok().json(&snapshot.projects))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/storage/cleanup/preview",
    operation_id = "storage.previewCleanup",
    tag = "Storage",
    request_body = CleanupRequest,
    responses(
        (status = 200, description = "Dry-run: rows a cleanup would remove", body = CleanupCounts),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Dry-run count, mutates nothing.
pub async fn preview_cleanup(
    pool: web::Data<DbPool>,
    actor: ApiActor,
    body: web::Json<CleanupRequest>,
) -> AppResult<HttpResponse> {
    actor.require(Resource::Storage, Action::Read, None)?;
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
    operation_id = "storage.executeCleanup",
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
/// Starts deleting old data and the issues it
/// empties. Returns at once: the purge runs detached, in batches, and
/// `GET /api/storage/cleanup/status` reports its progress.
pub async fn execute_cleanup(
    pool: web::Data<DbPool>,
    job: web::Data<CleanupJob>,
    actor: ApiActor,
    body: web::Json<CleanupRequest>,
) -> AppResult<HttpResponse> {
    actor.require(Resource::Storage, Action::Delete, None)?;
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
    operation_id = "storage.getCleanupStatus",
    tag = "Storage",
    responses(
        (status = 200, description = "The current or last cleanup and the rows it removed", body = CleanupStatus),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Progress of the running cleanup, or the
/// outcome of the last one.
pub async fn get_cleanup_status(
    job: web::Data<CleanupJob>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    actor.require(Resource::Storage, Action::Read, None)?;
    Ok(HttpResponse::Ok().json(job.status()))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/storage/source-maps/gc/preview",
    operation_id = "storage.previewSourceMapGc",
    tag = "Storage",
    responses(
        (status = 200, description = "Dry-run: orphaned source maps a GC would remove", body = SourceMapGcResult),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Dry-run orphan count, deletes nothing.
pub async fn preview_source_map_gc(
    pool: web::Data<DbPool>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    actor.require(Resource::Storage, Action::Read, None)?;
    let result = StorageService::preview_source_map_gc(pool.get_ref()).await?;
    Ok(HttpResponse::Ok().json(result))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/storage/source-maps/gc",
    operation_id = "storage.gcSourceMaps",
    tag = "Storage",
    responses(
        (status = 200, description = "Orphaned source maps removed", body = SourceMapGcResult),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Delete orphaned source maps (DB + disk).
pub async fn gc_source_maps(
    pool: web::Data<DbPool>,
    job: web::Data<CleanupJob>,
    store: web::Data<Arc<dyn SourceMapStore>>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    actor.require(Resource::Storage, Action::Delete, None)?;
    let result = StorageService::gc_source_maps(pool.get_ref(), store.get_ref().as_ref()).await?;
    job.forget_snapshot();
    Ok(HttpResponse::Ok().json(result))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/storage/compaction",
    operation_id = "storage.getCompaction",
    tag = "Storage",
    responses(
        (status = 200, description = "Whether the SQLite file gives freed space back, and what compacting it takes", body = Compaction),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Whether the database file shrinks when data
/// is deleted, and the disk a one-off compaction needs.
pub async fn get_compaction(pool: web::Data<DbPool>, actor: ApiActor) -> AppResult<HttpResponse> {
    actor.require(Resource::Storage, Action::Read, None)?;
    let compaction = crate::db::maintenance::compaction(pool.get_ref()).await?;
    Ok(HttpResponse::Ok().json(compaction))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/storage/compaction",
    operation_id = "storage.compact",
    tag = "Storage",
    responses(
        (status = 202, description = "Compaction started in the background; poll the cleanup status", body = CleanupStatus),
        (status = 400, description = "Nothing to compact, or not enough free disk", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
        (status = 409, description = "A cleanup is already running", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Rewrites a SQLite file created before
/// incremental mode, so it gives freed space back from then on. Writes wait
/// while it runs; follow it with `GET /api/storage/cleanup/status`.
pub async fn compact(
    pool: web::Data<DbPool>,
    job: web::Data<CleanupJob>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    actor.require(Resource::Storage, Action::Delete, None)?;
    let status = job.start_compaction(pool.get_ref().clone()).await?;
    Ok(HttpResponse::Accepted().json(status))
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
        get_compaction,
        compact,
        preview_source_map_gc,
        gc_source_maps,
        get_retention,
        update_retention,
        preview_retention,
        run_retention,
        get_age
    ),
    components(schemas(
        RetentionSettings,
        crate::models::RetentionWindows,
        crate::models::ProjectRetention,
        crate::models::CleanupFrequency,
        RetentionResponse,
        RetentionRun,
        crate::models::StorageAge,
        crate::models::ProjectAge,
        crate::models::KindAge,
        StorageSummary,
        SourceMapStorage,
        ProjectStorage,
        CleanupCounts,
        CleanupRequest,
        CleanupState,
        crate::models::CleanupKind,
        CleanupStatus,
        Compaction,
        SourceMapGcResult,
    ))
)]
pub struct StorageApi;

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/storage/retention",
    operation_id = "storage.getRetention",
    tag = "Storage",
    responses(
        (status = 200, description = "Retention settings, next cleanup and last cleanup", body = RetentionResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Missing storage:read", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
pub async fn get_retention(
    pool: web::Data<DbPool>,
    job: web::Data<CleanupJob>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    actor.require(Resource::Storage, Action::Read, None)?;
    let retention = retention_response(pool.get_ref(), &job).await?;
    Ok(HttpResponse::Ok().json(retention))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    put,
    path = "/api/storage/retention",
    operation_id = "storage.updateRetention",
    tag = "Storage",
    request_body = RetentionSettings,
    responses(
        (status = 200, description = "Saved; the new schedule applies within a minute", body = RetentionResponse),
        (status = 400, description = "Settings that cannot run", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Missing storage:update", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
pub async fn update_retention(
    pool: web::Data<DbPool>,
    job: web::Data<CleanupJob>,
    actor: ApiActor,
    body: web::Json<RetentionSettings>,
) -> AppResult<HttpResponse> {
    actor.require(Resource::Storage, Action::Update, None)?;
    let settings = body.into_inner();
    settings.validate()?;
    let settings =
        serde_json::to_value(&settings).map_err(|e| AppError::Internal(e.to_string()))?;
    Scheduler::save_settings(
        pool.get_ref(),
        StorageCleanupJob::KEY,
        settings.clone(),
        StorageCleanupJob::schedule_of(&settings),
        chrono::Utc::now(),
    )
    .await?;
    let retention = retention_response(pool.get_ref(), &job).await?;
    Ok(HttpResponse::Ok().json(retention))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/storage/retention/preview",
    operation_id = "storage.previewRetention",
    tag = "Storage",
    request_body = RetentionSettings,
    responses(
        (status = 200, description = "Dry-run: rows enforcing these settings now would remove", body = CleanupCounts),
        (status = 400, description = "Settings that cannot run", body = crate::error::ErrorResponse),
        (status = 403, description = "Missing storage:read", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// What saving these settings would
/// delete on the next cleanup, without saving or deleting anything.
pub async fn preview_retention(
    pool: web::Data<DbPool>,
    actor: ApiActor,
    body: web::Json<RetentionSettings>,
) -> AppResult<HttpResponse> {
    actor.require(Resource::Storage, Action::Read, None)?;
    let counts = StorageService::preview_retention(pool.get_ref(), &body).await?;
    Ok(HttpResponse::Ok().json(counts))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/storage/retention/run",
    operation_id = "storage.runRetention",
    tag = "Storage",
    responses(
        (status = 202, description = "Cleanup started in the background; poll its status", body = CleanupStatus),
        (status = 400, description = "Everything is kept forever: nothing to clean up", body = crate::error::ErrorResponse),
        (status = 403, description = "Missing storage:delete", body = crate::error::ErrorResponse),
        (status = 409, description = "A cleanup is already running", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Enforces the saved retention now,
/// without waiting for the schedule. Follow it with the cleanup status.
pub async fn run_retention(
    pool: web::Data<DbPool>,
    job: web::Data<CleanupJob>,
    store: web::Data<Arc<dyn SourceMapStore>>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    actor.require(Resource::Storage, Action::Delete, None)?;
    let settings = retention_response(pool.get_ref(), &job).await?.settings;
    if !settings.deletes_anything() {
        return Err(AppError::Validation(
            "Everything is kept forever: there is nothing to clean up".to_string(),
        ));
    }
    let (status, handle) = job.start_retention(
        pool.get_ref().clone(),
        settings,
        Arc::clone(store.get_ref()),
    )?;
    // Recorded as the job's last run, like a scheduled one, so the page's
    // "last cleanup" is this one.
    Scheduler::record_run_now(
        pool.get_ref().clone(),
        StorageCleanupJob::KEY,
        status.started_at.unwrap_or_else(chrono::Utc::now),
        async move {
            match handle.await {
                Ok(result) => result.and_then(|removed| {
                    serde_json::to_value(removed).map_err(|e| AppError::Internal(e.to_string()))
                }),
                Err(e) => Err(AppError::Internal(format!(
                    "The cleanup stopped unexpectedly: {e}"
                ))),
            }
        },
    )
    .await;
    Ok(HttpResponse::Accepted().json(status))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/storage/age",
    operation_id = "storage.getAge",
    tag = "Storage",
    responses(
        (status = 200, description = "How long ago each project's data arrived, per kind", body = crate::models::StorageAge),
        (status = 403, description = "Missing storage:read", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Rows per age bucket, per project and kind.
pub async fn get_age(
    pool: web::Data<DbPool>,
    job: web::Data<CleanupJob>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    actor.require(Resource::Storage, Action::Read, None)?;
    Ok(HttpResponse::Ok().json(&job.snapshot(pool.get_ref()).await?.age))
}

async fn retention_response(pool: &DbPool, job: &CleanupJob) -> AppResult<RetentionResponse> {
    let state = match Scheduler::job(pool, StorageCleanupJob::KEY).await {
        Ok(state) => Some(state),
        Err(AppError::NotFound(_)) => None,
        Err(e) => return Err(e),
    };
    let Some(state) = state else {
        return Ok(RetentionResponse {
            settings: RetentionSettings::default(),
            next_run_at: None,
            last_run: None,
        });
    };
    // An unfinished run with no cleanup running was cut short by a restart.
    let running = job.status().state == CleanupState::Running;
    let last_run = state.last_started_at.map(|started_at| {
        let finished = state.last_finished_at.filter(|f| *f >= started_at);
        RetentionRun {
            started_at,
            finished_at: finished,
            status: match (finished, state.last_status) {
                (None, _) if running => "running",
                (Some(_), Some(JobRunStatus::Ok)) => "ok",
                _ => "error",
            }
            .to_string(),
            removed: finished
                .and(state.last_result.clone())
                .and_then(|r| serde_json::from_value(r).ok()),
            error: match finished {
                Some(_) => state.last_error.clone(),
                None if running => None,
                None => Some("Interrupted by a server restart".to_string()),
            },
        }
    });
    Ok(RetentionResponse {
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
            .route("/compaction", web::get().to(get_compaction))
            .route("/compaction", web::post().to(compact))
            .route("/retention", web::get().to(get_retention))
            .route("/retention", web::put().to(update_retention))
            .route("/retention/preview", web::post().to(preview_retention))
            .route("/retention/run", web::post().to(run_retention))
            .route("/age", web::get().to(get_age))
            .route(
                "/source-maps/gc/preview",
                web::post().to(preview_source_map_gc),
            )
            .route("/source-maps/gc", web::post().to(gc_source_maps)),
    );
}
