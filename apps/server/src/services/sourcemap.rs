use std::collections::HashMap;
use std::io::Cursor;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use bytes::Bytes;
use sha1::Digest as _;
use uuid::Uuid;

use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::services::sourcemap_store::SourceMapStore;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default maximum size of a single chunk in bytes (10 MB).
pub const DEFAULT_MAX_CHUNK_SIZE_BYTES: usize = 10 * 1024 * 1024;

// ---------------------------------------------------------------------------
// SourceMapEntry + SourceMapProvider trait
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// ParsedMapCache: parsed source maps shared across events
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// DbSourceMapProvider (concrete implementation)
// ---------------------------------------------------------------------------

pub struct DbSourceMapProvider {
    pool: DbPool,
    store: Arc<dyn SourceMapStore>,
    cache: ParsedMapCache,
}

/// Default budget for parsed source maps kept across events.
pub const DEFAULT_SOURCEMAP_CACHE_BYTES: usize = 64 * 1024 * 1024;

impl DbSourceMapProvider {
    pub fn new(pool: DbPool, store: Arc<dyn SourceMapStore>) -> Self {
        Self {
            pool,
            store,
            cache: ParsedMapCache::new(DEFAULT_SOURCEMAP_CACHE_BYTES),
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
    ) -> AppResult<Option<(String, String)>> {
        // 1. Parse debug_id as UUID for typed query
        let debug_uuid = match Uuid::parse_str(debug_id) {
            Ok(u) => u,
            Err(_) => return Ok(None),
        };

        // 2. Query source_file_metadata joined with source_file
        // Returns (sfm_id_str, storage_path) — backend-agnostic String types
        #[cfg(feature = "postgres")]
        let row: Option<(String, String)> = sqlx::query_as(
            r#"
            SELECT sfm.id::text, sf.storage_path
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

        #[cfg(not(feature = "postgres"))]
        let row: Option<(String, String)> = sqlx::query_as(
            r#"
            SELECT sfm.id, sf.storage_path
            FROM source_file_metadata sfm
            JOIN source_file sf ON sf.id = sfm.file_id
            WHERE sfm.project_id = $1 AND sfm.debug_id = $2 AND sfm.file_type = $3
            "#,
        )
        .bind(project_id)
        .bind(debug_uuid.to_string())
        .bind(file_type)
        .fetch_optional(&self.pool)
        .await?;

        let (sfm_id, storage_path) = match row {
            Some(r) => r,
            None => return Ok(None),
        };

        // 3. Increment times_used (best-effort, fire-and-forget)
        // Parse the string sfm_id back to UUID for the UPDATE (works for both backends).
        if let Ok(sfm_uuid) = Uuid::parse_str(&sfm_id) {
            #[cfg(feature = "postgres")]
            let _ = sqlx::query(
                "UPDATE source_file_metadata SET times_used = times_used + 1 WHERE id = $1",
            )
            .bind(sfm_uuid)
            .execute(&self.pool)
            .await;

            #[cfg(not(feature = "postgres"))]
            let _ = sqlx::query(
                "UPDATE source_file_metadata SET times_used = times_used + 1 WHERE id = $1",
            )
            .bind(sfm_uuid.to_string())
            .execute(&self.pool)
            .await;
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

// ---------------------------------------------------------------------------
// normalize_sentry_position
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// get_missing_chunks
// ---------------------------------------------------------------------------

/// Returns checksums from `checksums` that are NOT present in the `chunk` table.
pub async fn get_missing_chunks(pool: &DbPool, checksums: &[String]) -> AppResult<Vec<String>> {
    if checksums.is_empty() {
        return Ok(vec![]);
    }

    #[cfg(feature = "postgres")]
    {
        let present: Vec<String> =
            sqlx::query_scalar("SELECT checksum FROM chunk WHERE checksum = ANY($1)")
                .bind(checksums)
                .fetch_all(pool)
                .await?;
        let present_set: std::collections::HashSet<&str> =
            present.iter().map(|s| s.as_str()).collect();
        Ok(checksums
            .iter()
            .filter(|c| !present_set.contains(c.as_str()))
            .cloned()
            .collect())
    }

    #[cfg(not(feature = "postgres"))]
    {
        use sqlx::QueryBuilder;
        let mut qb = QueryBuilder::new("SELECT checksum FROM chunk WHERE checksum IN (");
        let mut sep = qb.separated(", ");
        for c in checksums {
            sep.push_bind(c);
        }
        qb.push(")");
        let present: Vec<String> = qb.build_query_scalar().fetch_all(pool).await?;
        let present_set: std::collections::HashSet<&str> =
            present.iter().map(|s| s.as_str()).collect();
        Ok(checksums
            .iter()
            .filter(|c| !present_set.contains(c.as_str()))
            .cloned()
            .collect())
    }
}

// ---------------------------------------------------------------------------
// store_chunks
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// assemble_bundle
// ---------------------------------------------------------------------------

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
    // --- Steps 1+2: fetch + join chunk bytes, verify SHA1 ---
    let joined = join_chunks(
        pool,
        chunk_checksums,
        max_bundle_size_bytes,
        bundle_checksum,
    )
    .await?;

    // --- Step 3: write to temp file (non-blocking) ---
    let temp_dir = tempfile::tempdir().map_err(|e| AppError::Internal(e.to_string()))?;
    let zip_path = temp_dir.path().join("bundle.zip");
    tokio::fs::write(&zip_path, &joined)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    drop(joined); // free memory

    // --- Steps 4+5: open ZIP + validate + extract (blocking I/O via spawn_blocking) ---
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

            // Create parent dirs if needed
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

    // --- Step 6: parse manifest.json ---
    let manifest_path = extract_dir.join("manifest.json");
    let manifest_bytes = tokio::fs::read(&manifest_path).await.map_err(|_| {
        AppError::Validation("manifest.json not found in artifact bundle".to_string())
    })?;
    let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| AppError::Validation(format!("invalid manifest.json: {}", e)))?;

    let files = manifest_files(&manifest);

    // --- Steps 7+8: process each file entry, then delete chunks ---
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

        // Step 7: skip entries without a debug-id header
        let debug_id_str = match headers
            .get("debug-id")
            .or_else(|| headers.get("debug_id"))
            .and_then(|v| v.as_str())
        {
            Some(s) => s.to_string(),
            None => continue,
        };

        // Validate debug_id is a UUID
        let debug_uuid = match Uuid::parse_str(&debug_id_str) {
            Ok(u) => u,
            Err(_) => {
                log::warn!("invalid debug_id in manifest: {}", debug_id_str);
                continue;
            }
        };

        // Read file_type from manifest entry's `type` field verbatim
        let file_type = match file_info.get("type").and_then(|t| t.as_str()) {
            Some(t) => t.to_string(),
            None => continue,
        };

        // Read file bytes from extracted dir
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

    let new_sf_id = Uuid::new_v4();

    // Two-query upsert for source_file (avoids RETURNING NULL on conflict)
    // Query A: insert (idempotent)
    #[cfg(feature = "postgres")]
    let new_sf_id_bind = new_sf_id;
    #[cfg(not(feature = "postgres"))]
    let new_sf_id_bind = new_sf_id.to_string();
    sqlx::query(
        r#"
        INSERT INTO source_file(id, checksum, size, storage_path)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT(checksum) DO NOTHING
        "#,
    )
    .bind(new_sf_id_bind)
    .bind(&storage_key)
    .bind(file_size)
    .bind(&storage_key)
    .execute(&mut **tx)
    .await?;

    // Query B: always fetch (works whether just inserted or pre-existing)
    #[cfg(feature = "postgres")]
    let sf_id: Uuid = sqlx::query_scalar("SELECT id FROM source_file WHERE checksum = $1")
        .bind(&storage_key)
        .fetch_one(&mut **tx)
        .await?;

    #[cfg(not(feature = "postgres"))]
    let sf_id: String = sqlx::query_scalar("SELECT id FROM source_file WHERE checksum = $1")
        .bind(&storage_key)
        .fetch_one(&mut **tx)
        .await?;

    #[cfg(not(feature = "postgres"))]
    let sf_id = Uuid::parse_str(&sf_id).map_err(|e| AppError::Internal(e.to_string()))?;

    let new_sfm_id = Uuid::new_v4();
    #[cfg(feature = "postgres")]
    let (sfm_id, debug_id, file_id) = (new_sfm_id, debug_uuid, sf_id);
    #[cfg(not(feature = "postgres"))]
    let (sfm_id, debug_id, file_id) = (
        new_sfm_id.to_string(),
        debug_uuid.to_string(),
        sf_id.to_string(),
    );
    sqlx::query(
        r#"
        INSERT INTO source_file_metadata(id, project_id, debug_id, file_type, file_id)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT(project_id, debug_id, file_type) DO UPDATE SET file_id = EXCLUDED.file_id
        "#,
    )
    .bind(sfm_id)
    .bind(project_id)
    .bind(debug_id)
    .bind(file_type)
    .bind(file_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Releases a job's chunk references, deletes the chunks no other pending or
/// retryable job still owns, and marks a worker-owned job `ok`.
pub(crate) async fn finish_job(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    job_id: Option<i64>,
    chunk_checksums: &[String],
) -> AppResult<()> {
    if let Some(job_id) = job_id {
        sqlx::query("DELETE FROM assembly_job_chunks WHERE job_id = $1")
            .bind(job_id)
            .execute(&mut **tx)
            .await?;
    }

    #[cfg(feature = "postgres")]
    sqlx::query(
        "DELETE FROM chunk c WHERE c.checksum = ANY($1) AND NOT EXISTS (SELECT 1 FROM assembly_job_chunks r WHERE r.checksum = c.checksum)",
    )
        .bind(chunk_checksums)
        .execute(&mut **tx)
        .await?;

    #[cfg(not(feature = "postgres"))]
    {
        use sqlx::QueryBuilder;
        if !chunk_checksums.is_empty() {
            let mut qb = QueryBuilder::new("DELETE FROM chunk WHERE checksum IN (");
            let mut sep = qb.separated(", ");
            for c in chunk_checksums {
                sep.push_bind(c);
            }
            qb.push(") AND NOT EXISTS (SELECT 1 FROM assembly_job_chunks r WHERE r.checksum = chunk.checksum)");
            qb.build().execute(&mut **tx).await?;
        }
    }

    if let Some(job_id) = job_id {
        #[cfg(feature = "postgres")]
        let updated = sqlx::query(
            "UPDATE assembly_jobs SET state = 'ok', detail = NULL, updated_at = NOW() WHERE id = $1 AND state = 'assembling'",
        )
        .bind(job_id)
        .execute(&mut **tx)
        .await?;

        #[cfg(not(feature = "postgres"))]
        let updated = sqlx::query(
            "UPDATE assembly_jobs SET state = 'ok', detail = NULL, updated_at = datetime('now') WHERE id = $1 AND state = 'assembling'",
        )
        .bind(job_id)
        .execute(&mut **tx)
        .await?;

        if updated.rows_affected() != 1 {
            return Err(AppError::Internal(format!(
                "assembly job {} was not assembling",
                job_id
            )));
        }
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

// ---------------------------------------------------------------------------
// rewrite_frames
// ---------------------------------------------------------------------------

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
    // 1. Build code_file → debug_id_str map from debug_meta.images
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
            // 3a. Extract frame fields as owned values (no live borrow across .await)
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

            // 3b. Resolve debug_id from code_file map.
            // Try abs_path first (full URL matching code_file), then filename.
            let debug_id = match abs_path
                .as_deref()
                .and_then(|k| images_map.get(k))
                .or_else(|| filename.as_deref().and_then(|k| images_map.get(k)))
            {
                Some(id) => id.clone(),
                None => continue,
            };

            // 3c+3d. Fetch and parse the source map, once per debug_id per event.
            let Some(decoded) = load_decoded_map(provider, project_id, &debug_id, parsed).await
            else {
                continue;
            };

            // 3e. Normalize position — use let-else, NOT '?' (normalize returns Option not Result)
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

            // 3f. Lookup token
            let Some(token) = map.lookup_token(norm_lineno, norm_colno) else {
                continue;
            };

            // 3g. Skip unmapped tokens
            if token.get_src_line() == u32::MAX {
                continue;
            }

            // 3h. Original file from token
            let original_file = token.get_source().unwrap_or("").to_string();
            let src_line = token.get_src_line();
            let src_col = token.get_src_col();
            let token_name = token.get_name().map(|n| n.to_string());

            // 3i. Find source index via linear search (NEVER assume sourcesContent[0])
            let source_idx = (0..sm.get_source_count())
                .find(|&i| sm.get_source(i) == Some(original_file.as_str()));

            // 3j. Get source lines as owned Strings (sm is dropped after this loop body)
            let lines: Vec<String> = source_idx
                .and_then(|i| sm.get_source_contents(i))
                .map(|s| s.lines().map(|l| l.to_string()).collect())
                .unwrap_or_default();

            // 3k. Compute context window bounds
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

            // 3l. Write rewritten fields back by index path (fresh borrow, no cross-await alias)
            let frame =
                &mut event_data[root_key]["values"][value_idx]["stacktrace"]["frames"][frame_idx];
            // Only overwrite filename/abs_path when the token has a non-empty source name.
            // A None/empty source (malformed map) must not corrupt the original filename.
            // Reference: getsentry/symbolicator symbolication.rs L246-273 (e282ec0)
            if !original_file.is_empty() {
                frame["filename"] = original_file.into();
                // Clear abs_path — it pointed to the minified JS URL, now irrelevant
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
