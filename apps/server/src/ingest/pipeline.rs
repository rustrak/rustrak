//! The ingest pipeline behind the envelope endpoint: parse, dispatch each
//! item to its processor, queue error events for the digest, and replay what a
//! previous process left queued.

use std::sync::Arc;

use bytes::Bytes;
use chrono::Utc;
use sha2::{Digest as _, Sha256};

use crate::db::DbPool;
use crate::digest::processors::event::PreparedEvent;
use crate::digest::processors::ATTACHMENTS_KEY;
use crate::digest::processors::{
    CommitMode, Digested, Prepared, ProcessorCtx, Processors, SpoolRef,
};
use crate::error::{AppError, AppResult};
use crate::ingest::envelope::Attachment;
use crate::ingest::storage::list_pending_names;
use crate::ingest::{
    detach_payload_if_needed, store_event_with_metadata, EnvelopeItemKind, EnvelopeParser,
    EventMetadata, MAX_ATTACHMENT_SIZE,
};
use crate::models::monitor::{CheckIn, MonitorFailure};
use crate::models::{Product, Project};
use crate::services::monitor::MonitorService;

/// Validates an event payload is well-formed JSON without building a
/// [`serde_json::Value`] tree: [`IgnoredAny`](serde::de::IgnoredAny) walks and
/// discards every value. The digest re-parses the stored file anyway, so the
/// tree here would be pure waste and can multiply memory use for large events.
fn validate_event_json(payload: &[u8]) -> AppResult<()> {
    serde_json::from_slice::<serde::de::IgnoredAny>(payload)
        .map_err(|e| AppError::Validation(format!("Invalid event JSON: {}", e)))?;
    Ok(())
}

/// Relay parity for item payloads: a malformed item is dropped and its
/// siblings continue (relay-server/src/processing/relay.rs run_one — "This is
/// not a fatal error case ... other items from the same original envelope must
/// still be processed"). Only infrastructure failures (database, storage) may
/// fail the envelope: they return 5xx, so nothing is acknowledged that was
/// not stored (the SDK drops that envelope; it never resends one that got a
/// response). A 4xx for one bad item would drop its valid siblings with it,
/// so an annotated `Validation` is matched by [`AppError::kind`] too.
fn drop_malformed_item(result: AppResult<()>, item_type: &str) -> AppResult<()> {
    let Err(error) = result else {
        return Ok(());
    };
    if let AppError::Validation(message) = error.kind() {
        log::warn!("dropping malformed {item_type} item, keeping envelope siblings: {message}");
        return Ok(());
    }
    Err(error)
}

