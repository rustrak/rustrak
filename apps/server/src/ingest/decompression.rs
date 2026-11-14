use actix_web::{web, HttpRequest};
use bytes::Bytes;
use std::io::Read;
use tokio::sync::{Semaphore, SemaphorePermit};

use crate::error::{AppError, AppResult};

/// Largest request body read off the wire, before decoding.
pub const MAX_COMPRESSED_SIZE: usize = 100 * 1024 * 1024;

/// Largest body after decoding.
pub const MAX_DECOMPRESSED_SIZE: usize = 100 * 1024 * 1024;

// zstd windows are powers of two. A 2^27 window is the smallest one that
// covers the decompressed payload limit, and prevents a frame header from
// making the decoder reserve an unnecessarily large history buffer.
const MAX_ZSTD_WINDOW_LOG: u32 = 27;

/// A body above this size, on the wire or decoded, holds one of
/// [`LARGE_BODY_SLOTS`] before it grows further.
pub const LARGE_BODY_BYTES: usize = 1024 * 1024;

/// Large bodies in memory at once. One peaks near 256 MiB (the raw buffer
/// up to its 100 MiB cap, plus up to 100 MiB decoded), so two keep large
/// bodies near half a gigabyte; the others wait, held back by TCP.
/// ponytail: a fixed count, not a byte budget; weigh by size if 2 × 100 MiB
/// uploads must run beside many 2 MiB ones.
pub const LARGE_BODY_SLOTS: usize = 2;

static LARGE_BODIES: Semaphore = Semaphore::const_new(LARGE_BODY_SLOTS);

/// Held for as long as a large body is: drop it once the body is parsed.
pub type LargeBodySlot = Option<SemaphorePermit<'static>>;

/// How long a request body may take.
#[derive(Debug, Clone, Copy)]
struct BodyTimeouts {
    /// Longest gap between two pieces of the body.
    idle: std::time::Duration,
    /// Longest a large body may take to finish once it holds a slot, so a
    /// byte sent just inside `idle` cannot keep a slot for ever.
    large_body: std::time::Duration,
    /// Longest wait for a large-body slot before a 503 the SDK backs off from.
    slot_wait: std::time::Duration,
}

/// What ingest reads request bodies with. Fifteen minutes carries the
/// largest body (100 MiB) over about 1 Mbit/s; waiting for a slot reads no
/// byte either, so it gets the idle window.
const BODY_TIMEOUTS: BodyTimeouts = BodyTimeouts {
    idle: BODY_IDLE_TIMEOUT,
    large_body: std::time::Duration::from_secs(15 * 60),
    slot_wait: BODY_IDLE_TIMEOUT,
};

/// Reads the raw request body and decodes its `Content-Encoding` once.
///
/// Takes [`web::Payload`], never `Bytes`: actix's `Bytes` extractor decodes
/// the body itself and caps the decoded size, not the bytes on the wire.
pub async fn read_body(
    req: &HttpRequest,
    payload: web::Payload,
) -> AppResult<(Bytes, LargeBodySlot)> {
    let coding = content_encoding(req)?;
    // HTTP framing holds the body to it, so it sizes the buffer; the caps
    // still bound a value that lies.
    let declared = req
        .headers()
        .get(actix_web::http::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok()?.trim().parse().ok());
    let (raw, slot) = collect_body(
        payload,
        declared,
        MAX_COMPRESSED_SIZE,
        BODY_TIMEOUTS,
        &LARGE_BODIES,
    )
    .await?;
    crate::middleware::telemetry::mark_body_received(req);
    decode_body(raw, coding, slot, BODY_TIMEOUTS.slot_wait, &LARGE_BODIES).await
}

