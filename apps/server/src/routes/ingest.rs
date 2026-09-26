use actix_web::{web, HttpRequest, HttpResponse};
use bytes::Bytes;
use chrono::Utc;
use sha2::{Digest as _, Sha256};

use crate::auth::SentryAuth;
use crate::config::Config;
use crate::db::DbPool;
use crate::digest::processors::{
    is_retryable_write_contention, Digested, Processor, ProcessorCtx, Processors, SessionItem,
};
use crate::error::{AppError, AppResult};
use crate::ingest::storage::{is_missing_event_file, list_pending_event_metadata_except};
use crate::ingest::{
    decompress_body, detach_payload_if_needed, get_content_encoding, store_event_with_metadata,
    EnvelopeItemKind, EnvelopeParser, EventMetadata, MAX_COMPRESSED_SIZE,
};
use crate::services::RateLimitService;

/// Response for successful ingestion.
/// `id` is absent for session-only envelopes, mirroring Relay's StoreResponse.
#[derive(serde::Serialize)]
pub struct IngestResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

/// Validates an event payload is well-formed JSON without building a
/// [`serde_json::Value`] tree: [`IgnoredAny`](serde::de::IgnoredAny) walks and
/// discards every value. The digest re-parses the stored file anyway, so the
/// tree here would be pure waste and can multiply memory use for large events.
fn validate_event_json(payload: &[u8]) -> AppResult<()> {
    serde_json::from_slice::<serde::de::IgnoredAny>(payload)
        .map_err(|e| AppError::Validation(format!("Invalid event JSON: {}", e)))?;
    Ok(())
}

