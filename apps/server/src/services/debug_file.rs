//! Debug information files: what a `difs/assemble` job turns into.
//!
//! Detection follows Sentry's `detect_dif_from_path`
//! (`src/sentry/models/debugfile.py`, 2458718b2c): a ProGuard mapping is
//! known by its name, which carries its UUID; anything else must parse as a
//! known format. Only the JVM formats are handled for now.

use std::sync::LazyLock;

use uuid::Uuid;

use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::services::jvm::{
    build_proguard_cache, bundle_manifest, PROGUARD_CACHE_FILE_TYPE, PROGUARD_FILE_TYPE,
    SOURCE_BUNDLE_FILE_TYPE,
};
use crate::services::sourcemap::{finish_job, join_chunks, store_file};
use crate::services::sourcemap_store::SourceMapStore;

/// Sentry's `_proguard_file_re`.
static PROGUARD_NAME: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
    fancy_regex::Regex::new(r"/proguard/(?:mapping-)?(.*?)\.txt$").expect("valid regex")
});

/// The UUID a ProGuard mapping's name carries (`.../proguard/<uuid>.txt` or
/// `.../proguard/mapping-<uuid>.txt`).
pub fn proguard_uuid_from_name(name: &str) -> Option<Uuid> {
    let caps = PROGUARD_NAME.captures(name).ok().flatten()?;
    Uuid::parse_str(caps.get(1)?.as_str()).ok()
}

/// Assembles a worker-owned DIF job, storing what it turns out to be and
/// committing the job's terminal state with it.
pub async fn assemble_dif_for_job(
    pool: &DbPool,
    store: &dyn SourceMapStore,
    job: &crate::models::source_file::AssemblyJob,
    max_size_bytes: usize,
) -> AppResult<()> {
    let data = join_chunks(pool, job.chunk_list(), max_size_bytes, &job.bundle_checksum).await?;
    let name = job.name.as_deref().unwrap_or_default();

    let files: Vec<(Uuid, &str, Vec<u8>)> = if let Some(uuid) = proguard_uuid_from_name(name) {
        // ponytail: the cache build peaks at ~40x the mapping's size in RAM;
        // the single assembly worker builds one at a time.
        let (data, cache) = tokio::task::spawn_blocking(move || {
            let cache = build_proguard_cache(&data);
            (data, cache)
        })
        .await
        .map_err(|e| AppError::Internal(format!("proguard cache build panicked: {e}")))?;
        let cache = cache.map_err(AppError::Validation)?;
        vec![
            (uuid, PROGUARD_FILE_TYPE, data),
            (uuid, PROGUARD_CACHE_FILE_TYPE, cache),
        ]
    } else if data.starts_with(b"SYSB") {
        let uuid = source_bundle_debug_id(&data)?
            .or_else(|| {
                job.debug_id
                    .as_deref()
                    .and_then(|id| Uuid::parse_str(id).ok())
            })
            .ok_or_else(|| AppError::Validation("source bundle has no debug ID".into()))?;
        vec![(uuid, SOURCE_BUNDLE_FILE_TYPE, data)]
    } else {
        return Err(AppError::Validation(format!(
            "unsupported debug information file: {name}"
        )));
    };

    let mut tx = pool.begin().await?;
    for (uuid, file_type, bytes) in files {
        store_file(&mut tx, store, job.project_id, uuid, file_type, bytes).await?;
    }
    finish_job(&mut tx, Some(job.id), job.chunk_list()).await?;
    tx.commit().await?;
    Ok(())
}

/// The `debug_id` a source bundle's `manifest.json` declares, if any. A
/// bundle whose manifest cannot be read is not a source bundle.
fn source_bundle_debug_id(data: &[u8]) -> AppResult<Option<Uuid>> {
    // The zip's offsets count the 8-byte header, so it opens over all of it.
    let manifest = zip::ZipArchive::new(std::io::Cursor::new(data))
        .ok()
        .and_then(|mut archive| bundle_manifest(&mut archive))
        .ok_or_else(|| AppError::Validation("invalid source bundle manifest".into()))?;
    Ok(manifest["debug_id"]
        .as_str()
        .and_then(|id| Uuid::parse_str(id).ok()))
}

/// Records the mapping (and its cache) stored for `from` under `to` as well,
/// for the same mapping uploaded again under another UUID. Sentry's
/// `_clone_proguard_debug_file_for_reupload`: the stored files are shared.
pub async fn alias_proguard_mapping(
    pool: &DbPool,
    project_id: i32,
    from: Uuid,
    to: Uuid,
) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    for file_type in [PROGUARD_FILE_TYPE, PROGUARD_CACHE_FILE_TYPE] {
        #[cfg(feature = "postgres")]
        let (id, to, from) = (Uuid::new_v4(), to, from);
        #[cfg(not(feature = "postgres"))]
        let (id, to, from) = (Uuid::new_v4().to_string(), to.to_string(), from.to_string());
        sqlx::query(
            r#"
            INSERT INTO source_file_metadata(id, project_id, debug_id, file_type, file_id)
            SELECT $1, project_id, $2, file_type, file_id FROM source_file_metadata
            WHERE project_id = $3 AND debug_id = $4 AND file_type = $5
            ON CONFLICT(project_id, debug_id, file_type) DO UPDATE SET file_id = EXCLUDED.file_id
            "#,
        )
        .bind(id)
        .bind(to)
        .bind(project_id)
        .bind(from)
        .bind(file_type)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proguard_names_carry_the_uuid() {
        let uuid = "11111111-2222-3333-4444-555555555555";
        for name in [
            format!("/proguard/{uuid}.txt"),
            format!("/proguard/mapping-{uuid}.txt"),
            format!("x/proguard/{uuid}.txt"),
        ] {
            assert_eq!(
                proguard_uuid_from_name(&name).map(|u| u.to_string()),
                Some(uuid.to_string()),
                "{name}"
            );
        }
        assert_eq!(proguard_uuid_from_name("proguard/not-a-uuid.txt"), None);
        assert_eq!(proguard_uuid_from_name("mapping.txt"), None);
    }
}
