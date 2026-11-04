use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::{self, Write as _};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::fs;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::ingest::EventMetadata;

/// Default base directory for pending events
const DEFAULT_INGEST_DIR: &str = "/tmp/rustrak/ingest";
const PENDING_EVENT_VERSION: u8 = 1;
const ORPHANED_TEMPORARY_FILE_GRACE: Duration = Duration::from_secs(300);
const BASE64_WRITE_CHUNK_SIZE: usize = 48 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EventStorageLocation {
    Project,
    Legacy,
}

#[derive(Debug, Deserialize, Serialize)]
struct PendingEventRecord {
    version: u8,
    metadata: EventMetadata,
    event_data: String,
}

/// Gets the legacy file path for an event_id.
pub fn get_event_path(base_dir: &Path, event_id: &str) -> AppResult<PathBuf> {
    let uuid = Uuid::parse_str(event_id)
        .map_err(|_| AppError::Validation("Invalid event_id format".to_string()))?;
    Ok(base_dir.join(format!("{}.json", uuid.as_simple())))
}

fn get_event_metadata_path(base_dir: &Path, event_id: &str) -> AppResult<PathBuf> {
    Ok(get_event_path(base_dir, event_id)?.with_extension("meta.json"))
}

fn get_pending_event_path(base_dir: &Path, event_id: &str) -> AppResult<PathBuf> {
    Ok(get_event_path(base_dir, event_id)?.with_extension("pending.json"))
}

fn get_project_event_path(base_dir: &Path, project_id: i32, event_id: &str) -> AppResult<PathBuf> {
    let uuid = Uuid::parse_str(event_id)
        .map_err(|_| AppError::Validation("Invalid event_id format".to_string()))?;
    Ok(base_dir.join(format!("project-{project_id}-{}.json", uuid.as_simple())))
}

fn get_project_pending_event_path(
    base_dir: &Path,
    project_id: i32,
    event_id: &str,
) -> AppResult<PathBuf> {
    Ok(get_project_event_path(base_dir, project_id, event_id)?.with_extension("pending.json"))
}

fn same_event_id(left: &str, right: &str) -> bool {
    Uuid::parse_str(left).ok() == Uuid::parse_str(right).ok()
}

fn validate_pending_record(
    record: PendingEventRecord,
    event_id: &str,
    project_id: Option<i32>,
) -> AppResult<Vec<u8>> {
    if record.version != PENDING_EVENT_VERSION
        || !same_event_id(&record.metadata.event_id, event_id)
        || project_id.is_some_and(|id| record.metadata.project_id != id)
    {
        return Err(AppError::Internal(
            "Invalid pending event record identity".to_string(),
        ));
    }
    STANDARD
        .decode(record.event_data)
        .map_err(|e| AppError::Internal(format!("Invalid pending event payload: {}", e)))
}

async fn read_pending_record(
    path: &Path,
    event_id: &str,
    project_id: Option<i32>,
) -> AppResult<Vec<u8>> {
    let bytes = fs::read(path)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to read event file: {}", e)))?;
    decode_pending_record(&bytes, event_id, project_id)
}

fn decode_pending_record(
    bytes: &[u8],
    event_id: &str,
    project_id: Option<i32>,
) -> AppResult<Vec<u8>> {
    let record: PendingEventRecord = serde_json::from_slice(bytes)
        .map_err(|e| AppError::Internal(format!("Invalid pending event record: {}", e)))?;
    validate_pending_record(record, event_id, project_id)
}

/// Reads a file, or `None` when it does not exist. One round trip to the
/// blocking pool instead of the `try_exists` + `read` pair, which also left a
/// window for the file to vanish between the two.
async fn read_if_present(path: &Path) -> AppResult<Option<Vec<u8>>> {
    match fs::read(path).await {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(AppError::Internal(format!(
            "Failed to read event file: {}",
            e
        ))),
    }
}

fn base64_chunk_ranges(data_len: usize) -> impl Iterator<Item = Range<usize>> {
    let full_len = data_len - data_len % 3;
    let mut offset = 0;
    std::iter::from_fn(move || {
        if offset < full_len {
            let start = offset;
            offset = (offset + BASE64_WRITE_CHUNK_SIZE).min(full_len);
            Some(start..offset)
        } else if offset == full_len && full_len < data_len {
            offset = data_len;
            Some(full_len..data_len)
        } else {
            None
        }
    })
}

fn encode_base64_chunk(input: &[u8], output: &mut String) {
    output.clear();
    STANDARD.encode_string(input, output);
}

/// Runs a filesystem step on the blocking pool.
///
/// One hop per store or delete. The store used to go through `tokio::fs` call
/// by call, and each call is its own hop to the blocking pool and back: open,
/// write, flush, fsync, link, unlink, open the directory, fsync it, close.
/// Nine hand-offs per accepted event, all of them futex wakes and context
/// switches that showed up as the top of the profile. Batched, the request
/// thread parks once for the whole thing.
async fn on_blocking_pool<T, F>(what: &'static str, work: F) -> AppResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> AppResult<T> + Send + 'static,
{
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| AppError::Internal(format!("{what} task failed: {e}")))?
}

