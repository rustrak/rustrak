use actix_web::{web, HttpRequest};
use bytes::Bytes;
use std::io::Read;

use crate::error::{AppError, AppResult};

/// Largest request body read off the wire, before decoding.
pub const MAX_COMPRESSED_SIZE: usize = 100 * 1024 * 1024;

/// Largest body after decoding.
pub const MAX_DECOMPRESSED_SIZE: usize = 100 * 1024 * 1024;

// zstd windows are powers of two. A 2^27 window is the smallest one that
// covers the decompressed payload limit, and prevents a frame header from
// making the decoder reserve an unnecessarily large history buffer.
const MAX_ZSTD_WINDOW_LOG: u32 = 27;

/// Reads the raw request body and decodes its `Content-Encoding` once.
///
/// Takes [`web::Payload`], never `Bytes`: actix's `Bytes` extractor decodes
/// the body itself and caps the decoded size, not the bytes on the wire.
pub async fn read_body(req: &HttpRequest, payload: web::Payload) -> AppResult<Bytes> {
    let coding = content_encoding(req)?;
    let raw = payload
        .to_bytes_limited(MAX_COMPRESSED_SIZE)
        .await
        .map_err(|_| {
            AppError::PayloadTooLarge(format!(
                "Compressed payload exceeds {MAX_COMPRESSED_SIZE} bytes"
            ))
        })?
        .map_err(|e| AppError::Validation(format!("Could not read the body: {e}")))?;
    crate::middleware::telemetry::mark_body_received(req);
    decode(raw, coding.as_deref())
}

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
        .take(MAX_DECOMPRESSED_SIZE as u64 + 1)
        .read_to_end(&mut decoded)
        .map_err(invalid)?;
    if decoded.len() > MAX_DECOMPRESSED_SIZE {
        return Err(AppError::PayloadTooLarge(format!(
            "Decompressed payload exceeds {MAX_DECOMPRESSED_SIZE} bytes"
        )));
    }
    Ok(Bytes::from(decoded))
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
