pub mod event;
pub mod logs;
pub mod session;
pub mod span;
pub mod span_v2;
pub mod transaction;

pub(crate) use event::is_retryable_write_contention;
pub use event::ErrorProcessor;
pub use logs::LogsProcessor;
pub use session::{SessionItem, SessionProcessor};
pub use span::SpanProcessor;
pub use span_v2::SpanV2Processor;
pub use transaction::TransactionProcessor;

use crate::config::RateLimitConfig;
use crate::db::DbPool;
use crate::error::AppResult;
use crate::ingest::envelope::EnvelopeItemKind;
use crate::services::sourcemap::SourceMapProvider;
use crate::workers::session_aggregator::SessionAggregatorHandle;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// The error digests this process currently owns: spawned by the ingest route
/// and not finished yet, or committed and waiting for the durability
/// checkpoint that lets their file be deleted.
///
/// The recovery worker consults it so a backlog is never replayed on top of
/// the tasks already working through it. Before this existed, every scan
/// under load re-read and re-digested the whole queue: thousands of wasted
/// project lookups and file reads per scan, and the same event digested twice
/// whenever the scan won the race to the file.
#[derive(Default)]
pub struct InFlightDigests {
    entries: Mutex<HashMap<(i32, Uuid), usize>>,
}

impl InFlightDigests {
    /// Marks an event as owned until the returned guard is dropped. Counted,
    /// so a duplicate delivery in flight alongside the first keeps the entry
    /// alive until both are done.
    pub fn register(self: &Arc<Self>, project_id: i32, event_id: Uuid) -> InFlightGuard {
        let key = (project_id, event_id);
        *self.entries.lock().unwrap().entry(key).or_insert(0) += 1;
        InFlightGuard {
            registry: Arc::clone(self),
            key,
        }
    }

    /// Whether some task in this process owns the event.
    pub fn contains(&self, project_id: i32, event_id: Uuid) -> bool {
        self.entries
            .lock()
            .unwrap()
            .contains_key(&(project_id, event_id))
    }

    /// Number of distinct events currently owned.
    pub fn len(&self) -> usize {
        self.entries.lock().unwrap().len()
    }

    /// Whether no digest is in flight.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Ownership of one in-flight digest; dropping it releases the entry.
pub struct InFlightGuard {
    registry: Arc<InFlightDigests>,
    key: (i32, Uuid),
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        let mut entries = self.registry.entries.lock().unwrap();
        if let Some(count) = entries.get_mut(&self.key) {
            *count -= 1;
            if *count == 0 {
                entries.remove(&self.key);
            }
        }
    }
}

/// Cap on concurrently running spawned digest tasks. Each holds a full
/// payload plus its parsed JSON working set, so an unbounded spawn turns a
/// burst of events into an unbounded memory spike; queued tasks wait holding
/// only their metadata. The HTTP request path never waits on this gate:
/// transactions and spans are persisted inline regardless of the backlog.
pub(crate) const MAX_CONCURRENT_PROCESSING: usize = 16;

/// The processor registry: one instance per processor, built once at startup
/// and shared as application state. Mirrors Relay's `inner.processing` struct
/// (relay-server/src/services/processor.rs) — each processor owns the
/// dependencies it needs; per-request values travel in [`ProcessorCtx`].
///
/// This is the single dispatch surface for the ingest pipeline. Adding a new
/// item type means adding a field here, a [`Route`] variant, and a `match` arm
/// — the compiler enforces the rest.
pub struct Processors {
    pub errors: ErrorProcessor,
    pub transactions: TransactionProcessor,
    pub sessions: SessionProcessor,
    pub logs: LogsProcessor,
    pub spans: SpanProcessor,
    pub spans_v2: SpanV2Processor,
    /// One gate for the spawned digest tasks — bounds peak memory
    /// under bursts (see [`MAX_CONCURRENT_PROCESSING`]). Lives here,
    /// not in a static, so every app instance (each test server) gets
    /// its own budget.
    pub processing_slot: Arc<tokio::sync::Semaphore>,
    /// Where digest outcomes are counted for the anonymous telemetry. The
    /// process-wide set unless a test hands in its own.
    counters: &'static crate::telemetry::Counters,
}

impl Processors {
    pub fn new(
        ingest_dir: PathBuf,
        rate_limit_config: RateLimitConfig,
        sourcemap_provider: Arc<dyn SourceMapProvider>,
        session_aggregator: Option<SessionAggregatorHandle>,
    ) -> Self {
        Self {
            errors: ErrorProcessor::new(ingest_dir, rate_limit_config, sourcemap_provider),
            transactions: TransactionProcessor,
            sessions: SessionProcessor::new(session_aggregator),
            logs: LogsProcessor,
            spans: SpanProcessor,
            spans_v2: SpanV2Processor,
            processing_slot: Arc::new(tokio::sync::Semaphore::const_new(MAX_CONCURRENT_PROCESSING)),
            counters: crate::telemetry::Counters::global(),
        }
    }

