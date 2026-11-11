use std::collections::HashMap;
use std::io::Cursor;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use bytes::Bytes;
use sha1::Digest as _;
use uuid::Uuid;

use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::source_file::{AssembleResponse, AssemblyJob, SourceMapFile, DIF_JOB_KIND};
use crate::services::sourcemap_store::SourceMapStore;
use crate::workers::sourcemap_assembly::AssemblyQueue;

pub struct SourceMapEntry {
    pub data: Bytes,
}

#[async_trait::async_trait]
pub trait SourceMapProvider: Send + Sync {
    async fn fetch_sourcemap(
        &self,
        project_id: i32,
        debug_id: &str,
        file_type: &str,
    ) -> AppResult<Option<SourceMapEntry>>;

    /// The parsed source map for `debug_id`, or `None` when there is none or
    /// it does not parse. Implementations may serve this from a cache.
    async fn fetch_decoded(
        &self,
        project_id: i32,
        debug_id: &str,
    ) -> Option<Arc<sourcemap::DecodedMap>> {
        let entry = match self
            .fetch_sourcemap(project_id, debug_id, "source_map")
            .await
        {
            Ok(Some(e)) => e,
            Ok(None) => return None,
            Err(e) => {
                log::warn!("fetch_sourcemap error for {}: {:?}", debug_id, e);
                return None;
            }
        };
        parse_sourcemap(debug_id, &entry.data).map(Arc::new)
    }
}

/// DecodedMap auto-detects Hermes (`x_facebook_sources`) vs regular maps;
/// SourceMap::from_reader alone returns IncompatibleSourceMap for Hermes.
fn parse_sourcemap(debug_id: &str, data: &[u8]) -> Option<sourcemap::DecodedMap> {
    match sourcemap::DecodedMap::from_reader(Cursor::new(data)) {
        Ok(m) => Some(m),
        Err(e) => {
            log::warn!("failed to parse source map for {}: {}", debug_id, e);
            None
        }
    }
}

/// Keyed by the store key, which is the SHA-1 of the file's contents, so a
/// re-upload under the same debug_id can never hit a stale parse. Bounded by
/// the summed size of the source files, evicting the least recently used.
struct ParsedMapCache {
    budget: usize,
    inner: std::sync::Mutex<ParsedMapCacheInner>,
    /// One lock per key being loaded, so concurrent misses on the same file
    /// wait for the first read and parse instead of repeating it.
    loading: std::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

#[derive(Default)]
struct ParsedMapCacheInner {
    entries: HashMap<String, CachedMap>,
    used: usize,
    clock: u64,
}

struct CachedMap {
    map: Arc<sourcemap::DecodedMap>,
    size: usize,
    last_used: u64,
}

impl ParsedMapCache {
    fn new(budget: usize) -> Self {
        Self {
            budget,
            inner: std::sync::Mutex::new(ParsedMapCacheInner::default()),
            loading: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// The map for `key`, loading it through `load` on a miss. Only one
    /// loader runs per key at a time; the others find the result in the cache.
    async fn get_or_load<F, Fut>(&self, key: &str, load: F) -> Option<Arc<sourcemap::DecodedMap>>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Option<(Arc<sourcemap::DecodedMap>, usize)>>,
    {
        if let Some(hit) = self.get(key) {
            return Some(hit);
        }
        let lock = Arc::clone(
            self.loading
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .entry(key.to_string())
                .or_default(),
        );
        let _loading = lock.lock().await;
        if let Some(hit) = self.get(key) {
            return Some(hit);
        }
        let loaded = load().await;
        if let Some((map, size)) = &loaded {
            self.insert(key, Arc::clone(map), *size);
        }
        self.loading
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(key);
        loaded.map(|(map, _)| map)
    }

    fn get(&self, key: &str) -> Option<Arc<sourcemap::DecodedMap>> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.clock += 1;
        let clock = inner.clock;
        let entry = inner.entries.get_mut(key)?;
        entry.last_used = clock;
        Some(Arc::clone(&entry.map))
    }

    fn insert(&self, key: &str, map: Arc<sourcemap::DecodedMap>, size: usize) {
        if size > self.budget {
            log::warn!(
                "source map {} is {} MB, above the {} MB cache budget; it will be parsed again for every event (raise SOURCEMAP_CACHE_MB)",
                key,
                size / (1024 * 1024),
                self.budget / (1024 * 1024)
            );
            return;
        }
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(old) = inner.entries.remove(key) {
            inner.used -= old.size;
        }
        while inner.used + size > self.budget {
            let Some(victim) = inner
                .entries
                .iter()
                .min_by_key(|(_, e)| e.last_used)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            if let Some(evicted) = inner.entries.remove(&victim) {
                inner.used -= evicted.size;
            }
        }
        inner.clock += 1;
        let last_used = inner.clock;
        inner.used += size;
        inner.entries.insert(
            key.to_string(),
            CachedMap {
                map,
                size,
                last_used,
            },
        );
    }
}

pub struct DbSourceMapProvider {
    pool: DbPool,
    store: Arc<dyn SourceMapStore>,
    cache: ParsedMapCache,
}

impl DbSourceMapProvider {
    pub fn new(pool: DbPool, store: Arc<dyn SourceMapStore>) -> Self {
        Self {
            pool,
            store,
            cache: ParsedMapCache::new(crate::config::DEFAULT_SOURCEMAP_CACHE_BYTES),
        }
    }

    /// Bytes of parsed source maps (measured as the size of the files they
    /// came from) kept across events.
    pub fn with_cache_budget(mut self, bytes: usize) -> Self {
        self.cache = ParsedMapCache::new(bytes);
        self
    }

