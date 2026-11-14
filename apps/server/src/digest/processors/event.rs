use super::batch::{commit_batch_once, try_batch, BatchOutcome, DbConn, EventOutcome};
use super::feedback::{self, Feedback, NewAttachment};
use super::{InFlightDigests, InFlightGuard};
use crate::config::SmtpSettings;
use crate::config::{Config, RateLimitConfig};
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::ingest::storage::{
    delete_event, delete_paths, pending_event_path, read_pending, PendingRead,
};
use crate::ingest::EventMetadata;
use crate::models::{AlertType, Issue, Product};
use crate::protocol::{clamp, is_stale, relay_level};
use crate::services::alert::AlertRuleCache;
use crate::services::event::EventRow;
use crate::services::sourcemap::SourceMapProvider;
use crate::services::{
    calculate_grouping_key, get_denormalized_fields, hash_grouping_key, AlertService,
    DenormalizedFields, EventService, IssueService, PendingQuota, ProjectService, RateLimitService,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

/// One spool file a digest owns: its identity and the in-flight guard that
/// keeps the recovery worker off it until the digest is over.
pub struct SpoolRef {
    pub project_id: i32,
    pub event_id: Uuid,
    /// The file's size, as listed.
    pub size: u64,
    pub owner: InFlightGuard,
}

/// An event ready for the write: everything [`ErrorProcessor::prepare`]
/// computed from its file, the parsed tree already dropped.
pub(crate) struct PreparedEvent {
    pub r: SpoolRef,
    /// Its share of `Config::digest_memory_bytes`, held until the event is finished.
    #[allow(dead_code)] // held, never read
    pub budget: tokio::sync::OwnedSemaphorePermit,
    pub metadata: EventMetadata,
    /// Loaded in prep: its quota limits, products, platform and alerts.
    pub project: crate::models::Project,
    pub grouping_key: String,
    pub grouping_key_hash: String,
    /// The key this event would have had before the grouping changes, so an
    /// issue created by an older release still claims its own events.
    pub legacy_grouping_key_hash: String,
    pub denormalized: DenormalizedFields,
    pub level: Option<&'static str>,
    /// Cut to its column, for a new issue.
    pub platform: Option<String>,
    pub row: EventRow,
    /// Cut to its column, for `issue_environments`.
    pub environment: String,
    pub facts_environment: Option<String>,
    /// The raw `platform`, for the project's platform inference.
    pub event_platform: Option<String>,
    pub feedback: Option<Feedback>,
    pub attachments: Vec<NewAttachment>,
    /// Bytes of `data` plus attachments.
    pub data_bytes: usize,
}

impl PreparedEvent {
    /// The fields an alert rule's filters read.
    fn facts(&self) -> crate::services::alert::EventFacts<'_> {
        crate::services::alert::EventFacts {
            level: self.level,
            environment: self.facts_environment.as_deref(),
        }
    }
}

/// What [`ErrorProcessor::prepare`] made of a spool file.
pub(crate) enum Prepared {
    Ready(Box<PreparedEvent>),
    /// Over before the write: missing, or dropped by the cached quota.
    Done(SpoolRef, Digested),
    /// Failed before the write.
    Kept(SpoolRef, AppError),
    /// A file this release cannot digest, left where it is.
    Skipped(SpoolRef, Skip),
}

/// Why a spool file was left in place.
#[derive(Debug)]
pub(crate) enum Skip {
    /// A record of a version this release does not read.
    OtherVersion,
    /// The read failed (permissions, a directory, I/O).
    CannotOpen(String),
    /// Not a readable record of its event.
    Unreadable(String),
}

impl Skip {
    /// The skip as a digest failure: transient, so the file is kept.
    fn into_error(self) -> AppError {
        AppError::Internal(match self {
            Skip::OtherVersion => "Invalid pending event record identity".to_string(),
            Skip::CannotOpen(why) => format!("Failed to read event file: {why}"),
            Skip::Unreadable(why) => format!("Invalid pending event record: {why}"),
        })
    }
}

/// How [`ErrorProcessor::commit`] answers a database that failed.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommitMode {
    /// The live committer: keep the batch and retry it whole, with a
    /// backoff, until the database is back or the process drains.
    Live,
    /// One attempt; the files stay for a replay.
    Once,
}

/// An event of a batch after its commit, for [`ErrorProcessor::finish`].
pub(crate) struct Committed {
    pub e: PreparedEvent,
    pub outcome: AppResult<EventOutcome>,
    /// The issue as this event left it, when it was inserted.
    pub issue: Option<Issue>,
}

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
    /// Accepted events whose digest is not over, for the ingest quota check.
    pending: PendingQuota,
    /// `INGEST_MAX_PENDING_EVENTS`, 0 for no limit.
    max_pending_events: i64,
    /// `INGEST_MIN_FREE_DISK_MB`, in bytes.
    min_free_disk_bytes: u64,
    /// Free bytes on the spool's volume at the last sample; `u64::MAX`
    /// until the first one, so nothing is shed on a guess.
    free_disk_bytes: std::sync::atomic::AtomicU64,
    /// Set by [`Self::start_draining`] at shutdown.
    draining: std::sync::atomic::AtomicBool,
    /// Set by [`Self::abandon`].
    abandoned: std::sync::atomic::AtomicBool,
    /// Keeps shedding to a log line every few seconds.
    shed_log: ShedLog,
    /// Failed digest attempts per spool file that only a retry could fix,
    /// for dead-lettering. In memory: a restart resets them, which only
    /// buys a file [`MAX_DIGEST_ATTEMPTS`] more tries.
    failed_attempts: std::sync::Mutex<std::collections::HashMap<(i32, Uuid), u32>>,
    /// Spool files recovery skips (unreadable, another version) that were
    /// already logged. ponytail: never pruned, one entry per such file on
    /// disk; prune on removal if those ever number in the thousands.
    reported_skips: std::sync::Mutex<std::collections::HashSet<(i32, Uuid)>>,
    /// The base of the issue links in alert notifications.
    dashboard_url: String,
    /// The SMTP fallback of email alert integrations.
    smtp: SmtpSettings,
    /// Which projects have alert rules, so most digests skip that query.
    alert_rules: Arc<AlertRuleCache>,
    /// SQLite has one writer at a time. Digests queue for it here, in process,
    /// rather than in SQLite's busy handler: that handler polls with sleeps
    /// that grow to 100ms, so with several digests waiting the lock sat idle
    /// between one commit and the next poll, and each waiter burned a
    /// connection thread sleeping.
    #[cfg(feature = "sqlite")]
    write_slot: Arc<tokio::sync::Mutex<()>>,
    /// The digest's own durable connection (`db::connect_committer`),
    /// opened on first use and again after it failed. Lock order: commit
    /// gate, then this, then `write_slot`.
    committer: tokio::sync::Mutex<Option<DbConn>>,
    /// Set by [`Self::close_committer`]: never opened again.
    committer_closed: std::sync::atomic::AtomicBool,
    /// `Config::digest_batch_max_events`.
    batch_max_events: usize,
    /// `Config::digest_prep_workers`.
    prep_workers: usize,
    /// The circuit breaker: see [`Breaker`].
    breaker: Breaker,
    /// One permit: every digest transaction (live, recovery, `digest()`)
    /// takes it for its batch. `Processors::processing_slot` is this.
    commit_gate: Arc<tokio::sync::Semaphore>,
    /// `Config::digest_memory_bytes` in KiB: the spool bytes being read, waiting for
    /// their commit or inside one.
    budget: Arc<tokio::sync::Semaphore>,
    budget_kib: u32,
    /// The events ingest accepted, waiting for the live digest.
    queue: crate::ingest::pipeline::DigestQueue,
    /// Wakes the recovery worker: the queue overflowed, or the pipeline
    /// dropped what it held.
    recovery_wake: tokio::sync::Notify,
    /// Batches being finished (alerts, file removal) at once.
    finishers: Arc<tokio::sync::Semaphore>,
    /// The runtime the digest pipeline runs on: the one that built the
    /// processor, which in the server is main's. Spawned from a request it
    /// would land on that request's HTTP worker runtime, which Actix
    /// replaces when its thread dies while the process lives on.
    runtime: Option<tokio::runtime::Handle>,
    /// Where digest transactions are counted.
    pub(crate) counters: &'static crate::telemetry::Counters,
    /// Set by [`Self::fail_after_commit_once_for_test`].
    #[cfg(feature = "test-seams")]
    fail_after_commit: std::sync::atomic::AtomicBool,
}

/// Events that failed alone, distinct, since a commit last stored a row. A
/// handful of them asks whether the database refuses everything (a broken
/// schema, a missing grant) or they are poison: when it refuses a blank copy
/// of one too (same issue, environment, release and level, no payload), the
/// digest halts instead of dead-lettering the backlog one event at a time.
#[derive(Default)]
struct Breaker {
    streak: std::sync::Mutex<std::collections::HashSet<(i32, Uuid)>>,
    halted: std::sync::atomic::AtomicBool,
    /// When a halted digest may next try one event.
    probe_at: std::sync::Mutex<Option<std::time::Instant>>,
}

/// Distinct events failing alone, with no row stored in between, that halt
/// the digest.
const BREAKER_EVENTS: usize = 5;

/// How often a halted digest tries one event.
const BREAKER_PROBE_EVERY: std::time::Duration = std::time::Duration::from_secs(60);