/// Processes one envelope for `project`: items stored inline are committed
/// before this returns, an error event is stored and its digest spawned.
/// Returns the event id to acknowledge, if the envelope carried one.
///
/// `large_body` is given back once the envelope is parsed, so a large body
/// does not keep its slot while its items wait for the database.
/// ponytail: the items still borrow the body until they are stored, so up to
/// 100 MiB per request outlives its slot during those writes; keep the slot
/// to the end if that memory matters more than large-upload throughput.
pub async fn process_envelope(
    processors: &Arc<Processors>,
    pool: &DbPool,
    project: &Project,
    body: Bytes,
    remote_addr: Option<String>,
    ingested_at: chrono::DateTime<Utc>,
    large_body: crate::ingest::decompression::LargeBodySlot,
) -> AppResult<Option<String>> {
    let mut parser = EnvelopeParser::new(body);
    let envelope = parser.parse()?;
    drop(large_body);

    // event_id validation is deferred until after the loop: session-only
    // envelopes never need one (Relay: Item::requires_event() returns false
    // for Session/Sessions).
    let mut event_item: Option<Bytes> = None;
    let mut user_reports: Vec<Bytes> = Vec::new();
    let mut feedback_item: Option<Bytes> = None;
    let mut attachments: Vec<Attachment> = Vec::new();
    let mut transaction_item: Option<Bytes> = None;
    let mut span_items: Vec<Bytes> = Vec::new();
    let mut span_v2_items: Vec<Bytes> = Vec::new();
    let mut requires_event_id = false;
    let mut check_in_seen = false;
    let envelope_len = parser.data().len();
    let delivery_id = stable_delivery_id(envelope.headers.event_id.as_deref(), parser.data());
    for item_kind in envelope.items {
        if item_kind.requires_event() {
            requires_event_id = true;
        }
        // A product the project turned off is dropped like filtered data: a
        // 200 for the envelope, nothing stored, siblings kept.
        if let Some(product) = product_of(&item_kind) {
            if !project.keeps(product) {
                log::debug!(
                    "dropping {product:?} item: turned off for project {}",
                    project.id
                );
                continue;
            }
        }
        match item_kind {
            EnvelopeItemKind::Event(p) => {
                if event_item.is_none() {
                    event_item = Some(detach_payload_if_needed(p, envelope_len));
                }
            }
            EnvelopeItemKind::Transaction(p) => {
                if transaction_item.is_none() {
                    transaction_item = Some(detach_payload_if_needed(p, envelope_len));
                }
            }
            // No dedup identity, deliberately: session counts are additive
            // counters upstream too. Relay never dedups retried session
            // envelopes (relay-server/src/processing/sessions/,
            // metrics_extraction/sessions/types.rs BucketValue::Counter), so an
            // SDK resend double-counts in real Sentry as well.
            //
            // No flush here either: Relay's aggregator flushes on its own
            // cycle, never on the request path. Flushing per envelope ran one
            // transaction per request against the same few minute rows, which
            // deadlocked on PostgreSQL and answered the SDK with a 500 (#390).
            EnvelopeItemKind::Session(s) => {
                processors.sessions.ingest_session(project.id, &s).await;
            }
            EnvelopeItemKind::Sessions(s) => {
                processors.sessions.ingest_aggregates(project.id, &s).await;
            }
            EnvelopeItemKind::Log(payload) => {
                // Logs are processed inline (parse container + store): the work is
                // bounded and storing before responding keeps the batch durable.
                let ctx = item_ctx(pool, project, delivery_id, ingested_at, &remote_addr);
                // Every inline write takes its turn with the digests: see
                // `ErrorProcessor::write_turn`.
                #[cfg(feature = "sqlite")]
                let _turn = processors.errors.write_turn().await;
                drop_malformed_item(processors.logs.process(payload, &ctx).await, "log")?;
            }
            EnvelopeItemKind::Span(payload) => {
                // Unlike Event/Transaction, an envelope may carry many standalone
                // span items (Relay: one flat span object per item, no per-envelope
                // count cap) — collect them all instead of first-wins.
                span_items.push(detach_payload_if_needed(payload, envelope_len));
            }
            EnvelopeItemKind::SpanV2Batch(payload) => {
                // Each item is already a batch (one container can hold many
                // spans) — an envelope may still carry more than one such
                // container item, so collect them all.
                span_v2_items.push(detach_payload_if_needed(payload, envelope_len));
            }
            EnvelopeItemKind::CheckIn(payload) => {
                // Relay expands only the first check-in of an envelope and
                // drops the rest as duplicates (check_ins/process.rs).
                if check_in_seen {
                    log::debug!("dropping duplicate check_in item");
                    continue;
                }
                check_in_seen = true;
                let result = process_check_in(
                    processors,
                    pool,
                    project.id,
                    CheckIn::parse(&payload),
                    ingested_at,
                )
                .await;
                // An unknown monitor drops the item, as Sentry's consumer does.
                let result = match result {
                    Err(AppError::NotFound(message)) => Err(AppError::Validation(message)),
                    other => other,
                };
                drop_malformed_item(result, "check_in")?;
            }
            EnvelopeItemKind::Feedback(p) => {
                // Built after the loop: its attachments may come after it.
                feedback_item.get_or_insert(p);
            }
            EnvelopeItemKind::UserReport(p) => user_reports.push(p),
            EnvelopeItemKind::Attachment(a) => attachments.push(a),
            EnvelopeItemKind::Other(t, _) => {
                log::debug!("envelope item '{}' ignored", t);
            }
        }
    }
    drop(parser);

    // Relay builds the event from the item type, whatever the payload's own
    // `type` says (`UserReportV2` expansion), with the envelope's attachments
    // (`send_individual_attachments`). An error event keeps its attachments
    // too, of every `attachment_type`, as Sentry stores them.
    if let (None, Some(feedback)) = (&event_item, feedback_item) {
        match feedback_payload(&feedback, &attachments) {
            Ok(payload) => event_item = Some(payload),
            Err(e) => drop_malformed_item(Err(e), "feedback")?,
        }
    } else if let (Some(event), false) = (&event_item, attachments.is_empty()) {
        // An invalid event is rejected below, by `validate_event_json`.
        if let Ok(payload) =
            parse_event(event).and_then(|e| with_attachments(e, attachments.iter()))
        {
            event_item = Some(payload);
        }
    }
    drop(attachments);

    // Only required for event-bearing item types; if the SDK omitted it, it
    // derives from the durable delivery identity. A session-only envelope
    // passes through whatever the SDK provided (may be None).
    let event_id = resolve_event_id(envelope.headers.event_id, delivery_id, requires_event_id);
    let event_uuid = match (&event_id, requires_event_id) {
        (Some(id), true) => Some(
            uuid::Uuid::parse_str(id)
                .map_err(|_| AppError::Validation("event_id must be a valid UUID".to_string()))?,
        ),
        _ => None,
    };

    // Persist direct items before the early return; otherwise 200 could suppress
    // an SDK retry for data that was never stored.
    if let Some(txn_payload) = transaction_item {
        let event_id = event_uuid.unwrap_or(delivery_id);
        let ctx = item_ctx(pool, project, event_id, ingested_at, &remote_addr);
        #[cfg(feature = "sqlite")]
        let _turn = processors.errors.write_turn().await;
        drop_malformed_item(
            processors.transactions.process(txn_payload, &ctx).await,
            "transaction",
        )?;
    }

    // Persist standalone spans inline; they have no grouping or issue path.
    if !span_items.is_empty() {
        let ctx = item_ctx(pool, project, uuid::Uuid::nil(), ingested_at, &remote_addr);
        // One transaction for the whole span batch: per-item autocommit
        // INSERTs would each be their own SQLite transaction.
        #[cfg(feature = "sqlite")]
        let _turn = processors.errors.write_turn().await;
        drop_malformed_item(
            processors.spans.process_batch(span_items, &ctx).await,
            "span",
        )?;
    }

    // Persist v2 span batches inline for the same reason.
    if !span_v2_items.is_empty() {
        let ctx = item_ctx(pool, project, uuid::Uuid::nil(), ingested_at, &remote_addr);
        for batch_payload in span_v2_items {
            #[cfg(feature = "sqlite")]
            let _turn = processors.errors.write_turn().await;
            drop_malformed_item(
                processors.spans_v2.process(batch_payload, &ctx).await,
                "span container",
            )?;
        }
    }

    // Legacy user reports become feedback events, digested like any other.
    for report in user_reports {
        let shimmed = user_report_to_feedback(&report, event_id.as_deref(), "user_report_envelope");
        match shimmed {
            Ok(shim) => {
                enqueue_digest(
                    processors,
                    pool,
                    shim.feedback_id,
                    EventMetadata {
                        event_id: shim.feedback_id.simple().to_string(),
                        project_id: project.id,
                        ingested_at,
                        remote_addr: remote_addr.clone(),
                    },
                    shim.payload.into(),
                )
                .await?;
            }
            Err(e) => drop_malformed_item(Err(e), "user_report")?,
        }
    }

    let Some(event_item) = event_item else {
        log::debug!("No event item in envelope");
        return Ok(event_id);
    };
    let (Some(event_id), Some(event_uuid)) = (event_id, event_uuid) else {
        unreachable!("an event item requires an event id");
    };

    validate_event_json(&event_item)?;

    // Store metadata beside the raw event so a failed digest can be recovered.
    let metadata = EventMetadata {
        event_id: event_id.clone(),
        project_id: project.id,
        ingested_at,
        remote_addr,
    };
    enqueue_digest(processors, pool, event_uuid, metadata, event_item).await?;

    Ok(Some(event_id))
}

