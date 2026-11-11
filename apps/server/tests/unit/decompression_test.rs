//! The ingest body decoder. Every coding over HTTP, and the size caps, are
//! in `tests/integration/ingest_test.rs`.

use actix_web::{http::header, test as actix_test};
use bytes::Bytes;
use flate2::write::GzEncoder;
use flate2::Compression;
use rustrak::ingest::decompression::{content_encoding, decode};
use std::io::Write;

fn gzip(data: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

#[test]
fn no_coding_and_identity_pass_the_body_through_without_a_copy() {
    let body = Bytes::from_static(b"{\"event_id\":\"abc\"}");
    for coding in [None, Some("identity")] {
        let out = decode(body.clone(), coding).unwrap();
        assert_eq!(out.as_ptr(), body.as_ptr());
    }
}

#[test]
fn every_gzip_member_is_decoded() {
    let mut body = gzip(b"first");
    body.extend_from_slice(&gzip(b"second"));
    assert_eq!(
        decode(Bytes::from(body), Some("gzip")).unwrap().as_ref(),
        b"firstsecond"
    );
}

#[test]
fn trailing_bytes_after_gzip_are_rejected() {
    let mut body = gzip(b"payload");
    body.extend_from_slice(b"trailing");
    assert!(decode(Bytes::from(body), Some("gzip")).is_err());
}

#[test]
fn a_truncated_brotli_stream_is_rejected() {
    let mut out = Vec::new();
    {
        let mut encoder = brotli::CompressorWriter::new(&mut out, 4096, 5, 22);
        encoder.write_all(&[7; 100_000]).unwrap();
    }
    out.truncate(out.len() / 2);
    assert!(decode(Bytes::from(out), Some("br")).is_err());
}

#[test]
fn a_zstd_window_above_the_payload_budget_is_rejected() {
    let mut encoder = zstd::stream::write::Encoder::new(Vec::new(), 0).unwrap();
    encoder.window_log(28).unwrap();
    encoder.write_all(b"small payload").unwrap();
    assert!(decode(Bytes::from(encoder.finish().unwrap()), Some("zstd")).is_err());
}

#[test]
fn empty_and_utf8_content_encodings_are_ignored_like_relay_does() {
    // sentry-ruby 5.x sends an empty Content-Encoding and
    // sentry.java.android 2.0.0 sends "UTF-8".
    for value in ["", "   ", "UTF-8", "utf-8"] {
        let request = actix_test::TestRequest::default()
            .insert_header((header::CONTENT_ENCODING, value))
            .to_http_request();
        assert_eq!(content_encoding(&request).unwrap(), None, "{value:?}");
    }
}

#[test]
fn the_content_encoding_is_trimmed_and_lowercased() {
    let request = actix_test::TestRequest::default()
        .insert_header((header::CONTENT_ENCODING, " GZip "))
        .to_http_request();
    assert_eq!(content_encoding(&request).unwrap().as_deref(), Some("gzip"));
}

#[test]
fn a_content_encoding_that_is_not_ascii_is_rejected() {
    let request = actix_test::TestRequest::default()
        .insert_header((
            header::CONTENT_ENCODING,
            header::HeaderValue::from_bytes(b"gzip\xff").unwrap(),
        ))
        .to_http_request();
    assert!(content_encoding(&request).is_err());
}
