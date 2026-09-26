use super::{InFlightDigests, Processor, ProcessorCtx};
use crate::config::RateLimitConfig;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::ingest::storage::{delete_event_at, read_event_with_location, EventStorageLocation};
#[cfg(feature = "sqlite")]
use crate::ingest::storage::{delete_paths, event_paths_at};
use crate::ingest::EventMetadata;
use crate::models::{AlertType, Grouping, Issue};
use crate::services::rate_limit::QuotaScope;
use crate::services::sourcemap::SourceMapProvider;
use crate::services::{
    calculate_grouping_key, get_denormalized_fields, hash_grouping_key, AlertService,
    DenormalizedFields, EventService, IssueService, ProjectService, RateLimitService,
};
use chrono::Utc;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

const INVALID_EVENT_JSON_PREFIX: &str = "Invalid event JSON";

/// Processes error/exception events: source-map rewrite → grouping → issue → store.
///
/// Owns the deps the error pipeline needs (`ingest_dir`, `rate_limit_config`,
/// `sourcemap_provider`) — the registry pattern, so [`ProcessorCtx`] stays lean
/// for processors that don't need them. `Input = EventMetadata`: the event body
/// lives in the durable temp store and is read inside [`Self::process`].
pub struct ErrorProcessor {
    ingest_dir: PathBuf,
    rate_limit_config: RateLimitConfig,
    sourcemap_provider: Arc<dyn SourceMapProvider>,
    in_flight: Arc<InFlightDigests>,
    /// The base of the issue links in alert notifications.
    pub(super) dashboard_url: String,
    /// SQLite has one writer at a time. Digests queue for it here, in process,
    /// rather than in SQLite's busy handler: that handler polls with sleeps
    /// that grow to 100ms, so with several digests waiting the lock sat idle
    /// between one commit and the next poll, and each waiter burned a
    /// connection thread sleeping.
    #[cfg(feature = "sqlite")]
    write_slot: Arc<tokio::sync::Mutex<()>>,
    /// Batches the durability checkpoints that gate file deletion.
    #[cfg(feature = "sqlite")]
    durability: Arc<DurabilityQueue>,
}

impl ErrorProcessor {
    pub fn new(
        ingest_dir: PathBuf,
        rate_limit_config: RateLimitConfig,
        sourcemap_provider: Arc<dyn SourceMapProvider>,
    ) -> Self {
        #[cfg(feature = "sqlite")]
        let write_slot = Arc::new(tokio::sync::Mutex::new(()));
        Self {
            #[cfg(feature = "sqlite")]
            durability: Arc::new(DurabilityQueue::new(
                ingest_dir.clone(),
                Arc::clone(&write_slot),
            )),
            ingest_dir,
            rate_limit_config,
            sourcemap_provider,
            in_flight: Arc::new(InFlightDigests::default()),
            dashboard_url: "http://localhost:8080".to_string(),
            #[cfg(feature = "sqlite")]
            write_slot,
        }
    }

    pub(crate) fn ingest_dir(&self) -> &Path {
        &self.ingest_dir
    }

    /// The digests this processor's process currently owns.
    pub fn in_flight(&self) -> &Arc<InFlightDigests> {
        &self.in_flight
    }

    pub(crate) async fn process_ref(
        &self,
        metadata: &EventMetadata,
        ctx: &ProcessorCtx,
    ) -> AppResult<Digested> {
        let result = self.process_impl(metadata, ctx).await;
        if let Err(e) = &result {
            if should_retain_event(e) {
                return result;
            }
            match read_event_with_location(
                &self.ingest_dir,
                metadata.project_id,
                &metadata.event_id,
            )
            .await
            {
                Ok((_, location)) => {
                    if let Err(e) = delete_event_at(
                        &self.ingest_dir,
                        metadata.project_id,
                        &metadata.event_id,
                        location,
                    )
                    .await
                    {
                        log::warn!(
                            "Failed to clean up orphaned event file {}: {:?}",
                            metadata.event_id,
                            e
                        );
                    }
                }
                Err(e) => log::warn!(
                    "Failed to locate orphaned event file {}: {:?}",
                    metadata.event_id,
                    e
                ),
            }
        }
        result
    }