/// Stores an event durably and queues it for the live digest.
pub async fn enqueue_digest(
    processors: &Arc<Processors>,
    pool: &DbPool,
    event_uuid: uuid::Uuid,
    metadata: EventMetadata,
    payload: Bytes,
) -> AppResult<()> {
    // Claimed before the file exists so the recovery worker never sees a
    // pending file that no task owns. Released when its digest is over, or
    // here if the store fails and there is nothing to recover.
    let owner = processors
        .errors
        .in_flight()
        .register(metadata.project_id, event_uuid);
    let size = payload.len() as u64;
    crate::ingest::storage::store_event_bytes(
        processors.errors.ingest_dir(),
        &metadata.event_id,
        payload,
        &metadata,
    )
    .await?;
    crate::failpoint!("ingest.after_link_before_ack");
    // Counted against the quota from here until its digest is over, so the
    // ingest check sees it before the digest counts it in the database.
    processors
        .errors
        .pending()
        .insert(metadata.project_id, event_uuid);
    DigestPipeline::push(
        processors,
        pool,
        SpoolRef {
            project_id: metadata.project_id,
            event_id: event_uuid,
            size,
            owner,
        },
    );
    Ok(())
}

/// Spool files waiting for the live digest, at most this many. Past it a
/// file is left unowned and the recovery worker takes it from the spool.
pub(crate) const DIGEST_QUEUE_CAPACITY: usize = 4096;

/// A batch closes at this many bytes of `data` and attachments.
const DIGEST_BATCH_MAX_BYTES: usize = 8 * 1024 * 1024;

/// The accepted events waiting for the live digest, in acceptance order.
#[derive(Default)]
pub(crate) struct DigestQueue {
    refs: std::sync::Mutex<std::collections::VecDeque<SpoolRef>>,
    ready: tokio::sync::Notify,
    started: std::sync::atomic::AtomicBool,
    /// Events offered so far, queued or not: the digest rests while it
    /// changes (`yield_to_ingest`).
    pushes: std::sync::atomic::AtomicU64,
}

impl DigestQueue {
    /// Queues `r`, or hands it back when the queue is full.
    fn try_push(&self, r: SpoolRef) -> Result<(), SpoolRef> {
        self.pushes
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut refs = self.refs.lock().unwrap_or_else(|p| p.into_inner());
        if refs.len() >= DIGEST_QUEUE_CAPACITY {
            return Err(r);
        }
        refs.push_back(r);
        drop(refs);
        self.ready.notify_one();
        Ok(())
    }

    /// Events waiting for the live digest.
    #[cfg(feature = "sqlite")]
    pub(crate) fn len(&self) -> usize {
        self.refs.lock().unwrap_or_else(|p| p.into_inner()).len()
    }

    /// Events offered to the live digest since the start, a full queue's
    /// included.
    #[cfg(feature = "sqlite")]
    pub(crate) fn pushes(&self) -> u64 {
        self.pushes.load(std::sync::atomic::Ordering::Relaxed)
    }

    async fn pop(&self) -> SpoolRef {
        loop {
            let next = self
                .refs
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .pop_front();
            if let Some(r) = next {
                return r;
            }
            self.ready.notified().await;
        }
    }
}

/// The live digest: one task reads and prepares queued events,
/// `Config::digest_prep_workers` at a time off the async runtime and in order, and
/// one committer writes whatever is ready as one transaction, so batches
/// grow with the load and an idle event commits as soon as it is read.
/// Finishing a batch (alerts, file removal) runs beside the next commit.
struct DigestPipeline;

impl DigestPipeline {
    /// Queues `r` for the live digest, starting the pipeline the first
    /// time. A full queue leaves the file to the recovery worker.
    fn push(processors: &Arc<Processors>, pool: &DbPool, r: SpoolRef) {
        let errors = &processors.errors;
        if errors.is_draining() {
            // Its file is the next start's.
            return;
        }
        if errors.queue().try_push(r).is_err() {
            errors.recovery_wake().notify_one();
            return;
        }
        if !errors
            .queue()
            .started
            .swap(true, std::sync::atomic::Ordering::Relaxed)
        {
            Self::start(Arc::clone(processors), pool.clone());
        }
    }