    /// Which stored file serves `debug_id` right now, as
    /// `(source_file_metadata.id, storage_path)`. Also counts the use.
    async fn locate(
        &self,
        project_id: i32,
        debug_id: &str,
        file_type: &str,
    ) -> AppResult<Option<(Uuid, String)>> {
        let Ok(debug_uuid) = Uuid::parse_str(debug_id) else {
            return Ok(None);
        };
        let row: Option<(Uuid, String)> = sqlx::query_as(
            r#"
            SELECT sfm.id, sf.storage_path
            FROM source_file_metadata sfm
            JOIN source_file sf ON sf.id = sfm.file_id
            WHERE sfm.project_id = $1 AND sfm.debug_id = $2 AND sfm.file_type = $3
            "#,
        )
        .bind(project_id)
        .bind(debug_uuid)
        .bind(file_type)
        .fetch_optional(&self.pool)
        .await?;
        let Some((sfm_id, storage_path)) = row else {
            return Ok(None);
        };

        // times_used is a best-effort counter: a failed bump never fails the
        // lookup.
        if let Err(e) =
            sqlx::query("UPDATE source_file_metadata SET times_used = times_used + 1 WHERE id = $1")
                .bind(sfm_id)
                .execute(&self.pool)
                .await
        {
            log::debug!("Could not count a use of source map {sfm_id}: {e}");
        }

        Ok(Some((sfm_id, storage_path)))
    }

    async fn read(&self, storage_path: &str) -> Option<Bytes> {
        match self.store.get(storage_path).await {
            Ok(d) => Some(d),
            Err(crate::services::sourcemap_store::StoreError::NotFound(_)) => {
                log::warn!(
                    "source_file_metadata row exists but file missing on disk: {}",
                    storage_path
                );
                None
            }
            Err(e) => {
                log::warn!("failed to read source map from store: {}", e);
                None
            }
        }
    }
}

#[async_trait::async_trait]
impl SourceMapProvider for DbSourceMapProvider {
    async fn fetch_sourcemap(
        &self,
        project_id: i32,
        debug_id: &str,
        file_type: &str,
    ) -> AppResult<Option<SourceMapEntry>> {
        let Some((_, storage_path)) = self.locate(project_id, debug_id, file_type).await? else {
            return Ok(None);
        };
        Ok(self
            .read(&storage_path)
            .await
            .map(|data| SourceMapEntry { data }))
    }