    /// Runs the digest pipeline; retryable contention leaves the durable event queued.
    async fn process_impl(
        &self,
        metadata: &EventMetadata,
        ctx: &ProcessorCtx,
    ) -> AppResult<Digested> {
        let pool = &ctx.pool;

        // 0. Read the event before quota checks so cleanup can target the exact
        // project-scoped file that was read. The project row loaded here is
        // the one the quota check and the platform inference read too.
        let project = ProjectService::get_by_id(pool, metadata.project_id).await?;
        let (event_bytes, storage_location) =
            read_event_with_location(&self.ingest_dir, metadata.project_id, &metadata.event_id)
                .await?;

        // 2. Parse event from filesystem
        let mut event_data: serde_json::Value = serde_json::from_slice(&event_bytes)
            .map_err(|e| AppError::Internal(format!("{INVALID_EVENT_JSON_PREFIX}: {e}")))?;

        // 2b. Rewrite stack frames using source maps (non-fatal)
        if let Err(e) = crate::services::sourcemap::rewrite_frames(
            self.sourcemap_provider.as_ref(),
            metadata.project_id,
            &mut event_data,
        )
        .await
        {
            log::warn!(
                "source map rewriting failed for event {}: {:?}",
                metadata.event_id,
                e
            );
        }

        // 2c. Trim oversized fields (deep context windows, frame vars, huge
        // breadcrumb trails) for events whose raw payload came in above the
        // original per-item budget — see services::event_trim. Only runs for
        // the (rare) events that needed the relaxed ingest ceiling, so this
        // is a no-op for the common case.
        if event_bytes.len() > crate::ingest::parser::TARGET_EVENT_SIZE {
            crate::services::trim_oversized_event(&mut event_data);
        }
        // The raw file contents are no longer needed once the tree is parsed
        // and the size check ran; drop them so the digest's grouping phase and
        // DB writes do not carry a second copy of the payload in memory.
        drop(event_bytes);

        // 3. Parse event_id as UUID
        let event_id = Uuid::parse_str(&metadata.event_id)
            .map_err(|_| AppError::Validation("Invalid event_id".to_string()))?;

        // 4. Check for duplicates
        if EventService::exists(pool, metadata.project_id, event_id).await? {
            log::warn!("Duplicate event_id: {}", metadata.event_id);
            let existing =
                EventService::get_by_event_id(pool, metadata.project_id, event_id).await?;
            if let (Some(alert_type), Some(issue_id)) = (existing.alert_type, existing.issue_id) {
                if !AlertService::event_alert_exists(
                    pool,
                    metadata.project_id,
                    event_id,
                    alert_type,
                )
                .await?
                {
                    let issue = IssueService::get_by_id(pool, issue_id).await?;
                    AlertService::trigger_event_alert(
                        pool,
                        &project,
                        &issue,
                        alert_type,
                        event_id,
                        &self.dashboard_url,
                    )
                    .await?;
                }
            }
            self.delete_after_success(
                pool,
                metadata.project_id,
                event_id,
                &metadata.event_id,
                storage_location,
            )
            .await?;
            return Ok(Digested::Stored);
        }

        // 5. Calculate grouping key and hash
        let grouping_key = calculate_grouping_key(&event_data);
        let grouping_key_hash = hash_grouping_key(&grouping_key);
        let legacy_grouping_key_hash =
            hash_grouping_key(&crate::services::calculate_grouping_key_v1(&event_data));

        // 6. Extract denormalized fields
        let denormalized = get_denormalized_fields(&event_data);

        // 7+8. Write the digest: grouping, issue, the event itself and the
        // project's stored-event counter, in one transaction, retried as a
        // whole when SQLite reports the database busy. On SQLite the digests
        // take turns at the write here, in process (see `write_slot`).
        #[cfg(feature = "sqlite")]
        let write_slot = self.write_slot.lock().await;
        let landed = write_digest(
            pool,
            &DigestWrite {
                event_id,
                raw_event_id: &metadata.event_id,
                project_id: metadata.project_id,
                grouping_key: &grouping_key,
                grouping_key_hash: &grouping_key_hash,
                legacy_grouping_key_hash: &legacy_grouping_key_hash,
                timestamp: metadata.ingested_at,
                denormalized: &denormalized,
                level: event_data.get("level").and_then(|l| l.as_str()),
                platform: event_data.get("platform").and_then(|p| p.as_str()),
                event_data: &event_data,
                remote_addr: metadata.remote_addr.as_deref(),
                rate_limit_config: &self.rate_limit_config,
                project: &project,
            },
        )
        .await?;
        #[cfg(feature = "sqlite")]
        drop(write_slot);
        let (issue, issue_created, regressed) = match landed {
            DigestOutcome::Written(issue, created, regressed) => (*issue, created, regressed),
            DigestOutcome::RateLimited(_) => {
                log::debug!("Event {} dropped: quota exceeded", metadata.event_id);
                RateLimitService::record_rate_limited(pool, metadata.project_id).await?;
                delete_event_at(
                    &self.ingest_dir,
                    metadata.project_id,
                    &metadata.event_id,
                    storage_location,
                )
                .await?;
                return Ok(Digested::RateLimited);
            }
        };

        // 9b. Sentry-parity platform auto-detection (set once, never overwritten).
        // Best-effort: the digest transaction already committed the event, so
        // ancillary project metadata must not make the durable event fail.
        // Skipped once the project has a platform: the UPDATE would match no
        // row, but on SQLite it still takes the write lock on every event.
        if project.platform.is_none() {
            if let Some(event_platform) = event_data.get("platform").and_then(|p| p.as_str()) {
                if let Err(e) = ProjectService::infer_platform_from_event(
                    pool,
                    metadata.project_id,
                    event_platform,
                )
                .await
                {
                    log::warn!(
                        "platform inference failed for project {} (best-effort; a later event can self-heal): {:?}",
                        metadata.project_id,
                        e
                    );
                }
            }
        }

        // 10. Persist alert history before deleting the only durable source.
        if issue_created || regressed {
            let alert_type = if issue_created {
                AlertType::NewIssue
            } else {
                AlertType::Regression
            };
            AlertService::trigger_event_alert(
                pool,
                &project,
                &issue,
                alert_type,
                event_id,
                &self.dashboard_url,
            )
            .await?;
        }

        // 11. Delete after the event and alert writes complete; SQLite also
        // requires a successful durability checkpoint.
        self.delete_after_success(
            pool,
            metadata.project_id,
            event_id,
            &metadata.event_id,
            storage_location,
        )
        .await?;

        log::info!(
            "Digested event {} -> issue {} ({})",
            metadata.event_id,
            issue.id,
            if issue_created { "new" } else { "existing" }
        );

        Ok(Digested::Stored)
    }

