mod batch;
pub mod event;
mod feedback;
pub mod logs;
pub mod span;
pub mod span_v2;
pub mod transaction;

pub(crate) use event::{CommitMode, Prepared};
pub use event::{Digested, ErrorProcessor, SpoolRef};
pub(crate) use feedback::{Feedback, ATTACHMENTS_KEY};
pub use logs::LogsProcessor;
pub use span::SpanProcessor;
pub use span_v2::SpanV2Processor;
pub use transaction::TransactionProcessor;

use crate::config::Config;
use crate::db::DbPool;
use crate::digest::session_aggregator::SessionAggregator;
use crate::services::sourcemap::SourceMapProvider;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// The error digests this process currently owns: queued by the ingest route
/// or taken by recovery and not finished yet, or committed and waiting for
/// their alerts and the removal of their file.
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
                // A drained backlog gives its table back; amortised.
                if entries.len() < entries.capacity() / 4 {
                    entries.shrink_to_fit();
                }
            }
        }
    }
}

/// What the ingest pipeline hands envelope items to, built once at startup and
/// shared as application state. Each processor owns the dependencies it
/// needs; per-request values travel in [`ProcessorCtx`].
pub struct Processors {
    pub errors: ErrorProcessor,
    pub transactions: TransactionProcessor,
    /// Session updates are counted in memory and flushed on its own cycle.
    pub sessions: Arc<SessionAggregator>,
    pub logs: LogsProcessor,
    pub spans: SpanProcessor,
    pub spans_v2: SpanV2Processor,
    /// The commit gate: one permit, which every digest transaction takes
    /// for its batch. Taking it pauses the digest. Lives here, not in a
    /// static, so every app instance (each test server) gets its own.
    pub processing_slot: Arc<tokio::sync::Semaphore>,
    /// Where digest outcomes are counted for the anonymous telemetry. The
    /// process-wide set unless a test hands in its own.
    counters: &'static crate::telemetry::Counters,
}

impl Processors {
    pub fn new(
        config: &Config,
        sourcemap_provider: Arc<dyn SourceMapProvider>,
        session_aggregator: Arc<SessionAggregator>,
    ) -> Self {
        let errors = ErrorProcessor::new(config, sourcemap_provider);
        Self {
            processing_slot: Arc::clone(errors.commit_gate()),
            errors,
            transactions: TransactionProcessor,
            sessions: session_aggregator,
            logs: LogsProcessor,
            spans: SpanProcessor,
            spans_v2: SpanV2Processor,
            counters: crate::telemetry::Counters::global(),
        }
    }

    pub fn with_counters(mut self, counters: &'static crate::telemetry::Counters) -> Self {
        self.counters = counters;
        self.errors.counters = counters;
        self
    }

    pub fn counters(&self) -> &'static crate::telemetry::Counters {
        self.counters
    }

    /// [`ErrorProcessor::admit`], counting what it sheds.
    pub fn admit(&self, pending: i64) -> crate::error::AppResult<()> {
        self.errors.admit(pending, self.counters)
    }
}

/// The per-request values a processor reads.
pub struct ProcessorCtx {
    pub pool: DbPool,
    pub project_id: i32,
    pub event_id: Uuid,
    pub ingested_at: DateTime<Utc>,
    pub remote_addr: Option<String>,
    /// False when the project has turned agents off: AI spans are then left
    /// out wherever spans are stored, and the rest of the trace is kept.
    pub keep_ai_spans: bool,
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
    fn a_drained_backlog_shrinks_the_registry() {
        let registry = Arc::new(InFlightDigests::default());
        let guards: Vec<_> = (0..10_000)
            .map(|_| registry.register(1, Uuid::new_v4()))
            .collect();
        drop(guards);
        assert!(registry.entries.lock().unwrap().capacity() < 100);
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
