use super::{Processor, ProcessorCtx};
use crate::error::AppResult;
use crate::models::session::{SessionAggregates, SessionUpdate};
use crate::workers::session_aggregator::SessionAggregatorHandle;

/// A session envelope item: either a single update or a pre-aggregated batch.
pub enum SessionItem {
    Update(SessionUpdate),
    Aggregates(SessionAggregates),
}

/// Processes session items by forwarding them to the in-process aggregator.
///
/// Owns its dependency (the aggregator handle) — mirrors Relay's registry,
/// where each processor carries the deps it needs rather than fattening the
/// shared [`ProcessorCtx`]. When no aggregator is configured, processing is a
/// safe no-op (session tracking is optional).
pub struct SessionProcessor {
    aggregator: Option<SessionAggregatorHandle>,
}

impl SessionProcessor {
    pub fn new(aggregator: Option<SessionAggregatorHandle>) -> Self {
        Self { aggregator }
    }
}

impl Processor for SessionProcessor {
    type Input = SessionItem;

    async fn process(&self, work: SessionItem, ctx: &ProcessorCtx) -> AppResult<()> {
        // No dedup identity, deliberately: session counts are additive counters
        // upstream too. Relay never dedups retried session envelopes — its
        // session pipeline has no idempotency anywhere and extracts plain
        // Counter metrics (relay-server/src/processing/sessions/,
        // metrics_extraction/sessions/types.rs BucketValue::Counter), so an SDK
        // resend double-counts in real Sentry as well. Matching that is
        // Sentry parity, not a gap.
        //
        // No flush here either. Relay extracts sessions into metric buckets
        // that its aggregator flushes on its own cycle, never on the request
        // path. Flushing per envelope ran one transaction per request against
        // the same few minute rows, which deadlocked on PostgreSQL and
        // answered the SDK with a 500 (#390). The aggregator's interval loop
        // and the shutdown flush in main.rs persist the counters instead.
        if let Some(agg) = &self.aggregator {
            match work {
                SessionItem::Update(update) => {
                    agg.ingest_session(ctx.project_id, &update).await;
                }
                SessionItem::Aggregates(aggregates) => {
                    agg.ingest_aggregates(ctx.project_id, &aggregates).await;
                }
            }
        }
        Ok(())
    }
}