    /// Removes the durable copy of an event whose digest has committed.
    ///
    /// A legacy-location file goes at once. A project-scoped file is what a
    /// restart replays, so on SQLite it waits for a durability checkpoint
    /// first: the commit is in the write-ahead log, and with
    /// `synchronous=NORMAL` only a completed `FULL` checkpoint puts it in the
    /// database file. The wait happens on the [`DurabilityQueue`], not here,
    /// so the digest (and the processing slot it holds) is released as soon
    /// as the commit lands.
    #[cfg(feature = "sqlite")]
    async fn delete_after_success(
        &self,
        pool: &DbPool,
        project_id: i32,
        event_id: Uuid,
        raw_event_id: &str,
        storage_location: EventStorageLocation,
    ) -> AppResult<()> {
        if matches!(storage_location, EventStorageLocation::Legacy) {
            delete_event_at(&self.ingest_dir, project_id, raw_event_id, storage_location).await?;
            return Ok(());
        }
        self.durability.enqueue(
            pool,
            PendingDelete {
                project_id,
                event_id: raw_event_id.to_string(),
                location: storage_location,
                attempts: 0,
                _owner: self.in_flight.register(project_id, event_id),
            },
        );
        Ok(())
    }

    #[cfg(feature = "postgres")]
    async fn delete_after_success(
        &self,
        _pool: &DbPool,
        project_id: i32,
        _event_id: Uuid,
        raw_event_id: &str,
        storage_location: EventStorageLocation,
    ) -> AppResult<()> {
        delete_event_at(&self.ingest_dir, project_id, raw_event_id, storage_location).await?;
        Ok(())
    }
}

/// How long the durability worker lets commits accumulate before it runs a
/// checkpoint. A `FULL` checkpoint syncs the write-ahead log and the database
/// file, so one per event cost two fsyncs and a page copy each; one per pacing
/// window covers every digest that committed in it for the same price.
#[cfg(feature = "sqlite")]
const CHECKPOINT_PACING: std::time::Duration = std::time::Duration::from_millis(25);

/// Checkpoint attempts before a queued file is retained for recovery: a busy
/// checkpoint means a writer or reader would not yield, and a restart replays
/// the file into a duplicate check rather than losing anything.
#[cfg(feature = "sqlite")]
const MAX_CHECKPOINT_ATTEMPTS: u32 = 3;

/// A committed digest whose file waits for the next completed checkpoint.
#[cfg(feature = "sqlite")]
struct PendingDelete {
    project_id: i32,
    event_id: String,
    location: EventStorageLocation,
    attempts: u32,
    /// Keeps the event registered as in flight, so the recovery worker does
    /// not replay a file that only awaits its deletion.
    _owner: super::InFlightGuard,
}

/// Batches durability checkpoints across digests.
///
/// A commit is durably in the database file once any `FULL` checkpoint that
/// *started* after it completes. The worker takes every queued entry before it
/// starts a checkpoint, so one checkpoint covers all of them, and entries
/// queued while it runs wait for the next one. It runs only while something
/// is queued: the first enqueue after an idle period spawns it and it exits
/// when the queue drains.
#[cfg(feature = "sqlite")]
struct DurabilityQueue {
    ingest_dir: PathBuf,
    /// The digests' write turn-taking. A `FULL` checkpoint needs the writers
    /// out of the way, so it takes its turn in the same queue rather than
    /// polling SQLite's busy handler against a stream of commits.
    write_slot: Arc<tokio::sync::Mutex<()>>,
    state: std::sync::Mutex<DurabilityState>,
}

#[cfg(feature = "sqlite")]
#[derive(Default)]
struct DurabilityState {
    pending: Vec<PendingDelete>,
    worker_running: bool,
}

#[cfg(feature = "sqlite")]
impl DurabilityQueue {
    fn new(ingest_dir: PathBuf, write_slot: Arc<tokio::sync::Mutex<()>>) -> Self {
        Self {
            ingest_dir,
            write_slot,
            state: std::sync::Mutex::new(DurabilityState::default()),
        }
    }