    /// Where alert notifications link to; see [`crate::config::Config::dashboard_url`].
    pub fn with_dashboard_url(mut self, url: impl Into<String>) -> Self {
        self.errors.dashboard_url = url.into();
        self
    }

    pub fn with_counters(mut self, counters: &'static crate::telemetry::Counters) -> Self {
        self.counters = counters;
        self
    }

    pub fn counters(&self) -> &'static crate::telemetry::Counters {
        self.counters
    }
}

/// A processor for one category of envelope work.
///
/// One impl per [`Route`]. Mirrors Relay's `Processor` trait
/// (relay-server/src/processing/mod.rs): static dispatch, no `dyn`.
///
/// The return type is spelled `impl Future + Send` (RPITIT) rather than a bare
/// `async fn` so the future is guaranteed `Send` — processors run inside
/// `tokio::spawn` — and to avoid the `async_fn_in_trait` lint under `-D warnings`.
pub trait Processor {
    /// The unit of work this processor consumes (extracted from an `EnvelopeItemKind`).
    type Input;

    /// Process one unit of work. Errors are logged by the caller and never
    /// abort sibling items in the same envelope.
    fn process(
        &self,
        work: Self::Input,
        ctx: &ProcessorCtx,
    ) -> impl std::future::Future<Output = AppResult<()>> + Send;
}

/// Identifies which processor handles a given envelope item.
///
/// Pure routing — no DB, no side effects. This is the single source of truth
/// for "which processor owns this item type", mirroring Relay's
/// `ProcessingGroup` (relay-server/src/services/processor.rs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// Error/exception events — durable two-phase digest (temp file + worker).
    Error,
    /// Performance transactions — direct store, no grouping.
    Transaction,
    /// Session health updates and aggregates.
    Session,
    /// Standalone logs — direct store, no grouping.
    Log,
    /// Standalone spans — direct store, no grouping.
    Span,
    /// Standalone spans, Spans Protocol v2 (batched container) — direct
    /// store, no grouping.
    SpanV2,
    /// Forward-compatible catch-all: logged and dropped, never processed.
    Ignored,
}

/// Maps an envelope item to the processor that owns it.
///
/// Exhaustiveness is compiler-enforced: a new `EnvelopeItemKind` variant
/// without a route arm is a build error, never a silent drop.
pub fn route(item: &EnvelopeItemKind) -> Route {
    match item {
        EnvelopeItemKind::Event(_) => Route::Error,
        EnvelopeItemKind::Transaction(_) => Route::Transaction,
        EnvelopeItemKind::Session(_) | EnvelopeItemKind::Sessions(_) => Route::Session,
        EnvelopeItemKind::Log(_) => Route::Log,
        EnvelopeItemKind::Span(_) => Route::Span,
        EnvelopeItemKind::SpanV2Batch(_) => Route::SpanV2,
        EnvelopeItemKind::Other(_, _) => Route::Ignored,
    }
}

/// Shared context injected into all processors.
/// Add new fields here — not to individual processor signatures.
pub struct ProcessorCtx {
    pub pool: DbPool,
    pub project_id: i32,
    pub event_id: Uuid,
    pub ingested_at: DateTime<Utc>,
    pub remote_addr: Option<String>,
}

#[cfg(test)]
mod in_flight_tests {
    use super::*;

    #[test]
    fn a_guard_owns_the_event_until_dropped() {
        let registry = Arc::new(InFlightDigests::default());
        let id = Uuid::new_v4();
        assert!(!registry.contains(1, id));

        let guard = registry.register(1, id);
        assert!(registry.contains(1, id));
        assert!(!registry.contains(2, id), "ownership is per project");
        assert_eq!(registry.len(), 1);

        drop(guard);
        assert!(!registry.contains(1, id));
        assert!(registry.is_empty());
    }

    #[test]
    fn duplicate_deliveries_keep_the_entry_until_the_last_guard_drops() {
        let registry = Arc::new(InFlightDigests::default());
        let id = Uuid::new_v4();
        let first = registry.register(1, id);
        let second = registry.register(1, id);
        assert_eq!(registry.len(), 1);

        drop(first);
        assert!(
            registry.contains(1, id),
            "the second delivery still owns it"
        );
        drop(second);
        assert!(!registry.contains(1, id));
    }
}