    /// The database lookup runs on every event, so a deleted or replaced file
    /// stops resolving at once; only the read and the parse are cached.
    async fn fetch_decoded(
        &self,
        project_id: i32,
        debug_id: &str,
    ) -> Option<Arc<sourcemap::DecodedMap>> {
        let storage_path = match self.locate(project_id, debug_id, "source_map").await {
            Ok(Some((_, path))) => path,
            Ok(None) => return None,
            Err(e) => {
                log::warn!("fetch_sourcemap error for {}: {:?}", debug_id, e);
                return None;
            }
        };
        self.cache
            .get_or_load(&storage_path, || async {
                let data = self.read(&storage_path).await?;
                let map = Arc::new(parse_sourcemap(debug_id, &data)?);
                Some((map, data.len()))
            })
            .await
    }
}

/// Convert 1-indexed Sentry (lineno, colno) to 0-indexed sourcemap (line, col).
///
/// Returns `None` when lineno is `None` or `Some(0)` — both mean "unmapped" in
/// the Sentry protocol. Never uses plain `-1` arithmetic (avoids u32 wraparound).
pub fn normalize_sentry_position(lineno: Option<u32>, colno: Option<u32>) -> Option<(u32, u32)> {
    match lineno {
        None | Some(0) => None,
        Some(l) => Some((l.saturating_sub(1), colno.unwrap_or(0))),
    }
}

/// Returns checksums from `checksums` that are NOT present in the `chunk` table.
pub async fn get_missing_chunks(pool: &DbPool, checksums: &[String]) -> AppResult<Vec<String>> {
    if checksums.is_empty() {
        return Ok(vec![]);
    }
    let mut qb = sqlx::QueryBuilder::new("SELECT checksum FROM chunk WHERE checksum IN (");
    let mut sep = qb.separated(", ");
    for c in checksums {
        sep.push_bind(c);
    }
    qb.push(")");
    let present: Vec<String> = qb.build_query_scalar().fetch_all(pool).await?;
    let present: std::collections::HashSet<&str> = present.iter().map(String::as_str).collect();
    Ok(checksums
        .iter()
        .filter(|c| !present.contains(c.as_str()))
        .cloned()
        .collect())
}

/// Upsert chunk rows into the `chunk` table, enforcing the max-chunk-size limit.
pub async fn store_chunks(
    pool: &DbPool,
    parts: Vec<(String, Vec<u8>)>,
    max_chunk_size: usize,
) -> AppResult<()> {
    for (sha1, bytes) in parts {
        if bytes.len() > max_chunk_size {
            return Err(AppError::Validation(format!(
                "chunk too large: {} bytes exceeds limit {}",
                bytes.len(),
                max_chunk_size
            )));
        }
        let size = bytes.len() as i32;
        sqlx::query(
            "INSERT INTO chunk(checksum, size, data) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        )
        .bind(&sha1)
        .bind(size)
        .bind(&bytes)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Assembles all chunks into a ZIP bundle, extracts source map files, and stores
/// metadata in the database.
///
/// Steps:
/// 1. Fetch chunk rows in order; join bytes (enforcing `max_bundle_size_bytes`).
/// 2. Verify SHA1 matches `bundle_checksum`.
/// 3. Write to temp file; open as ZipArchive.
/// 4. Validate each entry for symlinks and path traversal.
/// 5. Extract to temp dir.
/// 6. Parse manifest.json.
/// 7. For each source-map entry: store file + upsert DB rows.
/// 8. Delete chunk rows.
pub async fn assemble_bundle(
    pool: &DbPool,
    store: &dyn SourceMapStore,
    project_id: i32,
    bundle_checksum: &str,
    chunk_checksums: &[String],
    max_bundle_size_bytes: usize,
) -> AppResult<()> {
    assemble_bundle_inner(
        pool,
        store,
        project_id,
        bundle_checksum,
        chunk_checksums,
        max_bundle_size_bytes,
        None,
    )
    .await
}

/// Assembles a worker-owned job and commits its terminal state with the source
/// metadata and chunk deletion.
pub async fn assemble_bundle_for_job(
    pool: &DbPool,
    store: &dyn SourceMapStore,
    project_id: i32,
    bundle_checksum: &str,
    chunk_checksums: &[String],
    max_bundle_size_bytes: usize,
    job_id: i64,
) -> AppResult<()> {
    assemble_bundle_inner(
        pool,
        store,
        project_id,
        bundle_checksum,
        chunk_checksums,
        max_bundle_size_bytes,
        Some(job_id),
    )
    .await
}

async fn assemble_bundle_inner(
    pool: &DbPool,
    store: &dyn SourceMapStore,
    project_id: i32,
    bundle_checksum: &str,
    chunk_checksums: &[String],
    max_bundle_size_bytes: usize,
    job_id: Option<i64>,
) -> AppResult<()> {
    let joined = join_chunks(
        pool,
        chunk_checksums,
        max_bundle_size_bytes,
        bundle_checksum,
    )
    .await?;

    let temp_dir = tempfile::tempdir().map_err(|e| AppError::Internal(e.to_string()))?;
    let zip_path = temp_dir.path().join("bundle.zip");
    tokio::fs::write(&zip_path, &joined)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    drop(joined); // free memory

    let extract_dir = temp_dir.path().join("extracted");
    let zip_path_owned = zip_path.clone();
    let extract_dir_owned = extract_dir.clone();
    tokio::task::spawn_blocking(move || -> Result<(), AppError> {
        let zip_file =
            std::fs::File::open(&zip_path_owned).map_err(|e| AppError::Internal(e.to_string()))?;
        let mut archive = zip::ZipArchive::new(zip_file)
            .map_err(|e| AppError::Validation(format!("invalid ZIP archive: {}", e)))?;

        std::fs::create_dir_all(&extract_dir_owned)
            .map_err(|e| AppError::Internal(e.to_string()))?;

        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| AppError::Internal(e.to_string()))?;

            // CVE-2025-29787: reject symlinks
            if file.is_symlink() {
                continue;
            }

            let name = file
                .name()
                .map_err(|e| AppError::Internal(e.to_string()))?
                .to_string();
            // Strip the "~/" prefix used by Sentry artifact bundles so that extraction
            // and manifest lookup both resolve to the same relative path.
            let name = name.trim_start_matches("~/").to_string();
            // Path traversal guard — do NOT use canonicalize() (file doesn't exist yet).
            // Iterate the archive entry name only (not raw_dest) so extract_dir is
            // not duplicated in the resolved path.
            let mut resolved = PathBuf::from(&extract_dir_owned);
            for component in Path::new(&name).components() {
                match component {
                    Component::ParentDir => {
                        resolved.pop();
                    }
                    Component::Normal(c) => resolved.push(c),
                    _ => {}
                }
            }
            if !resolved.starts_with(&extract_dir_owned) {
                return Err(AppError::Validation(
                    "path traversal in archive".to_string(),
                ));
            }

            if let Some(parent) = resolved.parent() {
                std::fs::create_dir_all(parent).map_err(|e| AppError::Internal(e.to_string()))?;
            }

            if !name.ends_with('/') {
                let mut out = std::fs::File::create(&resolved)
                    .map_err(|e| AppError::Internal(e.to_string()))?;
                std::io::copy(&mut file, &mut out)
                    .map_err(|e| AppError::Internal(e.to_string()))?;
            }
        }
        Ok(())
    })
    .await
    .map_err(|e| AppError::Internal(format!("zip extraction panicked: {}", e)))??;

    let manifest_path = extract_dir.join("manifest.json");
    let manifest_bytes = tokio::fs::read(&manifest_path).await.map_err(|_| {
        AppError::Validation("manifest.json not found in artifact bundle".to_string())
    })?;
    let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| AppError::Validation(format!("invalid manifest.json: {}", e)))?;

    let files = manifest_files(&manifest);

    // We run in a transaction for the DB writes; store writes are outside (idempotent CAS).
    // Write-first (INSERT opens the tx): deferred BEGIN deliberate — the first
    // entry's disk read + CAS store run before the first INSERT, without the
    // write lock (see db::begin_write). Later entries' reads/stores ride the
    // open tx: accepted, since the CAS stores are idempotent and reads are
    // local — staged I/O would be the fix if the lock hold ever matters.
    let mut tx = pool.begin().await?;

    for (file_path, file_info) in &files {
        let headers = match file_info.get("headers").and_then(|h| h.as_object()) {
            Some(h) => h,
            None => continue,
        };

        let debug_id_str = match headers
            .get("debug-id")
            .or_else(|| headers.get("debug_id"))
            .and_then(|v| v.as_str())
        {
            Some(s) => s.to_string(),
            None => continue,
        };

        let debug_uuid = match Uuid::parse_str(&debug_id_str) {
            Ok(u) => u,
            Err(_) => {
                log::warn!("invalid debug_id in manifest: {}", debug_id_str);
                continue;
            }
        };

        let file_type = match file_info.get("type").and_then(|t| t.as_str()) {
            Some(t) => t.to_string(),
            None => continue,
        };

        // file_path in manifest may start with "~/" — strip that prefix
        let relative = file_path.trim_start_matches("~/").trim_start_matches('/');
        // Guard: reject any manifest path with traversal components
        if Path::new(relative)
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::RootDir))
        {
            return Err(AppError::Validation(format!(
                "invalid manifest file path: {}",
                file_path
            )));
        }
        let file_on_disk = extract_dir.join(relative);
        let file_bytes = match tokio::fs::read(&file_on_disk).await {
            Ok(b) => b,
            Err(e) => {
                log::warn!("cannot read extracted file {}: {}", file_path, e);
                continue;
            }
        };