fn write_pending_record(
    file: &mut std::fs::File,
    metadata: &EventMetadata,
    event_data: &[u8],
) -> io::Result<()> {
    let metadata_bytes = serde_json::to_vec(metadata).map_err(io::Error::other)?;
    let mut writer = io::BufWriter::new(&mut *file);
    let version = PENDING_EVENT_VERSION;
    let payload = event_data;
    writer.write_all(format!(r#"{{"version":{version},"metadata":"#).as_bytes())?;
    writer.write_all(&metadata_bytes)?;
    writer.write_all(br#","event_data":""#)?;
    let initial_chunk_len = payload.len().min(BASE64_WRITE_CHUNK_SIZE);
    let mut encoded = String::with_capacity(initial_chunk_len.div_ceil(3) * 4);
    for range in base64_chunk_ranges(payload.len()) {
        encode_base64_chunk(&payload[range], &mut encoded);
        writer.write_all(encoded.as_bytes())?;
    }
    writer.write_all(br#""}"#)?;
    writer.flush()?;
    drop(writer);
    file.sync_all()
}

fn publish_pending_file(
    path: &Path,
    temporary_path: &Path,
    event_id: &str,
    project_id: i32,
) -> AppResult<()> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::Internal("Invalid ingest file path".to_string()))?;

    match std::fs::hard_link(temporary_path, path) {
        Ok(()) => {
            let _ = std::fs::remove_file(temporary_path);
        }
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            let is_symlink = match std::fs::symlink_metadata(path) {
                Ok(metadata) => metadata.file_type().is_symlink(),
                Err(metadata_error) => {
                    let _ = std::fs::remove_file(temporary_path);
                    return Err(AppError::Internal(format!(
                        "Failed to inspect existing ingest file: {}",
                        metadata_error
                    )));
                }
            };
            let is_complete = if is_symlink {
                false
            } else {
                let existing = match std::fs::read(path) {
                    Ok(existing) => existing,
                    Err(read_error) => {
                        let _ = std::fs::remove_file(temporary_path);
                        return Err(AppError::Internal(format!(
                            "Failed to inspect existing ingest file: {}",
                            read_error
                        )));
                    }
                };
                match serde_json::from_slice::<PendingEventRecord>(&existing) {
                    Ok(existing) => {
                        validate_pending_record(existing, event_id, Some(project_id)).is_ok()
                    }
                    Err(_) => false,
                }
            };
            if is_complete {
                // A duplicate event ID is idempotent: keep the first complete
                // record instead of allowing a later payload to win a race.
                let _ = std::fs::remove_file(temporary_path);
                return Ok(());
            }

            let quarantine =
                path.with_file_name(format!(".{file_name}.corrupt-{}", Uuid::new_v4()));
            log::warn!(
                "Quarantining malformed pending ingest file {:?} as {:?}",
                path,
                quarantine
            );
            // Preserve the malformed inode without removing the canonical
            // name first.  The subsequent rename replaces that name
            // atomically, so a crash cannot leave recovery with neither the
            // old record nor the new durable record.
            if let Err(link_error) = std::fs::hard_link(path, &quarantine) {
                let _ = std::fs::remove_file(temporary_path);
                return Err(AppError::Internal(format!(
                    "Failed to quarantine ingest file: {}",
                    link_error
                )));
            }
            if let Err(rename_error) = std::fs::rename(temporary_path, path) {
                let _ = std::fs::remove_file(temporary_path);
                return Err(AppError::Internal(format!(
                    "Failed to publish replacement ingest file: {}",
                    rename_error
                )));
            }
        }
        Err(e) => {
            let _ = std::fs::remove_file(temporary_path);
            return Err(AppError::Internal(format!(
                "Failed to publish ingest file: {}",
                e
            )));
        }
    }

    sync_parent_directory(path)?;

    Ok(())
}

fn write_pending_record_atomically(
    path: &Path,
    event_id: &str,
    project_id: i32,
    metadata: &EventMetadata,
    event_data: &[u8],
) -> AppResult<()> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::Internal("Invalid ingest file path".to_string()))?;
    let temporary_path = path.with_file_name(format!(".{file_name}.tmp-{}", Uuid::new_v4()));
    let mut temporary_file = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary_path)
    {
        Ok(file) => file,
        Err(e) => {
            let _ = std::fs::remove_file(&temporary_path);
            return Err(AppError::Internal(format!(
                "Failed to write temporary ingest file: {}",
                e
            )));
        }
    };
    if let Err(e) = write_pending_record(&mut temporary_file, metadata, event_data) {
        let _ = std::fs::remove_file(&temporary_path);
        return Err(AppError::Internal(format!(
            "Failed to write temporary ingest file: {}",
            e
        )));
    }
    drop(temporary_file);
    publish_pending_file(path, &temporary_path, event_id, project_id)
}

/// Creates the ingest directory once during server startup.
pub async fn prepare_ingest_dir(base_dir: &Path) -> AppResult<()> {
    fs::create_dir_all(base_dir)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create ingest directory: {}", e)))
}

fn sync_parent_directory(path: &Path) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        let directory = std::fs::File::open(parent)
            .map_err(|e| AppError::Internal(format!("Failed to open ingest directory: {}", e)))?;
        directory
            .sync_all()
            .map_err(|e| AppError::Internal(format!("Failed to sync ingest directory: {}", e)))?;
    }
    Ok(())
}