/// Decodes `raw`, taking a slot from `large` first if the output passes
/// [`LARGE_BODY_BYTES`]: without one, it is decoded up to that size, and
/// decoded again in full once a slot is free.
async fn decode_body(
    raw: Bytes,
    coding: Option<String>,
    mut slot: LargeBodySlot,
    slot_wait: std::time::Duration,
    large: &'static Semaphore,
) -> AppResult<(Bytes, LargeBodySlot)> {
    if matches!(coding.as_deref(), None | Some("identity")) {
        return Ok((raw, slot));
    }
    if slot.is_none() {
        let inline = raw.len() <= INLINE_DECODE_LIMIT;
        match run_decode(raw.clone(), coding.clone(), LARGE_BODY_BYTES, inline).await {
            Err(AppError::PayloadTooLarge(_)) => slot = Some(acquire(large, slot_wait).await?),
            small => return Ok((small?, None)),
        }
    }
    // Up to 100 MB of output: off the worker thread, which serves every
    // other request on it meanwhile.
    Ok((
        run_decode(raw, coding, MAX_DECOMPRESSED_SIZE, false).await?,
        slot,
    ))
}

async fn run_decode(
    raw: Bytes,
    coding: Option<String>,
    limit: usize,
    inline: bool,
) -> AppResult<Bytes> {
    if inline {
        return decode_up_to(raw, coding.as_deref(), limit);
    }
    tokio::task::spawn_blocking(move || decode_up_to(raw, coding.as_deref(), limit))
        .await
        .map_err(|e| AppError::Internal(format!("Body decoding stopped: {e}")))?
}

/// A large-body slot, or 503 once `wait` passes without one.
async fn acquire(
    large: &'static Semaphore,
    wait: std::time::Duration,
) -> AppResult<SemaphorePermit<'static>> {
    tokio::time::timeout(wait, large.acquire())
        .await
        .map_err(|_| {
            AppError::Unavailable("too many large uploads at once; retry later".to_string())
        })?
        .map_err(|e| AppError::Internal(format!("Large body slots closed: {e}")))
}

/// Longest wait for the next piece of a request body. Actix times out only
/// the request head (`client_request_timeout`), so a client that stops
/// sending mid-body held the request open forever. Per chunk, like nginx's
/// `client_body_timeout`, so a large envelope on a slow link is never cut off
/// while it keeps arriving.
pub const BODY_IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// The whole body, at most `limit` bytes, refused once `timeouts.idle`
/// passes with no new bytes. Past [`LARGE_BODY_BYTES`] it waits for a slot
/// from `large` before reading on (503 after `timeouts.slot_wait`), must
/// finish within `timeouts.large_body` of getting it, and returns it with
/// the body. `declared` (`Content-Length`) sizes the buffer up front, see
/// [`body_reserve`].
async fn collect_body<S>(
    mut body: S,
    declared: Option<usize>,
    limit: usize,
    timeouts: BodyTimeouts,
    large: &'static Semaphore,
) -> AppResult<(Bytes, LargeBodySlot)>
where
    S: futures_util::Stream<Item = Result<Bytes, actix_web::error::PayloadError>> + Unpin,
{
    use futures_util::StreamExt as _;
    let mut raw = bytes::BytesMut::with_capacity(body_reserve(declared, limit, false));
    let mut slot = None;
    let mut deadline = None;
    loop {
        let wait = deadline.map_or(timeouts.idle, |at: tokio::time::Instant| {
            timeouts
                .idle
                .min(at.saturating_duration_since(tokio::time::Instant::now()))
        });
        let next = tokio::time::timeout(wait, body.next())
            .await
            .map_err(|_| match deadline {
                Some(at) if tokio::time::Instant::now() >= at => AppError::Validation(format!(
                    "The body did not finish within {:?}",
                    timeouts.large_body
                )),
                _ => AppError::Validation(format!("No body bytes for {:?}", timeouts.idle)),
            })?;
        let Some(chunk) = next else {
            return Ok((raw.freeze(), slot));
        };
        let chunk =
            chunk.map_err(|e| AppError::Validation(format!("Could not read the body: {e}")))?;
        if raw.len() + chunk.len() > limit {
            return Err(AppError::PayloadTooLarge(format!(
                "Compressed payload exceeds {limit} bytes"
            )));
        }
        if slot.is_none() && raw.len() + chunk.len() > LARGE_BODY_BYTES {
            slot = Some(acquire(large, timeouts.slot_wait).await?);
            deadline = Some(tokio::time::Instant::now() + timeouts.large_body);
            raw.reserve(body_reserve(declared, limit, true).saturating_sub(raw.len()));
        }
        raw.extend_from_slice(&chunk);
    }
}