        store_file(
            &mut tx,
            store,
            project_id,
            debug_uuid,
            &file_type,
            file_bytes.into(),
        )
        .await?;
    }

    finish_job(&mut tx, job_id, chunk_checksums).await?;
    tx.commit().await?;

    Ok(())
}

/// The bytes of `chunk_checksums` in order, checked against `checksum`.
pub(crate) async fn join_chunks(
    pool: &DbPool,
    chunk_checksums: &[String],
    max_size_bytes: usize,
    checksum: &str,
) -> AppResult<Vec<u8>> {
    let mut joined: Vec<u8> = Vec::new();
    for chunk in chunk_checksums {
        let data: Vec<u8> = sqlx::query_scalar("SELECT data FROM chunk WHERE checksum = $1")
            .bind(chunk)
            .fetch_one(pool)
            .await
            .map_err(|_| AppError::Validation(format!("chunk not found: {}", chunk)))?;
        if joined.len() + data.len() > max_size_bytes {
            return Err(AppError::Validation(format!(
                "bundle too large: exceeds {} bytes limit",
                max_size_bytes
            )));
        }
        joined.extend_from_slice(&data);
    }

    let mut hasher = sha1::Sha1::new();
    hasher.update(&joined);
    let computed = hex::encode(hasher.finalize());
    if computed != checksum {
        return Err(AppError::Validation(format!(
            "checksum mismatch: expected {}, got {}",
            checksum, computed
        )));
    }
    Ok(joined)
}