fn write_legacy_atomically(path: &Path, bytes: &[u8]) -> AppResult<()> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::Internal("Invalid ingest file path".to_string()))?;
    let temporary_path = path.with_file_name(format!(".{file_name}.tmp-{}", Uuid::new_v4()));
    let mut temporary_file = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary_path)
    {
        Ok(file) => file,
        Err(e) => {
            let _ = std::fs::remove_file(&temporary_path);
            return Err(AppError::Internal(format!(
                "Failed to write temporary ingest file: {}",
                e
            )));
        }
    };
    if let Err(e) = temporary_file.write_all(bytes) {
        let _ = std::fs::remove_file(&temporary_path);
        return Err(AppError::Internal(format!(
            "Failed to write temporary ingest file: {}",
            e
        )));
    }
    if let Err(e) = temporary_file.sync_all() {
        let _ = std::fs::remove_file(&temporary_path);
        return Err(AppError::Internal(format!(
            "Failed to sync temporary ingest file: {}",
            e
        )));
    }
    if let Err(e) = std::fs::rename(&temporary_path, path) {
        let _ = std::fs::remove_file(&temporary_path);
        return Err(AppError::Internal(format!(
            "Failed to publish ingest file: {}",
            e
        )));
    }
    sync_parent_directory(path)
}

/// Saves an event using the legacy, unscoped path.
pub async fn store_event(base_dir: &Path, event_id: &str, event_data: &[u8]) -> AppResult<PathBuf> {
    prepare_ingest_dir(base_dir).await?;
    let path = get_event_path(base_dir, event_id)?;
    let bytes = event_data.to_vec();
    let write_path = path.clone();
    on_blocking_pool("legacy event store", move || {
        write_legacy_atomically(&write_path, &bytes)
    })
    .await?;
    Ok(path)
}

/// Saves an event and the metadata needed to recover it after a worker restart.
pub async fn store_event_with_metadata(
    base_dir: &Path,
    event_id: &str,
    event_data: &[u8],
    metadata: &EventMetadata,
) -> AppResult<PathBuf> {
    if !same_event_id(&metadata.event_id, event_id) {
        return Err(AppError::Validation(
            "Event metadata does not match event_id".to_string(),
        ));
    }
    let path = get_project_event_path(base_dir, metadata.project_id, event_id)?;
    let pending_path = path.with_extension("pending.json");
    let base_dir = base_dir.to_path_buf();
    let event_id = event_id.to_string();
    let metadata = metadata.clone();
    let event_data = event_data.to_vec();
    on_blocking_pool("event store", move || {
        // The directory was created at startup (`prepare_ingest_dir`); the
        // retry below covers it disappearing since. Creating it again on
        // every event was one more round trip per request for an `EEXIST`.
        match write_pending_record_atomically(
            &pending_path,
            &event_id,
            metadata.project_id,
            &metadata,
            &event_data,
        ) {
            Ok(()) => Ok(()),
            Err(_error)
                if matches!(
                    std::fs::metadata(&base_dir),
                    Err(e) if e.kind() == io::ErrorKind::NotFound
                ) =>
            {
                // Cleanup can remove the directory after preparation but before
                // the temporary file is opened; recreate it once and retry.
                std::fs::create_dir_all(&base_dir).map_err(|e| {
                    AppError::Internal(format!("Failed to create ingest directory: {}", e))
                })?;
                write_pending_record_atomically(
                    &pending_path,
                    &event_id,
                    metadata.project_id,
                    &metadata,
                    &event_data,
                )
            }
            Err(error) => Err(error),
        }
    })
    .await?;
    Ok(path)
}

/// Reads an event from the legacy filesystem path.
pub async fn read_event(base_dir: &Path, event_id: &str) -> AppResult<Vec<u8>> {
    let path = get_event_path(base_dir, event_id)?;
    let pending_path = get_pending_event_path(base_dir, event_id)?;
    match fs::try_exists(&pending_path).await {
        Ok(true) => read_pending_record(&pending_path, event_id, None).await,
        Ok(false) => fs::read(&path)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to read event file: {}", e))),
        Err(e) => Err(AppError::Internal(format!(
            "Failed to read event file: {}",
            e
        ))),
    }
}

