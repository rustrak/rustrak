pub mod event;
mod feedback;
pub mod logs;
pub mod span;
pub mod span_v2;
pub mod transaction;

pub use event::{Digested, ErrorProcessor};
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
        config: &Config,
        sourcemap_provider: Arc<dyn SourceMapProvider>,
        session_aggregator: Arc<SessionAggregator>,
    ) -> Self {
        Self {
            errors: ErrorProcessor::new(config, sourcemap_provider),
            transactions: TransactionProcessor,
            sessions: session_aggregator,
            logs: LogsProcessor,
            spans: SpanProcessor,
            spans_v2: SpanV2Processor,
            processing_slot: Arc::new(tokio::sync::Semaphore::const_new(MAX_CONCURRENT_PROCESSING)),
            counters: crate::telemetry::Counters::global(),
        }
    }

    pub fn with_counters(mut self, counters: &'static crate::telemetry::Counters) -> Self {
        self.counters = counters;
        self
    }

    pub fn counters(&self) -> &'static crate::telemetry::Counters {
        self.counters
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