impl ErrorProcessor {
    pub fn new(config: &Config, sourcemap_provider: Arc<dyn SourceMapProvider>) -> Self {
        let ingest_dir = config.ingest_dir.clone();
        let budget_kib = (config.digest_memory_bytes / 1024).clamp(1, u32::MAX as u64 / 2) as u32;
        #[cfg(feature = "sqlite")]
        let write_slot = Arc::new(tokio::sync::Mutex::new(()));
        Self {
            committer: tokio::sync::Mutex::new(None),
            committer_closed: Default::default(),
            ingest_dir,
            rate_limit_config: config.rate_limit.clone(),
            sourcemap_provider,
            in_flight: Arc::new(InFlightDigests::default()),
            pending: PendingQuota::default(),
            max_pending_events: config.ingest_max_pending_events,
            min_free_disk_bytes: free_disk_floor(config.ingest_min_free_disk_bytes),
            free_disk_bytes: std::sync::atomic::AtomicU64::new(u64::MAX),
            draining: std::sync::atomic::AtomicBool::new(false),
            abandoned: std::sync::atomic::AtomicBool::new(false),
            shed_log: ShedLog::default(),
            failed_attempts: Default::default(),
            reported_skips: Default::default(),
            dashboard_url: config.dashboard_url(),
            smtp: config.smtp.clone(),
            alert_rules: Arc::default(),
            #[cfg(feature = "sqlite")]
            write_slot,
            batch_max_events: config.digest_batch_max_events,
            prep_workers: config.digest_prep_workers,
            breaker: Breaker::default(),
            commit_gate: Arc::new(tokio::sync::Semaphore::new(1)),
            budget: Arc::new(tokio::sync::Semaphore::new(budget_kib as usize)),
            budget_kib,
            queue: Default::default(),
            recovery_wake: tokio::sync::Notify::new(),
            finishers: Arc::new(tokio::sync::Semaphore::new(FINISHERS)),
            runtime: tokio::runtime::Handle::try_current().ok(),
            counters: crate::telemetry::Counters::global(),
            #[cfg(feature = "test-seams")]
            fail_after_commit: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// `Config::digest_batch_max_events`.
    pub(crate) fn batch_max_events(&self) -> usize {
        self.batch_max_events
    }

    /// `Config::digest_prep_workers`.
    pub(crate) fn prep_workers(&self) -> usize {
        self.prep_workers
    }

    /// The commit gate; see `Processors::processing_slot`.
    pub(crate) fn commit_gate(&self) -> &Arc<tokio::sync::Semaphore> {
        &self.commit_gate
    }

    /// The live digest's queue.
    pub(crate) fn queue(&self) -> &crate::ingest::pipeline::DigestQueue {
        &self.queue
    }

    /// What wakes the recovery worker before its timer.
    pub(crate) fn recovery_wake(&self) -> &tokio::sync::Notify {
        &self.recovery_wake
    }

    /// Where the live digest pipeline runs.
    pub(crate) fn runtime(&self) -> Option<&tokio::runtime::Handle> {
        self.runtime.as_ref()
    }

    /// `Config::digest_memory_bytes`, in bytes: what a recovery chunk keeps under half of.
    pub(crate) fn budget_bytes(&self) -> u64 {
        u64::from(self.budget_kib) * 1024
    }

    /// The budget a spool file of `size` bytes takes before it is read:
    /// one permit per KiB, at least one, at most the whole budget, so an
    /// event larger than the budget runs alone.
    pub(crate) async fn reserve(&self, size: u64) -> tokio::sync::OwnedSemaphorePermit {
        let kib = size.div_ceil(1024).clamp(1, u64::from(self.budget_kib)) as u32;
        Arc::clone(&self.budget)
            .acquire_many_owned(kib)
            .await
            .expect("the digest budget is never closed")
    }

    /// A permit to finish one committed batch.
    pub(crate) async fn finisher(&self) -> tokio::sync::OwnedSemaphorePermit {
        Arc::clone(&self.finishers)
            .acquire_owned()
            .await
            .expect("the finisher slots are never closed")
    }

    /// The live committer gave a batch up to the drain: from here on it
    /// lets go of everything else too, and the files are the next start's.
    fn abandon(&self) {
        self.abandoned
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Whether [`Self::abandon`] ran.
    pub(crate) fn is_abandoned(&self) -> bool {
        self.abandoned.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// While the breaker holds the digest halted, waits until it may try
    /// one event. `false` when the process started draining meanwhile.
    pub(crate) async fn wait_for_probe(&self) -> bool {
        loop {
            if self.is_draining() {
                return false;
            }
            if !self.is_halted() || self.take_probe() {
                return true;
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    }

    /// Whether the circuit breaker stopped the digest: distinct events kept
    /// failing alone with nothing stored in between.
    pub fn is_halted(&self) -> bool {
        self.breaker
            .halted
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Whether a halted digest may try one event now; if so the next try is
    /// a probe interval away. Logs that it is halted, once per interval.
    pub(crate) fn take_probe(&self) -> bool {
        let now = std::time::Instant::now();
        let mut probe_at = self
            .breaker
            .probe_at
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if probe_at.is_some_and(|at| now < at) {
            return false;
        }
        *probe_at = Some(now + BREAKER_PROBE_EVERY);
        log::error!("The digest is halted: every event it tried failed alone; trying one again");
        true
    }

    /// An event that failed on its own, outside an outage. When enough
    /// distinct ones did, asks the database whether it stores a blank copy
    /// of `e`: then they are poison; if it refuses that too, the digest halts.
    async fn note_isolated_failure(&self, pool: &DbPool, e: &PreparedEvent) {
        let suspect = {
            let mut streak = self
                .breaker
                .streak
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            streak.insert((e.r.project_id, e.r.event_id));
            streak.len() >= BREAKER_EVENTS
        };
        if !suspect {
            return;
        }
        match self.stores_a_blank_event(pool, e).await {
            Some(true) => self.note_stored(),
            // The probe proved nothing; the next isolated failure asks again.
            None => {}
            Some(false) => self.halt(),
        }
    }

    /// Stops the digest: see [`Breaker`].
    fn halt(&self) {
        if !self
            .breaker
            .halted
            .swap(true, std::sync::atomic::Ordering::Relaxed)
        {
            *self
                .breaker
                .probe_at
                .lock()
                .unwrap_or_else(|p| p.into_inner()) =
                Some(std::time::Instant::now() + BREAKER_PROBE_EVERY);
            log::error!(
                "The digest is halted: {BREAKER_EVENTS} distinct events failed alone and the database \
                 refuses a blank copy of the last one too. Nothing is dead-lettered; one event is tried every {}s",
                BREAKER_PROBE_EVERY.as_secs()
            );
        }
    }

    /// Whether the database stores, in a transaction it rolls back, an event
    /// that goes where `like` goes (its project, issue, environment, release
    /// and level, so the same rows are written) with nothing else in it.
    /// `None` when that says nothing: the database failed as in an outage,
    /// or the event ended before its row.
    async fn stores_a_blank_event(&self, pool: &DbPool, like: &PreparedEvent) -> Option<bool> {
        let (project_id, event_id) = (like.r.project_id, Uuid::new_v4());
        let mut blank = serde_json::json!({
            "release": like.denormalized.release,
            "environment": like.environment,
        });
        if let Some(level) = like.level {
            blank["level"] = level.into();
        }
        let denormalized = get_denormalized_fields(&blank);
        let canary = PreparedEvent {
            r: SpoolRef {
                project_id,
                event_id,
                size: 0,
                owner: self.in_flight.register(project_id, event_id),
            },
            // Its own permit: the digest budget may be all taken.
            budget: Arc::new(tokio::sync::Semaphore::new(1))
                .try_acquire_owned()
                .expect("a new semaphore has its permit"),
            metadata: EventMetadata {
                event_id: event_id.simple().to_string(),
                ..like.metadata.clone()
            },
            project: like.project.clone(),
            legacy_grouping_key_hash: like.legacy_grouping_key_hash.clone(),
            grouping_key_hash: like.grouping_key_hash.clone(),
            grouping_key: like.grouping_key.clone(),
            row: EventRow::new(
                &blank,
                like.metadata.ingested_at,
                &denormalized,
                None,
                event_id,
                project_id,
            ),
            denormalized,
            level: like.level,
            platform: None,
            environment: like.environment.clone(),
            facts_environment: None,
            event_platform: None,
            feedback: None,
            attachments: Vec::new(),
            data_bytes: 0,
        };
        let mut committer = self.committer.lock().await;
        let result = match self.committer_conn(pool, &mut committer).await {
            Ok(conn) => {
                #[cfg(feature = "sqlite")]
                let _turn = self.write_slot.lock().await;
                try_batch(conn, std::slice::from_ref(&canary), &self.rate_limit_config).await
            }
            Err(e) => Err(e),
        };
        if let Err(e) = &result {
            drop_if_broken(&mut committer, e);
        }
        probe_verdict(&result)
    }

    /// A commit stored a row: whatever failed before was the events'.
    fn note_stored(&self) {
        self.breaker
            .streak
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
        if self
            .breaker
            .halted
            .swap(false, std::sync::atomic::Ordering::Relaxed)
        {
            log::info!("The digest stores events again");
        }
    }

    /// The next recovery pass probes at once instead of after the interval.
    #[cfg(feature = "test-seams")]
    #[doc(hidden)]
    pub fn probe_now_for_test(&self) {
        *self.breaker.probe_at.lock().unwrap() = None;
    }

    /// The failed attempts counted toward dead-lettering this spool file.
    #[cfg(feature = "test-seams")]
    #[doc(hidden)]
    pub fn failed_attempts_for_test(&self, project_id: i32, event_id: Uuid) -> u32 {
        self.failed_attempts
            .lock()
            .unwrap()
            .get(&(project_id, event_id))
            .copied()
            .unwrap_or(0)
    }

    /// The next commit lands and then reports a lost connection, as when
    /// the connection drops during `COMMIT` after the database applied it.
    #[cfg(feature = "test-seams")]
    #[doc(hidden)]
    pub fn fail_after_commit_once_for_test(&self) {
        self.fail_after_commit
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// [`Self::sync_barrier`].
    #[cfg(all(feature = "test-seams", feature = "sqlite"))]
    #[doc(hidden)]
    pub async fn sync_barrier_for_test(&self, pool: &DbPool) -> AppResult<()> {
        self.sync_barrier(pool).await
    }

    /// What the alert-rule handlers invalidate when they write a rule.
    pub fn alert_rules(&self) -> &AlertRuleCache {
        &self.alert_rules
    }

    /// [`AlertService::trigger_event_alert`], skipped when the project has
    /// no enabled rule for `alert_type`.
    #[allow(clippy::too_many_arguments)]
    async fn trigger_event_alert(
        &self,
        pool: &DbPool,
        project: &crate::models::Project,
        issue: &Issue,
        alert_type: AlertType,
        event_id: Uuid,
        facts: crate::services::alert::EventFacts<'_>,
        ctx: &mut FinishCtx,
    ) -> AppResult<()> {
        if !self
            .alert_rules
            .has_enabled(pool, project.id, alert_type)
            .await?
        {
            return Ok(());
        }
        ctx.alerted = true;
        // Its writes go through the pool: in turn with the committer's.
        #[cfg(feature = "sqlite")]
        let _turn = self.write_turn().await;
        AlertService::trigger_event_alert(
            pool,
            project,
            issue,
            alert_type,
            event_id,
            facts,
            &self.dashboard_url,
            &self.smtp,
        )
        .await
    }

    pub(crate) fn ingest_dir(&self) -> &Path {
        &self.ingest_dir
    }

    /// Waits for this process's turn at SQLite's one writer, in the queue
    /// the digests take turns in. Every write on the ingest path takes it:
    /// left to SQLite's busy handler, a writer lost each race to the digests
    /// handing the lock to each other and failed after `busy_timeout`.
    #[cfg(feature = "sqlite")]
    pub async fn write_turn(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.write_slot.lock().await
    }

    /// The digests this processor's process currently owns.
    pub fn in_flight(&self) -> &Arc<InFlightDigests> {
        &self.in_flight
    }

    /// The accepted events whose digest is not over.
    pub fn pending(&self) -> &PendingQuota {
        &self.pending
    }

    /// Whether ingest may spool another event, given `pending` events already
    /// waiting: 503 while the digest is too far behind or the spool's disk
    /// is nearly full, so an SDK backs off rather than the disk filling.
    /// Two loads, no I/O: the free space is sampled on a timer.
    pub fn admit(&self, pending: i64, counters: &crate::telemetry::Counters) -> AppResult<()> {
        use crate::telemetry::Shed;
        use std::sync::atomic::Ordering;
        if self.draining.load(Ordering::Relaxed) {
            return Err(AppError::Unavailable(
                "the server is shutting down; retry later".to_string(),
            ));
        }
        if self.max_pending_events > 0 && pending >= self.max_pending_events {
            counters.ingest_shed(Shed::Backlog);
            if let Some(shed) = self.shed_log.note(std::time::Instant::now()) {
                log::warn!(
                    "Shedding ingest: {pending} events wait for the digest ({shed} requests refused since the last line)"
                );
            }
            return Err(AppError::Unavailable(
                "the digest is behind; retry later".to_string(),
            ));
        }
        let free = self.free_disk_bytes.load(Ordering::Relaxed);
        if free < self.min_free_disk_bytes {
            counters.ingest_shed(Shed::Disk);
            if let Some(shed) = self.shed_log.note(std::time::Instant::now()) {
                log::warn!(
                    "Shedding ingest: {} MiB free on the spool's volume ({shed} requests refused since the last line)",
                    free / (1024 * 1024)
                );
            }
            return Err(AppError::Unavailable(
                "the disk is nearly full; retry later".to_string(),
            ));
        }
        Ok(())
    }

    /// From now on [`Self::admit`] refuses everything: the process is
    /// shutting down and new events would only add to what it must finish.
    /// The live digest goes on with what it already queued while the
    /// commit gate is free; once a batch had to wait for the gate, it lets
    /// go of the rest (see [`Self::abandon`]).
    pub fn start_draining(&self) {
        self.draining
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// `true` the first time recovery skips this spool file, so it is
    /// logged once rather than on every pass.
    pub fn first_skip(&self, project_id: i32, event_id: Uuid) -> bool {
        self.reported_skips
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert((project_id, event_id))
    }

    /// Whether [`Self::start_draining`] ran: a digest still waiting for its
    /// slot then leaves its event in the spool for the next start.
    pub fn is_draining(&self) -> bool {
        self.draining.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Reads the free space on the spool's volume for [`Self::admit`]. A
    /// failed read keeps the last value.
    pub async fn sample_free_disk(&self) {
        let dir = self.ingest_dir.clone();
        match tokio::task::spawn_blocking(move || free_bytes(&dir)).await {
            Ok(Ok(free)) => {
                let first = self
                    .free_disk_bytes
                    .swap(free, std::sync::atomic::Ordering::Relaxed)
                    == u64::MAX;
                if first && free < self.min_free_disk_bytes {
                    log::warn!(
                        "The spool's volume has {} MiB free, under the {} MiB floor (INGEST_MIN_FREE_DISK_MB, plus room for SQLite's log): ingest refuses events until there is more",
                        free / (1024 * 1024),
                        self.min_free_disk_bytes / (1024 * 1024)
                    );
                }
            }
            Ok(Err(e)) => log::warn!("Could not read the spool volume's free space: {e}"),
            Err(e) => log::warn!("Could not read the spool volume's free space: {e}"),
        }
    }

    /// Digests one stored event: [`Self::prepare`], [`Self::commit`] and
    /// [`Self::finish`], inline. A failure worth replaying keeps its file
    /// for the recovery worker; any other drops it.
    ///
    /// A kept file whose digest fails the same way [`MAX_DIGEST_ATTEMPTS`]
    /// times is moved to the spool's `dead/` and reported as
    /// [`Digested::DeadLettered`]. Transient failures never count.
    pub async fn digest(&self, pool: &DbPool, metadata: &EventMetadata) -> AppResult<Digested> {
        let Ok(event_id) = Uuid::parse_str(&metadata.event_id) else {
            // No file can hold it: an invalid event, dropped.
            return Err(AppError::Validation("Invalid event_id format".to_string()));
        };
        let r = SpoolRef {
            project_id: metadata.project_id,
            event_id,
            size: 0,
            owner: self.in_flight.register(metadata.project_id, event_id),
        };
        let budget = self.reserve(0).await;
        // ponytail: reads the file on the async thread. Only monitor
        // failures (a small file just written) come this way; give it a
        // blocking thread if a large event ever does.
        let prepared = self.prepare(pool, r, budget, Some(metadata)).await;
        self.run_prepared(pool, prepared).await
    }

    /// Commits and finishes a prepared event on its own, or settles one
    /// that ended before its write.
    pub(crate) async fn run_prepared(
        &self,
        pool: &DbPool,
        prepared: Prepared,
    ) -> AppResult<Digested> {
        match prepared {
            Prepared::Ready(e) => {
                let committed = self.commit(pool, vec![*e], CommitMode::Once).await;
                self.finish(pool, committed)
                    .await
                    .pop()
                    .unwrap_or_else(|| Err(AppError::Internal("no digest result".to_string())))
            }
            Prepared::Done(r, digested) => {
                self.settle(r.project_id, r.event_id, Ok(digested)).await
            }
            Prepared::Kept(r, e) => self.settle(r.project_id, r.event_id, Err(e)).await,
            Prepared::Skipped(r, skip) => {
                self.settle(r.project_id, r.event_id, Err(skip.into_error()))
                    .await
            }
        }
    }

    /// What recovery does with a file it cannot digest: one of another
    /// version or that cannot be opened stays where it is and stops being
    /// pending (counted, it could keep ingest shedding for as long as it
    /// sits there), logged once; one that is not a readable record goes to
    /// `dead/`.
    pub(crate) async fn settle_skipped(&self, r: SpoolRef, skip: Skip) -> AppResult<Digested> {
        let result = self.settle_skipped_inner(r, skip).await;
        self.count(&result);
        result
    }

    async fn settle_skipped_inner(&self, r: SpoolRef, skip: Skip) -> AppResult<Digested> {
        let (project_id, event_id) = (r.project_id, r.event_id);
        match skip {
            Skip::Unreadable(why) => {
                log::warn!("Malformed pending event {event_id} of project {project_id}: {why}");
                let raw_event_id = event_id.simple().to_string();
                if self
                    .dead_letter(project_id, &raw_event_id, &"not a readable pending record")
                    .await
                {
                    return Ok(Digested::DeadLettered);
                }
            }
            Skip::OtherVersion => {
                self.pending.remove(project_id, event_id);
                if self.first_skip(project_id, event_id) {
                    log::warn!(
                        "Spool file of event {event_id} (project {project_id}) holds a record version this release does not read; left in place"
                    );
                }
            }
            Skip::CannotOpen(why) => {
                self.pending.remove(project_id, event_id);
                if self.first_skip(project_id, event_id) {
                    log::warn!(
                        "Spool file of event {event_id} (project {project_id}) cannot be read ({why}); left in place"
                    );
                }
            }
        }
        Ok(Digested::Skipped)
    }

    /// Where every digest outcome ends: a failure worth replaying keeps the
    /// file, counted toward dead-lettering when only a retry could fix it;
    /// any other failure drops the file; whatever is over stops being
    /// pending.
    async fn settle(
        &self,
        project_id: i32,
        event_id: Uuid,
        result: AppResult<Digested>,
    ) -> AppResult<Digested> {
        let result = self.settle_inner(project_id, event_id, result).await;
        self.count(&result);
        result
    }

    /// Every digest outcome is counted here, whichever path settled it.
    fn count(&self, result: &AppResult<Digested>) {
        let counters = self.counters;
        match result {
            Ok(Digested::Stored) => counters.digest_ok(),
            Ok(Digested::RateLimited) => counters.digest_rate_limited(),
            Ok(Digested::DeadLettered) => {
                counters.digest_failed();
                counters.dead_lettered();
            }
            Ok(Digested::Skipped) => {}
            Ok(Digested::Missing) => counters.digest_failed(),
            Err(e) => {
                counters.digest_failed();
                if let Some(code) = e.db_code() {
                    counters.db_error(&code);
                }
            }
        }
    }

    async fn settle_inner(
        &self,
        project_id: i32,
        event_id: Uuid,
        result: AppResult<Digested>,
    ) -> AppResult<Digested> {
        let key = (project_id, event_id);
        // Owned until settled: recovery must not take a file being moved
        // to `dead/` or deleted.
        debug_assert!(
            self.in_flight.contains(project_id, event_id),
            "event {event_id} settled without its owner"
        );
        let raw_event_id = event_id.simple().to_string();
        if let Err(e) = &result {
            if should_retain_event(e) {
                // Still queued, so still pending: the replay ends it. While
                // the breaker holds the digest halted nothing is counted:
                // the database refuses every event, not this one.
                if e.is_transient() || self.is_halted() {
                    return result;
                }
                let attempts = {
                    let mut failed = self
                        .failed_attempts
                        .lock()
                        .unwrap_or_else(|p| p.into_inner());
                    let attempts = failed.entry(key).or_default();
                    *attempts += 1;
                    *attempts
                };
                if attempts < MAX_DIGEST_ATTEMPTS
                    || !self.dead_letter(project_id, &raw_event_id, e).await
                {
                    return result;
                }
                return Ok(Digested::DeadLettered);
            }
            if let Err(e) = delete_event(&self.ingest_dir, project_id, &raw_event_id).await {
                log::warn!("Failed to clean up orphaned event file {raw_event_id}: {e:?}");
            }
        }
        self.failed_attempts
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&key);
        self.pending.remove(project_id, event_id);
        result
    }

    /// Everything a digest does before its write: the project row, the
    /// event read and decoded, its frames processed, its grouping keys and
    /// the `events` row it will become. The parsed tree is dropped here.
    ///
    /// `given` is the metadata the caller already holds (the live path, a
    /// test digesting at a set time); without it, the file's own is read.
    ///
    /// It reads the spool file blocking, so the pipeline runs it on a
    /// blocking thread.
    pub(crate) async fn prepare(
        &self,
        pool: &DbPool,
        r: SpoolRef,
        budget: tokio::sync::OwnedSemaphorePermit,
        given: Option<&EventMetadata>,
    ) -> Prepared {
        macro_rules! check {
            ($result:expr) => {
                match $result {
                    Ok(value) => value,
                    Err(e) => return Prepared::Kept(r, e),
                }
            };
        }
        let (project_id, event_id) = (r.project_id, r.event_id);
        // The project row loaded here is the one the quota check and the
        // platform inference read too.
        let (project, installation_quota) =
            check!(ProjectService::get_with_installation_quota(pool, project_id).await);
        let (stored, event_bytes) = match read_pending(&self.ingest_dir, project_id, event_id) {
            PendingRead::Event(stored, bytes) => (stored, bytes),
            PendingRead::Missing => return Prepared::Done(r, Digested::Missing),
            PendingRead::OtherVersion => return Prepared::Skipped(r, Skip::OtherVersion),
            PendingRead::CannotOpen(why) => return Prepared::Skipped(r, Skip::CannotOpen(why)),
            PendingRead::Unreadable(why) => return Prepared::Skipped(r, Skip::Unreadable(why)),
        };
        let metadata = given.cloned().unwrap_or(stored);

        // Parse event from filesystem. Ingest strips NUL escapes already;
        // this covers files queued by a release that did not.
        let mut event_data: serde_json::Value = check!(serde_json::from_slice(
            &crate::ingest::parser::strip_nul_escapes(&event_bytes)
        )
        .map_err(|e| AppError::Validation(format!("Invalid event JSON: {e}"))));

        // Relay's `TimestampProcessor`: an event from before 1970 is invalid,
        // and so is each breadcrumb from before 1970.
        if is_stale(&event_data, "timestamp") || is_stale(&event_data, "start_timestamp") {
            return Prepared::Kept(
                r,
                AppError::Validation("event timestamp is before 1970".to_string()),
            );
        }
        drop_stale_breadcrumbs(&mut event_data);
        // Taken out of every event so they are never stored in its data.
        let attachments = feedback::take_attachments(&mut event_data);
        // A feedback Sentry would filter is dropped here, like an invalid event.
        let feedback = check!(Feedback::from_event(&event_data));
        // Checked here and not at ingest: a feedback also arrives as an event
        // of type `feedback` and from the user-report endpoint, and every one
        // of them is digested through this line.
        if feedback.is_some() && !project.keeps(Product::Feedback) {
            return Prepared::Kept(
                r,
                AppError::Validation("feedback dropped: turned off for this project".to_string()),
            );
        }
        // A feedback occurrence defaults to `info`, and the event, its issue
        // and its alert rules all read that level from here.
        if feedback.is_some()
            && event_data
                .get("level")
                .and_then(|v| v.as_str())
                .and_then(relay_level)
                .is_none()
        {
            event_data["level"] = serde_json::json!("info");
        }
        // Rewrite stack frames using source maps (non-fatal)
        if let Err(e) = crate::services::sourcemap::rewrite_frames(
            self.sourcemap_provider.as_ref(),
            project_id,
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
        // Deobfuscate JVM frames with ProGuard mappings and source
        // bundles, before grouping as Sentry does (non-fatal).
        crate::services::jvm::process_event(
            self.sourcemap_provider.as_ref(),
            project_id,
            &mut event_data,
        )
        .await;
        // Symbolicate native (Apple) frames with dSYMs, likewise.
        crate::services::native::process_event(
            self.sourcemap_provider.as_ref(),
            project_id,
            &mut event_data,
        )
        .await;

        // Trim oversized fields (deep context windows, frame vars, huge
        // breadcrumb trails) for events whose raw payload came in above the
        // original per-item budget — see services::event_trim. Only runs for
        // the (rare) events that needed the relaxed ingest ceiling, so this
        // is a no-op for the common case.
        // Measured without the attachments it carried, as Relay measures it.
        let event_size = if attachments.is_empty() {
            event_bytes.len()
        } else {
            serde_json::to_vec(&event_data).map_or(event_bytes.len(), |bytes| bytes.len())
        };
        if event_size > crate::ingest::parser::TARGET_EVENT_SIZE {
            crate::services::trim_oversized_event(&mut event_data);
        }
        // The raw file contents are no longer needed once the tree is parsed
        // and the size check ran; drop them so the digest's grouping phase and
        // DB writes do not carry a second copy of the payload in memory.
        drop(event_bytes);

        // The cached check, as Relay's `CombinedRateLimiter` runs first: the
        // counters this project row was loaded with already show whether the
        // window is full. Dropping here skips the write lock and the rows a
        // rollback would undo; the quota in the transaction stays the one
        // that decides a race. An event already stored is a replay, not a
        // new event: it goes on to the transaction, which finds it.
        if RateLimitService::check_quota(
            &installation_quota,
            &project,
            &self.rate_limit_config,
            Default::default(),
        )
        .is_some()
            && !check!(EventService::exists(pool, project_id, event_id).await)
        {
            return match self.drop_rate_limited(pool, project_id, event_id).await {
                Ok(digested) => Prepared::Done(r, digested),
                Err(e) => Prepared::Kept(r, e),
            };
        }

        // Calculate grouping key and hash. Every feedback is its own issue:
        // Sentry gives each one a random fingerprint.
        let grouping_key = match &feedback {
            Some(_) => format!("feedback:{}", event_id.simple()),
            None => calculate_grouping_key(&event_data),
        };
        let grouping_key_hash = hash_grouping_key(&grouping_key);
        let legacy_grouping_key_hash = match &feedback {
            Some(_) => grouping_key_hash.clone(),
            None => hash_grouping_key(&crate::services::calculate_grouping_key_v1(&event_data)),
        };

        let mut denormalized = get_denormalized_fields(&event_data);
        if let Some(feedback) = &feedback {
            // Sentry's occurrence: title "User Feedback", the message as its
            // subtitle, culprit "user".
            denormalized.calculated_type = "User Feedback".to_string();
            denormalized.calculated_value = feedback.message.clone();
            denormalized.culprit = "user".to_string();
        }
        let facts = event_facts(&event_data);
        let level = event_data
            .get("level")
            .and_then(|v| v.as_str())
            .and_then(relay_level);
        let row = EventRow::new(
            &event_data,
            metadata.ingested_at,
            &denormalized,
            metadata.remote_addr.as_deref(),
            event_id,
            project_id,
        );
        let data_bytes = row.data.len() + attachments.iter().map(|a| a.data.len()).sum::<usize>();
        Prepared::Ready(Box::new(PreparedEvent {
            budget,
            metadata,
            project,
            grouping_key,
            grouping_key_hash,
            legacy_grouping_key_hash,
            level,
            // Cut to the VARCHAR(64) column, which Postgres enforces.
            platform: event_data
                .get("platform")
                .and_then(|v| v.as_str())
                .map(|s| clamp(s, 64)),
            environment: clamp(
                event_data
                    .get("environment")
                    .and_then(|v| v.as_str())
                    .unwrap_or(""),
                64,
            ),
            facts_environment: facts.environment.map(str::to_string),
            event_platform: event_data
                .get("platform")
                .and_then(|p| p.as_str())
                .map(str::to_string),
            denormalized,
            row,
            feedback,
            attachments,
            data_bytes,
            r,
        }))
    }

    /// Writes `batch`, as one transaction when it can. A failure of the
    /// database itself (an outage, a busy SQLite) fails every event, kept
    /// for a replay. Any other failure splits the batch in halves, order
    /// kept, until the event that fails is alone: its neighbours are stored,
    /// and it fails on its own, toward the circuit breaker.
    ///
    /// Each attempt holds the commit gate. In [`CommitMode::Live`] the events
    /// a failed database left are retried whole, with a backoff, for as long
    /// as it takes; a drain drops them before their next attempt, and their
    /// files are the next start's.
    pub(crate) async fn commit(
        &self,
        pool: &DbPool,
        batch: Vec<PreparedEvent>,
        mode: CommitMode,
    ) -> Vec<Committed> {
        let mut done = Vec::with_capacity(batch.len());
        let mut todo = batch;
        for attempt in 0.. {
            // Once the process drains, a batch that had to wait for the
            // gate is dropped; its files are the next start's.
            let gate = match self.commit_gate.try_acquire() {
                Ok(gate) => gate,
                Err(_) => {
                    let gate = self
                        .commit_gate
                        .acquire()
                        .await
                        .expect("the commit gate is never closed");
                    if mode == CommitMode::Live && self.is_draining() {
                        self.abandon();
                        return done;
                    }
                    gate
                }
            };
            let from = done.len();
            self.commit_split(pool, todo, &mut done).await;
            drop(gate);
            if mode == CommitMode::Once {
                break;
            }
            let mut retry = Vec::new();
            for c in done.split_off(from) {
                match &c.outcome {
                    Err(e) if is_outage(e) => {
                        if retry.is_empty() && attempt % 20 == 0 {
                            log::warn!("The database failed a digest batch; retrying it: {e:?}");
                        }
                        retry.push(c.e);
                    }
                    _ => done.push(c),
                }
            }
            if retry.is_empty() {
                break;
            }
            if self.is_draining() {
                self.abandon();
                break;
            }
            todo = retry;
            tokio::time::sleep(outage_backoff(attempt)).await;
        }
        done
    }

    fn commit_split<'a>(
        &'a self,
        pool: &'a DbPool,
        mut batch: Vec<PreparedEvent>,
        done: &'a mut Vec<Committed>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            match self.commit_once(pool, &batch).await {
                Ok(BatchOutcome { events, issues }) => {
                    if events
                        .iter()
                        .any(|o| matches!(o, EventOutcome::Inserted { .. }))
                    {
                        self.note_stored();
                    }
                    // The quota's stored counters hold these from here on:
                    // still pending, the ingest check would count them twice.
                    for e in &batch {
                        self.pending.remove(e.r.project_id, e.r.event_id);
                    }
                    done.extend(batch.into_iter().zip(events).map(|(e, outcome)| {
                        // The issue as this event left it: its own count.
                        let issue = match &outcome {
                            EventOutcome::Inserted {
                                issue_id,
                                issue_event_count,
                                ..
                            } => issues.get(issue_id).cloned().map(|mut issue| {
                                if let Some(count) = issue_event_count {
                                    issue.digested_event_count = *count;
                                }
                                issue
                            }),
                            _ => None,
                        };
                        Committed {
                            e,
                            outcome: Ok(outcome),
                            issue,
                        }
                    }));
                }
                Err(error) if batch.len() > 1 && !is_outage(&error) => {
                    // No savepoints: hundreds of them overflow Postgres's
                    // subtransaction cache. 2·⌈log₂ n⌉ + 1 transactions at most.
                    let right = batch.split_off(batch.len() / 2);
                    self.commit_split(pool, batch, done).await;
                    self.commit_split(pool, right, done).await;
                }
                Err(error) => {
                    if !is_outage(&error) {
                        if let Some(e) = batch.first() {
                            self.note_isolated_failure(pool, e).await;
                        }
                    }
                    let mut error = Some(error);
                    done.extend(batch.into_iter().map(|e| Committed {
                        e,
                        outcome: Err(error.take().unwrap_or_else(|| {
                            AppError::Unavailable("the database failed the batch".to_string())
                        })),
                        issue: None,
                    }));
                }
            }
        })
    }

    /// One transaction for `batch`. On SQLite the digests take turns at the
    /// write here, in process (see `write_slot`). One attempt: a busy
    /// database fails the batch, which a replay retries.
    async fn commit_once(&self, pool: &DbPool, batch: &[PreparedEvent]) -> AppResult<BatchOutcome> {
        self.counters.digest_batch(batch.len());
        #[cfg(feature = "sqlite")]
        let (started, pushes) = (std::time::Instant::now(), self.queue.pushes());
        let mut committer = self.committer.lock().await;
        let conn = self.committer_conn(pool, &mut committer).await?;
        #[cfg(feature = "sqlite")]
        let pages = checkpoint_pages(
            self.pending.counts(0).installation as usize,
            self.batch_max_events,
        );
        let result = async {
            #[cfg(feature = "sqlite")]
            {
                sqlx::query(sqlx::AssertSqlSafe(format!(
                    "PRAGMA wal_autocheckpoint = {pages}"
                )))
                .execute(&mut *conn)
                .await?;
            }
            #[cfg(feature = "sqlite")]
            let _turn = self.write_slot.lock().await;
            commit_batch_once(conn, batch, &self.rate_limit_config).await
        }
        .await;
        if let Err(e) = &result {
            drop_if_broken(&mut committer, e);
        }
        // Outside the write turn: the checkpoint waits on SQLite's own lock.
        #[cfg(feature = "sqlite")]
        if result.is_ok() && pages == crate::db::CAUGHT_UP_CHECKPOINT_PAGES {
            if let Some(conn) = committer.as_mut() {
                if let Err(e) = crate::db::checkpoint_caught_up(conn).await {
                    log::warn!("The digest's checkpoint failed; it opens a new connection: {e:?}");
                    *committer = None;
                }
            }
        }
        #[cfg(feature = "test-seams")]
        if result.is_ok()
            && self
                .fail_after_commit
                .swap(false, std::sync::atomic::Ordering::Relaxed)
        {
            *committer = None;
            return Err(AppError::Database(sqlx::Error::Io(std::io::Error::other(
                "injected: the connection dropped during COMMIT",
            ))));
        }
        #[cfg(feature = "sqlite")]
        {
            let rest = yield_to_ingest(
                started.elapsed(),
                self.queue.pushes() != pushes,
                self.queue.len(),
                self.batch_max_events,
            );
            drop(committer);
            if !rest.is_zero() && !self.is_draining() {
                tokio::time::sleep(rest).await;
            }
        }
        result
    }

    /// Makes every commit before it, and the pool's own writes, survive a
    /// power loss: one commit on the `synchronous=FULL` connection fsyncs
    /// the write-ahead log, which holds them all. Alert rows commit through
    /// the pool (`NORMAL`), so a batch whose alerts ran calls this before
    /// its files go.
    #[cfg(feature = "sqlite")]
    async fn sync_barrier(&self, pool: &DbPool) -> AppResult<()> {
        let mut committer = self.committer.lock().await;
        let conn = self.committer_conn(pool, &mut committer).await?;
        let result: AppResult<()> = async {
            let _turn = self.write_slot.lock().await;
            let mut tx = sqlx::Connection::begin_with(&mut *conn, "BEGIN IMMEDIATE").await?;
            // A commit that changed no page writes no frame and syncs
            // nothing (SQLite skips an update that leaves the bytes as they
            // were), so the page is changed and changed back.
            for step in [
                "UPDATE installation SET digested_event_count = digested_event_count + 1 WHERE id = 1",
                "UPDATE installation SET digested_event_count = digested_event_count - 1 WHERE id = 1",
            ] {
                sqlx::query(step).execute(&mut *tx).await?;
            }
            tx.commit().await?;
            Ok(())
        }
        .await;
        if let Err(e) = &result {
            drop_if_broken(&mut committer, e);
        }
        result
    }

    /// A `PASSIVE` checkpoint on the digest's connection, once the digest is
    /// caught up with ingest; while it is behind, its batches' own deferred
    /// threshold applies (`checkpoint_pages`). Pool connections never
    /// checkpoint, so this is what checkpoints the writes that never reach
    /// the digest (transactions, logs, spans, sessions, alert rows), and the
    /// log a burst deferred, right after the burst. On a timer, never in a
    /// request; `PASSIVE` waits for no reader and no writer.
    #[cfg(feature = "sqlite")]
    pub async fn checkpoint_when_caught_up(&self, pool: &DbPool) -> AppResult<()> {
        let pages = checkpoint_pages(
            self.pending.counts(0).installation as usize,
            self.batch_max_events,
        );
        if pages != crate::db::CAUGHT_UP_CHECKPOINT_PAGES {
            return Ok(());
        }
        let mut committer = self.committer.lock().await;
        let conn = self.committer_conn(pool, &mut committer).await?;
        let result = sqlx::query("PRAGMA wal_checkpoint(PASSIVE)")
            .execute(&mut *conn)
            .await
            .map(drop)
            .map_err(AppError::from);
        if let Err(e) = &result {
            drop_if_broken(&mut committer, e);
        }
        result
    }

    /// The digest's own connection (held in `slot`, the locked `committer`),
    /// opened when there is none. Fails as an outage once it was closed for
    /// shutdown, when the pool is closed or a new connection does not answer.
    async fn committer_conn<'a>(
        &self,
        pool: &DbPool,
        slot: &'a mut Option<DbConn>,
    ) -> AppResult<&'a mut DbConn> {
        if pool.is_closed()
            || self
                .committer_closed
                .load(std::sync::atomic::Ordering::Relaxed)
        {
            return Err(AppError::Unavailable("the database is closing".to_string()));
        }
        if slot.is_none() {
            let mut conn = crate::db::connect_committer(pool).await.map_err(|e| {
                AppError::Unavailable(format!("cannot open the digest's connection: {e}"))
            })?;
            sqlx::query("SELECT 1")
                .execute(&mut conn)
                .await
                .map_err(|e| {
                    AppError::Unavailable(format!("the digest's connection does not answer: {e}"))
                })?;
            *slot = Some(conn);
        }
        Ok(slot.as_mut().expect("opened above"))
    }

    /// Closes the digest's own connection for good, after the batch on it,
    /// if any: shutdown checkpoints and closes the pool next, and a digest
    /// after this leaves its file for the next start.
    pub async fn close_committer(&self) {
        // Before the lock: shutdown gives up waiting for a batch that holds
        // it, and the connection must stay closed all the same.
        self.committer_closed
            .store(true, std::sync::atomic::Ordering::Relaxed);
        let mut committer = self.committer.lock().await;
        if let Some(conn) = committer.take() {
            if let Err(e) = sqlx::Connection::close(conn).await {
                log::warn!("Failed to close the digest's connection: {e:?}");
            }
        }
    }

    /// What follows a commit, per event in batch order: alerts, the replay
    /// of an event already stored; then, for the whole batch, the removal of
    /// the files whose digest is over and the bookkeeping.
    pub(crate) async fn finish(
        &self,
        pool: &DbPool,
        done: Vec<Committed>,
    ) -> Vec<AppResult<Digested>> {
        let mut ctx = FinishCtx::default();
        let mut steps = Vec::with_capacity(done.len());
        let mut seen = std::collections::HashSet::new();
        for committed in done {
            let key = (committed.e.r.project_id, committed.e.r.event_id);
            // A split can part two copies, each then first in its half.
            if !seen.insert(key) || matches!(committed.outcome, Ok(EventOutcome::DuplicateInBatch))
            {
                // Its file is the first copy's; its guard goes now.
                steps.push(Step::Copy(key));
                continue;
            }
            let (result, r) = self.finish_one(pool, committed, &mut ctx).await;
            steps.push(Step::Over(key, result, r));
        }
        crate::failpoint!("digest.after_alerts_before_delete");

        // Alert rows are pool commits: on SQLite they become power-safe only
        // with a commit on the durable connection, before their file goes.
        #[cfg(feature = "sqlite")]
        if ctx.alerted && steps.iter().any(|s| matches!(s, Step::Over(_, Ok(_), _))) {
            if let Err(e) = self.sync_barrier(pool).await {
                log::warn!("The digest could not make its alerts durable; their files stay: {e:?}");
                let why = e.to_string();
                for step in &mut steps {
                    if let Step::Over(_, result @ Ok(_), _) = step {
                        *result = Err(AppError::Unavailable(why.clone()));
                    }
                }
            }
        }
        let paths: Vec<_> = steps
            .iter()
            .filter_map(|s| match s {
                Step::Over(_, Ok(_), r) => pending_event_path(
                    &self.ingest_dir,
                    r.project_id,
                    &r.event_id.simple().to_string(),
                )
                .ok(),
                _ => None,
            })
            .collect();
        if let Err(e) = delete_paths(paths).await {
            let why = e.to_string();
            for step in &mut steps {
                if let Step::Over(_, result @ Ok(_), _) = step {
                    *result = Err(AppError::Internal(format!(
                        "Failed to delete event files: {why}"
                    )));
                }
            }
        }

        let mut results = Vec::with_capacity(steps.len());
        // What each event's first copy came to, for the copies after it.
        let mut firsts: std::collections::HashMap<(i32, Uuid), Option<Digested>> =
            std::collections::HashMap::new();
        for step in steps {
            match step {
                Step::Over((project_id, event_id), result, r) => {
                    // The guard goes after the file, once settled.
                    let result = self.settle(project_id, event_id, result).await;
                    drop(r);
                    firsts.insert((project_id, event_id), result.as_ref().ok().copied());
                    results.push(result);
                }
                Step::Copy(key) => {
                    // Counted like any delivery when its first copy is over;
                    // a failed first copy was counted once, for both.
                    let result = match firsts.get(&key) {
                        Some(Some(digested)) => Ok(*digested),
                        _ => Err(AppError::Internal(
                            "a copy of an event whose digest failed".to_string(),
                        )),
                    };
                    if result.is_ok() {
                        self.count(&result);
                    }
                    results.push(result);
                }
            }
        }
        results
    }

    /// One event's alerts and side effects. With its result, its spool file,
    /// deleted when the result is `Ok`.
    async fn finish_one(
        &self,
        pool: &DbPool,
        committed: Committed,
        ctx: &mut FinishCtx,
    ) -> (AppResult<Digested>, SpoolRef) {
        let Committed { e, outcome, issue } = committed;
        let raw_event_id = e.r.event_id.simple().to_string();
        let result = match outcome {
            Err(error) => Err(error),
            Ok(EventOutcome::RateLimited(scope)) => {
                // Counted on its project in the transaction that dropped it.
                log::debug!("Event {raw_event_id} dropped: the {scope:?} quota is full");
                Ok(Digested::RateLimited)
            }
            Ok(EventOutcome::DuplicateInBatch) => Ok(Digested::Stored),
            Ok(EventOutcome::AlreadyStored {
                issue_id,
                alert_type,
                issue_event_count,
            }) => {
                log::warn!("Duplicate event_id: {raw_event_id}");
                let replayed = self
                    .replay(pool, &e, issue_id, alert_type, issue_event_count, ctx)
                    .await;
                after_commit(replayed, &raw_event_id).map(|()| Digested::Stored)
            }
            Ok(EventOutcome::Inserted {
                lifecycle,
                muted,
                regressed,
                ..
            }) => match issue {
                Some(issue) => self
                    .after_insert(pool, &e, issue, lifecycle, muted, regressed, ctx)
                    .await
                    .map(|()| Digested::Stored),
                None => Err(AppError::Internal(
                    "a stored event without its issue".to_string(),
                )),
            },
        };
        (result, e.r)
    }

    /// An inserted event's alerts and side effects.
    #[allow(clippy::too_many_arguments)]
    async fn after_insert(
        &self,
        pool: &DbPool,
        e: &PreparedEvent,
        issue: Issue,
        lifecycle: Option<AlertType>,
        muted: bool,
        regressed: bool,
        ctx: &mut FinishCtx,
    ) -> AppResult<()> {
        let (project_id, event_id) = (e.r.project_id, e.r.event_id);
        let raw_event_id = event_id.simple().to_string();
        // Sentry-parity platform auto-detection (set once, never overwritten).
        // Best-effort: the digest transaction already committed the event, so
        // ancillary project metadata must not make the durable event fail.
        // Skipped once the project has a platform: the UPDATE would match no
        // row, but on SQLite it still takes the write lock. Once per project
        // per batch.
        if e.project.platform.is_none() {
            if let Some(event_platform) = &e.event_platform {
                if ctx.inferred.insert(project_id) {
                    #[cfg(feature = "sqlite")]
                    let _turn = self.write_turn().await;
                    if let Err(err) =
                        ProjectService::infer_platform_from_event(pool, project_id, event_platform)
                            .await
                    {
                        log::warn!(
                            "platform inference failed for project {project_id} (best-effort; a later event can self-heal): {err:?}"
                        );
                    }
                }
            }
        }

        // Persist alert history before deleting the only durable source.
        let facts = e.facts();
        let types = event_alert_types(lifecycle, muted, e.feedback.is_some());
        let alerted: AppResult<()> = async {
            for &alert_type in &types {
                self.trigger_event_alert(
                    pool, &e.project, &issue, alert_type, event_id, facts, ctx,
                )
                .await?;
            }
            Ok(())
        }
        .await;
        after_commit(alerted, &raw_event_id)?;
        if types.contains(&AlertType::NewIssue) {
            self.spawn_auto_create(pool, &e.project, &issue, facts);
        }
        // A regression reopens the linked GitHub issues too.
        if regressed {
            crate::services::github::sync::spawn(
                "status sync",
                crate::services::github::sync::mirror_status(pool.clone(), issue.id),
            );
        }
        log::debug!(
            "Digested event {raw_event_id} -> issue {} ({})",
            issue.id,
            if matches!(
                lifecycle,
                Some(AlertType::NewIssue | AlertType::NewFeedback)
            ) {
                "new"
            } else {
                "existing"
            }
        );
        Ok(())
    }

    /// The alerts of an event that was already stored, run again: each rule
    /// skips what it already recorded for this event.
    async fn replay(
        &self,
        pool: &DbPool,
        e: &PreparedEvent,
        issue_id: Option<Uuid>,
        alert_type: Option<AlertType>,
        issue_event_count: Option<i32>,
        ctx: &mut FinishCtx,
    ) -> AppResult<()> {
        let Some(issue_id) = issue_id else {
            return Ok(());
        };
        let mut issue = IssueService::get_by_id(pool, issue_id).await?;
        let facts = e.facts();
        // Rules match the count this event brought the issue to, not
        // today's. A row without that count (from before it was kept,
        // or digested while muted) replays its lifecycle alert only,
        // and a new issue's came from its first event.
        let types = match issue_event_count {
            Some(count) => {
                issue.digested_event_count = count;
                event_alert_types(
                    alert_type,
                    issue.is_muted(),
                    issue.issue_category == feedback::CATEGORY,
                )
            }
            None => {
                if alert_type == Some(AlertType::NewIssue) {
                    issue.digested_event_count = 1;
                }
                alert_type.into_iter().collect()
            }
        };
        // Each rule skips what it already recorded for this event.
        for &alert_type in &types {
            self.trigger_event_alert(
                pool,
                &e.project,
                &issue,
                alert_type,
                e.r.event_id,
                facts,
                ctx,
            )
            .await?;
        }
        // Opens nothing when the first run already linked one.
        if types.contains(&AlertType::NewIssue) {
            self.spawn_auto_create(pool, &e.project, &issue, facts);
        }
        // Mirrors the issue's current status, so a no-op when the
        // first run already did.
        if alert_type == Some(AlertType::Regression) {
            crate::services::github::sync::spawn(
                "status sync",
                crate::services::github::sync::mirror_status(pool.clone(), issue.id),
            );
        }
        Ok(())
    }

    /// Moves a spool file that will never digest to `dead/` and stops
    /// counting it as pending. `false` when the move failed: the file stays
    /// queued, as before.
    pub async fn dead_letter(
        &self,
        project_id: i32,
        event_id: &str,
        why: &(dyn std::fmt::Display + Sync),
    ) -> bool {
        match crate::ingest::storage::dead_letter_event(&self.ingest_dir, project_id, event_id)
            .await
        {
            Ok(path) => {
                log::error!(
                    "Event {event_id} of project {project_id} cannot be digested ({why}); moved to {}",
                    path.display()
                );
                if let Ok(event_id) = Uuid::parse_str(event_id) {
                    self.failed_attempts
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .remove(&(project_id, event_id));
                    self.pending.remove(project_id, event_id);
                }
                true
            }
            Err(e) => {
                log::error!("Failed to dead-letter event {event_id}: {e:?}");
                false
            }
        }
    }

    /// Opens the GitHub issue for a new-issue event, when the project's
    /// filters let it.
    fn spawn_auto_create(
        &self,
        pool: &DbPool,
        project: &crate::models::Project,
        issue: &crate::models::Issue,
        facts: crate::services::alert::EventFacts<'_>,
    ) {
        crate::services::github::sync::spawn(
            "issue auto-create",
            crate::services::github::sync::auto_create(
                pool.clone(),
                project.clone(),
                issue.clone(),
                facts.level.map(str::to_string),
                facts.environment.map(str::to_string),
                self.dashboard_url.clone(),
            ),
        );
    }

    /// Drops an accepted event the quota has no room for: its durable copy
    /// removed, then counted on its project. In that order, so a failed
    /// removal replays without having counted it, and it is counted at most
    /// once (a failed count after the removal loses one count, not an event).
    async fn drop_rate_limited(
        &self,
        pool: &DbPool,
        project_id: i32,
        event_id: Uuid,
    ) -> AppResult<Digested> {
        log::debug!("Event {event_id} dropped: quota exceeded");
        delete_event(&self.ingest_dir, project_id, &event_id.simple().to_string()).await?;
        if let Err(e) = RateLimitService::record_rate_limited(pool, project_id).await {
            log::warn!("Event {event_id} dropped over quota, but not counted: {e:?}");
        }
        Ok(Digested::RateLimited)
    }
}

/// What [`ErrorProcessor::finish`] carries from one event to the next.
#[derive(Default)]
struct FinishCtx {
    /// An alert rule ran: its rows are pool commits.
    #[cfg_attr(feature = "postgres", allow(dead_code))]
    alerted: bool,
    /// Projects whose platform inference this batch already ran.
    inferred: std::collections::HashSet<i32>,
}

/// An event of a finished batch, before its file goes.
enum Step {
    /// Its result, and its file, deleted when the result is `Ok`.
    Over((i32, Uuid), AppResult<Digested>, SpoolRef),
    /// A copy of an event earlier in the batch.
    Copy((i32, Uuid)),
}

/// What a probe of the breaker says: the database stored the blank event
/// (`Some(true)`) or refused it (`Some(false)`). `None` when it failed as in
/// an outage (busy, unreachable, a rollback that did not end) or the event
/// ended before its row (the quota dropped it): that says nothing either way.
fn probe_verdict(result: &AppResult<BatchOutcome>) -> Option<bool> {
    match result {
        Ok(o) => matches!(o.events.as_slice(), [EventOutcome::Inserted { .. }]).then_some(true),
        Err(e) if is_outage(e) => None,
        Err(_) => Some(false),
    }
}

/// The committer's automatic checkpoint for its next commit, from the events
/// not digested yet.
#[cfg(feature = "sqlite")]
fn checkpoint_pages(pending: usize, batch_max: usize) -> u32 {
    if pending >= batch_max {
        crate::db::COMMITTER_CHECKPOINT_PAGES
    } else {
        crate::db::CAUGHT_UP_CHECKPOINT_PAGES
    }
}

/// How long the committer rests after a commit that took `took`: as long
/// again while a burst is still arriving (an event was accepted during the
/// commit and a full batch waits), so the digest takes at most half the
/// disk from ingest; otherwise not at all, so a backlog drains at full speed
/// once the burst is over. Each accepted event waits on the spool's fsyncs,
/// which the digest's log writes and checkpoints slow down.
#[cfg(feature = "sqlite")]
fn yield_to_ingest(
    took: std::time::Duration,
    pushed_meanwhile: bool,
    queued: usize,
    batch_max: usize,
) -> std::time::Duration {
    if pushed_meanwhile && queued >= batch_max {
        took
    } else {
        std::time::Duration::ZERO
    }
}

/// Drops the digest's connection after a failure that may have broken it
/// (lost, or left inside a transaction the rollback could not end); the next
/// batch opens a new one. A busy SQLite leaves it as it is.
fn drop_if_broken(slot: &mut Option<DbConn>, e: &AppError) {
    let broken = is_outage(e) && !is_retryable_write_contention(e);
    if broken
        || slot
            .as_ref()
            .is_some_and(sqlx::Connection::is_in_transaction)
    {
        *slot = None;
    }
}

/// Shortest gap between two shedding log lines.
const SHED_LOG_EVERY: std::time::Duration = std::time::Duration::from_secs(5);

/// Shed requests since the last log line, and when that line was written.
/// Under load shedding every request is refused, so a line per request
/// would be thousands a second; the counter metric has the exact figure.
#[derive(Default)]
struct ShedLog(std::sync::Mutex<(Option<std::time::Instant>, u64)>);

impl ShedLog {
    /// Counts one shed request. `Some(n)` when a line is due, `n` being the
    /// requests shed since the last one, this one included.
    fn note(&self, now: std::time::Instant) -> Option<u64> {
        let mut state = self.0.lock().unwrap_or_else(|p| p.into_inner());
        let (last, shed) = &mut *state;
        *shed += 1;
        if last.is_some_and(|at| now.saturating_duration_since(at) < SHED_LOG_EVERY) {
            return None;
        }
        *last = Some(now);
        Some(std::mem::take(shed))
    }
}

/// Where ingest starts shedding on free disk: `INGEST_MIN_FREE_DISK_MB`
/// plus, on SQLite, what the write-ahead log can still grow by before the
/// digest's deferred checkpoint (`db::COMMITTER_CHECKPOINT_PAGES`, 4 KiB
/// pages). The digest goes on writing while ingest sheds, so the database
/// gets that room before the disk fills. 0 keeps the check off.
fn free_disk_floor(configured: u64) -> u64 {
    #[cfg(feature = "sqlite")]
    if configured > 0 {
        return configured.saturating_add(u64::from(crate::db::COMMITTER_CHECKPOINT_PAGES) * 4096);
    }
    configured
}

/// Bytes an unprivileged process can still write on `dir`'s volume.
#[cfg(unix)]
#[allow(clippy::useless_conversion)] // the field types differ per platform
fn free_bytes(dir: &Path) -> std::io::Result<u64> {
    let stat = nix::sys::statvfs::statvfs(dir)?;
    Ok(u64::from(stat.blocks_available()).saturating_mul(u64::from(stat.fragment_size())))
}

/// No portable reading: never shed on disk space.
#[cfg(not(unix))]
fn free_bytes(_dir: &Path) -> std::io::Result<u64> {
    Ok(u64::MAX)
}

/// Database, internal and unavailable failures keep the file for a replay:
/// an unknown transient failure must not silently become data loss.
/// Anything else (an invalid event) is dropped with its file.
fn should_retain_event(err: &AppError) -> bool {
    matches!(
        err.kind(),
        AppError::Database(_) | AppError::Internal(_) | AppError::Unavailable(_)
    )
}

/// The result of a step after the event row committed (its alerts). A
/// transient failure is returned, so the file stays and a replay runs the
/// step again (each rule skips what it already recorded). Any other (data
/// the database rejects) is logged, counted in
/// `rustrak_digest_post_commit_dropped_total` and given up: the event is
/// stored, so its file still leaves the
/// spool through the durable delete, never through `dead/`, and never stays
/// in the spool for ever.
fn after_commit(result: AppResult<()>, event_id: &str) -> AppResult<()> {
    match result {
        Err(e) if !e.is_transient() => {
            log::error!("Event {event_id} is stored, but a step after it failed for good: {e:?}");
            crate::telemetry::Counters::global().post_commit_dropped();
            Ok(())
        }
        other => other,
    }
}

/// How long the live committer waits before retrying a batch the database
/// failed: 50 ms, doubling, at most 5 s.
fn outage_backoff(attempt: u32) -> std::time::Duration {
    std::time::Duration::from_millis(50u64 << attempt.min(7)).min(std::time::Duration::from_secs(5))
}

/// Batches finished at once; the committer waits for a slot.
const FINISHERS: usize = 2;

/// True for transient SQLite write contention: the busy family (codes 5,
/// 261, 517, 773) and the locked one (6, 262). Always false on Postgres, whose SQLSTATEs never collide
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

/// The database failed, not the batch: down, unreachable, out of
/// connections, or busy. Retrying the same batch later is the answer;
/// splitting it is not.
fn is_outage(err: &AppError) -> bool {
    use sqlx::Error as E;
    if let AppError::Database(E::Database(db)) = err.kind() {
        if db
            .code()
            .as_deref()
            .is_some_and(|code| is_conflict_code(code) || is_unavailable_code(code))
        {
            return true;
        }
    }
    is_retryable_write_contention(err)
        || matches!(
            err.kind(),
            AppError::Unavailable(_)
                | AppError::Database(
                    E::Io(_) | E::Protocol(_) | E::PoolTimedOut | E::PoolClosed | E::WorkerCrashed
                )
        )
}

/// A Postgres deadlock (`40P01`) or serialization failure (`40001`): the
/// batch met another writer, and the same batch succeeds when retried.
fn is_conflict_code(code: &str) -> bool {
    matches!(code, "40P01" | "40001")
}

/// The database cannot take any write right now, whatever the batch. On
/// Postgres (SQLSTATE, appendix A): class 08 connection exception, class 53
/// insufficient resources but `out_of_memory` and
/// `configuration_limit_exceeded`, the 57P01-05 shutdowns, dropped database
/// and idle-session timeout, 25006 read-only transaction, 58030 I/O error,
/// XX001-002 corrupted data or index. On SQLite (extended codes, by their
/// primary code, rescode.html): READONLY 8, IOERR 10, CORRUPT 11, FULL 13,
/// CANTOPEN 14, PROTOCOL 15.
///
/// Out of memory (Postgres 53200, SQLite NOMEM 7) is left out: one huge
/// event can cause it alone, and retried whole the batch would fail for
/// ever. Split like any failure, it fails alone and its neighbours store;
/// a database out of memory for every event refuses the breaker's probe too
/// and halts the digest.
fn is_unavailable_code(code: &str) -> bool {
    if code.len() == 5 {
        return code.starts_with("08")
            || (code.starts_with("53") && !matches!(code, "53200" | "53400"))
            || matches!(
                code,
                "57P01"
                    | "57P02"
                    | "57P03"
                    | "57P04"
                    | "57P05"
                    | "25006"
                    | "58030"
                    | "XX001"
                    | "XX002"
            );
    }
    code.parse::<u32>()
        .is_ok_and(|extended| matches!(extended & 0xff, 8 | 10 | 11 | 13 | 14 | 15))
}

/// Whether a SQLite extended result code string means "database is locked":
/// BUSY and its extended codes, and LOCKED and LOCKED_SHAREDCACHE, which a
/// shared cache (`sqlite::memory:`, pool and committer) returns instead.
fn is_sqlite_busy_code(code: &str) -> bool {
    matches!(code, "5" | "261" | "517" | "773" | "6" | "262")
}

/// How an accepted event left the digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Digested {
    /// Stored, or already stored under the same id.
    Stored,
    /// Dropped by the quota: nothing was written.
    RateLimited,
    /// No file held the event: its digest already finished and deleted it.
    Missing,
    /// Failed the same way too often: its file was moved to `dead/`.
    DeadLettered,
    /// Recovery left the file in place (another version, unreadable for
    /// now): no longer pending.
    Skipped,
}

/// Failed attempts after which a spool file whose digest keeps failing the
/// same way is dead-lettered. The first is the digest ingest spawned, the
/// rest are replays a backoff of seconds apart.
pub const MAX_DIGEST_ATTEMPTS: u32 = 5;

/// The alerts an event fires: its lifecycle's, and a new-issue rule's, since
/// any unmuted event, a regression too, can be the one that brings the issue
/// to the rule's `min_events`; the rule decides. A feedback fires only its
/// own `new_feedback` rule.
fn event_alert_types(lifecycle: Option<AlertType>, muted: bool, feedback: bool) -> Vec<AlertType> {
    let mut types: Vec<AlertType> = lifecycle.into_iter().collect();
    if !feedback && lifecycle != Some(AlertType::NewIssue) && !muted {
        types.push(AlertType::NewIssue);
    }
    types
}

pub(crate) fn alert_type_for_lifecycle(
    created: bool,
    regressed: bool,
    feedback: bool,
) -> Option<AlertType> {
    if created && feedback {
        Some(AlertType::NewFeedback)
    } else if created {
        Some(AlertType::NewIssue)
    } else if regressed {
        Some(AlertType::Regression)
    } else {
        None
    }
}

/// Removes breadcrumbs from before 1970, in both the `{ "values": [...] }`
/// form and the bare array form that `Values<T>` also accepts.
fn drop_stale_breadcrumbs(event_data: &mut serde_json::Value) {
    let Some(crumbs) = event_data.get_mut("breadcrumbs") else {
        return;
    };
    let crumbs = if crumbs.get("values").is_some() {
        &mut crumbs["values"]
    } else {
        crumbs
    };
    if let Some(crumbs) = crumbs.as_array_mut() {
        crumbs.retain(|crumb| !is_stale(crumb, "timestamp"));
    }
}

/// The fields an alert rule's filters read, straight from the event.
fn event_facts(event_data: &serde_json::Value) -> crate::services::alert::EventFacts<'_> {
    crate::services::alert::EventFacts {
        level: event_data
            .get("level")
            .and_then(|v| v.as_str())
            .and_then(relay_level),
        environment: event_data.get("environment").and_then(|v| v.as_str()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shedding refuses every request; the log gets a line every few
    /// seconds with how many were refused, not one per request.
    #[test]
    fn shedding_is_logged_at_most_once_per_window() {
        let log = ShedLog::default();
        let start = std::time::Instant::now();

        assert_eq!(log.note(start), Some(1), "the first is logged");
        for _ in 0..99 {
            assert_eq!(log.note(start + std::time::Duration::from_secs(1)), None);
        }
        assert_eq!(log.note(start + SHED_LOG_EVERY), Some(100));
    }

    /// Caught up, the committer checkpoints after each commit; behind by a
    /// batch or more, it lets the log grow between checkpoints.
    #[cfg(feature = "sqlite")]
    #[test]
    fn checkpoints_grow_with_the_backlog() {
        use crate::db::{CAUGHT_UP_CHECKPOINT_PAGES, COMMITTER_CHECKPOINT_PAGES};
        assert_eq!(checkpoint_pages(255, 256), CAUGHT_UP_CHECKPOINT_PAGES);
        assert_eq!(checkpoint_pages(256, 256), COMMITTER_CHECKPOINT_PAGES);
        const { assert!(CAUGHT_UP_CHECKPOINT_PAGES < COMMITTER_CHECKPOINT_PAGES) };
    }

    /// Behind a burst still arriving, the committer rests as long as its
    /// commit took; caught up, or draining once the burst is over, never.
    #[cfg(feature = "sqlite")]
    #[test]
    fn the_digest_yields_the_disk_only_to_a_burst_still_arriving() {
        use std::time::Duration;
        let took = Duration::from_millis(40);
        assert_eq!(yield_to_ingest(took, true, 256, 256), took);
        assert_eq!(yield_to_ingest(took, false, 4096, 256), Duration::ZERO);
        assert_eq!(yield_to_ingest(took, true, 255, 256), Duration::ZERO);
    }

    #[test]
    fn stale_breadcrumbs_are_dropped_in_both_forms() {
        let crumbs = serde_json::json!([{ "timestamp": -5.0 }, { "timestamp": 1.0 }]);
        let kept = serde_json::json!([{ "timestamp": 1.0 }]);

        let mut wrapped = serde_json::json!({ "breadcrumbs": { "values": crumbs } });
        drop_stale_breadcrumbs(&mut wrapped);
        assert_eq!(wrapped["breadcrumbs"]["values"], kept);

        let mut bare = serde_json::json!({ "breadcrumbs": crumbs });
        drop_stale_breadcrumbs(&mut bare);
        assert_eq!(bare["breadcrumbs"], kept);
    }

    /// LOCKED (6) and LOCKED_SHAREDCACHE (262) are the busy of a shared
    /// cache (`sqlite::memory:`), where `busy_timeout` does not wait.
    #[test]
    fn busy_codes_are_recognized_and_others_are_not() {
        for code in ["5", "261", "517", "773", "6", "262"] {
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
            alert_type_for_lifecycle(true, false, false),
            Some(AlertType::NewIssue)
        );
        assert_eq!(
            alert_type_for_lifecycle(false, true, false),
            Some(AlertType::Regression)
        );
        assert_eq!(alert_type_for_lifecycle(false, false, false), None);
        assert_eq!(
            alert_type_for_lifecycle(true, false, true),
            Some(AlertType::NewFeedback)
        );
    }

    #[test]
    fn feedback_fires_only_its_own_rule() {
        assert_eq!(
            event_alert_types(Some(AlertType::NewFeedback), false, true),
            [AlertType::NewFeedback]
        );
        assert_eq!(
            event_alert_types(Some(AlertType::Regression), false, false),
            [AlertType::Regression, AlertType::NewIssue]
        );
    }

    #[test]
    fn the_live_committer_backs_off_from_50ms_to_5s() {
        assert_eq!(outage_backoff(0), std::time::Duration::from_millis(50));
        assert_eq!(outage_backoff(1), std::time::Duration::from_millis(100));
        assert_eq!(outage_backoff(40), std::time::Duration::from_secs(5));
    }

    /// An outage is retried whole; anything else splits the batch down to
    /// the event that fails.
    #[test]
    fn only_a_database_that_failed_is_an_outage() {
        for outage in [
            AppError::Database(sqlx::Error::PoolTimedOut),
            AppError::Database(sqlx::Error::PoolClosed),
            AppError::Database(sqlx::Error::Io(std::io::Error::other("reset"))),
            AppError::Unavailable("the disk is full".to_string()),
        ] {
            assert!(is_outage(&outage), "{outage:?}");
        }
        for batch in [
            AppError::Database(sqlx::Error::RowNotFound),
            AppError::Internal("events present that the dedupe did not see".to_string()),
        ] {
            assert!(!is_outage(&batch), "{batch:?}");
        }
    }

    /// A database that is full, read-only, unreachable or in a Postgres
    /// deadlock or serialization failure fails every batch alike: retried
    /// whole, never split toward the breaker. A limit one statement hits
    /// (Postgres 54000) still splits.
    #[test]
    fn a_full_or_read_only_database_is_an_outage() {
        use crate::error::tests::db_error;
        for code in [
            "40P01", "40001",
            // SQLite: NOMEM, READONLY (and READONLY_DBMOVED), IOERR (and
            // IOERR_FSYNC), FULL, CANTOPEN, PROTOCOL.
            "8", "1032", "10", "1034", "13", "14", "15", "11", "267",
            // Postgres: connection exceptions, insufficient resources,
            // shutdowns, a dropped database, an idle session ended, a
            // read-only transaction, an I/O error, corrupted data or index.
            "08006", "08001", "53100", "53300", "57P01", "57P02", "57P03", "57P04", "57P05",
            "25006", "58030", "XX001", "XX002",
        ] {
            assert!(is_outage(&db_error(code)), "{code}");
        }
        // Constraint, too big, SQL error, a statement too complex, and out
        // of memory (SQLite NOMEM, Postgres 53200), which one huge event
        // can cause alone: split, so it fails alone and its neighbours store.
        for code in [
            "19", "2067", "18", "1", "23505", "22001", "42P01", "54000", "57014", "7", "53200",
        ] {
            assert!(!is_outage(&db_error(code)), "{code}");
        }
    }

    /// A probe that stored its event clears the breaker, one the database
    /// refused halts it, and one that failed as in an outage, or ended
    /// before its row, decides nothing.
    #[test]
    fn only_a_probe_that_reached_its_row_decides() {
        use crate::error::tests::db_error;
        let outcome = |events| -> AppResult<BatchOutcome> {
            Ok(BatchOutcome {
                events,
                issues: Default::default(),
            })
        };
        let inserted = EventOutcome::Inserted {
            issue_id: Uuid::nil(),
            lifecycle: None,
            muted: false,
            issue_event_count: Some(1),
            regressed: false,
        };
        assert_eq!(probe_verdict(&outcome(vec![inserted])), Some(true));
        assert_eq!(probe_verdict(&Err(db_error("2067"))), Some(false));
        assert_eq!(probe_verdict(&Err(db_error("42P01"))), Some(false));
        for (i, undecided) in [
            Err(db_error("5")),
            Err(db_error("40P01")),
            Err(AppError::Unavailable("the database is closing".into())),
            Err(AppError::Database(sqlx::Error::PoolTimedOut)),
            outcome(vec![EventOutcome::RateLimited(
                crate::services::rate_limit::QuotaScope::Project,
            )]),
        ]
        .iter()
        .enumerate()
        {
            assert_eq!(probe_verdict(undecided), None, "case {i}");
        }
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
        // An event that does not parse is invalid, not a failure to retry.
        assert!(!should_retain_event(&AppError::Validation(
            "Invalid event JSON: trailing data".to_string(),
        )));
    }
}