    /// On the runtime the processor was built on, supervised: a panic
    /// drops what was in flight (its guards release, the files stay), the
    /// recovery worker is woken to replay them, and the pipeline starts
    /// again on the queue.
    fn start(processors: Arc<Processors>, pool: DbPool) {
        let _on = processors.errors.runtime().map(|runtime| runtime.enter());
        let mut runs = 0u64;
        crate::workers::supervise("digest pipeline", move || {
            runs += 1;
            if runs > 1 {
                processors.errors.recovery_wake().notify_one();
            }
            Self::run(Arc::clone(&processors), pool.clone())
        });
    }

    async fn run(processors: Arc<Processors>, pool: DbPool) {
        let (tx, rx) = tokio::sync::mpsc::channel(2 * processors.errors.batch_max_events());
        futures_util::join!(
            Self::prep(&processors, &pool, tx),
            Self::commit(&processors, &pool, rx)
        );
    }

    /// Takes queued events in order, each with its share of the memory
    /// budget before its file is read, and sends the prepared ones on.
    async fn prep(
        processors: &Arc<Processors>,
        pool: &DbPool,
        tx: tokio::sync::mpsc::Sender<Box<PreparedEvent>>,
    ) {
        use futures_util::StreamExt as _;
        let errors = &processors.errors;
        // Owned, not borrowed: futures the supervisor spawns must not hold
        // a borrow a closure returns.
        let refs = futures_util::stream::unfold(Arc::clone(processors), |p| async move {
            loop {
                let r = p.errors.queue().pop().await;
                // Given up to the drain: its file is the next start's.
                if !p.errors.is_abandoned() {
                    return Some((r, p));
                }
            }
        });
        let mut prepared = std::pin::pin!(prepare_in_order(processors, pool, refs));
        while let Some(prepared) = prepared.next().await {
            match prepared {
                // A panic dropped the event; recovery replays its file.
                None => errors.recovery_wake().notify_one(),
                Some(Prepared::Ready(e)) => {
                    if tx.send(e).await.is_err() {
                        return;
                    }
                }
                Some(other) => {
                    if let Err(e) = errors.run_prepared(pool, other).await {
                        log::error!("Failed to digest an event; it remains queued: {e:?}");
                    }
                }
            }
        }
    }

    /// Commits whatever is ready as one batch, then hands it to a finisher.
    async fn commit(
        processors: &Arc<Processors>,
        pool: &DbPool,
        mut rx: tokio::sync::mpsc::Receiver<Box<PreparedEvent>>,
    ) {
        let errors = &processors.errors;
        let max = errors.batch_max_events();
        while let Some(first) = rx.recv().await {
            if errors.is_abandoned() {
                continue;
            }
            let mut bytes = first.data_bytes;
            let mut batch = vec![*first];
            if errors.is_halted() {
                // The breaker takes no batches: one event at a time, a
                // probe interval apart.
                if !errors.wait_for_probe().await {
                    continue;
                }
            } else {
                while batch.len() < max && bytes < DIGEST_BATCH_MAX_BYTES {
                    match rx.try_recv() {
                        Ok(e) => {
                            bytes += e.data_bytes;
                            batch.push(*e);
                        }
                        Err(_) => break,
                    }
                }
            }
            let committed = errors.commit(pool, batch, CommitMode::Live).await;
            if committed.is_empty() {
                continue;
            }
            let finisher = errors.finisher().await;
            let (processors, pool) = (Arc::clone(processors), pool.clone());
            tokio::spawn(async move {
                for result in processors.errors.finish(&pool, committed).await {
                    if let Err(e) = result {
                        log::error!("Failed to digest an event; it remains queued: {e:?}");
                    }
                }
                drop(finisher);
            });
        }
    }
}

/// A feedback item's payload with `type` forced to `feedback`, carrying its
/// plain attachments: a minidump or a view hierarchy is not a screenshot.
fn feedback_payload(payload: &[u8], attachments: &[Attachment]) -> AppResult<Bytes> {
    let mut event = parse_event(payload)?;
    event["type"] = "feedback".into();
    let plain = attachments.iter().filter(|a| {
        matches!(
            a.attachment_type.as_deref(),
            None | Some("event.attachment")
        )
    });
    with_attachments(event, plain)
}

/// An item payload that must be a JSON object.
fn parse_event(payload: &[u8]) -> AppResult<serde_json::Value> {
    match serde_json::from_slice(payload) {
        Ok(event @ serde_json::Value::Object(_)) => Ok(event),
        Ok(_) => Err(AppError::Validation(
            "an event must be a JSON object".to_string(),
        )),
        Err(e) => Err(AppError::Validation(format!("Invalid event JSON: {e}"))),
    }
}

/// An event carrying `attachments` to the digest under
/// [`ATTACHMENTS_KEY`], base64-encoded: the stored event file is the only
/// durable copy until the digest commits.
fn with_attachments<'a>(
    mut event: serde_json::Value,
    attachments: impl Iterator<Item = &'a Attachment>,
) -> AppResult<Bytes> {
    use base64::Engine as _;
    let Some(object) = event.as_object_mut() else {
        return Err(AppError::Validation(
            "an event must be a JSON object".to_string(),
        ));
    };

    let kept: Vec<serde_json::Value> = attachments
        .filter(|a| {
            let fits = a.payload.len() <= MAX_ATTACHMENT_SIZE;
            if !fits {
                log::warn!("dropping a {} byte attachment", a.payload.len());
            }
            fits
        })
        .map(|a| {
            serde_json::json!({
                "filename": a.filename.as_deref().unwrap_or("attachment"),
                "content_type": a.content_type.as_deref().unwrap_or("application/octet-stream"),
                "data": base64::engine::general_purpose::STANDARD.encode(&a.payload),
            })
        })
        .collect();
    if kept.is_empty() {
        object.remove(ATTACHMENTS_KEY);
    } else {
        object.insert(ATTACHMENTS_KEY.to_string(), kept.into());
    }
    Ok(Bytes::from(event.to_string()))
}