/// Stores `file_bytes` in the CAS and records it as `project_id`'s
/// `file_type` for `debug_uuid`. A re-upload for the same debug ID wins, as
/// in Sentry, which resolves to the newest file.
pub(crate) async fn store_file(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    store: &dyn SourceMapStore,
    project_id: i32,
    debug_uuid: Uuid,
    file_type: &str,
    file_bytes: Bytes,
) -> AppResult<()> {
    let mut fhasher = sha1::Sha1::new();
    fhasher.update(&file_bytes);
    let storage_key = hex::encode(fhasher.finalize());
    let file_size = file_bytes.len() as i32;

    // Store file in CAS (outside transaction — idempotent).
    // Propagate errors: a store failure must abort the job so chunks are NOT
    // deleted and the assembly can be retried.
    store
        .put(&storage_key, file_bytes)
        .await
        .map_err(|e| AppError::Internal(format!("failed to store source file: {}", e)))?;

    // Insert, then select: an upsert's RETURNING is empty on conflict.
    sqlx::query(
        r#"
        INSERT INTO source_file(id, checksum, size, storage_path)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT(checksum) DO NOTHING
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(&storage_key)
    .bind(file_size)
    .bind(&storage_key)
    .execute(&mut **tx)
    .await?;
    let file_id: Uuid = sqlx::query_scalar("SELECT id FROM source_file WHERE checksum = $1")
        .bind(&storage_key)
        .fetch_one(&mut **tx)
        .await?;

    sqlx::query(
        r#"
        INSERT INTO source_file_metadata(id, project_id, debug_id, file_type, file_id)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT(project_id, debug_id, file_type) DO UPDATE SET file_id = EXCLUDED.file_id
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(project_id)
    .bind(debug_uuid)
    .bind(file_type)
    .bind(file_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// The chunks a queued job assembles, in order.
pub(crate) async fn job_chunks(pool: &DbPool, job_id: i64) -> AppResult<Vec<String>> {
    Ok(sqlx::query_scalar(
        "SELECT checksum FROM assembly_job_chunks WHERE job_id = $1 ORDER BY position",
    )
    .bind(job_id)
    .fetch_all(pool)
    .await?)
}

/// Marks a worker-owned job `ok` and lets its chunks go; without a job,
/// deletes `chunk_checksums` that no job holds.
pub(crate) async fn finish_job(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    job_id: Option<i64>,
    chunk_checksums: &[String],
) -> AppResult<()> {
    let Some(job_id) = job_id else {
        return delete_unheld_chunks(tx, chunk_checksums).await;
    };
    release_job_chunks(tx, job_id).await?;
    let updated = sqlx::query(
        "UPDATE assembly_jobs SET state = 'ok', detail = NULL, updated_at = CURRENT_TIMESTAMP \
         WHERE id = $1 AND state = 'assembling'",
    )
    .bind(job_id)
    .execute(&mut **tx)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(AppError::Internal(format!(
            "assembly job {job_id} was not assembling"
        )));
    }
    Ok(())
}

/// Drops a job's hold on its chunks and deletes those no other job holds.
pub(crate) async fn release_job_chunks(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    job_id: i64,
) -> AppResult<()> {
    let checksums: Vec<String> =
        sqlx::query_scalar("DELETE FROM assembly_job_chunks WHERE job_id = $1 RETURNING checksum")
            .bind(job_id)
            .fetch_all(&mut **tx)
            .await?;
    delete_unheld_chunks(tx, &checksums).await
}

async fn delete_unheld_chunks(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    checksums: &[String],
) -> AppResult<()> {
    for checksum in checksums {
        sqlx::query(
            "DELETE FROM chunk WHERE checksum = $1 \
             AND NOT EXISTS (SELECT 1 FROM assembly_job_chunks WHERE checksum = $1)",
        )
        .bind(checksum)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

fn manifest_files(manifest: &serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    manifest
        .get("files")
        .and_then(|files| files.as_object())
        .cloned()
        .unwrap_or_default()
}

/// Rewrites stack frames in `event_data` using stored source maps.
///
/// Walks both places Sentry's protocol allows a stack trace to live:
/// `exception.values[*].stacktrace.frames` and `threads.values[*].stacktrace.frames`
/// (the latter carries crashes reported via threads instead of an exception —
/// e.g. JS worker-thread crashes). Both have the identical `values[i].stacktrace.frames`
/// shape, so the same walk works under either root key.
///
/// Frame-rewriting errors are non-fatal: we `warn!` and continue.
pub async fn rewrite_frames(
    provider: &dyn SourceMapProvider,
    project_id: i32,
    event_data: &mut serde_json::Value,
) -> AppResult<()> {
    // Build code_file → debug_id_str map from debug_meta.images
    let images_map: HashMap<String, String> = event_data
        .get("debug_meta")
        .and_then(|dm| dm.get("images"))
        .and_then(|imgs| imgs.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|img| {
                    Some((
                        img.get("code_file")?.as_str()?.to_string(),
                        img.get("debug_id")?.as_str()?.to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default();

    if images_map.is_empty() {
        return Ok(());
    }

    // Parsed maps live for the whole event: one fetch and one parse per
    // debug_id, shared by every frame under either root key. Symbolicator does
    // the same in `SourceMapLookup::get_module`.
    let mut parsed: ParsedMaps = HashMap::new();
    rewrite_frames_under(
        provider,
        project_id,
        event_data,
        "exception",
        &images_map,
        &mut parsed,
    )
    .await?;
    rewrite_frames_under(
        provider,
        project_id,
        event_data,
        "threads",
        &images_map,
        &mut parsed,
    )
    .await?;

    Ok(())
}

/// The map and 0-indexed position a frame lookup should use. An index map
/// (`sections`, which Turbopack writes for nearly every chunk) resolves inside
/// the section that contains the position, as `SourceMapIndex::lookup_token`
/// does: a position before that section's first mapping stays unresolved
/// instead of borrowing the preceding section's last token. Sections are
/// ordered by offset; one that is referenced by URL rather than embedded is a
/// miss.
fn section_for(
    map: &sourcemap::DecodedMap,
    line: u32,
    col: u32,
) -> Option<(&sourcemap::DecodedMap, u32, u32)> {
    let sourcemap::DecodedMap::Index(index) = map else {
        return Some((map, line, col));
    };
    let section = index
        .sections()
        .take_while(|s| s.get_offset() <= (line, col))
        .last()?;
    let (off_line, off_col) = section.get_offset();
    let col = if line == off_line { col - off_col } else { col };
    section_for(section.get_sourcemap()?, line - off_line, col)
}

/// Source maps already resolved for the event in flight, by debug_id. `None`
/// records a miss so the next frame does not ask the provider again.
type ParsedMaps = HashMap<String, Option<Arc<sourcemap::DecodedMap>>>;

async fn load_decoded_map(
    provider: &dyn SourceMapProvider,
    project_id: i32,
    debug_id: &str,
    parsed: &mut ParsedMaps,
) -> Option<Arc<sourcemap::DecodedMap>> {
    if let Some(hit) = parsed.get(debug_id) {
        return hit.clone();
    }
    let decoded = provider.fetch_decoded(project_id, debug_id).await;
    parsed.insert(debug_id.to_string(), decoded.clone());
    decoded
}

/// Rewrites every frame under `event_data[root_key]["values"][*]["stacktrace"]["frames"]`.
/// `root_key` is `"exception"` or `"threads"` — see [`rewrite_frames`].
async fn rewrite_frames_under(
    provider: &dyn SourceMapProvider,
    project_id: i32,
    event_data: &mut serde_json::Value,
    root_key: &str,
    images_map: &HashMap<String, String>,
    parsed: &mut ParsedMaps,
) -> AppResult<()> {
    // Iterate by index to avoid holding a &mut borrow across .await points.
    let value_count = event_data[root_key]["values"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);
    if value_count == 0 {
        return Ok(());
    }

    for value_idx in 0..value_count {
        let frame_count = event_data[root_key]["values"][value_idx]["stacktrace"]["frames"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0);

        for frame_idx in 0..frame_count {
            let filename: Option<String> = event_data[root_key]["values"][value_idx]["stacktrace"]
                ["frames"][frame_idx]
                .get("filename")
                .and_then(|f| f.as_str())
                .map(|s| s.to_string());
            let abs_path: Option<String> = event_data[root_key]["values"][value_idx]["stacktrace"]
                ["frames"][frame_idx]
                .get("abs_path")
                .and_then(|f| f.as_str())
                .map(|s| s.to_string());
            // Skip only when both are absent — abs_path-only frames are valid
            if abs_path.is_none() && filename.is_none() {
                continue;
            }
            let frame_lineno: Option<u32> = event_data[root_key]["values"][value_idx]["stacktrace"]
                ["frames"][frame_idx]
                .get("lineno")
                .and_then(|l| l.as_u64())
                .map(|l| l as u32);
            let frame_colno: Option<u32> = event_data[root_key]["values"][value_idx]["stacktrace"]
                ["frames"][frame_idx]
                .get("colno")
                .and_then(|c| c.as_u64())
                .map(|c| c as u32);

            // Resolve debug_id from code_file map.
            // Try abs_path first (full URL matching code_file), then filename.
            let debug_id = match abs_path
                .as_deref()
                .and_then(|k| images_map.get(k))
                .or_else(|| filename.as_deref().and_then(|k| images_map.get(k)))
            {
                Some(id) => id.clone(),
                None => continue,
            };

            // Fetched and parsed once per debug_id per event.
            let Some(decoded) = load_decoded_map(provider, project_id, &debug_id, parsed).await
            else {
                continue;
            };

            let Some((norm_lineno, norm_colno)) =
                normalize_sentry_position(frame_lineno, frame_colno)
            else {
                continue;
            };

            // An index map resolves inside the section holding the position.
            let Some((map, norm_lineno, norm_colno)) =
                section_for(&decoded, norm_lineno, norm_colno)
            else {
                continue;
            };
            let sm: &sourcemap::SourceMap = match map {
                sourcemap::DecodedMap::Regular(sm) => sm,
                sourcemap::DecodedMap::Hermes(smh) => smh,
                // section_for never returns an index map.
                sourcemap::DecodedMap::Index(_) => continue,
            };

            let Some(token) = map.lookup_token(norm_lineno, norm_colno) else {
                continue;
            };

            if token.get_src_line() == u32::MAX {
                continue;
            }

            let original_file = token.get_source().unwrap_or("").to_string();
            let src_line = token.get_src_line();
            let src_col = token.get_src_col();
            let token_name = token.get_name().map(|n| n.to_string());

            // Find source index via linear search (NEVER assume sourcesContent[0])
            let source_idx = (0..sm.get_source_count())
                .find(|&i| sm.get_source(i) == Some(original_file.as_str()));

            // Get source lines as owned Strings (sm is dropped after this loop body)
            let lines: Vec<String> = source_idx
                .and_then(|i| sm.get_source_contents(i))
                .map(|s| s.lines().map(|l| l.to_string()).collect())
                .unwrap_or_default();

            let l = src_line as usize;
            let pre_start = l.saturating_sub(3);
            let post_end = lines.len().min(l + 4);

            let existing_function = event_data[root_key]["values"][value_idx]["stacktrace"]
                ["frames"][frame_idx]
                .get("function")
                .and_then(|f| f.as_str())
                .unwrap_or("")
                .to_string();

            let new_lineno = src_line + 1; // back to 1-indexed
            let new_colno = src_col;
            // Hermes resolves names from x_facebook_sources (line==0, col=bytecode offset).
            // Regular maps return None without a minified SourceView; fall back to token name.
            let new_function = map
                .get_original_function_name(norm_lineno, norm_colno, Some(&existing_function), None)
                .map(|n| n.to_string())
                .or(token_name)
                .unwrap_or(existing_function);
            let context_line = lines.get(l).cloned().unwrap_or_default();
            let pre_context: Vec<serde_json::Value> = lines
                .get(pre_start..l)
                .unwrap_or_default()
                .iter()
                .map(|s| serde_json::Value::String(s.clone()))
                .collect();
            let post_context: Vec<serde_json::Value> = lines
                .get(l + 1..post_end)
                .unwrap_or_default()
                .iter()
                .map(|s| serde_json::Value::String(s.clone()))
                .collect();

            let frame =
                &mut event_data[root_key]["values"][value_idx]["stacktrace"]["frames"][frame_idx];
            // Only overwrite filename/abs_path when the token has a non-empty source name.
            // A None/empty source (malformed map) must not corrupt the original filename.
            // Reference: getsentry/symbolicator symbolication.rs L246-273 (e282ec0)
            if !original_file.is_empty() {
                frame["filename"] = original_file.into();
                frame["abs_path"] = serde_json::Value::Null;
            }
            // Record which source map was applied (Sentry UI observability)
            if !frame["data"].is_object() {
                frame["data"] = serde_json::json!({});
            }
            frame["data"]["sourcemap"] = debug_id.clone().into();
            frame["lineno"] = new_lineno.into();
            frame["colno"] = new_colno.into();
            frame["function"] = new_function.into();
            frame["context_line"] = context_line.into();
            frame["pre_context"] = pre_context.into();
            frame["post_context"] = post_context.into();
        }
    }

    Ok(())
}

/// One file to assemble, as an assemble request describes it.
pub struct NewJob<'a> {
    pub checksum: &'a str,
    pub chunks: &'a [String],
    pub kind: &'a str,
    pub name: Option<&'a str>,
    pub debug_id: Option<&'a str>,
}

/// Answers an assemble request for one file: the state of its job, or the
/// chunks still missing, queueing the job once they are all there.
pub async fn enqueue_assembly(
    pool: &DbPool,
    queue: &AssemblyQueue,
    project_id: i32,
    new: NewJob<'_>,
) -> AppResult<AssembleResponse> {
    // A finished (or in-flight) job answers the poll before the chunk check:
    // the assembly worker deletes the consumed chunk rows on success, so a
    // sentry-cli `--wait` poll after completion would otherwise see its own
    // chunks as missing forever. Error jobs fall through so a re-submit with
    // corrected chunks can re-queue via the ON CONFLICT below.
    let existing_job: Option<AssemblyJob> = sqlx::query_as(
        "SELECT * FROM assembly_jobs WHERE bundle_checksum = $1 AND project_id = $2",
    )
    .bind(new.checksum)
    .bind(project_id)
    .fetch_optional(pool)
    .await?;
    let failed = match existing_job {
        Some(job) if job.state == "ok" && job.kind == DIF_JOB_KIND => {
            match dif_reupload(pool, project_id, &job, &new).await? {
                Some(response) => return Ok(response),
                None => None,
            }
        }
        Some(job) if job.state != "error" => {
            return Ok(job_response(job.state.as_str(), job.detail))
        }
        Some(job) => Some(job),
        None => None,
    };

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

    let job: Option<AssemblyJob> = sqlx::query_as(
        r#"
        INSERT INTO assembly_jobs(bundle_checksum, project_id, state, kind, name, debug_id)
        VALUES ($1, $2, 'created', $3, $4, $5)
        ON CONFLICT(bundle_checksum, project_id) DO UPDATE
          SET state = 'created', detail = NULL, retry_count = 0,
              kind = EXCLUDED.kind, name = EXCLUDED.name, debug_id = EXCLUDED.debug_id
          WHERE assembly_jobs.state = 'error'
        RETURNING *
        "#,
    )
    .bind(new.checksum)
    .bind(project_id)
    .bind(new.kind)
    .bind(new.name)
    .bind(new.debug_id)
    .fetch_optional(&mut *tx)
    .await?;

    let queued = job.is_some();
    let job = match job {
        Some(job) => {
            sqlx::query("DELETE FROM assembly_job_chunks WHERE job_id = $1")
                .bind(job.id)
                .execute(&mut *tx)
                .await?;
            for (position, checksum) in new.chunks.iter().enumerate() {
                sqlx::query(
                    "INSERT INTO assembly_job_chunks(job_id, position, checksum) VALUES ($1, $2, $3)",
                )
                .bind(job.id)
                .bind(position as i32)
                .bind(checksum)
                .execute(&mut *tx)
                .await?;
            }
            job
        }
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
    tx.commit().await?;
    if queued {
        queue.wake();
    }
    Ok(job_response(job.state.as_str(), job.detail))
}

/// Answers for a DIF already assembled, from the debug files that still hold
/// it, as Sentry's `batch_assemble` does. The same ProGuard mapping requested
/// under another UUID is stored for that UUID too
/// (`_is_proguard_reupload_clone_request`).
///
/// `None` when no debug ID holds the file any more: the job is forgotten and
/// the file is assembled again like a new one.
async fn dif_reupload(
    pool: &DbPool,
    project_id: i32,
    job: &AssemblyJob,
    new: &NewJob<'_>,
) -> AppResult<Option<AssembleResponse>> {
    use crate::services::debug_file::{
        alias_proguard_mapping, debug_file_holder, proguard_uuid_from_name, COVERED_BY_DSYM,
    };
    let requested = new.name.and_then(proguard_uuid_from_name);
    if requested.is_some()
        && job
            .name
            .as_deref()
            .and_then(proguard_uuid_from_name)
            .is_none()
    {
        return Ok(Some(job_response(
            "error",
            Some("This file is not a ProGuard mapping.".into()),
        )));
    }
    let file_type = requested.map(|_| crate::services::jvm::PROGUARD_FILE_TYPE);
    match (
        debug_file_holder(pool, project_id, &job.bundle_checksum, file_type).await?,
        requested,
    ) {
        (Some(from), Some(to)) if from != to => {
            alias_proguard_mapping(pool, project_id, from, to).await?
        }
        (Some(_), _) => {}
        // An executable whose dSYMs were already stored is held by them.
        (None, _) if job.detail.as_deref() == Some(COVERED_BY_DSYM) => {}
        // Every debug ID it was stored under now holds another file.
        (None, _) => {
            sqlx::query("DELETE FROM assembly_jobs WHERE id = $1")
                .bind(job.id)
                .execute(pool)
                .await?;
            return Ok(None);
        }
    }
    Ok(Some(job_response(job.state.as_str(), job.detail.clone())))
}

pub fn not_found(missing_chunks: Vec<String>) -> AssembleResponse {
    AssembleResponse {
        state: "not_found".to_string(),
        missing_chunks,
        detail: None,
    }
}

/// Maps an assembly job state to the appropriate HTTP response.
/// All terminal states return HTTP 200; the state field in the body signals outcome.
pub(crate) fn job_response(state: &str, detail: Option<String>) -> AssembleResponse {
    AssembleResponse {
        state: state.to_string(),
        missing_chunks: vec![],
        // Only a failed job says why.
        detail: detail.filter(|_| state == "error"),
    }
}

/// The source maps a project holds, newest first, as sentry-cli lists them.
pub async fn list_files(pool: &DbPool, project_id: i32) -> AppResult<Vec<SourceMapFile>> {
    #[derive(sqlx::FromRow)]
    struct Row {
        debug_id: Uuid,
        file_type: String,
        size: i32,
        times_used: i32,
        date_uploaded: String,
    }
    let rows: Vec<Row> = sqlx::query_as(
        r#"
        SELECT
            sfm.debug_id,
            sfm.file_type,
            sf.size,
            sfm.times_used,
            CAST(sfm.created_at AS TEXT) AS date_uploaded
        FROM source_file_metadata sfm
        JOIN source_file sf ON sf.id = sfm.file_id
        WHERE sfm.project_id = $1
        ORDER BY sfm.created_at DESC
        "#,
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| SourceMapFile {
            debug_id: r.debug_id.to_string(),
            file_type: r.file_type,
            size: r.size,
            times_used: r.times_used,
            date_uploaded: r.date_uploaded,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider {
        data: Bytes,
    }

    #[async_trait::async_trait]
    impl SourceMapProvider for MockProvider {
        async fn fetch_sourcemap(
            &self,
            _project_id: i32,
            _debug_id: &str,
            _file_type: &str,
        ) -> AppResult<Option<SourceMapEntry>> {
            Ok(Some(SourceMapEntry {
                data: self.data.clone(),
            }))
        }
    }

    fn mock_provider(sourcemap_json: &str) -> MockProvider {
        MockProvider {
            data: Bytes::from(sourcemap_json.to_string()),
        }
    }

    // Source map where sources[0] is "" (empty) — token has a valid position and
    // name but no source file name. VLQ "AAIEA" = gen_col=0, src_idx=0,
    // src_line=4, src_col=2, name_idx=0 on generated line 0.
    const SOURCEMAP_EMPTY_SOURCE: &str = r#"{
        "version": 3,
        "sources": [""],
        "sourcesContent": [null],
        "names": ["originalFunction"],
        "mappings": "AAIEA"
    }"#;

    #[tokio::test]
    async fn rewrite_preserves_filename_when_token_source_is_empty() {
        let provider = mock_provider(SOURCEMAP_EMPTY_SOURCE);
        let mut event = serde_json::json!({
            "debug_meta": {
                "images": [{"type": "sourcemap", "code_file": "app.min.js", "debug_id": "test-debug-id"}]
            },
            "exception": { "values": [{ "type": "Error", "value": "boom", "stacktrace": { "frames": [{
                "filename": "app.min.js",
                "abs_path": "https://example.com/app.min.js",
                "lineno": 1,
                "colno": 0,
                "function": "minifiedFn"
            }]}}]}
        });

        rewrite_frames(&provider, 1, &mut event).await.unwrap();

        let frame = &event["exception"]["values"][0]["stacktrace"]["frames"][0];
        // filename and abs_path must be preserved — empty source name must not corrupt them
        assert_eq!(
            frame["filename"], "app.min.js",
            "filename should not be overwritten with empty string"
        );
        assert_eq!(
            frame["abs_path"], "https://example.com/app.min.js",
            "abs_path should not be cleared when source name is empty"
        );
    }

    #[tokio::test]
    async fn rewrite_frames_also_rewrites_thread_stacktrace_frames() {
        let provider = mock_provider(SOURCEMAP_EMPTY_SOURCE);
        let mut event = serde_json::json!({
            "debug_meta": {
                "images": [{"type": "sourcemap", "code_file": "app.min.js", "debug_id": "test-debug-id"}]
            },
            "threads": { "values": [{
                "id": "0",
                "crashed": true,
                "stacktrace": { "frames": [{
                    "filename": "app.min.js",
                    "lineno": 1,
                    "colno": 0,
                    "function": "minifiedFn"
                }]}
            }]}
        });

        rewrite_frames(&provider, 1, &mut event).await.unwrap();

        let frame = &event["threads"]["values"][0]["stacktrace"]["frames"][0];
        assert_eq!(
            frame["lineno"], 5,
            "thread frame lineno should be remapped just like exception frames"
        );
        assert_eq!(
            frame["function"], "originalFunction",
            "thread frame function should be resolved from the source map, same as exception frames"
        );
    }

    #[tokio::test]
    async fn rewrite_still_updates_lineno_colno_function_when_token_source_is_empty() {
        let provider = mock_provider(SOURCEMAP_EMPTY_SOURCE);
        let mut event = serde_json::json!({
            "debug_meta": {
                "images": [{"type": "sourcemap", "code_file": "app.min.js", "debug_id": "test-debug-id"}]
            },
            "exception": { "values": [{ "type": "Error", "value": "boom", "stacktrace": { "frames": [{
                "filename": "app.min.js",
                "lineno": 1,
                "colno": 0,
                "function": "minifiedFn"
            }]}}]}
        });

        rewrite_frames(&provider, 1, &mut event).await.unwrap();

        let frame = &event["exception"]["values"][0]["stacktrace"]["frames"][0];
        // lineno/colno/function must still be remapped even without a source file name
        assert_eq!(
            frame["lineno"], 5,
            "lineno should be remapped to original (src_line 4 + 1)"
        );
        assert_eq!(frame["colno"], 2, "colno should be remapped to original");
        assert_eq!(
            frame["function"], "originalFunction",
            "function name should be resolved from names table"
        );
    }

    /// Metro/Hermes maps include `x_facebook_sources`. SourceMap::from_reader
    /// rejects them; DecodedMap::from_reader does not.
    const HERMES_SOURCEMAP: &str = r#"{
        "version": 3,
        "sources": ["src/App.tsx"],
        "sourcesContent": ["line 1\nline 2\nline 3\nline 4\nfunction foo() {\n  throw new Error('boom');\n}"],
        "names": [],
        "mappings": "AAIA",
        "x_facebook_sources": [[{"names": ["foo"], "mappings": "AAA"}]]
    }"#;

    #[test]
    fn hermes_map_parses_as_decoded_map_not_regular() {
        let data = HERMES_SOURCEMAP.as_bytes();
        let regular = sourcemap::SourceMap::from_reader(Cursor::new(data));
        assert!(
            matches!(regular, Err(sourcemap::Error::IncompatibleSourceMap)),
            "regular SourceMap::from_reader must reject Hermes maps; got {:?}",
            regular.err()
        );
        let decoded = sourcemap::DecodedMap::from_reader(Cursor::new(data)).unwrap();
        assert!(
            matches!(decoded, sourcemap::DecodedMap::Hermes(_)),
            "DecodedMap must detect Hermes from x_facebook_sources"
        );
    }

    #[test]
    fn invalid_manifest_files_are_treated_as_empty() {
        assert!(manifest_files(&serde_json::json!({})).is_empty());
        assert!(manifest_files(&serde_json::json!({"files": []})).is_empty());
    }
}
