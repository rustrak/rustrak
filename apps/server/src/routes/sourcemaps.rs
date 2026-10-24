use actix_multipart::Multipart;
use actix_web::{web, HttpResponse};
use futures_util::StreamExt as _;
use serde::{Deserialize, Serialize};
use sha1::Digest as _;
use std::collections::HashMap;
use std::sync::Arc;

use crate::auth::ApiActor;
use crate::config::Config;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::source_file::DIF_JOB_KIND;
use crate::services::access::{self, Action};
use crate::services::sourcemap::{get_missing_chunks, store_chunks, SourceMapProvider};

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

/// Multipart chunk upload.
/// Each part's **field name** may be the SHA-1 hex digest of that part's binary content
/// (sentry-cli v2 protocol, e.g. `da39a3ee5e6b4b0d3255bfef95601890afd80709`) or an arbitrary
/// name like `file` (sentry-cli v3.x). Up to 64 parts per request; each part body is raw binary.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[allow(dead_code)]
struct ChunkUploadBody {}

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AssembleBody {
    pub checksum: String,
    pub chunks: Vec<String>,
    pub projects: Vec<String>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AssembleResponse {
    pub state: String,
    #[serde(rename = "missingChunks")]
    pub missing_chunks: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Synthetic organization sentry-cli checks before uploading.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct OrgDetails {
    pub slug: String,
    pub name: String,
    pub id: String,
    pub features: Vec<String>,
}

/// Chunk upload capabilities, in sentry-cli's camelCase.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ChunkUploadOptions {
    pub url: String,
    pub chunk_size: u64,
    pub chunks_per_request: u64,
    pub max_request_size: u64,
    pub hash_algorithm: String,
    pub accept: Vec<String>,
    pub concurrency: u64,
    /// Required by the `sentry` npm uploader that Sentry SDK v11 bundler
    /// plugins use; it rejects the response without it. Empty means chunks
    /// are sent uncompressed, which is what `chunk_upload` reads.
    pub compression: Vec<String>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct SourceMapFile {
    pub debug_id: String,
    pub file_type: String,
    pub size: i32,
    pub times_used: i32,
    pub date_uploaded: String,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SourceMapList {
    pub data: Vec<SourceMapFile>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/0/organizations/{org_slug}/",
    operation_id = "sourceMaps.orgDetails",
    tag = "Source Maps",
    params(("org_slug" = String, Path, description = "Organization slug")),
    responses(
        (status = 200, description = "Synthetic org metadata accepted by sentry-cli", body = OrgDetails),
    ),
))]
/// GET /api/0/organizations/{org_slug}/
/// Always returns 200 with synthetic org data — sentry-cli validates this before uploading.
pub async fn org_details(path: web::Path<String>) -> AppResult<HttpResponse> {
    let org_slug = path.into_inner();
    Ok(HttpResponse::Ok().json(OrgDetails {
        name: org_slug.clone(),
        slug: org_slug,
        id: "1".to_string(),
        features: vec![
            "artifact-bundles".to_string(),
            "artifact-bundles-v2".to_string(),
        ],
    }))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/0/organizations/{org_slug}/chunk-upload/",
    operation_id = "sourceMaps.chunkUploadCapability",
    tag = "Source Maps",
    params(("org_slug" = String, Path, description = "Organization slug")),
    responses(
        (status = 200, description = "Chunk upload capabilities and URL", body = ChunkUploadOptions),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/0/organizations/{org_slug}/chunk-upload/
pub async fn chunk_upload_capability(
    path: web::Path<String>,
    _actor: ApiActor,
    config: web::Data<Config>,
) -> AppResult<HttpResponse> {
    let org_slug = path.into_inner();

    let base_url = config
        .public_url
        .clone()
        .unwrap_or_else(|| format!("http://{}:{}", config.host, config.port));

    let chunk_size = config.max_chunk_size_bytes as u64;
    Ok(HttpResponse::Ok().json(ChunkUploadOptions {
        url: format!(
            "{}/api/0/organizations/{}/chunk-upload/",
            base_url, org_slug
        ),
        chunk_size,
        chunks_per_request: 64,
        max_request_size: chunk_size * 64,
        hash_algorithm: "sha1".to_string(),
        accept: [
            "release_files",
            "sources",
            "artifact_bundles",
            "artifact_bundles_v2",
            "debug_files",
            "proguard",
        ]
        .map(String::from)
        .to_vec(),
        concurrency: 8,
        compression: Vec::new(),
    }))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/0/organizations/{org_slug}/chunk-upload/",
    operation_id = "sourceMaps.chunkUpload",
    tag = "Source Maps",
    params(("org_slug" = String, Path, description = "Organization slug")),
    request_body(
        content_type = "multipart/form-data",
        description = "One or more binary chunk files. Field names may be SHA-1 hashes (v2 protocol) or arbitrary names like 'file' (v3.x protocol).",
        content = ChunkUploadBody,
    ),
    responses(
        (status = 200, description = "Chunks stored successfully"),
        (status = 400, description = "Invalid multipart or chunk too large", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// POST /api/0/organizations/{org_slug}/chunk-upload/
pub async fn chunk_upload(
    _path: web::Path<String>,
    _actor: ApiActor,
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    mut payload: Multipart,
) -> AppResult<HttpResponse> {
    let max_chunk_size = config.max_chunk_size_bytes;
    const MAX_PARTS: usize = 64;
    let max_total_bytes = max_chunk_size.saturating_mul(MAX_PARTS);
    let mut parts: Vec<(String, Vec<u8>)> = Vec::new();
    let mut total_bytes: usize = 0;

    while let Some(field_result) = payload.next().await {
        let mut field =
            field_result.map_err(|e| AppError::Validation(format!("multipart error: {}", e)))?;

        let field_name = field
            .content_disposition()
            .and_then(|cd| cd.get_name().map(|s| s.to_string()))
            .unwrap_or_default();

        // Skip unnamed fields; named fields are validated by chunk_storage_key()
        if field_name.is_empty() {
            continue;
        }

        if parts.len() >= MAX_PARTS {
            return Err(AppError::Validation(format!(
                "too many parts: request has more than {} chunks",
                MAX_PARTS
            )));
        }

        let mut hasher = sha1::Sha1::new();
        let mut buf: Vec<u8> = Vec::new();

        while let Some(chunk) = field.next().await {
            let data =
                chunk.map_err(|e| AppError::Validation(format!("multipart chunk error: {}", e)))?;
            if buf.len() + data.len() > max_chunk_size {
                return Err(AppError::Validation(format!(
                    "chunk too large: {} bytes exceeds limit {}",
                    buf.len() + data.len(),
                    max_chunk_size
                )));
            }
            hasher.update(&data);
            buf.extend_from_slice(&data);
        }

        total_bytes += buf.len();
        if total_bytes > max_total_bytes {
            return Err(AppError::Validation(format!(
                "request too large: {} bytes exceeds maximum {} bytes",
                total_bytes, max_total_bytes
            )));
        }

        let sha1_hex = hex::encode(hasher.finalize());
        let key = chunk_storage_key(&field_name, &sha1_hex)?;
        parts.push((key, buf));
    }

    if parts.is_empty() {
        return Err(AppError::Validation(
            "no valid file parts found in multipart request".to_string(),
        ));
    }

    store_chunks(pool.get_ref(), parts, max_chunk_size).await?;

    Ok(HttpResponse::Ok().finish())
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/0/organizations/{org_slug}/artifactbundle/assemble/",
    operation_id = "sourceMaps.artifactBundleAssemble",
    tag = "Source Maps",
    params(("org_slug" = String, Path, description = "Organization slug")),
    request_body = AssembleBody,
    responses(
        (status = 200, description = "Assembly job state (not_found / created / assembling / ok / error). Missing chunks are listed in missingChunks.", body = AssembleResponse),
        (status = 400, description = "Bad request or checksum mismatch", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Project not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// POST /api/0/organizations/{org_slug}/artifactbundle/assemble/
pub async fn artifact_bundle_assemble(
    _path: web::Path<String>,
    actor: ApiActor,
    pool: web::Data<DbPool>,
    _provider: web::Data<Arc<dyn SourceMapProvider>>,
    body: web::Json<AssembleBody>,
) -> AppResult<HttpResponse> {
    // Guard: projects array must not be empty
    if body.projects.is_empty() {
        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "detail": "projects array must not be empty"
        })));
    }

    let Some(project_id) = resolve_project(pool.get_ref(), &body.projects[0]).await? else {
        return Ok(HttpResponse::NotFound().json(serde_json::json!({
            "detail": "project not found"
        })));
    };

    // Enforce mutation access on the resolved project.
    access::require(
        pool.get_ref(),
        actor.is_admin(),
        actor.user_id(),
        project_id,
        Action::UpdateProject,
    )
    .await?;

    let response = enqueue_assembly(
        pool.get_ref(),
        project_id,
        NewJob {
            checksum: &body.checksum,
            chunks: &body.chunks,
            kind: "artifact_bundle",
            name: None,
            debug_id: None,
        },
    )
    .await?;
    Ok(HttpResponse::Ok().json(response))
}

/// The project a sentry-cli request names: by slug first, then by numeric ID
/// (sentry-cli may send either).
async fn resolve_project(pool: &DbPool, project_ref: &str) -> AppResult<Option<i32>> {
    let by_slug: Option<(i32,)> = sqlx::query_as("SELECT id FROM projects WHERE slug = $1")
        .bind(project_ref)
        .fetch_optional(pool)
        .await?;
    if let Some((id,)) = by_slug {
        return Ok(Some(id));
    }
    let Ok(numeric_id) = project_ref.parse::<i32>() else {
        return Ok(None);
    };
    let by_id: Option<(i32,)> = sqlx::query_as("SELECT id FROM projects WHERE id = $1")
        .bind(numeric_id)
        .fetch_optional(pool)
        .await?;
    Ok(by_id.map(|(id,)| id))
}

/// One file to assemble, as an assemble request describes it.
struct NewJob<'a> {
    checksum: &'a str,
    chunks: &'a [String],
    kind: &'a str,
    name: Option<&'a str>,
    debug_id: Option<&'a str>,
}

/// Answers an assemble request for one file: the state of its job, or the
/// chunks still missing, queueing the job once they are all there.
async fn enqueue_assembly(
    pool: &DbPool,
    project_id: i32,
    new: NewJob<'_>,
) -> AppResult<AssembleResponse> {
    // A finished (or in-flight) job answers the poll before the chunk check:
    // the assembly worker deletes the consumed chunk rows on success, so a
    // sentry-cli `--wait` poll after completion would otherwise see its own
    // chunks as missing forever. Error jobs fall through so a re-submit with
    // corrected chunks can re-queue via the ON CONFLICT below.
    let existing_job: Option<crate::models::source_file::AssemblyJob> = sqlx::query_as(
        "SELECT * FROM assembly_jobs WHERE bundle_checksum = $1 AND project_id = $2",
    )
    .bind(new.checksum)
    .bind(project_id)
    .fetch_optional(pool)
    .await?;
    let failed = match existing_job {
        Some(job) if job.state == "ok" && job.kind == DIF_JOB_KIND => {
            return proguard_reupload(pool, project_id, &job, &new).await
        }
        Some(job) if job.state != "error" => {
            return Ok(job_response(job.state.as_str(), job.detail))
        }
        Some(job) => Some(job),
        None => None,
    };

    // Check for missing chunks
    let missing = get_missing_chunks(pool, new.chunks).await?;
    if !missing.is_empty() {
        // A failed job's chunks are gone, so a poll after the failure must
        // hear the failure, as Sentry answers, not be asked to upload again.
        // Sentry forgets that state after 10 minutes (`set_assemble_status`,
        // `ex=600`); then the file is unknown and its chunks are asked for.
        if let Some(job) = failed {
            if chrono::Utc::now() - job.updated_at < chrono::Duration::minutes(10) {
                return Ok(job_response("error", job.detail));
            }
        }
        return Ok(not_found(missing));
    }

    // INSERT ... ON CONFLICT: re-queue if the existing job errored, otherwise leave it.
    // Keep ownership rows in the same transaction so a worker cannot complete
    // between the job update and the reference insert.
    let mut tx = pool.begin().await?;
    let mut missing = Vec::new();
    for checksum in new.chunks {
        let present: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM chunk WHERE checksum = $1)")
                .bind(checksum)
                .fetch_one(&mut *tx)
                .await?;
        if !present {
            missing.push(checksum.clone());
        }
    }
    if !missing.is_empty() {
        return Ok(not_found(missing));
    }

    const UPSERT: &str = r#"
        INSERT INTO assembly_jobs(bundle_checksum, project_id, chunks, state, kind, name, debug_id)
        VALUES ($1, $2, $3, 'created', $4, $5, $6)
        ON CONFLICT(bundle_checksum, project_id) DO UPDATE
          SET state = 'created', detail = NULL, chunks = EXCLUDED.chunks, retry_count = 0,
              kind = EXCLUDED.kind, name = EXCLUDED.name, debug_id = EXCLUDED.debug_id
          WHERE assembly_jobs.state = 'error'
        RETURNING *
        "#;

    #[cfg(feature = "postgres")]
    let chunks = new.chunks.to_vec();
    #[cfg(not(feature = "postgres"))]
    let chunks =
        serde_json::to_string(new.chunks).map_err(|e| AppError::Internal(e.to_string()))?;

    let job: Option<crate::models::source_file::AssemblyJob> = sqlx::query_as(UPSERT)
        .bind(new.checksum)
        .bind(project_id)
        .bind(&chunks)
        .bind(new.kind)
        .bind(new.name)
        .bind(new.debug_id)
        .fetch_optional(&mut *tx)
        .await?;

    // If None (conflict — job already exists): fetch existing
    let inserted_job = job.is_some();
    let job = match job {
        Some(j) => j,
        None => {
            sqlx::query_as(
                "SELECT * FROM assembly_jobs WHERE bundle_checksum = $1 AND project_id = $2",
            )
            .bind(new.checksum)
            .bind(project_id)
            .fetch_one(&mut *tx)
            .await?
        }
    };

    if inserted_job {
        sqlx::query("DELETE FROM assembly_job_chunks WHERE job_id = $1")
            .bind(job.id)
            .execute(&mut *tx)
            .await?;
        for checksum in new.chunks {
            sqlx::query(
                "INSERT INTO assembly_job_chunks(job_id, checksum) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            )
            .bind(job.id)
            .bind(checksum)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;

    Ok(job_response(job.state.as_str(), job.detail.clone()))
}

/// Answers for a DIF already assembled. The same ProGuard mapping requested
/// under another UUID is stored for that UUID too, as Sentry's
/// `batch_assemble` clones it (`_is_proguard_reupload_clone_request`).
async fn proguard_reupload(
    pool: &DbPool,
    project_id: i32,
    job: &crate::models::source_file::AssemblyJob,
    new: &NewJob<'_>,
) -> AppResult<AssembleResponse> {
    use crate::services::debug_file::{alias_proguard_mapping, proguard_uuid_from_name};
    let requested = new.name.and_then(proguard_uuid_from_name);
    let stored = job.name.as_deref().and_then(proguard_uuid_from_name);
    match (requested, stored) {
        (Some(to), Some(from)) if to != from => {
            alias_proguard_mapping(pool, project_id, from, to).await?
        }
        (Some(_), None) => {
            return Ok(job_response(
                "error",
                Some("This file is not a ProGuard mapping.".into()),
            ))
        }
        _ => {}
    }
    Ok(job_response(job.state.as_str(), job.detail.clone()))
}

fn not_found(missing_chunks: Vec<String>) -> AssembleResponse {
    AssembleResponse {
        state: "not_found".to_string(),
        missing_chunks,
        detail: None,
    }
}

/// One file of a debug information file assemble request.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DifAssembleFile {
    pub name: String,
    pub chunks: Vec<String>,
    #[serde(default)]
    pub debug_id: Option<String>,
}

fn is_sha1(s: &str) -> bool {
    s.len() == 40 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/0/projects/{org_slug}/{project_slug}/files/difs/assemble/",
    operation_id = "debugFiles.assemble",
    tag = "Source Maps",
    params(
        ("org_slug" = String, Path, description = "Organization slug"),
        ("project_slug" = String, Path, description = "Project slug"),
    ),
    request_body = HashMap<String, DifAssembleFile>,
    responses(
        (status = 200, description = "Assembly state per file checksum (not_found / created / assembling / ok / error)", body = HashMap<String, AssembleResponse>),
        (status = 400, description = "Malformed request", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Project not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Assemble uploaded chunks into debug information files (ProGuard mappings, JVM source bundles).
///
/// Sentry's `DifAssembleEndpoint`: the body maps each file's SHA-1 to its
/// name, chunks and optional debug ID, and the answer maps it to its state.
pub async fn dif_assemble(
    path: web::Path<(String, String)>,
    actor: ApiActor,
    pool: web::Data<DbPool>,
    body: web::Json<HashMap<String, DifAssembleFile>>,
) -> AppResult<HttpResponse> {
    let (_, project_slug) = path.into_inner();
    for (checksum, file) in body.iter() {
        if !is_sha1(checksum) || !file.chunks.iter().all(|c| is_sha1(c)) {
            return Err(AppError::Validation(format!(
                "'{checksum}' and its chunks must be SHA-1 hex digests"
            )));
        }
        if let Some(id) = &file.debug_id {
            if id.is_empty() || !id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') {
                return Err(AppError::Validation(format!(
                    "'{id}' does not match '^[A-Fa-f0-9-]+$'"
                )));
            }
        }
    }

    let Some(project_id) = resolve_project(pool.get_ref(), &project_slug).await? else {
        return Err(AppError::NotFound(format!(
            "project not found: {project_slug}"
        )));
    };
    access::require(
        pool.get_ref(),
        actor.is_admin(),
        actor.user_id(),
        project_id,
        Action::UpdateProject,
    )
    .await?;

    let mut out = HashMap::new();
    for (checksum, file) in body.iter() {
        let response = if file.chunks.is_empty() {
            not_found(Vec::new())
        } else {
            enqueue_assembly(
                pool.get_ref(),
                project_id,
                NewJob {
                    checksum,
                    chunks: &file.chunks,
                    kind: DIF_JOB_KIND,
                    name: Some(&file.name),
                    debug_id: file.debug_id.as_deref(),
                },
            )
            .await?
        };
        out.insert(checksum.clone(), response);
    }
    Ok(HttpResponse::Ok().json(out))
}

/// A debug information file as Sentry's `DebugFileSerializer` describes it.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct DebugFileResponse {
    pub id: String,
    pub uuid: String,
    pub debug_id: String,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub code_id: Option<String>,
    pub cpu_name: String,
    pub object_name: String,
    pub symbol_type: String,
    pub headers: HashMap<String, String>,
    pub size: u64,
    pub sha1: String,
    pub date_created: String,
    pub data: serde_json::Value,
}

/// Multipart legacy debug file upload: one `file` part holding a zip.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[allow(dead_code)]
struct DifUploadBody {}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/0/projects/{org_slug}/{project_slug}/files/dsyms/",
    operation_id = "debugFiles.upload",
    tag = "Source Maps",
    params(
        ("org_slug" = String, Path, description = "Organization slug"),
        ("project_slug" = String, Path, description = "Project slug"),
    ),
    request_body(
        content_type = "multipart/form-data",
        description = "A `file` part: a zip of debug files, such as `proguard/<uuid>.txt`",
        content = DifUploadBody,
    ),
    responses(
        (status = 201, description = "The debug files queued for processing", body = Vec<DebugFileResponse>),
        (status = 400, description = "Missing or invalid upload", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Project not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Upload a zip of debug information files (the path `sentry-cli upload-proguard` takes).
///
/// Sentry's `DebugFilesEndpoint.post` processes the zip in the request.
/// Here each file goes through the assembly worker instead, the same as a
/// chunked upload, because building a ProGuard cache can take gigabytes.
/// Only ProGuard mappings are recognised; other entries are skipped.
pub async fn dif_upload(
    path: web::Path<(String, String)>,
    actor: ApiActor,
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    mut payload: Multipart,
) -> AppResult<HttpResponse> {
    let (_, project_slug) = path.into_inner();
    let Some(project_id) = resolve_project(pool.get_ref(), &project_slug).await? else {
        return Err(AppError::NotFound(format!(
            "project not found: {project_slug}"
        )));
    };
    access::require(
        pool.get_ref(),
        actor.is_admin(),
        actor.user_id(),
        project_id,
        Action::UpdateProject,
    )
    .await?;

    let max_size = config.max_chunk_size_bytes.saturating_mul(64);
    let mut upload: Option<Vec<u8>> = None;
    while let Some(field) = payload.next().await {
        let mut field =
            field.map_err(|e| AppError::Validation(format!("multipart error: {}", e)))?;
        let is_file = field
            .content_disposition()
            .and_then(|cd| cd.get_name())
            .is_some_and(|name| name == "file");
        if !is_file {
            while let Some(chunk) = field.next().await {
                chunk.map_err(|e| AppError::Validation(format!("multipart chunk error: {}", e)))?;
            }
            continue;
        }
        let mut buf = Vec::new();
        while let Some(chunk) = field.next().await {
            let data =
                chunk.map_err(|e| AppError::Validation(format!("multipart chunk error: {}", e)))?;
            if buf.len() + data.len() > max_size {
                return Err(AppError::Validation(format!(
                    "upload too large: exceeds {max_size} bytes"
                )));
            }
            buf.extend_from_slice(&data);
        }
        upload = Some(buf);
    }
    let Some(upload) = upload else {
        return Err(AppError::Validation("Missing uploaded file".into()));
    };

    let mappings = tokio::task::spawn_blocking(move || proguard_entries(&upload, max_size))
        .await
        .map_err(|e| AppError::Internal(format!("zip read panicked: {e}")))??;

    let chunk_size = config.max_chunk_size_bytes.max(1);
    let mut out = Vec::new();
    for (uuid, name, data) in mappings {
        let checksum = sha1_hex(&data);
        let parts: Vec<(String, Vec<u8>)> = data
            .chunks(chunk_size)
            .map(|c| (sha1_hex(c), c.to_vec()))
            .collect();
        let chunks: Vec<String> = parts.iter().map(|(c, _)| c.clone()).collect();
        store_chunks(pool.get_ref(), parts, chunk_size).await?;
        enqueue_assembly(
            pool.get_ref(),
            project_id,
            NewJob {
                checksum: &checksum,
                chunks: &chunks,
                kind: DIF_JOB_KIND,
                name: Some(&name),
                debug_id: Some(&uuid),
            },
        )
        .await?;
        out.push(DebugFileResponse {
            id: checksum.clone(),
            uuid: uuid.clone(),
            debug_id: uuid,
            code_id: None,
            cpu_name: "any".into(),
            object_name: "proguard-mapping".into(),
            symbol_type: "proguard".into(),
            headers: HashMap::from([("Content-Type".into(), "text/x-proguard+plain".into())]),
            size: data.len() as u64,
            sha1: checksum,
            date_created: chrono::Utc::now().to_rfc3339(),
            data: serde_json::json!({"features": ["mapping"]}),
        });
    }
    Ok(HttpResponse::Created().json(out))
}

/// The ProGuard mappings in an uploaded zip, as `(uuid, name, bytes)`.
/// `max_size` bounds all of them together, since they are all held at once.
fn proguard_entries(zip: &[u8], max_size: usize) -> AppResult<Vec<(String, String, Vec<u8>)>> {
    use std::io::Read as _;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip))
        .map_err(|e| AppError::Validation(format!("invalid ZIP archive: {}", e)))?;
    let mut out = Vec::new();
    let mut total = 0;
    for i in 0..archive.len() {
        let file = archive
            .by_index(i)
            .map_err(|e| AppError::Validation(format!("invalid ZIP entry: {}", e)))?;
        if file.is_dir() || file.is_symlink() {
            continue;
        }
        // Sentry matches the path the entry extracts to, which has a parent.
        let name = format!("/{}", file.name().unwrap_or_default());
        let Some(uuid) = crate::services::debug_file::proguard_uuid_from_name(&name) else {
            continue;
        };
        let mut data = Vec::new();
        file.take((max_size - total) as u64 + 1)
            .read_to_end(&mut data)
            .map_err(|e| AppError::Validation(format!("unreadable ZIP entry: {}", e)))?;
        total += data.len();
        if total > max_size {
            return Err(AppError::Validation(format!(
                "upload too large: exceeds {max_size} bytes"
            )));
        }
        out.push((uuid.to_string(), name, data));
    }
    Ok(out)
}

fn sha1_hex(data: &[u8]) -> String {
    hex::encode(sha1::Sha1::digest(data))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/0/projects/{org_slug}/{project_slug}/files/source-maps/",
    operation_id = "sourceMaps.list",
    tag = "Source Maps",
    params(
        ("org_slug" = String, Path, description = "Organization slug"),
        ("project_slug" = String, Path, description = "Project slug"),
    ),
    responses(
        (status = 200, description = "List of uploaded source map files", body = SourceMapList),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Project not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/0/projects/{org_slug}/{project_slug}/files/source-maps/
pub async fn list_source_maps(
    path: web::Path<(String, String)>,
    actor: ApiActor,
    pool: web::Data<DbPool>,
) -> AppResult<HttpResponse> {
    let (_, project_slug) = path.into_inner();

    let project: Option<(i32,)> = sqlx::query_as("SELECT id FROM projects WHERE slug = $1")
        .bind(&project_slug)
        .fetch_optional(pool.get_ref())
        .await?;
    let project_id = match project {
        Some((id,)) => id,
        None => {
            return Err(AppError::NotFound(format!(
                "project not found: {}",
                project_slug
            )));
        }
    };

    access::require(
        pool.get_ref(),
        actor.is_admin(),
        actor.user_id(),
        project_id,
        Action::ViewProject,
    )
    .await?;

    #[derive(sqlx::FromRow)]
    struct Row {
        debug_id: String,
        file_type: String,
        size: i32,
        times_used: i32,
        date_uploaded: String,
    }

    #[cfg(feature = "postgres")]
    let rows: Vec<Row> = sqlx::query_as(
        r#"
        SELECT
            sfm.debug_id::text AS debug_id,
            sfm.file_type,
            sf.size,
            sfm.times_used,
            sfm.created_at::text AS date_uploaded
        FROM source_file_metadata sfm
        JOIN source_file sf ON sf.id = sfm.file_id
        WHERE sfm.project_id = $1
        ORDER BY sfm.created_at DESC
        "#,
    )
    .bind(project_id)
    .fetch_all(pool.get_ref())
    .await?;

    #[cfg(not(feature = "postgres"))]
    let rows: Vec<Row> = sqlx::query_as(
        r#"
        SELECT
            sfm.debug_id AS debug_id,
            sfm.file_type,
            sf.size,
            sfm.times_used,
            sfm.created_at AS date_uploaded
        FROM source_file_metadata sfm
        JOIN source_file sf ON sf.id = sfm.file_id
        WHERE sfm.project_id = $1
        ORDER BY sfm.created_at DESC
        "#,
    )
    .bind(project_id)
    .fetch_all(pool.get_ref())
    .await?;

    let data = rows
        .into_iter()
        .map(|r| SourceMapFile {
            debug_id: r.debug_id,
            file_type: r.file_type,
            size: r.size,
            times_used: r.times_used,
            date_uploaded: r.date_uploaded,
        })
        .collect();

    Ok(HttpResponse::Ok().json(SourceMapList { data }))
}

// ---------------------------------------------------------------------------
// configure
// ---------------------------------------------------------------------------

/// Returns the storage key for a chunk given the multipart field name and computed SHA1.
///
/// sentry-cli v2 uses the SHA1 hash as the field name (verifiable).
/// sentry-cli v3.x uses "file" or other arbitrary names — in that case we accept the
/// chunk and key it by the computed SHA1 to stay idempotent with the assemble flow.
fn chunk_storage_key(field_name: &str, computed_sha1: &str) -> Result<String, AppError> {
    let is_sha1_name = field_name.len() == 40 && field_name.bytes().all(|b| b.is_ascii_hexdigit());

    if is_sha1_name && !computed_sha1.eq_ignore_ascii_case(field_name) {
        return Err(AppError::Validation(format!(
            "checksum mismatch for field '{}': content SHA1 is {}",
            field_name, computed_sha1
        )));
    }

    Ok(computed_sha1.to_string())
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg
        // Organization endpoints
        .route(
            "/api/0/organizations/{org_slug}/",
            web::get().to(org_details),
        )
        .route(
            "/api/0/organizations/{org_slug}/chunk-upload/",
            web::get().to(chunk_upload_capability),
        )
        .route(
            "/api/0/organizations/{org_slug}/chunk-upload/",
            web::post().to(chunk_upload),
        )
        .route(
            "/api/0/organizations/{org_slug}/artifactbundle/assemble/",
            web::post().to(artifact_bundle_assemble),
        )
        // Project source-maps listing
        .route(
            "/api/0/projects/{org_slug}/{project_slug}/files/dsyms/",
            web::post().to(dif_upload),
        )
        .route(
            "/api/0/projects/{org_slug}/{project_slug}/files/difs/assemble/",
            web::post().to(dif_assemble),
        )
        .route(
            "/api/0/projects/{org_slug}/{project_slug}/files/source-maps/",
            web::get().to(list_source_maps),
        );
}

/// Maps an assembly job state to the appropriate HTTP response.
/// All terminal states return HTTP 200; the state field in the body signals outcome.
fn job_response(state: &str, detail: Option<String>) -> AssembleResponse {
    AssembleResponse {
        state: state.to_string(),
        missing_chunks: vec![],
        // Only a failed job says why.
        detail: detail.filter(|_| state == "error"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::body::to_bytes;

    #[actix_web::test]
    async fn test_assemble_error_state_returns_200_with_missing_chunks() {
        let resp = HttpResponse::Ok().json(job_response(
            "error",
            Some("zip extraction failed".to_string()),
        ));
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
        let body = to_bytes(resp.into_body()).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["state"], "error");
        assert_eq!(json["missingChunks"], serde_json::json!([]));
    }

    // sentry-cli v2 / old protocol: field name IS the SHA1 hash
    #[test]
    fn test_chunk_storage_key_sha1_field_name_matching_accepted() {
        let sha1 = "da39a3ee5e6b4b0d3255bfef95601890afd80709";
        let result = chunk_storage_key(sha1, sha1);
        assert_eq!(result.unwrap(), sha1);
    }

    // field name looks like SHA1 but content is different → corruption, reject
    #[test]
    fn test_chunk_storage_key_sha1_field_name_mismatch_rejected() {
        let field_name = "da39a3ee5e6b4b0d3255bfef95601890afd80709";
        let actual_sha1 = "aabbccdd5e6b4b0d3255bfef95601890afd80709";
        let result = chunk_storage_key(field_name, actual_sha1);
        assert!(result.is_err());
        let err = format!("{:?}", result.unwrap_err());
        assert!(err.contains("checksum mismatch"));
        assert!(err.contains(field_name));
        assert!(err.contains(actual_sha1));
    }

    // sentry-cli v3.4.2: field name is "file", not SHA1 → accept, key by computed SHA1
    #[test]
    fn test_chunk_storage_key_file_field_name_accepted_keyed_by_sha1() {
        let computed_sha1 = "612d44793ab6d1b3d311b9e1b73785e805ef239a";
        let result = chunk_storage_key("file", computed_sha1);
        assert_eq!(result.unwrap(), computed_sha1);
    }

    // any non-SHA1 field name → accept, key by computed SHA1
    #[test]
    fn test_chunk_storage_key_arbitrary_field_name_accepted_keyed_by_sha1() {
        let computed_sha1 = "612d44793ab6d1b3d311b9e1b73785e805ef239a";
        let result = chunk_storage_key("chunk_0", computed_sha1);
        assert_eq!(result.unwrap(), computed_sha1);
    }

    // uppercase SHA1 field name with matching content → accept, return lowercase canonical SHA1
    #[test]
    fn test_chunk_storage_key_uppercase_sha1_field_name_accepted() {
        let lowercase_sha1 = "da39a3ee5e6b4b0d3255bfef95601890afd80709";
        let uppercase_field = "DA39A3EE5E6B4B0D3255BFEF95601890AFD80709";
        let result = chunk_storage_key(uppercase_field, lowercase_sha1);
        assert_eq!(result.unwrap(), lowercase_sha1);
    }

    // uppercase SHA1 field name with mismatched content → reject
    #[test]
    fn test_chunk_storage_key_uppercase_sha1_field_name_mismatch_rejected() {
        let uppercase_field = "DA39A3EE5E6B4B0D3255BFEF95601890AFD80709";
        let actual_sha1 = "aabbccdd5e6b4b0d3255bfef95601890afd80709";
        let result = chunk_storage_key(uppercase_field, actual_sha1);
        assert!(result.is_err());
        let err = format!("{:?}", result.unwrap_err());
        assert!(err.contains("checksum mismatch"));
    }
}