/// POST /api/{project_id}/envelope/
/// Main ingestion endpoint compatible with Sentry SDK
pub async fn ingest_envelope(
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    req: HttpRequest,
    auth: SentryAuth,
    body: Bytes,
    processors: web::Data<Processors>,
) -> AppResult<HttpResponse> {
    // 0. Check rate limits (fail fast before processing)
    if let Some(exceeded) =
        RateLimitService::check_quota(pool.get_ref(), &auth.project, &config.rate_limit).await?
    {
        log::warn!(
            "Rate limit exceeded for project {}: retry_after={}s",
            auth.project.id,
            exceeded.retry_after
        );
        return Ok(HttpResponse::TooManyRequests()
            .insert_header(("Retry-After", exceeded.retry_after.to_string()))
            .insert_header(("X-Sentry-Rate-Limits", exceeded.sentry_rate_limits_header()))
            .json(serde_json::json!({
                "error": "rate_limit_exceeded",
                "retry_after": exceeded.retry_after
            })));
    }

    let ingested_at = Utc::now();
    // 1. Get client IP
    let remote_addr = req
        .connection_info()
        .realip_remote_addr()
        .map(|s| s.to_string());

    // 2. Decompress if needed
    let content_encoding = get_content_encoding(&req)?;
    let decompressed = decompress_body(body, content_encoding.as_deref())?;

    // 3. Parse envelope
    let mut parser = EnvelopeParser::new(decompressed);
    let envelope = parser.parse()?;

    // Relay parity for item payloads: a malformed item is dropped and its
    // siblings continue (relay-server/src/processing/relay.rs run_one — "This
    // is not a fatal error case ... other items from the same original
    // envelope must still be processed"). Only infrastructure failures
    // (database, storage) may fail the envelope: they return 5xx so the SDK
    // retries, preserving the commit-before-ack durability promise. A 4xx here
    // would make the SDK drop the sibling data permanently.
    fn drop_malformed_item(result: AppResult<()>, item_type: &str) -> AppResult<()> {
        match result {
            Err(AppError::Validation(message)) => {
                log::warn!(
                    "dropping malformed {item_type} item, keeping envelope siblings: {message}"
                );
                Ok(())
            }
            other => other,
        }
    }

    // 4. Typed dispatch — exhaustive, compiler-verified.
    //    event_id validation is deferred until after the loop — session-only envelopes never need
    //    one (Relay: Item::requires_event() returns false for Session/Sessions).
    let mut event_item: Option<Bytes> = None;
    let mut transaction_item: Option<Bytes> = None;
    let mut span_items: Vec<Bytes> = Vec::new();
    let mut span_v2_items: Vec<Bytes> = Vec::new();
    let mut requires_event_id = false;
    let envelope_len = parser.data().len();
    let delivery_id = stable_delivery_id(envelope.headers.event_id.as_deref(), parser.data());
    for item_kind in envelope.items {
        if item_kind.requires_event() {
            requires_event_id = true;
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
            EnvelopeItemKind::Session(s) => {
                let ctx = direct_store_ctx(
                    &pool,
                    auth.project.id,
                    delivery_id,
                    ingested_at,
                    &remote_addr,
                );
                drop_malformed_item(
                    processors
                        .sessions
                        .process(SessionItem::Update(s), &ctx)
                        .await,
                    "session",
                )?;
            }
            EnvelopeItemKind::Sessions(s) => {
                let ctx = direct_store_ctx(
                    &pool,
                    auth.project.id,
                    delivery_id,
                    ingested_at,
                    &remote_addr,
                );
                drop_malformed_item(
                    processors
                        .sessions
                        .process(SessionItem::Aggregates(s), &ctx)
                        .await,
                    "sessions",
                )?;
            }
            EnvelopeItemKind::Log(payload) => {
                // Logs are processed inline (parse container + store): the work is
                // bounded and storing before responding keeps the batch durable.
                let ctx = direct_store_ctx(
                    &pool,
                    auth.project.id,
                    delivery_id,
                    ingested_at,
                    &remote_addr,
                );
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
            EnvelopeItemKind::Other(t, _) => {
                log::debug!("envelope item '{}' ignored", t);
            }
        }
    }
    drop(parser);

    // 5. Resolve event_id — only required for event-bearing item types.
    //    If the SDK omitted it, derive it from the durable delivery identity.
    //    For session-only envelopes, pass through whatever the SDK provided (may be None).
    let event_id = resolve_event_id(envelope.headers.event_id, delivery_id, requires_event_id);
    if requires_event_id {
        if let Some(id) = &event_id {
            uuid::Uuid::parse_str(id)
                .map_err(|_| AppError::Validation("event_id must be a valid UUID".to_string()))?;
        }
    }

    // Persist direct items before the early return; otherwise 200 could suppress
    // an SDK retry for data that was never stored.
    if let Some(txn_payload) = transaction_item {
        let event_id_txn = event_id
            .clone()
            .unwrap_or_else(|| delivery_id.simple().to_string());
        let parsed_id =
            uuid::Uuid::parse_str(&event_id_txn).unwrap_or_else(|_| uuid::Uuid::new_v4());
        let ctx = ProcessorCtx {
            pool: pool.get_ref().clone(),
            project_id: auth.project.id,
            event_id: parsed_id,
            ingested_at,
            remote_addr: remote_addr.clone(),
        };
        drop_malformed_item(
            processors.transactions.process(txn_payload, &ctx).await,
            "transaction",
        )?;
    }

    // Persist standalone spans inline; they have no grouping or issue path.
    if !span_items.is_empty() {
        let ctx = ProcessorCtx {
            pool: pool.get_ref().clone(),
            project_id: auth.project.id,
            event_id: uuid::Uuid::nil(),
            ingested_at,
            remote_addr: remote_addr.clone(),
        };
        // One transaction for the whole span batch: per-item autocommit
        // INSERTs would each be their own SQLite transaction.
        drop_malformed_item(
            processors.spans.process_batch(span_items, &ctx).await,
            "span",
        )?;
    }

    // Persist v2 span batches inline for the same reason.
    if !span_v2_items.is_empty() {
        let ctx = ProcessorCtx {
            pool: pool.get_ref().clone(),
            project_id: auth.project.id,
            event_id: uuid::Uuid::nil(),
            ingested_at,
            remote_addr: remote_addr.clone(),
        };
        for batch_payload in span_v2_items {
            drop_malformed_item(
                processors.spans_v2.process(batch_payload, &ctx).await,
                "span container",
            )?;
        }
    }

    // Return for session-only and other non-event envelopes.
    let event_item = match event_item {
        Some(item) => item,
        None => {
            log::debug!("No event item in envelope");
            return Ok(HttpResponse::Ok().json(IngestResponse { id: event_id }));
        }
    };

    let event_id = event_id.expect("event_id is Some when event_item is Some");
    let event_uuid = uuid::Uuid::parse_str(&event_id)
        .map_err(|_| AppError::Validation("event_id must be a valid UUID".to_string()))?;

    // 6. Validate that the payload is valid JSON without building a Value tree;
    //    the digest re-parses the stored file.
    validate_event_json(&event_item)?;

    // Store metadata beside the raw event so a failed digest can be recovered.
    let metadata = EventMetadata {
        event_id: event_id.clone(),
        project_id: auth.project.id,
        ingested_at,
        remote_addr,
    };
    // Claimed before the file exists so the recovery worker never sees a
    // pending file that no task owns. Released when the spawned digest ends,
    // or here if the store fails and there is nothing to recover.
    let in_flight = processors
        .errors
        .in_flight()
        .register(auth.project.id, event_uuid);
    store_event_with_metadata(
        processors.errors.ingest_dir(),
        &event_id,
        &event_item,
        &metadata,
    )
    .await?;

    // The digest worker owns grouping, issue creation, and its durable retry path.
    let processors = processors.clone();
    let pool_clone = pool.get_ref().clone();
    tokio::spawn(async move {
        // The permit gates the whole digest: the file read, JSON parse and
        // grouping working set stay bounded to the concurrent cap, not the
        // burst size.
        let _permit = processors.processing_slot.acquire().await;
        // Boxed here, after the permit: the digest's state machine is several
        // kilobytes, and a burst leaves thousands of tasks waiting on the
        // permit. Unboxed, each waiter carried the whole thing inline and
        // the backlog's memory grew with the burst rather than the cap.
        Box::pin(digest_stored_event(&processors, &pool_clone, &metadata)).await;
        drop(in_flight);
    });

    Ok(HttpResponse::Ok().json(IngestResponse { id: Some(event_id) }))
}

/// Digests one stored event, retrying transient write contention. Failures
/// leave the durable file in place for the recovery worker.
pub async fn digest_stored_event(processors: &Processors, pool: &DbPool, metadata: &EventMetadata) {
    let ctx = ProcessorCtx {
        pool: pool.clone(),
        project_id: metadata.project_id,
        event_id: uuid::Uuid::parse_str(&metadata.event_id).unwrap_or_else(|_| uuid::Uuid::nil()),
        ingested_at: metadata.ingested_at,
        remote_addr: None,
    };
    for attempt in 0..4 {
        match processors.errors.process_ref(metadata, &ctx).await {
            Ok(Digested::Stored) => {
                processors.counters().digest_ok();
                break;
            }
            Ok(Digested::RateLimited) => {
                processors.counters().digest_rate_limited();
                break;
            }
            Err(e) if is_retryable_write_contention(&e) && attempt < 3 => {
                let delay = std::time::Duration::from_millis(250 << attempt);
                log::warn!(
                    "Digest {} remains locked; retrying after {:?}: {:?}",
                    metadata.event_id,
                    delay,
                    e
                );
                tokio::time::sleep(delay).await;
            }
            Err(e) => {
                processors.counters().digest_failed();
                log::error!(
                    "Failed to digest event {}; it remains queued: {:?}",
                    metadata.event_id,
                    e
                );
                break;
            }
        }
    }
}

/// Replays event files left by a previous process after a digest failure.
pub async fn recover_pending_events(
    pool: DbPool,
    processors: web::Data<Processors>,
    ingest_dir: std::path::PathBuf,
) {
    let mut previous_delay = None;

    loop {
        let has_pending = recover_pending_events_once_with_status(
            pool.clone(),
            processors.clone(),
            ingest_dir.clone(),
        )
        .await;
        let delay = next_recovery_delay(previous_delay, has_pending);
        previous_delay = has_pending.then_some(delay);
        tokio::time::sleep(delay).await;
    }
}

/// Processes the current set of pending event files once.
pub async fn recover_pending_events_once(
    pool: DbPool,
    processors: web::Data<Processors>,
    ingest_dir: std::path::PathBuf,
) {
    let _ = recover_pending_events_once_with_status(pool, processors, ingest_dir).await;
}

async fn recover_pending_events_once_with_status(
    pool: DbPool,
    processors: web::Data<Processors>,
    ingest_dir: std::path::PathBuf,
) -> bool {
    // Files owned by a task in this process (digest in flight, or committed
    // and awaiting its durability checkpoint) are not leftovers: replaying
    // one would digest the event twice and race that task for the file.
    let in_flight = processors.errors.in_flight();
    let pending = match list_pending_event_metadata_except(&ingest_dir, |project_id, event_id| {
        in_flight.contains(project_id, event_id)
    })
    .await
    {
        Ok(pending) => pending,
        Err(e) => {
            log::error!("Failed to scan pending event metadata: {:?}", e);
            return true;
        }
    };

    let mut has_pending = false;
    for metadata in pending {
        // Re-checked per file: the listing was filtered, but a task can have
        // claimed a legacy-named file since, or finished one listed earlier.
        let owned = uuid::Uuid::parse_str(&metadata.event_id)
            .is_ok_and(|event_id| in_flight.contains(metadata.project_id, event_id));
        if owned {
            continue;
        }
        let ctx = ProcessorCtx {
            pool: pool.clone(),
            project_id: metadata.project_id,
            event_id: uuid::Uuid::parse_str(&metadata.event_id)
                .unwrap_or_else(|_| uuid::Uuid::nil()),
            ingested_at: metadata.ingested_at,
            remote_addr: None,
        };
        let result = processors.errors.process_ref(&metadata, &ctx).await;
        if let Ok(Digested::RateLimited) = result {
            processors.counters().digest_rate_limited();
        }
        if let Err(e) = result {
            if is_missing_event_file(&e) {
                // Listed a moment ago, finished and deleted by its owner
                // since: nothing left to replay.
                log::debug!(
                    "Pending digest {} was completed by its owner before replay",
                    metadata.event_id
                );
                continue;
            }
            has_pending = true;
            log::warn!(
                "Pending digest {} remains queued: {:?}",
                metadata.event_id,
                e
            );
        }
    }
    has_pending
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

/// Builds a [`ProcessorCtx`] for session items, which carry no event id.
fn direct_store_ctx(
    pool: &web::Data<DbPool>,
    project_id: i32,
    event_id: uuid::Uuid,
    ingested_at: chrono::DateTime<Utc>,
    remote_addr: &Option<String>,
) -> ProcessorCtx {
    ProcessorCtx {
        pool: pool.get_ref().clone(),
        project_id,
        event_id,
        ingested_at,
        remote_addr: remote_addr.clone(),
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

/// POST /api/{project_id}/store/
/// Legacy endpoint (deprecated)
pub async fn ingest_store(
    _pool: web::Data<DbPool>,
    _config: web::Data<Config>,
    _req: HttpRequest,
    _auth: SentryAuth,
    _body: Bytes,
) -> AppResult<HttpResponse> {
    Err(AppError::Validation(
        "The /store/ endpoint is deprecated. Please use /envelope/ instead.".to_string(),
    ))
}

/// OPTIONS for CORS preflight (handled by middleware, but kept for explicit routing)
pub async fn options() -> HttpResponse {
    HttpResponse::Ok().finish()
}

/// Configures the ingest routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/{project_id}")
            .app_data(web::PayloadConfig::new(MAX_COMPRESSED_SIZE))
            .route("/envelope/", web::post().to(ingest_envelope))
            .route(
                "/envelope/",
                web::method(actix_web::http::Method::OPTIONS).to(options),
            )
            .route("/store/", web::post().to(ingest_store))
            .route(
                "/store/",
                web::method(actix_web::http::Method::OPTIONS).to(options),
            ),
    );
}

#[cfg(test)]
mod tests {
    use super::validate_event_json;
    use super::{next_recovery_delay, resolve_event_id, stable_delivery_id};
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