    fn enqueue(self: &Arc<Self>, pool: &DbPool, entry: PendingDelete) {
        let start_worker = {
            let mut state = self.state.lock().unwrap();
            state.pending.push(entry);
            !std::mem::replace(&mut state.worker_running, true)
        };
        if start_worker {
            tokio::spawn(Arc::clone(self).run(pool.clone()));
        }
    }

    async fn run(self: Arc<Self>, pool: DbPool) {
        let mut delay = CHECKPOINT_PACING;
        loop {
            tokio::time::sleep(delay).await;
            let batch = {
                let mut state = self.state.lock().unwrap();
                if state.pending.is_empty() {
                    state.worker_running = false;
                    return;
                }
                std::mem::take(&mut state.pending)
            };

            let completed = {
                let _turn = self.write_slot.lock().await;
                match crate::db::checkpoint_full(&pool).await {
                    Ok(completed) => completed,
                    Err(e) => {
                        log::warn!("SQLite durability checkpoint failed: {:?}", e);
                        false
                    }
                }
            };

            delay = CHECKPOINT_PACING;
            if completed {
                // One round trip to the blocking pool for the whole batch.
                let mut paths = Vec::with_capacity(batch.len() * 2);
                for entry in &batch {
                    match event_paths_at(
                        &self.ingest_dir,
                        entry.project_id,
                        &entry.event_id,
                        entry.location,
                    ) {
                        Ok(entry_paths) => paths.extend(entry_paths),
                        Err(e) => log::warn!(
                            "Failed to resolve digested event file {}: {:?}",
                            entry.event_id,
                            e
                        ),
                    }
                }
                if let Err(e) = delete_paths(paths).await {
                    log::warn!("Failed to delete digested event files: {:?}", e);
                }
                // The batch's in-flight guards drop here, after the files.
                drop(batch);
                continue;
            }

            let mut retry = Vec::with_capacity(batch.len());
            for mut entry in batch {
                entry.attempts += 1;
                if entry.attempts >= MAX_CHECKPOINT_ATTEMPTS {
                    log::warn!(
                        "SQLite durability checkpoint busy; retaining event file {}",
                        entry.event_id
                    );
                } else {
                    delay = delay.max(sqlite_retry_delay(entry.attempts as usize - 1));
                    retry.push(entry);
                }
            }
            if !retry.is_empty() {
                self.state.lock().unwrap().pending.extend(retry);
            }
        }
    }
}

impl Processor for ErrorProcessor {
    type Input = EventMetadata;

    async fn process(&self, metadata: EventMetadata, ctx: &ProcessorCtx) -> AppResult<()> {
        self.process_ref(&metadata, ctx).await.map(|_| ())
    }
}

fn should_retain_event(err: &AppError) -> bool {
    match err.kind() {
        AppError::Database(_) => true,
        // Unknown internal failures are safer to replay than to discard: a new
        // transient failure must not silently become data loss.
        AppError::Internal(message) => !message.starts_with(INVALID_EVENT_JSON_PREFIX),
        _ => false,
    }
}

/// Write attempts per digest on SQLite. Bounded: sustained contention
/// fails fast instead of piling up more waiters.
const MAX_SQLITE_WRITE_ATTEMPTS: usize = 3;

/// Backoff before retry `attempt` (0-based): 50ms, then 100ms — brief,
/// so writers that timed out together don't retry in lockstep.
#[cfg_attr(feature = "postgres", allow(dead_code))]
fn sqlite_retry_delay(attempt: usize) -> std::time::Duration {
    std::time::Duration::from_millis(50 << attempt)
}

/// True for transient SQLite write contention: the busy family (codes 5,
/// 261, 517, 773). Always false on Postgres, whose SQLSTATEs never collide
/// with these. Pool exhaustion is deliberately excluded: it is a capacity
/// signal, and retrying it only lengthens the queue that caused it.
pub(crate) fn is_retryable_write_contention(err: &AppError) -> bool {
    match err.kind() {
        AppError::Database(sqlx::Error::Database(db)) => {
            db.code().as_deref().is_some_and(is_sqlite_busy_code)
        }
        _ => false,
    }
}

/// Whether a SQLite extended result code string means "database is locked".
fn is_sqlite_busy_code(code: &str) -> bool {
    matches!(code, "5" | "261" | "517" | "773")
}

/// Everything one digest writes, gathered so the retried transaction takes a
/// single argument instead of a dozen positional ones.
struct DigestWrite<'a> {
    event_id: Uuid,
    /// The event id as it arrived, for log lines. Not the primary key.
    raw_event_id: &'a str,
    project_id: i32,
    grouping_key: &'a str,
    grouping_key_hash: &'a str,
    /// The key this event would have had before the grouping changes, so an
    /// issue created by an older release still claims its own events.
    legacy_grouping_key_hash: &'a str,
    timestamp: chrono::DateTime<Utc>,
    denormalized: &'a DenormalizedFields,
    level: Option<&'a str>,
    platform: Option<&'a str>,
    event_data: &'a serde_json::Value,
    remote_addr: Option<&'a str>,
    rate_limit_config: &'a RateLimitConfig,
    /// The row loaded at the start of the digest, for its own quota limits.
    project: &'a crate::models::Project,
}