/// Relay's `UserReport` (relay-event-schema/src/protocol/user_report.rs):
/// a missing or null string is empty.
#[derive(serde::Deserialize)]
pub(crate) struct UserReport {
    event_id: Option<String>,
    #[serde(default, deserialize_with = "null_to_default")]
    pub name: String,
    #[serde(default, deserialize_with = "null_to_default")]
    pub email: String,
    #[serde(default, deserialize_with = "null_to_default")]
    pub comments: String,
}

/// A user report and the feedback event it became.
pub(crate) struct ShimmedReport {
    pub report: UserReport,
    /// The error event the report is about.
    pub event_id: uuid::Uuid,
    pub feedback_id: uuid::Uuid,
    pub payload: Vec<u8>,
}

fn null_to_default<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(<Option<String> as serde::Deserialize>::deserialize(d)?.unwrap_or_default())
}

/// Sentry's `shim_to_feedback`: a legacy user report as a feedback event about
/// `event_id`. The feedback's own id derives from that event id and the
/// report, so a retried report digests into the same feedback rather than a
/// second one, and a different report about the same event is kept.
/// `source` is Sentry's `FeedbackCreationSource` for where it came from.
pub(crate) fn user_report_to_feedback(
    payload: &[u8],
    envelope_event_id: Option<&str>,
    source: &str,
) -> AppResult<ShimmedReport> {
    // Relay trims surrounding whitespace first: an SDK sends a trailing `\n`.
    let report: UserReport = serde_json::from_slice(payload.trim_ascii())
        .map_err(|e| AppError::Validation(format!("Invalid user report JSON: {e}")))?;
    let event_id = report
        .event_id
        .as_deref()
        .or(envelope_event_id)
        .and_then(|id| uuid::Uuid::parse_str(id).ok())
        .ok_or_else(|| AppError::Validation("user report needs a valid event_id".to_string()))?;

    let digest = Sha256::digest(
        serde_json::json!([
            "user_report",
            event_id.simple().to_string(),
            report.name,
            report.email,
            report.comments,
        ])
        .to_string()
        .as_bytes(),
    );
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest[..16]);
    let feedback_id = uuid::Uuid::from_bytes(bytes);

    let event = serde_json::json!({
        "type": "feedback",
        "event_id": feedback_id.simple().to_string(),
        "timestamp": Utc::now().timestamp_millis() as f64 / 1000.0,
        "contexts": {
            "feedback": {
                "message": report.comments,
                "contact_email": report.email,
                "name": report.name,
                "associated_event_id": event_id.simple().to_string(),
                "source": source,
            }
        }
    });
    // The digest's checks, run here so the endpoint can answer 400 instead of
    // acknowledging a report the digest would drop.
    crate::digest::processors::Feedback::from_event(&event)?;
    Ok(ShimmedReport {
        payload: event.to_string().into_bytes(),
        report,
        event_id,
        feedback_id,
    })
}

/// Digests one stored event, inline. Failures leave the durable file in
/// place for the recovery worker. The digest counts its own outcome.
pub async fn digest_stored_event(processors: &Processors, pool: &DbPool, metadata: &EventMetadata) {
    match processors.errors.digest(pool, metadata).await {
        Ok(Digested::Missing) => {
            log::error!(
                "Failed to digest event {}: no file holds it",
                metadata.event_id
            );
        }
        Ok(_) => {}
        Err(e) => {
            log::error!(
                "Failed to digest event {}; it remains queued: {:?}",
                metadata.event_id,
                e
            );
        }
    }
}

/// Digests the spool files `refs` names as one batch, with the metadata each
/// was stored with: read and parsed `Config::digest_prep_workers` at a time off the
/// async runtime, committed together, finished in order. One result per
/// ref, in the same order. A file this release cannot read is settled the
/// way recovery settles it.
pub async fn digest_refs(
    processors: &Arc<Processors>,
    pool: &DbPool,
    refs: Vec<crate::digest::processors::SpoolRef>,
) -> Vec<AppResult<Digested>> {
    let prepared = prep_refs(processors, pool, refs).await;
    commit_prepared(processors, pool, prepared).await
}

/// The first half of [`digest_refs`].
async fn prep_refs(
    processors: &Arc<Processors>,
    pool: &DbPool,
    refs: Vec<SpoolRef>,
) -> Vec<Option<Prepared>> {
    use futures_util::StreamExt as _;
    prepare_in_order(processors, pool, futures_util::stream::iter(refs))
        .collect()
        .await
}

