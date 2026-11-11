use actix_web::{http::header, web, HttpRequest, HttpResponse};
use bytes::Bytes;
use chrono::Utc;

use crate::auth::SentryAuth;
use crate::config::Config;
use crate::db::DbPool;
use crate::digest::processors::Processors;
use crate::error::{AppError, AppResult};
use crate::ingest::{decompression, pipeline, read_body, MAX_COMPRESSED_SIZE};
use crate::models::monitor::{CheckIn, CheckInStatus};
use crate::models::Product;
use crate::services::RateLimitService;

/// Response for successful ingestion.
/// `id` is absent for session-only envelopes, mirroring Relay's StoreResponse.
#[derive(serde::Serialize)]
pub struct IngestResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

pub async fn ingest_envelope(
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    req: HttpRequest,
    auth: SentryAuth,
    payload: web::Payload,
    processors: web::Data<Processors>,
) -> AppResult<HttpResponse> {
    ingest(pool, config, req, auth, payload, processors, false).await
}

/// Relay's legacy store endpoint: a JSON event becomes the one item of an
/// envelope, and an envelope body is taken as it is.
pub async fn ingest_store(
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    req: HttpRequest,
    auth: SentryAuth,
    payload: web::Payload,
    processors: web::Data<Processors>,
) -> AppResult<HttpResponse> {
    let is_envelope = req
        .headers()
        .get(actix_web::http::header::CONTENT_TYPE)
        .is_some_and(|v| v.as_bytes().starts_with(b"application/x-sentry-envelope"));
    ingest(pool, config, req, auth, payload, processors, !is_envelope).await
}

/// The few fields Relay's `parse_event` reads before building the envelope.
#[derive(serde::Deserialize)]
struct MinimalEvent {
    event_id: Option<String>,
    #[serde(rename = "type")]
    ty: Option<String>,
}

fn event_envelope(event: Bytes) -> AppResult<Bytes> {
    let minimal: MinimalEvent = serde_json::from_slice(&event)
        .map_err(|e| AppError::Validation(format!("Invalid event JSON: {e}")))?;
    let header = match minimal.event_id {
        Some(id) => serde_json::json!({ "event_id": id }),
        None => serde_json::json!({}),
    };
    let item_type = match minimal.ty.as_deref() {
        Some("transaction") => "transaction",
        _ => "event",
    };
    let mut envelope = format!(
        "{header}\n{{\"type\":\"{item_type}\",\"length\":{}}}\n",
        event.len()
    )
    .into_bytes();
    envelope.extend_from_slice(&event);
    Ok(Bytes::from(envelope))
}

async fn ingest(
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    req: HttpRequest,
    auth: SentryAuth,
    payload: web::Payload,
    processors: web::Data<Processors>,
    is_event: bool,
) -> AppResult<HttpResponse> {
    if !decompression::is_supported(decompression::content_encoding(&req)?.as_deref()) {
        return Ok(HttpResponse::UnsupportedMediaType()
            .insert_header((header::ACCEPT_ENCODING, decompression::ACCEPT_ENCODING))
            .finish());
    }
    let body = read_body(&req, payload).await?;
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

    let body = if is_event {
        event_envelope(body)?
    } else {
        body
    };
    let ingested_at = Utc::now();
    let remote_addr = req
        .connection_info()
        .realip_remote_addr()
        .map(|s| s.to_string());
    let id = pipeline::process_envelope(
        &processors,
        pool.get_ref(),
        &auth.project,
        body,
        remote_addr,
        ingested_at,
    )
    .await?;
    Ok(HttpResponse::Ok().json(IngestResponse { id }))
}

/// OPTIONS for CORS preflight (handled by middleware, but kept for explicit routing)
pub async fn options() -> HttpResponse {
    HttpResponse::Ok().finish()
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    let options = || web::method(actix_web::http::Method::OPTIONS).to(options);
    cfg.service(
        web::resource("/api/{project_id}/envelope/")
            .route(web::post().to(ingest_envelope))
            .route(options()),
    )
    .service(
        web::resource("/api/{project_id}/store/")
            .route(web::post().to(ingest_store))
            .route(options()),
    );
    let payload = || web::PayloadConfig::new(MAX_COMPRESSED_SIZE);
    for path in [
        "/api/{project_id}/cron/{monitor_slug}/{sentry_key}/",
        "/api/{project_id}/cron/{monitor_slug}/{sentry_key}",
        "/api/{project_id}/cron/{monitor_slug}/",
        "/api/{project_id}/cron/{monitor_slug}",
    ] {
        cfg.service(
            web::resource(path)
                .app_data(payload())
                .route(web::route().to(cron_check_in)),
        );
    }
}

/// Query string of Relay's HTTP check-in endpoint (endpoints/monitor.rs).
#[derive(serde::Deserialize)]
pub struct CronQuery {
    status: CheckInStatus,
    check_in_id: Option<uuid::Uuid>,
    environment: Option<String>,
    duration: Option<f64>,
}

/// Relay's HTTP check-in endpoint, for jobs that check in with `curl`. A
/// JSON body is a full check-in (the slug in the path wins); otherwise the
/// query string carries it and no `monitor_config`. Answers 202 like Relay,
/// but a check-in that cannot be recorded is a 400 with the reason, not a
/// silent drop: nothing else would tell a `curl` user.
pub async fn cron_check_in(
    pool: web::Data<DbPool>,
    processors: web::Data<Processors>,
    req: HttpRequest,
    auth: SentryAuth,
    body: Bytes,
) -> AppResult<HttpResponse> {
    let slug = req.match_info().get("monitor_slug").unwrap_or_default();
    let is_json = req
        .headers()
        .get(actix_web::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("application/json"));

    let check_in = if is_json {
        let mut payload: serde_json::Value = serde_json::from_slice(&body)
            .map_err(|e| AppError::Validation(format!("Invalid check-in JSON: {e}")))?;
        if let Some(object) = payload.as_object_mut() {
            object.insert("monitor_slug".to_string(), slug.into());
        }
        CheckIn::parse(payload.to_string().as_bytes())?
    } else {
        let query = web::Query::<CronQuery>::from_query(req.query_string())
            .map_err(|e| AppError::Validation(format!("Invalid check-in query: {e}")))?
            .into_inner();
        CheckIn::new(
            query.check_in_id.unwrap_or_default(),
            slug,
            query.status,
            query.environment,
            query.duration,
            None,
            None,
        )?
    };

    // Crons turned off: accepted and not stored, the same answer the envelope
    // path gives, so a monitored job never sees a failure from its monitor.
    if !auth.project.keeps(Product::Crons) {
        return Ok(HttpResponse::Accepted().finish());
    }

    pipeline::process_check_in(
        &processors,
        &pool,
        auth.project.id,
        Ok(check_in),
        Utc::now(),
    )
    .await?;
    Ok(HttpResponse::Accepted().finish())
}