/// Bytes to reserve for a body that declared `declared`: grown by doubling,
/// a buffer ends up to twice the body and `freeze` keeps all of it. Before a
/// large-body slot is held no more than [`LARGE_BODY_BYTES`], so a header
/// alone cannot make the server allocate; after, the declared length, never
/// past `limit`.
fn body_reserve(declared: Option<usize>, limit: usize, slot_held: bool) -> usize {
    let cap = if slot_held { limit } else { LARGE_BODY_BYTES };
    declared.unwrap_or(0).min(cap)
}

/// `decoded` without the slack it grew by doubling, when that slack is over a
/// quarter of it. A large block shrinks in place (`mremap` on Linux).
fn without_slack(mut decoded: Vec<u8>) -> Vec<u8> {
    if decoded.capacity() - decoded.len() > decoded.len() / 4 {
        decoded.shrink_to_fit();
    }
    decoded
}

/// Largest compressed body decoded on the worker thread itself; a small one
/// costs less to decode than to hand to the blocking pool.
pub const INLINE_DECODE_LIMIT: usize = 64 * 1024;

/// The codings [`decode`] reads, as Relay's 415 lists them.
pub const ACCEPT_ENCODING: &str = "zstd,gzip,deflate,br";

/// Whether [`decode`] reads `coding`. Relay (tower-http's
/// `RequestDecompression`) answers any other with 415 and [`ACCEPT_ENCODING`].
pub fn is_supported(coding: Option<&str>) -> bool {
    matches!(
        coding,
        None | Some("identity" | "gzip" | "deflate" | "br" | "zstd")
    )
}

/// Decodes `body` per one content coding, the set Relay accepts
/// (tower-http's `RequestDecompression`): `deflate` is zlib, and a list of
/// codings is not accepted.
pub fn decode(body: Bytes, coding: Option<&str>) -> AppResult<Bytes> {
    decode_up_to(body, coding, MAX_DECOMPRESSED_SIZE)
}

/// [`decode`], refusing output past `limit` bytes as `PayloadTooLarge`.
fn decode_up_to(body: Bytes, coding: Option<&str>, limit: usize) -> AppResult<Bytes> {
    let invalid = |e: std::io::Error| {
        AppError::Validation(format!("Invalid {} data: {e}", coding.unwrap_or_default()))
    };
    let data = &body[..];
    let reader: Box<dyn Read + '_> = match coding {
        None | Some("identity") => return Ok(body),
        Some("gzip") => Box::new(flate2::read::MultiGzDecoder::new(data)),
        Some("deflate") => Box::new(flate2::read::ZlibDecoder::new(data)),
        Some("br") => Box::new(brotli::Decompressor::new(data, 8192)),
        Some("zstd") => {
            let mut decoder = zstd::stream::read::Decoder::new(data).map_err(invalid)?;
            decoder
                .window_log_max(MAX_ZSTD_WINDOW_LOG)
                .map_err(invalid)?;
            Box::new(decoder)
        }
        Some(other) => {
            return Err(AppError::Validation(format!(
                "Unsupported Content-Encoding: {other}"
            )))
        }
    };
    let mut decoded = Vec::new();
    reader
        .take(limit as u64 + 1)
        .read_to_end(&mut decoded)
        .map_err(invalid)?;
    if decoded.len() > limit {
        return Err(AppError::PayloadTooLarge(format!(
            "Decompressed payload exceeds {limit} bytes"
        )));
    }
    Ok(Bytes::from(without_slack(decoded)))
}