pub(crate) async fn read_event_with_location(
    base_dir: &Path,
    project_id: i32,
    event_id: &str,
) -> AppResult<(Vec<u8>, EventStorageLocation)> {
    let project_pending_path = get_project_pending_event_path(base_dir, project_id, event_id)?;
    if let Some(bytes) = read_if_present(&project_pending_path).await? {
        return Ok((
            decode_pending_record(&bytes, event_id, Some(project_id))?,
            EventStorageLocation::Project,
        ));
    }

    let project_path = get_project_event_path(base_dir, project_id, event_id)?;
    if let Some(bytes) = read_if_present(&project_path).await? {
        return Ok((bytes, EventStorageLocation::Project));
    }

    let legacy_pending_path = get_pending_event_path(base_dir, event_id)?;
    if let Some(bytes) = read_if_present(&legacy_pending_path).await? {
        return Ok((
            decode_pending_record(&bytes, event_id, Some(project_id))?,
            EventStorageLocation::Legacy,
        ));
    }

    let legacy_path = get_event_path(base_dir, event_id)?;
    match fs::read(&legacy_path).await {
        Ok(bytes) => Ok((bytes, EventStorageLocation::Legacy)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Err(AppError::Internal(format!(
            "{MISSING_EVENT_FILE}: {}",
            event_id
        ))),
        Err(e) => Err(AppError::Internal(format!(
            "Failed to read event file: {}",
            e
        ))),
    }
}

/// Prefix of the error an event read reports when no copy of the event exists
/// at any location. The recovery worker reads it as "already handled": the
/// file it listed a moment ago was finished and deleted by the task that
/// owned it, which is not a failure to replay.
pub(crate) const MISSING_EVENT_FILE: &str = "Event file not found";

/// Whether an error is the event-file-missing case described at
/// [`MISSING_EVENT_FILE`].
pub(crate) fn is_missing_event_file(error: &AppError) -> bool {
    matches!(error.kind(), AppError::Internal(message) if message.starts_with(MISSING_EVENT_FILE))
}

pub async fn read_event_for_project(
    base_dir: &Path,
    project_id: i32,
    event_id: &str,
) -> AppResult<Vec<u8>> {
    Ok(read_event_with_location(base_dir, project_id, event_id)
        .await?
        .0)
}

/// Deletes an event from the legacy filesystem path.
pub async fn delete_event(base_dir: &Path, event_id: &str) -> AppResult<()> {
    delete_paths(vec![
        get_event_path(base_dir, event_id)?,
        get_event_metadata_path(base_dir, event_id)?,
        get_pending_event_path(base_dir, event_id)?,
    ])
    .await
}

pub(crate) async fn delete_event_at(
    base_dir: &Path,
    project_id: i32,
    event_id: &str,
    location: EventStorageLocation,
) -> AppResult<()> {
    delete_paths(event_paths_at(base_dir, project_id, event_id, location)?).await
}

/// Every path an event's durable copy may occupy at `location`.
pub(crate) fn event_paths_at(
    base_dir: &Path,
    project_id: i32,
    event_id: &str,
    location: EventStorageLocation,
) -> AppResult<Vec<PathBuf>> {
    Ok(match location {
        EventStorageLocation::Project => vec![
            get_project_event_path(base_dir, project_id, event_id)?,
            get_project_pending_event_path(base_dir, project_id, event_id)?,
        ],
        EventStorageLocation::Legacy => vec![
            get_event_path(base_dir, event_id)?,
            get_event_metadata_path(base_dir, event_id)?,
            get_pending_event_path(base_dir, event_id)?,
        ],
    })
}

pub async fn delete_event_for_project(
    base_dir: &Path,
    project_id: i32,
    event_id: &str,
) -> AppResult<()> {
    delete_event_at(
        base_dir,
        project_id,
        event_id,
        EventStorageLocation::Project,
    )
    .await
}

/// Removes the given files, ignoring ones that are already gone, in one
/// blocking-pool round trip however many there are.
pub(crate) async fn delete_paths(paths: Vec<PathBuf>) -> AppResult<()> {
    on_blocking_pool("event delete", move || {
        for path in paths {
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => log::warn!("Failed to delete event file {:?}: {}", path, e),
            }
        }
        Ok(())
    })
    .await
}

fn is_orphaned_temporary_file(file_name: &str) -> bool {
    file_name.starts_with('.') && file_name.contains(".tmp-")
}

async fn cleanup_orphaned_temporary_files(base_dir: &Path) -> AppResult<()> {
    cleanup_orphaned_temporary_files_with_grace(base_dir, ORPHANED_TEMPORARY_FILE_GRACE).await
}

async fn cleanup_orphaned_temporary_files_with_grace(
    base_dir: &Path,
    grace: Duration,
) -> AppResult<()> {
    let mut entries = match fs::read_dir(base_dir).await {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            return Err(AppError::Internal(format!(
                "Failed to read ingest directory: {}",
                e
            )))
        }
    };
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to read ingest entry: {}", e)))?
    {
        let path = entry.path();
        let is_expired = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(is_orphaned_temporary_file)
            && fs::metadata(&path)
                .await
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .and_then(|modified| modified.elapsed().ok())
                .is_some_and(|age| age >= grace);
        if is_expired {
            match fs::remove_file(&path).await {
                Ok(()) => log::debug!("Removed orphaned ingest temp file {:?}", path),
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => log::warn!(
                    "Failed to remove orphaned ingest temp file {:?}: {}",
                    path,
                    e
                ),
            }
        }
    }
    Ok(())
}

fn parse_project_pending_filename(file_name: &str) -> Option<(i32, Uuid)> {
    let stem = file_name.strip_suffix(".pending.json")?;
    let stem = stem.strip_prefix("project-")?;
    let (project_id, event_id) = stem.rsplit_once('-')?;
    Some((project_id.parse().ok()?, Uuid::parse_str(event_id).ok()?))
}

fn pending_filename_matches(file_name: &str, metadata: &EventMetadata, event_id: Uuid) -> bool {
    match parse_project_pending_filename(file_name) {
        Some((project_id, filename_id)) => {
            project_id == metadata.project_id && filename_id == event_id
        }
        None => {
            file_name
                .strip_suffix(".pending.json")
                .and_then(|id| Uuid::parse_str(id).ok())
                == Some(event_id)
        }
    }
}

/// Reads pending event metadata left by an interrupted or exhausted digest.
pub async fn list_pending_event_metadata(base_dir: &Path) -> AppResult<Vec<EventMetadata>> {
    list_pending_event_metadata_except(base_dir, |_, _| false).await
}