/// How an accepted event left the digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Digested {
    /// Stored, or already stored under the same id.
    Stored,
    /// Dropped by the quota: nothing was written.
    RateLimited,
}

/// What one digest attempt left behind.
enum DigestOutcome {
    Written(Box<Issue>, bool, bool),
    /// Nothing was written: the quota had no room for this event.
    RateLimited(QuotaScope),
}

/// Writes the digest, retrying the whole transaction when the database is
/// busy. Only a whole-tx restart is safe: a `BUSY_SNAPSHOT` snapshot can never
/// be resumed, and `busy_timeout` does not retry the read→write upgrade that
/// produces it. Each attempt waits on the write lock itself, so a slow holder
/// delays a digest instead of dropping it. On Postgres the retry never
/// triggers: a per-project advisory lock serializes issue creation and no busy
/// code is ever reported.
///
/// Returns the issue and its lifecycle flags, or that the quota had no room.
async fn write_digest(pool: &DbPool, write: &DigestWrite<'_>) -> AppResult<DigestOutcome> {
    let mut attempt = 0usize;
    let event_id = write.raw_event_id;
    loop {
        let result = write_digest_once(pool, write).await;

        match result {
            Err(e)
                if is_retryable_write_contention(&e) && attempt + 1 < MAX_SQLITE_WRITE_ATTEMPTS =>
            {
                // Expected under load — one debug line per failed attempt;
                // the healed episode itself is reported once at info below.
                log::debug!(
                    "SQLite write lock busy for event {event_id}; attempt {}/{} failed, retrying",
                    attempt + 1,
                    MAX_SQLITE_WRITE_ATTEMPTS
                );
                tokio::time::sleep(sqlite_retry_delay(attempt)).await;
                attempt += 1;
            }
            result => {
                if attempt > 0 && result.is_ok() {
                    log::info!(
                        "SQLite write lock busy for event {event_id}; healed after {attempt} retries"
                    );
                }
                return result;
            }
        }
    }
}

/// One attempt at the digest's writes.
///
/// Runs in a write transaction: `BEGIN IMMEDIATE` on SQLite (write lock
/// up front, so `busy_timeout` can wait on it), plus a per-project
/// advisory lock on Postgres.
async fn write_digest_once(pool: &DbPool, write: &DigestWrite<'_>) -> AppResult<DigestOutcome> {
    // Start a write transaction. On SQLite this is `BEGIN IMMEDIATE` so the
    // read-then-write below (SELECT MAX(digest_order) → INSERT) takes the write
    // lock up front instead of failing with "database is locked" on upgrade.
    let mut tx = crate::db::begin_write(pool).await?;

    // Acquire advisory lock for this project (Postgres only).
    // SQLite serializes all writes, so no advisory lock needed.
    #[cfg(feature = "postgres")]
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(write.project_id as i64)
        .execute(&mut *tx)
        .await?;

    let result = write_digest_rows(&mut tx, write).await;

    match result {
        Ok(DigestOutcome::RateLimited(scope)) => {
            tx.rollback().await?;
            Ok(DigestOutcome::RateLimited(scope))
        }
        Ok(landed) => {
            // Commit the transaction (releases the advisory lock)
            tx.commit().await?;
            Ok(landed)
        }
        Err(e) => {
            // Rollback on error (also releases the advisory lock)
            tx.rollback().await?;
            Err(e)
        }
    }
}

/// The digest's writes, all inside the caller's transaction.
///
/// The issue counters (`digested_event_count`, `stored_event_count`) and the
/// project's `stored_event_count` claim the event row inserted here, so all of
/// it commits together or none of it does. Deliberately kept to inserts and
/// counter bumps: quota state counts rows over time windows, and running those
/// scans here would hold the write lock across a full table scan, which is the
/// pathological holder the retry loop exists to survive.
async fn write_digest_rows(
    tx: &mut sqlx::Transaction<'_, DbBackend>,
    write: &DigestWrite<'_>,
) -> AppResult<DigestOutcome> {
    let (issue, grouping, created, regressed) = find_or_create_issue_and_grouping_inner(
        tx,
        write.project_id,
        write.grouping_key,
        write.grouping_key_hash,
        write.legacy_grouping_key_hash,
        write.timestamp,
        write.denormalized,
        write.level,
        write.platform,
    )
    .await?;

    if regressed {
        crate::services::IssueSocialService::add_activity_on(
            tx,
            issue.id,
            None,
            "set_regression",
            &serde_json::json!({ "status": crate::models::STATUS_UNRESOLVED }).to_string(),
        )
        .await?;
    }

    // No digest_order to compute: events order within an issue by
    // (timestamp, id) (see idx_events_issue_timestamp), which needs no
    // per-issue counter to derive or keep in sync.
    EventService::create(
        &mut **tx,
        write.event_id,
        write.project_id,
        issue.id,
        grouping.id,
        write.event_data,
        write.timestamp,
        write.denormalized,
        write.remote_addr,
        alert_type_for_lifecycle(created, regressed),
    )
    .await?;

    // Last, so the installation row every digest shares is locked only from
    // here to the commit, as its counter bump always was. A full quota rolls
    // back everything above with it.
    if let Some(scope) =
        RateLimitService::try_consume(tx, write.project, write.rate_limit_config, Utc::now())
            .await?
    {
        return Ok(DigestOutcome::RateLimited(scope));
    }
    RateLimitService::increment_event_counters(tx, write.project_id).await?;

    Ok(DigestOutcome::Written(Box::new(issue), created, regressed))
}