/// The request's content coding, lowercased.
///
/// Mirrors Relay's `should_ignore_encoding`: an empty value (sentry-ruby 5.x)
/// and `UTF-8` (sentry.java.android 2.0.0) are treated as no encoding.
pub fn content_encoding(req: &HttpRequest) -> AppResult<Option<String>> {
    let Some(value) = req.headers().get(actix_web::http::header::CONTENT_ENCODING) else {
        return Ok(None);
    };
    let value = value
        .to_str()
        .map_err(|_| AppError::Validation("Content-Encoding header is not valid ASCII".into()))?
        .trim();
    if value.is_empty() || value.eq_ignore_ascii_case("utf-8") {
        return Ok(None);
    }
    Ok(Some(value.to_ascii_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{stream, StreamExt};
    use std::time::Duration;

    type Chunk = Result<Bytes, actix_web::error::PayloadError>;

    /// A client that stops sending mid-body (slow-loris) is cut off once no
    /// byte has arrived for the idle window, instead of holding the request
    /// open for as long as it likes.
    #[tokio::test]
    async fn a_body_that_stalls_is_refused() {
        let stalled = stream::iter([Ok::<_, actix_web::error::PayloadError>(Bytes::from_static(
            b"{",
        ))])
        .chain(stream::pending::<Chunk>());

        let result = collect_body(stalled, None, 1024, idle(50), slots(1)).await;

        assert!(matches!(result, Err(AppError::Validation(_))), "{result:?}");
    }

    /// A slow but steady upload is never cut off: the window is per chunk,
    /// not for the whole body.
    #[tokio::test]
    async fn a_slow_steady_body_is_read_whole() {
        let slow = Box::pin(stream::iter(0..5).then(|_| async {
            tokio::time::sleep(Duration::from_millis(20)).await;
            Ok::<_, actix_web::error::PayloadError>(Bytes::from_static(b"ab"))
        }));

        let (body, _) = collect_body(slow, None, 1024, idle(50), slots(1))
            .await
            .unwrap();

        assert_eq!(&body[..], b"ababababab");
    }

    #[tokio::test]
    async fn a_body_over_the_limit_is_too_large() {
        let big = stream::iter([Ok::<_, actix_web::error::PayloadError>(Bytes::from_static(
            b"0123456789",
        ))]);

        let result = collect_body(big, None, 4, idle(1000), slots(1)).await;

        assert!(
            matches!(result, Err(AppError::PayloadTooLarge(_))),
            "{result:?}"
        );
    }

    /// A semaphore of its own per test, like the process-wide one.
    fn slots(n: usize) -> &'static Semaphore {
        Box::leak(Box::new(Semaphore::new(n)))
    }

    fn chunks(n: usize, size: usize) -> impl futures_util::Stream<Item = Chunk> + Unpin {
        stream::iter((0..n).map(move |_| Ok(Bytes::from(vec![b'a'; size]))))
    }

    /// `idle_ms` between chunks; a minute for the rest.
    fn idle(idle_ms: u64) -> BodyTimeouts {
        BodyTimeouts {
            idle: Duration::from_millis(idle_ms),
            large_body: Duration::from_secs(60),
            slot_wait: Duration::from_secs(60),
        }
    }

    /// A client that passes the large-body mark and then trickles a byte
    /// just inside the idle window cannot keep its slot for ever: the body
    /// must finish within `large_body` of taking it.
    #[tokio::test]
    async fn a_trickling_large_body_loses_its_slot_at_the_deadline() {
        let large = slots(1);
        let trickle = Box::pin(chunks(1, LARGE_BODY_BYTES + 1).chain(
            stream::repeat_with(|| Ok(Bytes::from_static(b"a"))).then(|chunk| async {
                tokio::time::sleep(Duration::from_millis(20)).await;
                chunk
            }),
        ));
        let timeouts = BodyTimeouts {
            large_body: Duration::from_millis(200),
            ..idle(50)
        };

        let result = tokio::time::timeout(
            Duration::from_secs(5),
            collect_body(trickle, None, usize::MAX, timeouts, large),
        )
        .await
        .expect("held the slot past its deadline");

        assert!(matches!(result, Err(AppError::Validation(_))), "{result:?}");
        assert_eq!(large.available_permits(), 1, "the slot is free again");
    }

    /// Waiting for a large-body slot is bounded, and the refusal is a 503
    /// the SDK backs off from.
    #[tokio::test]
    async fn waiting_for_a_large_body_slot_is_bounded() {
        let large = slots(1);
        let _held = large.acquire().await.unwrap();
        let timeouts = BodyTimeouts {
            slot_wait: Duration::from_millis(50),
            ..idle(1000)
        };

        let result = collect_body(
            chunks(2, LARGE_BODY_BYTES),
            None,
            usize::MAX,
            timeouts,
            large,
        )
        .await;
        assert!(
            matches!(result, Err(AppError::Unavailable(_))),
            "{result:?}"
        );

        let gzip = {
            use std::io::Write as _;
            let mut gzip =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            gzip.write_all(&vec![0; 2 * LARGE_BODY_BYTES]).unwrap();
            Bytes::from(gzip.finish().unwrap())
        };
        let result = decode_body(
            gzip,
            Some("gzip".to_string()),
            None,
            Duration::from_millis(50),
            large,
        )
        .await;
        assert!(
            matches!(result, Err(AppError::Unavailable(_))),
            "{result:?}"
        );
    }

    /// A body past [`LARGE_BODY_BYTES`] does not grow further until it holds a
    /// large-body slot, so only so many large bodies are in memory at once.
    #[tokio::test]
    async fn a_large_body_waits_for_a_slot() {
        let large = slots(1);
        let held = large.acquire().await.unwrap();
        let half = LARGE_BODY_BYTES / 2 + 1;

        let waiting = tokio::time::timeout(
            Duration::from_millis(100),
            collect_body(chunks(2, half), None, usize::MAX, idle(1000), large),
        )
        .await;
        assert!(waiting.is_err(), "read a large body without a slot");

        drop(held);
        let (body, slot) = collect_body(chunks(2, half), None, usize::MAX, idle(1000), large)
            .await
            .unwrap();
        assert_eq!(body.len(), 2 * half);
        assert!(slot.is_some());
    }

    #[tokio::test]
    async fn a_small_body_needs_no_slot() {
        let (body, slot) = collect_body(chunks(4, 1024), None, usize::MAX, idle(1000), slots(0))
            .await
            .unwrap();
        assert_eq!(body.len(), 4096);
        assert!(slot.is_none());
    }

    /// A few kilobytes of gzip can decode to a hundred megabytes: the decoded
    /// size needs a slot as much as the raw one.
    #[tokio::test]
    async fn a_small_body_that_decodes_large_waits_for_a_slot() {
        use std::io::Write as _;
        let decoded_len = 2 * LARGE_BODY_BYTES;
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gzip.write_all(&vec![0; decoded_len]).unwrap();
        let raw = Bytes::from(gzip.finish().unwrap());
        assert!(raw.len() < LARGE_BODY_BYTES);

        let none = slots(0);
        let waiting = tokio::time::timeout(
            Duration::from_millis(100),
            decode_body(
                raw.clone(),
                Some("gzip".to_string()),
                None,
                Duration::from_secs(1),
                none,
            ),
        )
        .await;
        assert!(waiting.is_err(), "decoded a large body without a slot");

        let (body, slot) = decode_body(
            raw,
            Some("gzip".to_string()),
            None,
            Duration::from_secs(1),
            slots(1),
        )
        .await
        .unwrap();
        assert_eq!(body.len(), decoded_len);
        assert!(slot.is_some());
    }

    /// The buffer is sized from `Content-Length` instead of doubling (which
    /// leaves up to half of it as slack): up to 1 MiB before a large-body slot
    /// is held, the whole declared length (never past the limit) after.
    #[test]
    fn the_body_buffer_is_reserved_from_the_declared_length() {
        let mib = LARGE_BODY_BYTES;
        assert_eq!(body_reserve(None, 100 * mib, false), 0);
        assert_eq!(body_reserve(Some(500), 100 * mib, false), 500);
        assert_eq!(body_reserve(Some(50 * mib), 100 * mib, false), mib);
        assert_eq!(body_reserve(Some(50 * mib), 100 * mib, true), 50 * mib);
        // A lying header gets no more than the limit.
        assert_eq!(body_reserve(Some(usize::MAX), 100 * mib, true), 100 * mib);
    }

    /// The decoded body keeps no growth slack: a `Vec` that doubled toward
    /// its size would hold up to twice the body for as long as it lives.
    #[test]
    fn decoded_output_keeps_no_slack() {
        let mut decoded = Vec::with_capacity(4 * LARGE_BODY_BYTES);
        decoded.resize(LARGE_BODY_BYTES + 1, 0);
        let decoded = without_slack(decoded);
        assert!(decoded.capacity() <= decoded.len() + decoded.len() / 4);
    }
}