/// Each file read and prepared, in order and `prep_workers` at a time, with
/// its share of the memory budget taken before it is read. `None` when the
/// preparation panicked.
fn prepare_in_order<'a>(
    processors: &'a Arc<Processors>,
    pool: &'a DbPool,
    refs: impl futures_util::Stream<Item = SpoolRef> + 'a,
) -> impl futures_util::Stream<Item = Option<Prepared>> + 'a {
    use futures_util::StreamExt as _;
    let runtime = tokio::runtime::Handle::current();
    // ponytail: `block_on` inside a blocking thread is the smallest way to
    // run the async symbolication off the current-thread main runtime; split
    // prep into an async part and a CPU part if profiles show I/O stalls.
    refs.then({
        let p = Arc::clone(processors);
        move |r: SpoolRef| {
            let p = Arc::clone(&p);
            async move {
                let budget = p.errors.reserve(r.size).await;
                (r, budget)
            }
        }
    })
    .map(move |(r, budget)| {
        let (processors, pool, runtime) = (Arc::clone(processors), pool.clone(), runtime.clone());
        async move {
            tokio::task::spawn_blocking(move || {
                runtime.block_on(processors.errors.prepare(&pool, r, budget, None))
            })
            .await
            .ok()
        }
    })
    .buffered(processors.errors.prep_workers())
}

/// The second half of [`digest_refs`]: one commit, then the finish.
async fn commit_prepared(
    processors: &Arc<Processors>,
    pool: &DbPool,
    prepared: Vec<Option<Prepared>>,
) -> Vec<AppResult<Digested>> {
    let errors = &processors.errors;
    let mut results: Vec<Option<AppResult<Digested>>> = std::iter::repeat_with(|| None)
        .take(prepared.len())
        .collect();
    let (mut ready, mut ready_at) = (Vec::new(), Vec::new());
    for (i, prepared) in prepared.into_iter().enumerate() {
        results[i] = match prepared {
            // Its file stays; a later pass reads it again.
            None => Some(Err(AppError::Internal(
                "preparing the event panicked".to_string(),
            ))),
            Some(Prepared::Ready(e)) => {
                ready.push(*e);
                ready_at.push(i);
                None
            }
            Some(Prepared::Skipped(r, skip)) => Some(errors.settle_skipped(r, skip).await),
            Some(other) => Some(errors.run_prepared(pool, other).await),
        };
    }
    if !ready.is_empty() {
        let committed = errors.commit(pool, ready, CommitMode::Once).await;
        for (i, result) in ready_at
            .into_iter()
            .zip(errors.finish(pool, committed).await)
        {
            results[i] = Some(result);
        }
    }
    results
        .into_iter()
        .map(|r| r.unwrap_or_else(|| Err(AppError::Internal("no digest result".to_string()))))
        .collect()
}

/// Replays event files left by a previous process after a digest failure.
pub async fn recover_pending_events(
    pool: DbPool,
    processors: Arc<Processors>,
    ingest_dir: std::path::PathBuf,
) {
    let mut previous_delay = None;

    loop {
        let has_pending =
            recover_pending_events_once(pool.clone(), processors.clone(), ingest_dir.clone()).await;
        let delay = next_recovery_delay(previous_delay, has_pending);
        previous_delay = has_pending.then_some(delay);
        // The live queue overflowing (or its pipeline restarting) wakes
        // the worker early: those files have no owner now.
        tokio::select! {
            () = tokio::time::sleep(delay) => {}
            () = processors.errors.recovery_wake().notified() => {}
        }
    }
}

/// Processes the current set of pending event files once, in batches of
/// `Config::digest_batch_max_events` and at most half of `Config::digest_memory_bytes`, so the
/// live digest always has the other half. `true` when some are still
/// waiting.
///
/// While the circuit breaker holds the digest halted a pass does nothing,
/// except once a probe interval, when it tries one event.
pub async fn recover_pending_events_once(
    pool: DbPool,
    processors: Arc<Processors>,
    ingest_dir: std::path::PathBuf,
) -> bool {
    let errors = &processors.errors;
    let probe = errors.is_halted();
    if probe && !errors.take_probe() {
        return true;
    }
    // Files owned by a task in this process (digest in flight, or committed
    // and awaiting its alerts and file removal) are not leftovers: replaying
    // one would digest the event twice and race that task for the file.
    let in_flight = errors.in_flight();
    let listing = match list_pending_names(&ingest_dir, |project_id, event_id| {
        in_flight.contains(project_id, event_id)
    })
    .await
    {
        Ok(listing) => listing,
        Err(e) => {
            log::error!("Failed to scan pending event files: {:?}", e);
            return true;
        }
    };

    let max_events = if probe { 1 } else { errors.batch_max_events() };
    let max_bytes = errors.budget_bytes() / 2;
    let mut chunks: Vec<&[(i32, uuid::Uuid, u64)]> = Vec::new();
    let mut rest = &listing[..];
    while !rest.is_empty() {
        let mut bytes = 0;
        let n = rest
            .iter()
            .take(max_events)
            .take_while(|(_, _, size)| {
                bytes += size;
                bytes <= max_bytes
            })
            .count()
            .max(1);
        let (chunk, tail) = rest.split_at(n);
        chunks.push(chunk);
        rest = tail;
    }
    let mut has_pending = false;
    // Re-checked per file as each chunk is taken: a task can have claimed
    // one since the listing, or finished one listed earlier.
    let mut refs = chunks.into_iter().filter_map(|names| {
        let refs: Vec<_> = names
            .iter()
            .filter(|(project_id, event_id, _)| !in_flight.contains(*project_id, *event_id))
            .map(|&(project_id, event_id, size)| SpoolRef {
                project_id,
                event_id,
                size,
                owner: in_flight.register(project_id, event_id),
            })
            .collect();
        (!refs.is_empty()).then_some(refs)
    });
    let prep = |refs: Vec<SpoolRef>| {
        let ids: Vec<uuid::Uuid> = refs.iter().map(|r| r.event_id).collect();
        let (processors, pool) = (&processors, &pool);
        async move { (ids, prep_refs(processors, pool, refs).await) }
    };
    let mut next = match refs.next() {
        Some(first) => Some(prep(first).await),
        None => None,
    };
    while let Some((ids, prepared)) = next.take() {
        if errors.is_draining() {
            release(&processors, &pool, prepared).await;
            return true;
        }
        // The next chunk is read while this one commits: each is at most
        // half the budget, so the two fit. Its commit takes the same gate
        // as the live digest's, so the two interleave by batch.
        let (results, following) =
            futures_util::join!(commit_prepared(&processors, &pool, prepared), async {
                match refs.next() {
                    Some(chunk) => Some(prep(chunk).await),
                    None => None,
                }
            });
        next = following;
        for (event_id, digested) in ids.into_iter().zip(results) {
            match digested {
                Ok(Digested::Stored)
                | Ok(Digested::Skipped)
                | Ok(Digested::RateLimited)
                | Ok(Digested::DeadLettered) => {}
                // Listed a moment ago, finished and deleted by its owner since:
                // nothing left to replay.
                Ok(Digested::Missing) => log::debug!(
                    "Pending digest {event_id} was completed by its owner before replay"
                ),
                Err(e) => {
                    has_pending = true;
                    log::warn!("Pending digest {event_id} remains queued: {e:?}");
                }
            }
        }
        // Halted (or still halted after a probe): the rest waits.
        if probe || errors.is_halted() {
            if let Some((_, prepared)) = next {
                release(&processors, &pool, prepared).await;
            }
            return true;
        }
    }
    has_pending
}