/// [`list_pending_event_metadata`], skipping every project-scoped file whose
/// `(project_id, event_id)` the caller says it owns, before the file is read.
///
/// A scan reads and parses each candidate in full (payload included), so
/// under a backlog of thousands of in-flight events a scan that only filtered
/// afterwards cost hundreds of megabytes of reads for nothing. Legacy files
/// carry no project id in their name and are read as before.
pub(crate) async fn list_pending_event_metadata_except(
    base_dir: &Path,
    owned: impl Fn(i32, Uuid) -> bool,
) -> AppResult<Vec<EventMetadata>> {
    cleanup_orphaned_temporary_files(base_dir).await?;
    let mut entries = match fs::read_dir(base_dir).await {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(AppError::Internal(format!(
                "Failed to read ingest directory: {}",
                e
            )))
        }
    };
    let mut pending = Vec::new();
    let mut seen_events = HashSet::new();

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to read ingest entry: {}", e)))?
    {
        let path = entry.path();
        let file_name = match path.file_name().and_then(|name| name.to_str()) {
            Some(file_name) => file_name,
            None => continue,
        };
        let is_pending_record = file_name.ends_with(".pending.json");
        let is_legacy_metadata = file_name.ends_with(".meta.json");
        if !is_pending_record && !is_legacy_metadata {
            continue;
        }
        if is_pending_record
            && parse_project_pending_filename(file_name)
                .is_some_and(|(project_id, event_id)| owned(project_id, event_id))
        {
            continue;
        }

        let bytes = match fs::read(&path).await {
            Ok(bytes) => bytes,
            Err(e) => {
                log::warn!("Skipping unreadable pending event file {:?}: {}", path, e);
                continue;
            }
        };
        if is_pending_record {
            match serde_json::from_slice::<PendingEventRecord>(&bytes) {
                Ok(record) if record.version == PENDING_EVENT_VERSION => {
                    match Uuid::parse_str(&record.metadata.event_id) {
                        Ok(metadata_id) => {
                            let filename_matches =
                                pending_filename_matches(file_name, &record.metadata, metadata_id);
                            if filename_matches
                                && seen_events.insert((record.metadata.project_id, metadata_id))
                            {
                                pending.push(record.metadata);
                            } else if !filename_matches {
                                log::warn!("Skipping invalid pending event identity: {:?}", path);
                            }
                        }
                        Err(_) => log::warn!("Skipping invalid pending event identity: {:?}", path),
                    }
                }
                Ok(_) => log::warn!("Skipping invalid pending event identity: {:?}", path),
                Err(e) => log::warn!("Skipping malformed pending event {:?}: {}", path, e),
            }
        } else {
            match serde_json::from_slice::<EventMetadata>(&bytes) {
                Ok(metadata) => {
                    if let Ok(event_id) = Uuid::parse_str(&metadata.event_id) {
                        if seen_events.insert((metadata.project_id, event_id)) {
                            pending.push(metadata);
                        }
                    } else {
                        log::warn!("Skipping invalid event metadata identity: {:?}", path);
                    }
                }
                Err(e) => log::warn!("Skipping malformed event metadata {:?}: {}", path, e),
            }
        }
    }

    Ok(pending)
}