/// Inner function that performs the actual find-or-create logic within a transaction
#[cfg(feature = "postgres")]
type DbBackend = sqlx::Postgres;
#[cfg(feature = "sqlite")]
type DbBackend = sqlx::Sqlite;

#[allow(clippy::too_many_arguments)]
async fn find_or_create_issue_and_grouping_inner(
    tx: &mut sqlx::Transaction<'_, DbBackend>,
    project_id: i32,
    grouping_key: &str,
    grouping_key_hash: &str,
    legacy_grouping_key_hash: &str,
    timestamp: chrono::DateTime<Utc>,
    denormalized: &DenormalizedFields,
    level: Option<&str>,
    platform: Option<&str>,
) -> AppResult<(Issue, Grouping, bool, bool)> {
    // The grouping and the issue columns the update path reads, in one round
    // trip: on SQLite every statement is a hand-off to the connection's
    // worker thread, and this one runs under the write lock.
    let find = |hash: String| async move {
        sqlx::query_as::<_, GroupingWithIssue>(
            r#"
            SELECT g.id, g.project_id, g.issue_id, g.grouping_key, g.grouping_key_hash,
                   g.created_at,
                   i.status, i.status_details, i.last_release, i.calculated_type,
                   i.calculated_value, i.first_seen, i.last_seen, i.last_frame_filename,
                   i.last_frame_module, i.last_frame_function, i.level
            FROM groupings g
            JOIN issues i ON i.id = g.issue_id
            WHERE g.project_id = $1 AND g.grouping_key_hash = $2
            "#,
        )
        .bind(project_id)
        .bind(hash)
    };

    let mut existing: Option<GroupingWithIssue> = find(grouping_key_hash.to_string())
        .await
        .fetch_optional(&mut **tx)
        .await?;

    // Nothing under the current key: the issue may predate a change to how the
    // key is built. Claim it under the key that release would have produced,
    // and record the current one so later events resolve directly.
    if existing.is_none() && legacy_grouping_key_hash != grouping_key_hash {
        if let Some(legacy) = find(legacy_grouping_key_hash.to_string())
            .await
            .fetch_optional(&mut **tx)
            .await?
        {
            let grouping: Grouping = sqlx::query_as(
                r#"
                INSERT INTO groupings (project_id, issue_id, grouping_key, grouping_key_hash)
                VALUES ($1, $2, $3, $4)
                RETURNING *
                "#,
            )
            .bind(project_id)
            .bind(legacy.grouping.issue_id)
            .bind(grouping_key)
            .bind(grouping_key_hash)
            .fetch_one(&mut **tx)
            .await?;
            existing = Some(GroupingWithIssue {
                grouping,
                issue: legacy.issue,
            });
        }
    }

    if let Some(GroupingWithIssue {
        grouping,
        issue: prev,
    }) = existing
    {
        // Detect regression: a new event for an already-resolved issue must
        // reopen it as `regressed` (mirrors Sentry's lifecycle) — unless the
        // issue was "resolved in next release" and this event is still from the
        // same release (no new deploy yet), in which case it stays resolved.
        let in_next_release = serde_json::from_str::<serde_json::Value>(&prev.status_details)
            .ok()
            .and_then(|v| v.get("in_next_release").and_then(|b| b.as_bool()))
            .unwrap_or(false);
        // An event with no release metadata can't prove it's from a newer
        // release, so treat it as the same release rather than triggering a
        // spurious regression reopen.
        let same_release =
            denormalized.release.is_empty() || denormalized.release == prev.last_release;
        let suppress = in_next_release && same_release;
        let regressed = prev.status == crate::models::STATUS_RESOLVED && !suppress;

        let (calculated_type, calculated_value) = updated_title(
            (&prev.calculated_type, &prev.calculated_value),
            (
                &denormalized.calculated_type,
                &denormalized.calculated_value,
            ),
        );
        // Sentry merges the incoming metadata over the existing bag, so a field
        // the new event does not carry keeps the value it had.
        fn keep_if_blank<'a>(incoming: &'a str, existing: &'a str) -> &'a str {
            if incoming.is_empty() {
                existing
            } else {
                incoming
            }
        }
        // An event can arrive older than the issue (clock skew across hosts, a
        // replayed envelope), so each bound moves only in its own direction.
        let first_seen = prev.first_seen.min(timestamp);
        let last_seen = prev.last_seen.max(timestamp);
        // SDKs omit `level` when it is their default, so an absent one keeps
        // whatever the issue already had rather than clearing it.
        let level = level.or(prev.level.as_deref());
        let frame_filename =
            keep_if_blank(&denormalized.last_frame_filename, &prev.last_frame_filename);
        let frame_module = keep_if_blank(&denormalized.last_frame_module, &prev.last_frame_module);
        let frame_function =
            keep_if_blank(&denormalized.last_frame_function, &prev.last_frame_function);

        // Grouping exists, update issue (reopening it if it regressed).
        let issue: Issue = if regressed {
            sqlx::query_as(
                r#"
                UPDATE issues
                SET first_seen = $8,
                    last_seen = $2,
                    digested_event_count = digested_event_count + 1,
                    stored_event_count = stored_event_count + 1,
                    calculated_type = $4,
                    calculated_value = $5,
                    level = $6,
                    culprit = $7,
                    last_frame_filename = $9,
                    last_frame_module = $10,
                    last_frame_function = $11,
                    status = 'unresolved',
                    substatus = 'regressed',
                    status_details = '{}',
                    last_release = CASE WHEN $3 <> '' THEN $3 ELSE last_release END,
                    first_release = CASE WHEN first_release = '' THEN $3 ELSE first_release END
                WHERE id = $1
                RETURNING *
                "#,
            )
            .bind(grouping.issue_id)
            .bind(last_seen)
            .bind(&denormalized.release)
            .bind(calculated_type)
            .bind(calculated_value)
            .bind(level)
            .bind(&denormalized.culprit)
            .bind(first_seen)
            .bind(frame_filename)
            .bind(frame_module)
            .bind(frame_function)
            .fetch_one(&mut **tx)
            .await?
        } else {
            sqlx::query_as(
                r#"
                UPDATE issues
                SET first_seen = $8,
                    last_seen = $2,
                    digested_event_count = digested_event_count + 1,
                    stored_event_count = stored_event_count + 1,
                    calculated_type = $4,
                    calculated_value = $5,
                    level = $6,
                    culprit = $7,
                    last_frame_filename = $9,
                    last_frame_module = $10,
                    last_frame_function = $11,
                    last_release = CASE WHEN $3 <> '' THEN $3 ELSE last_release END,
                    first_release = CASE WHEN first_release = '' THEN $3 ELSE first_release END
                WHERE id = $1
                RETURNING *
                "#,
            )
            .bind(grouping.issue_id)
            .bind(last_seen)
            .bind(&denormalized.release)
            .bind(calculated_type)
            .bind(calculated_value)
            .bind(level)
            .bind(&denormalized.culprit)
            .bind(first_seen)
            .bind(frame_filename)
            .bind(frame_module)
            .bind(frame_function)
            .fetch_one(&mut **tx)
            .await?
        };

        return Ok((issue, grouping, false, regressed));
    }

    // Get the next digest_order for this project (safe because we hold the advisory lock)
    let max_order: Option<i32> =
        sqlx::query_scalar("SELECT MAX(digest_order) FROM issues WHERE project_id = $1")
            .bind(project_id)
            .fetch_one(&mut **tx)
            .await?;

    let digest_order = max_order.unwrap_or(0) + 1;

    // Create new issue (generate UUID in application for cross-DB compatibility)
    let issue_id = Uuid::new_v4();
    let priority = crate::services::issue::derive_priority(level);
    let issue: Issue = sqlx::query_as(
        r#"
        INSERT INTO issues (
            id, project_id, digest_order, first_seen, last_seen,
            digested_event_count, stored_event_count,
            calculated_type, calculated_value, "transaction",
            last_frame_filename, last_frame_module, last_frame_function,
            level, platform, status, substatus, priority, culprit, logger,
            first_release, last_release
        )
        VALUES (
            $1, $2, $3, $4, $4, 1, 1, $5, $6, $7, $8, $9, $10, $11, $12,
            'unresolved', 'new', $13, $14, $15, $16, $16
        )
        RETURNING *
        "#,
    )
    .bind(issue_id)
    .bind(project_id)
    .bind(digest_order)
    .bind(timestamp)
    .bind(&denormalized.calculated_type)
    .bind(&denormalized.calculated_value)
    .bind(&denormalized.transaction)
    .bind(&denormalized.last_frame_filename)
    .bind(&denormalized.last_frame_module)
    .bind(&denormalized.last_frame_function)
    .bind(level)
    .bind(platform)
    .bind(priority)
    .bind(&denormalized.culprit)
    .bind(&denormalized.logger)
    .bind(&denormalized.release)
    .fetch_one(&mut **tx)
    .await?;

    // Create new grouping
    let grouping: Grouping = sqlx::query_as(
        r#"
        INSERT INTO groupings (project_id, issue_id, grouping_key, grouping_key_hash)
        VALUES ($1, $2, $3, $4)
        RETURNING *
        "#,
    )
    .bind(project_id)
    .bind(issue.id)
    .bind(grouping_key)
    .bind(grouping_key_hash)
    .fetch_one(&mut **tx)
    .await?;

    Ok((issue, grouping, true, false))
}