/// Lets prepared events go uncommitted. Those already over before their
/// write (dropped by the quota, invalid, unreadable) are settled, so none
/// stays counted as pending; the others' files stay for a later pass.
async fn release(processors: &Arc<Processors>, pool: &DbPool, prepared: Vec<Option<Prepared>>) {
    let errors = &processors.errors;
    for p in prepared.into_iter().flatten() {
        match p {
            Prepared::Ready(_) => {}
            Prepared::Skipped(r, skip) => {
                let _ = errors.settle_skipped(r, skip).await;
            }
            other => {
                let _ = errors.run_prepared(pool, other).await;
            }
        }
    }
}

fn next_recovery_delay(
    previous_delay: Option<std::time::Duration>,
    has_pending: bool,
) -> std::time::Duration {
    const MIN: std::time::Duration = std::time::Duration::from_secs(1);
    const MAX: std::time::Duration = std::time::Duration::from_secs(30);

    if !has_pending {
        return MAX;
    }
    previous_delay
        .map(|delay| delay.saturating_mul(2).min(MAX))
        .unwrap_or(MIN)
}

/// The [`ProcessorCtx`] of an item stored inline. `event_id` is what the
/// item's rows are keyed by: the event id of a transaction, the delivery id of
/// a log batch, nothing for spans.
fn item_ctx(
    pool: &DbPool,
    project: &Project,
    event_id: uuid::Uuid,
    ingested_at: chrono::DateTime<Utc>,
    remote_addr: &Option<String>,
) -> ProcessorCtx {
    ProcessorCtx {
        pool: pool.clone(),
        project_id: project.id,
        event_id,
        ingested_at,
        remote_addr: remote_addr.clone(),
        keep_ai_spans: project.keeps(Product::Agents),
    }
}

fn stable_delivery_id(event_id: Option<&str>, envelope: &[u8]) -> uuid::Uuid {
    event_id
        .and_then(|id| uuid::Uuid::parse_str(id).ok())
        .unwrap_or_else(|| {
            let digest = Sha256::digest(envelope);
            let mut bytes = [0; 16];
            bytes.copy_from_slice(&digest[..16]);
            uuid::Uuid::from_bytes(bytes)
        })
}

fn resolve_event_id(
    event_id: Option<String>,
    delivery_id: uuid::Uuid,
    requires_event_id: bool,
) -> Option<String> {
    if requires_event_id {
        Some(event_id.unwrap_or_else(|| delivery_id.simple().to_string()))
    } else {
        event_id
    }
}

/// Records a check-in and reports any failure it causes.
pub async fn process_check_in(
    processors: &Processors,
    pool: &DbPool,
    project_id: i32,
    check_in: AppResult<CheckIn>,
    received: chrono::DateTime<Utc>,
) -> AppResult<()> {
    let check_in = check_in?;
    // `Sentry.withMonitor` sends a run's in_progress (which carries the
    // monitor_config) and its result as two concurrent requests, so on a
    // job's first run the result can arrive before the upsert has created
    // the monitor. A short wait lets the upsert land instead of losing the
    // run's outcome.
    let mut retries = [100, 250, 500].into_iter();
    let failures = loop {
        match MonitorService::process_check_in(pool, project_id, check_in.clone(), received).await {
            Err(AppError::NotFound(_)) if check_in.monitor_config.is_none() => {
                match retries.next() {
                    Some(ms) => tokio::time::sleep(std::time::Duration::from_millis(ms)).await,
                    None => {
                        return Err(AppError::NotFound(format!(
                            "Monitor {} does not exist; send a monitor_config to create it",
                            check_in.monitor_slug
                        )))
                    }
                }
            }
            other => break other?,
        }
    };
    report_monitor_failures(processors, pool, failures).await;
    Ok(())
}