/// Gets the ingest directory from config or uses default
pub fn get_ingest_dir(configured_dir: Option<&str>) -> PathBuf {
    configured_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_INGEST_DIR))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn metadata(event_id: &str, project_id: i32) -> EventMetadata {
        EventMetadata {
            event_id: event_id.to_string(),
            project_id,
            ingested_at: chrono::Utc::now(),
            remote_addr: None,
        }
    }

    #[test]
    fn test_get_event_path_valid_uuid() {
        let base = Path::new("/tmp/test");
        let path = get_event_path(base, "9ec79c33-ec99-42ab-8353-589fcb2e04dc").unwrap();
        assert!(path
            .to_string_lossy()
            .contains("9ec79c33ec9942ab8353589fcb2e04dc.json"));
    }

    #[test]
    fn test_get_event_path_invalid_uuid() {
        let base = Path::new("/tmp/test");
        let result = get_event_path(base, "not-a-uuid");
        assert!(result.is_err());
    }

    #[test]
    fn test_get_ingest_dir_default() {
        let dir = get_ingest_dir(None);
        assert_eq!(dir, PathBuf::from("/tmp/rustrak/ingest"));
    }

    #[test]
    fn test_get_ingest_dir_custom() {
        let dir = get_ingest_dir(Some("/custom/path"));
        assert_eq!(dir, PathBuf::from("/custom/path"));
    }

    #[tokio::test]
    async fn prepare_ingest_dir_creates_nested_directory_idempotently() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("nested/ingest");

        prepare_ingest_dir(&dir).await.unwrap();
        prepare_ingest_dir(&dir).await.unwrap();

        assert!(dir.is_dir());
    }

    #[test]
    fn base64_scratch_buffer_replaces_previous_chunk() {
        let mut encoded = String::new();

        encode_base64_chunk(b"abc", &mut encoded);
        assert_eq!(encoded, "YWJj");
        encode_base64_chunk(b"d", &mut encoded);
        assert_eq!(encoded, "ZA==");
    }

    #[tokio::test]
    async fn project_scopes_same_event_id_and_recovery_listing() {
        let dir = tempfile::tempdir().unwrap();
        let event_id = "9ec79c33-ec99-42ab-8353-589fcb2e04dc";
        let first = metadata(event_id, 7);
        let second = metadata(event_id, 8);

        store_event_with_metadata(dir.path(), event_id, br#"{"project":7}"#, &first)
            .await
            .unwrap();
        store_event_with_metadata(dir.path(), event_id, br#"{"project":8}"#, &second)
            .await
            .unwrap();

        assert_eq!(
            read_event_for_project(dir.path(), 7, event_id)
                .await
                .unwrap(),
            br#"{"project":7}"#
        );
        assert_eq!(
            read_event_for_project(dir.path(), 8, event_id)
                .await
                .unwrap(),
            br#"{"project":8}"#
        );
        let pending = list_pending_event_metadata(dir.path()).await.unwrap();
        assert_eq!(pending.len(), 2);
        assert!(pending.iter().any(|item| item.project_id == 7));
        assert!(pending.iter().any(|item| item.project_id == 8));

        delete_event_at(dir.path(), 7, event_id, EventStorageLocation::Project)
            .await
            .unwrap();
        assert_eq!(
            read_event_for_project(dir.path(), 8, event_id)
                .await
                .unwrap(),
            br#"{"project":8}"#
        );
    }

    #[tokio::test]
    async fn duplicate_event_id_keeps_the_first_payload() {
        let dir = tempfile::tempdir().unwrap();
        let event_id = "9ec79c33-ec99-42ab-8353-589fcb2e04dc";
        let metadata = metadata(event_id, 7);

        store_event_with_metadata(dir.path(), event_id, br#"{"first":true}"#, &metadata)
            .await
            .unwrap();
        store_event_with_metadata(dir.path(), event_id, br#"{"second":true}"#, &metadata)
            .await
            .unwrap();

        assert_eq!(
            read_event_for_project(dir.path(), 7, event_id)
                .await
                .unwrap(),
            br#"{"first":true}"#
        );
    }

    #[tokio::test]
    async fn streamed_pending_payload_round_trips_at_base64_boundaries() {
        let dir = tempfile::tempdir().unwrap();
        let sizes = [
            0,
            1,
            2,
            3,
            4,
            32 * 1024,
            32 * 1024 + 1,
            BASE64_WRITE_CHUNK_SIZE - 1,
            BASE64_WRITE_CHUNK_SIZE,
            BASE64_WRITE_CHUNK_SIZE + 1,
            BASE64_WRITE_CHUNK_SIZE + 2,
            BASE64_WRITE_CHUNK_SIZE + 3,
            4 * 1024 * 1024,
        ];

        for (index, size) in sizes.into_iter().enumerate() {
            let event_id = Uuid::from_u128((index + 1) as u128).to_string();
            let metadata = metadata(&event_id, 7);
            let payload: Vec<u8> = (0..size).map(|byte| (byte % 251) as u8).collect();

            store_event_with_metadata(dir.path(), &event_id, &payload, &metadata)
                .await
                .unwrap();

            let pending_path = get_project_pending_event_path(dir.path(), 7, &event_id).unwrap();
            let record: PendingEventRecord =
                serde_json::from_slice(&fs::read(&pending_path).await.unwrap()).unwrap();
            assert_eq!(record.version, PENDING_EVENT_VERSION);
            assert_eq!(record.metadata.event_id, event_id);
            let stored_payload = STANDARD.decode(record.event_data).unwrap();
            assert_eq!(stored_payload, payload);
            assert_eq!(
                read_event_for_project(dir.path(), 7, &event_id)
                    .await
                    .unwrap(),
                payload
            );
        }
    }

    #[tokio::test]
    async fn pending_records_with_an_unknown_version_are_neither_listed_nor_read() {
        // Only version 1 has ever been written to disk. Anything else is a
        // foreign or corrupted record and must not be recovered as an event.
        let dir = tempfile::tempdir().unwrap();
        let event_id = "9ec79c33-ec99-42ab-8353-589fcb2e04dc";
        let pending_path = get_project_pending_event_path(dir.path(), 7, event_id).unwrap();
        let record = PendingEventRecord {
            version: 2,
            metadata: metadata(event_id, 7),
            event_data: STANDARD.encode(b"{}"),
        };
        fs::create_dir_all(dir.path()).await.unwrap();
        fs::write(&pending_path, serde_json::to_vec(&record).unwrap())
            .await
            .unwrap();

        assert!(list_pending_event_metadata(dir.path())
            .await
            .unwrap()
            .is_empty());
        assert!(read_event_for_project(dir.path(), 7, event_id)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn metadata_store_recreates_directory_removed_after_startup() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("ingest");
        let event_id = "9ec79c33-ec99-42ab-8353-589fcb2e04dc";
        let event_metadata = metadata(event_id, 7);

        prepare_ingest_dir(&dir).await.unwrap();
        fs::remove_dir_all(&dir).await.unwrap();
        store_event_with_metadata(&dir, event_id, br"{}", &event_metadata)
            .await
            .unwrap();
        assert!(dir.is_dir());
    }

    #[tokio::test]
    async fn recovery_listing_skips_owned_files_without_reading_them() {
        let dir = tempfile::tempdir().unwrap();
        let owned_id = "9ec79c33-ec99-42ab-8353-589fcb2e04dc";
        let other_id = "1ec79c33-ec99-42ab-8353-589fcb2e04dc";
        store_event_with_metadata(dir.path(), owned_id, br"{}", &metadata(owned_id, 7))
            .await
            .unwrap();
        store_event_with_metadata(dir.path(), other_id, br"{}", &metadata(other_id, 7))
            .await
            .unwrap();
        // An owned file that is not even valid JSON: proof it was never read.
        let owned_path = get_project_pending_event_path(dir.path(), 7, owned_id).unwrap();
        fs::write(&owned_path, b"not json").await.unwrap();

        let owned_uuid = Uuid::parse_str(owned_id).unwrap();
        let listed = list_pending_event_metadata_except(dir.path(), |project_id, event_id| {
            project_id == 7 && event_id == owned_uuid
        })
        .await
        .unwrap();

        assert_eq!(listed.len(), 1);
        assert!(same_event_id(&listed[0].event_id, other_id));
    }

    #[tokio::test]
    async fn a_read_of_a_vanished_event_reports_it_as_missing() {
        let dir = tempfile::tempdir().unwrap();
        let error = read_event_for_project(dir.path(), 7, "9ec79c33-ec99-42ab-8353-589fcb2e04dc")
            .await
            .unwrap_err();
        assert!(is_missing_event_file(&error), "{error}");
        assert!(!is_missing_event_file(&AppError::Internal(
            "Failed to read event file: permission denied".to_string()
        )));
    }

    proptest! {
        #[test]
        fn base64_chunk_ranges_cover_input_without_midstream_padding(
            data in prop::collection::vec(
                any::<u8>(),
                0..(BASE64_WRITE_CHUNK_SIZE * 3 + 10)
            )
        ) {
            let ranges: Vec<_> = base64_chunk_ranges(data.len()).collect();
            let mut covered = 0;
            let mut chunked_encoding = String::new();

            for (index, range) in ranges.iter().enumerate() {
                prop_assert_eq!(range.start, covered);
                prop_assert!(range.end > range.start);
                prop_assert!(range.end - range.start <= BASE64_WRITE_CHUNK_SIZE);
                if index + 1 < ranges.len() {
                    prop_assert_eq!((range.end - range.start) % 3, 0);
                }
                chunked_encoding.push_str(&STANDARD.encode(&data[range.clone()]));
                covered = range.end;
            }

            prop_assert_eq!(covered, data.len());
            prop_assert_eq!(chunked_encoding, STANDARD.encode(&data));
        }
    }

    #[tokio::test]
    async fn publishing_leaves_only_the_pending_record_in_the_ingest_dir() {
        let dir = tempfile::tempdir().unwrap();
        let event_id = "9ec79c33-ec99-42ab-8353-589fcb2e04dc";

        store_event_with_metadata(dir.path(), event_id, br#"{"a":1}"#, &metadata(event_id, 7))
            .await
            .unwrap();

        let mut names = Vec::new();
        let mut entries = fs::read_dir(dir.path()).await.unwrap();
        while let Some(entry) = entries.next_entry().await.unwrap() {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
        let pending = get_project_pending_event_path(dir.path(), 7, event_id).unwrap();
        assert_eq!(
            names,
            vec![pending.file_name().unwrap().to_string_lossy().into_owned()],
            "no lock or scratch files may be left behind per event"
        );
    }

    #[tokio::test]
    async fn concurrent_duplicate_writes_keep_one_complete_record() {
        let dir = tempfile::tempdir().unwrap();
        let event_id = "9ec79c33-ec99-42ab-8353-589fcb2e04dc";
        let metadata = metadata(event_id, 7);

        let (first, second) = tokio::join!(
            store_event_with_metadata(dir.path(), event_id, br#"{"first":true}"#, &metadata),
            store_event_with_metadata(dir.path(), event_id, br#"{"second":true}"#, &metadata),
        );
        first.unwrap();
        second.unwrap();

        let stored = read_event_for_project(dir.path(), 7, event_id)
            .await
            .unwrap();
        assert!(stored == br#"{"first":true}"# || stored == br#"{"second":true}"#);
        assert_eq!(
            list_pending_event_metadata(dir.path()).await.unwrap().len(),
            1
        );

        let mut entries = fs::read_dir(dir.path()).await.unwrap();
        while let Some(entry) = entries.next_entry().await.unwrap() {
            assert!(!is_orphaned_temporary_file(
                &entry.file_name().to_string_lossy()
            ));
        }
    }

    #[tokio::test]
    async fn concurrent_replacement_of_malformed_record_keeps_one_payload() {
        let dir = tempfile::tempdir().unwrap();
        let event_id = "9ec79c33-ec99-42ab-8353-589fcb2e04dc";
        let event_metadata = metadata(event_id, 7);
        let pending_path = get_project_pending_event_path(dir.path(), 7, event_id).unwrap();

        fs::create_dir_all(dir.path()).await.unwrap();
        fs::write(&pending_path, b"malformed").await.unwrap();

        let (first, second) = tokio::join!(
            store_event_with_metadata(dir.path(), event_id, br#"{"first":true}"#, &event_metadata),
            store_event_with_metadata(dir.path(), event_id, br#"{"second":true}"#, &event_metadata),
        );
        first.unwrap();
        second.unwrap();

        let stored = read_event_for_project(dir.path(), 7, event_id)
            .await
            .unwrap();
        assert!(stored == br#"{"first":true}"# || stored == br#"{"second":true}"#);
    }

    #[tokio::test]
    async fn mismatched_metadata_is_rejected_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let event_id = "9ec79c33-ec99-42ab-8353-589fcb2e04dc";
        let result =
            store_event_with_metadata(dir.path(), event_id, br"{}", &metadata("bad", 7)).await;
        assert!(result.is_err());
        let mut entries = fs::read_dir(dir.path()).await.unwrap();
        assert!(entries.next_entry().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn malformed_pending_record_is_retained_for_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let event_id = "9ec79c33-ec99-42ab-8353-589fcb2e04dc";
        store_event(dir.path(), event_id, br#"{"legacy":true}"#)
            .await
            .unwrap();
        store_event_with_metadata(dir.path(), event_id, br"{}", &metadata(event_id, 7))
            .await
            .unwrap();
        let pending_path = get_project_pending_event_path(dir.path(), 7, event_id).unwrap();
        let malformed = PendingEventRecord {
            version: PENDING_EVENT_VERSION,
            metadata: metadata(event_id, 7),
            event_data: "not-base64".to_string(),
        };
        fs::write(&pending_path, serde_json::to_vec(&malformed).unwrap())
            .await
            .unwrap();

        assert!(read_event_for_project(dir.path(), 7, event_id)
            .await
            .is_err());
        assert!(fs::try_exists(pending_path).await.unwrap());
        assert!(
            fs::try_exists(get_event_path(dir.path(), event_id).unwrap())
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn legacy_event_write_is_atomic_and_does_not_follow_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let event_id = "9ec79c33-ec99-42ab-8353-589fcb2e04dc";
        let path = get_event_path(dir.path(), event_id).unwrap();
        let outside_path = outside.path().join("outside.json");
        fs::write(&outside_path, b"keep").await.unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside_path, &path).unwrap();

        #[cfg(unix)]
        {
            store_event(dir.path(), event_id, br#"{"updated":true}"#)
                .await
                .unwrap();
            assert_eq!(fs::read(&outside_path).await.unwrap(), b"keep");
            assert_eq!(fs::read(&path).await.unwrap(), br#"{"updated":true}"#);
            assert!(!fs::symlink_metadata(&path)
                .await
                .unwrap()
                .file_type()
                .is_symlink());
        }

        #[cfg(not(unix))]
        let _ = (outside_path, path);
    }

    #[tokio::test]
    async fn malformed_duplicate_record_is_quarantined_before_replacement() {
        let dir = tempfile::tempdir().unwrap();
        let event_id = "9ec79c33-ec99-42ab-8353-589fcb2e04dc";
        let pending_path = get_project_pending_event_path(dir.path(), 7, event_id).unwrap();
        fs::create_dir_all(dir.path()).await.unwrap();
        fs::write(&pending_path, b"{").await.unwrap();

        store_event_with_metadata(
            dir.path(),
            event_id,
            br#"{"recovered":true}"#,
            &metadata(event_id, 7),
        )
        .await
        .unwrap();

        assert_eq!(
            read_event_for_project(dir.path(), 7, event_id)
                .await
                .unwrap(),
            br#"{"recovered":true}"#
        );
        let mut entries = fs::read_dir(dir.path()).await.unwrap();
        let mut quarantined = false;
        while let Some(entry) = entries.next_entry().await.unwrap() {
            if entry.file_name().to_string_lossy().contains(".corrupt-") {
                quarantined = true;
            }
        }
        assert!(quarantined);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn pending_symlink_is_replaced_without_following_or_overwriting_target() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let event_id = "9ec79c33-ec99-42ab-8353-589fcb2e04dc";
        let pending_path = get_project_pending_event_path(dir.path(), 7, event_id).unwrap();
        let outside_path = outside.path().join("outside.pending.json");
        let old_record = PendingEventRecord {
            version: PENDING_EVENT_VERSION,
            metadata: metadata(event_id, 7),
            event_data: STANDARD.encode(br#"{"outside":true}"#),
        };
        let old_bytes = serde_json::to_vec(&old_record).unwrap();

        fs::create_dir_all(dir.path()).await.unwrap();
        fs::write(&outside_path, &old_bytes).await.unwrap();
        std::os::unix::fs::symlink(&outside_path, &pending_path).unwrap();

        store_event_with_metadata(
            dir.path(),
            event_id,
            br#"{"replacement":true}"#,
            &metadata(event_id, 7),
        )
        .await
        .unwrap();

        assert_eq!(
            read_event_for_project(dir.path(), 7, event_id)
                .await
                .unwrap(),
            br#"{"replacement":true}"#
        );
        assert_eq!(fs::read(&outside_path).await.unwrap(), old_bytes);
        assert!(!fs::symlink_metadata(&pending_path)
            .await
            .unwrap()
            .file_type()
            .is_symlink());
    }

    #[tokio::test]
    async fn orphaned_atomic_write_temp_files_are_cleaned() {
        let dir = tempfile::tempdir().unwrap();
        let orphan = dir.path().join(".project-7-event.pending.json.tmp-crash");
        fs::write(&orphan, b"partial").await.unwrap();

        cleanup_orphaned_temporary_files_with_grace(dir.path(), Duration::ZERO)
            .await
            .unwrap();
        assert!(!fs::try_exists(orphan).await.unwrap());
    }

    #[tokio::test]
    async fn active_atomic_write_temp_files_are_retained() {
        let dir = tempfile::tempdir().unwrap();
        let active = dir.path().join(".project-7-event.pending.json.tmp-active");
        fs::write(&active, b"partial").await.unwrap();

        list_pending_event_metadata(dir.path()).await.unwrap();

        assert!(fs::try_exists(active).await.unwrap());
    }

    #[tokio::test]
    async fn legacy_pending_event_metadata_round_trips_and_deletes_with_event() {
        let dir = tempfile::tempdir().unwrap();
        let event_id = "9ec79c33-ec99-42ab-8353-589fcb2e04dc";
        let metadata = metadata(event_id, 7);

        let legacy_path = get_pending_event_path(dir.path(), event_id).unwrap();
        fs::create_dir_all(dir.path()).await.unwrap();
        let record = PendingEventRecord {
            version: PENDING_EVENT_VERSION,
            metadata: metadata.clone(),
            event_data: STANDARD.encode(br"{}"),
        };
        fs::write(&legacy_path, serde_json::to_vec(&record).unwrap())
            .await
            .unwrap();
        assert_eq!(
            read_event_for_project(dir.path(), 7, event_id)
                .await
                .unwrap(),
            br"{}"
        );
        delete_event_at(dir.path(), 7, event_id, EventStorageLocation::Legacy)
            .await
            .unwrap();
        assert!(list_pending_event_metadata(dir.path())
            .await
            .unwrap()
            .is_empty());
    }
}