fn alert_type_for_lifecycle(created: bool, regressed: bool) -> Option<AlertType> {
    if created {
        Some(AlertType::NewIssue)
    } else if regressed {
        Some(AlertType::Regression)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_real_incoming_title_always_wins() {
        assert_eq!(
            updated_title(("Log Message", "old"), ("TypeError", "boom")),
            ("TypeError", "boom")
        );
        assert_eq!(
            updated_title(("Unknown", ""), ("TypeError", "boom")),
            ("TypeError", "boom")
        );
    }

    #[test]
    fn a_placeholder_incoming_title_loses_to_a_real_one() {
        assert_eq!(
            updated_title(("TypeError", "boom"), ("Unknown", "")),
            ("TypeError", "boom")
        );
        assert_eq!(
            updated_title(("TypeError", "boom"), ("Error", "")),
            ("TypeError", "boom")
        );
    }

    #[test]
    fn a_placeholder_replaces_another_placeholder() {
        assert_eq!(updated_title(("Unknown", ""), ("Error", "")), ("Error", ""));
    }

    #[test]
    fn an_error_carrying_a_value_is_a_real_title() {
        assert_eq!(
            updated_title(("TypeError", "boom"), ("Error", "connection reset")),
            ("Error", "connection reset")
        );
    }

    #[test]
    fn busy_codes_are_recognized_and_others_are_not() {
        for code in ["5", "261", "517", "773"] {
            assert!(is_sqlite_busy_code(code), "{code} must classify as busy");
        }
        assert!(
            !is_sqlite_busy_code("2067"),
            "a unique-constraint code is not a busy error"
        );
    }

    #[test]
    fn persists_only_alerting_lifecycles() {
        assert_eq!(
            alert_type_for_lifecycle(true, false),
            Some(AlertType::NewIssue)
        );
        assert_eq!(
            alert_type_for_lifecycle(false, true),
            Some(AlertType::Regression)
        );
        assert_eq!(alert_type_for_lifecycle(false, false), None);
    }

    #[test]
    fn retries_back_off_exponentially_from_50ms() {
        assert_eq!(sqlite_retry_delay(0), std::time::Duration::from_millis(50));
        assert_eq!(sqlite_retry_delay(1), std::time::Duration::from_millis(100));
    }

    #[test]
    fn only_lock_contention_is_retryable() {
        // A busy database is a wait; everything else is a decision the retry
        // loop cannot change by trying again.
        assert!(!is_retryable_write_contention(&AppError::Database(
            sqlx::Error::RowNotFound
        )));
        // Pool exhaustion is capacity, not lock contention: retrying it would
        // add three more waiters to the queue that caused it.
        assert!(!is_retryable_write_contention(&AppError::Database(
            sqlx::Error::PoolTimedOut
        )));
    }

    #[test]
    fn database_and_event_read_failures_are_retained_for_recovery() {
        assert!(should_retain_event(&AppError::Database(
            sqlx::Error::PoolTimedOut
        )));
        assert!(should_retain_event(&AppError::Internal(
            "Failed to read event file: connection reset".to_string(),
        )));
        assert!(should_retain_event(&AppError::Internal(
            "Invalid pending event record: truncated".to_string(),
        )));
        assert!(should_retain_event(&AppError::Internal(
            "temporary database connection failure".to_string(),
        )));
        assert!(!should_retain_event(&AppError::Internal(
            "Invalid event JSON: trailing data".to_string(),
        )));
        assert!(!should_retain_event(&AppError::Validation(
            "Invalid event JSON".to_string(),
        )));
    }
}

/// A title that says nothing about the event. Mirrors Sentry's
/// `PLACEHOLDER_EVENT_TITLES` in Rustrak's vocabulary: "Unknown" is what an
/// event with neither exception nor message produces, "Error" an exception
/// carrying neither type nor value.
fn is_placeholder_title(calculated_type: &str, calculated_value: &str) -> bool {
    calculated_value.trim().is_empty() && matches!(calculated_type, "Unknown" | "Error")
}

/// Picks the type and value an issue should carry after a new event. The
/// incoming pair wins unless it is a placeholder about to replace a real title.
fn updated_title<'a>(
    existing: (&'a str, &'a str),
    incoming: (&'a str, &'a str),
) -> (&'a str, &'a str) {
    if is_placeholder_title(incoming.0, incoming.1) && !is_placeholder_title(existing.0, existing.1)
    {
        existing
    } else {
        incoming
    }
}

/// A grouping joined to the issue columns the update path reads.
#[derive(sqlx::FromRow)]
struct GroupingWithIssue {
    #[sqlx(flatten)]
    grouping: Grouping,
    #[sqlx(flatten)]
    issue: PrevIssue,
}

/// The issue columns the update path reads before deciding what to write.
#[derive(sqlx::FromRow)]
struct PrevIssue {
    status: String,
    status_details: String,
    last_release: String,
    calculated_type: String,
    calculated_value: String,
    first_seen: chrono::DateTime<Utc>,
    last_seen: chrono::DateTime<Utc>,
    last_frame_filename: String,
    last_frame_module: String,
    last_frame_function: String,
    level: Option<String>,
}