/// Marks missed and timed-out check-ins as of `now` and reports the
/// failures. Returns how many were reported.
pub async fn run_monitor_sweep(
    processors: &Processors,
    pool: &DbPool,
    now: chrono::DateTime<Utc>,
) -> AppResult<usize> {
    let failures = MonitorService::sweep(pool, now).await?;
    let reported = failures.len();
    report_monitor_failures(processors, pool, failures).await;
    Ok(reported)
}

/// Sends each failure through the error digest as a synthetic event, so a
/// failing monitor gets an issue, regressions and alerts like any error.
/// The event file is stored first, so a digest that fails here is replayed
/// by the recovery worker.
async fn report_monitor_failures(
    processors: &Processors,
    pool: &DbPool,
    failures: Vec<MonitorFailure>,
) {
    for failure in failures {
        let event_id = uuid::Uuid::new_v4();
        let metadata = EventMetadata {
            event_id: event_id.simple().to_string(),
            project_id: failure.project_id,
            ingested_at: Utc::now(),
            remote_addr: None,
        };
        let payload = failure.to_event(event_id).to_string();
        let _in_flight = processors
            .errors
            .in_flight()
            .register(failure.project_id, event_id);
        if let Err(e) = store_event_with_metadata(
            processors.errors.ingest_dir(),
            &metadata.event_id,
            payload.as_bytes(),
            &metadata,
        )
        .await
        {
            log::error!(
                "Failed to store the failure event of monitor {}: {:?}",
                failure.monitor_slug,
                e
            );
            continue;
        }
        digest_stored_event(processors, pool, &metadata).await;
    }
}

/// The product an envelope item belongs to, when it is one a project can turn
/// off. Error events are not: issues are always kept.
fn product_of(item: &EnvelopeItemKind) -> Option<Product> {
    match item {
        EnvelopeItemKind::Transaction(_)
        | EnvelopeItemKind::Span(_)
        | EnvelopeItemKind::SpanV2Batch(_) => Some(Product::Performance),
        EnvelopeItemKind::Session(_) | EnvelopeItemKind::Sessions(_) => Some(Product::Releases),
        EnvelopeItemKind::Log(_) => Some(Product::Logs),
        EnvelopeItemKind::CheckIn(_) => Some(Product::Crons),
        EnvelopeItemKind::Feedback(_) | EnvelopeItemKind::UserReport(_) => Some(Product::Feedback),
        EnvelopeItemKind::Event(_)
        | EnvelopeItemKind::Attachment(_)
        | EnvelopeItemKind::Other(..) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{next_recovery_delay, resolve_event_id, stable_delivery_id, validate_event_json};
    use std::time::Duration;

    #[test]
    fn recovery_backoff_starts_small_and_caps() {
        assert_eq!(next_recovery_delay(None, true), Duration::from_secs(1));
        assert_eq!(
            next_recovery_delay(Some(Duration::from_secs(8)), true),
            Duration::from_secs(16)
        );
        assert_eq!(
            next_recovery_delay(Some(Duration::from_secs(30)), true),
            Duration::from_secs(30)
        );
    }

    #[test]
    fn recovery_backoff_resets_when_idle() {
        assert_eq!(
            next_recovery_delay(Some(Duration::from_secs(16)), false),
            Duration::from_secs(30)
        );
    }

    #[test]
    fn headerless_delivery_id_is_stable_for_retries() {
        let first = stable_delivery_id(None, b"same envelope");
        assert_eq!(first, stable_delivery_id(None, b"same envelope"));
        assert_ne!(first, stable_delivery_id(None, b"different envelope"));
    }

    #[test]
    fn valid_event_id_remains_the_delivery_id() {
        let event_id = uuid::Uuid::new_v4();
        assert_eq!(
            stable_delivery_id(Some(&event_id.to_string()), b"ignored"),
            event_id
        );
    }

    #[test]
    fn invalid_event_id_uses_the_same_fallback_as_a_missing_header() {
        let fallback = stable_delivery_id(None, b"same envelope");
        assert_eq!(
            stable_delivery_id(Some("not-a-uuid"), b"same envelope"),
            fallback
        );

        let event_id = uuid::Uuid::new_v4();
        assert_eq!(
            stable_delivery_id(Some(&event_id.to_string()), b"different envelope"),
            event_id
        );
    }

    #[test]
    fn headerless_required_event_uses_delivery_id() {
        let delivery_id = uuid::Uuid::nil();
        assert_eq!(
            resolve_event_id(None, delivery_id, true),
            Some(delivery_id.simple().to_string())
        );
    }

    #[test]
    fn session_event_id_is_not_synthesized() {
        assert_eq!(resolve_event_id(None, uuid::Uuid::nil(), false), None);
        assert_eq!(
            resolve_event_id(Some("session-id".to_string()), uuid::Uuid::nil(), false),
            Some("session-id".to_string())
        );
    }

    #[test]
    fn valid_event_json_passes() {
        let payload = br#"{"event_id":"abc","exception":{"values":[{"type":"Error"}]}}"#;
        assert!(validate_event_json(payload).is_ok());
    }

    #[test]
    fn invalid_event_json_is_rejected() {
        assert!(validate_event_json(b"{not json").is_err());
        assert!(validate_event_json(b"").is_err());
        // Trailing garbage after the value must be rejected too, matching
        // the previous full-parse semantics.
        assert!(validate_event_json(br#"{"a":1} garbage"#).is_err());
    }
}
