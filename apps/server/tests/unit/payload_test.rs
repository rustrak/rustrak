//! The payload codec (issue #532): JSON in, a self-describing header plus a
//! zstd frame (or the JSON itself) out, and back.

use rustrak::db::payload::{decode_with, dictionary_id, encode};
use serde_json::json;
use zstd::dict::{DecoderDictionary, EncoderDictionary};

const BROWSERS: [&str; 3] = ["Chrome", "Firefox", "Safari"];

/// An error event the way an SDK sends it, varied by `i`.
fn event(i: usize) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "event_id": format!("{i:032x}"),
        "timestamp": 1_700_000_000 + i,
        "platform": "python",
        "level": "error",
        "release": format!("shop@1.{}.0", i % 7),
        "environment": "production",
        "tags": { "browser": BROWSERS[i % 3], "route": format!("/orders/{i}") },
        "user": { "id": format!("user-{}", i % 50), "email": format!("u{}@example.com", i % 50) },
        "sdk": { "name": "sentry.python", "version": "2.19.0" },
        "exception": { "values": [{
            "type": "KeyError",
            "value": format!("'order_{i}'"),
            "stacktrace": { "frames": (0..8).map(|f| json!({
                "filename": format!("shop/views/orders_{f}.py"),
                "function": format!("handle_{f}"),
                "lineno": 10 + f * 3 + i % 5,
                "in_app": f > 3,
                "context_line": "    order = orders[request.args['id']]",
            })).collect::<Vec<_>>() }
        }] },
        "breadcrumbs": { "values": (0..5).map(|b| json!({
            "category": "query", "message": format!("SELECT * FROM orders WHERE id = {}", i + b),
            "timestamp": 1_700_000_000 + i + b,
        })).collect::<Vec<_>>() },
    }))
    .unwrap()
}

fn dictionary() -> Vec<u8> {
    let samples: Vec<Vec<u8>> = (0..300).map(event).collect();
    zstd::dict::from_samples(&samples, 16 * 1024).unwrap()
}

#[test]
fn json_that_does_not_shrink_is_stored_raw_behind_a_header() {
    let json = br#"{"a":1}"#;
    let stored = encode(json, None);
    assert_eq!(stored[0], 1, "raw format");
    assert_eq!(&stored[1..], json);
    assert_eq!(decode_with(&stored, None).unwrap(), json);
    assert_eq!(dictionary_id(&stored).unwrap(), None);
}

#[test]
fn json_is_zstd_compressed_without_a_dictionary() {
    let json = event(1);
    let stored = encode(&json, None);
    assert_eq!(stored[0], 2, "zstd format");
    assert!(
        stored.len() < json.len() / 2,
        "{} of {}",
        stored.len(),
        json.len()
    );
    assert_eq!(decode_with(&stored, None).unwrap(), json);
    assert_eq!(dictionary_id(&stored).unwrap(), None);
}

#[test]
fn json_is_compressed_with_a_dictionary_whose_id_is_in_the_header() {
    let dictionary = dictionary();
    let id = 4_000_000_007_u32;
    let json = event(1_000);
    let stored = encode(&json, Some((id, &EncoderDictionary::copy(&dictionary, 3))));
    assert_eq!(stored[0], 3, "zstd with dictionary format");
    assert_eq!(dictionary_id(&stored).unwrap(), Some(id));
    assert!(
        stored.len() < encode(&json, None).len() / 2,
        "{} with the dictionary, {} without",
        stored.len(),
        encode(&json, None).len()
    );
    let decoded = decode_with(&stored, Some(&DecoderDictionary::copy(&dictionary))).unwrap();
    assert_eq!(decoded, json);
}

#[test]
fn a_frame_carries_no_checksum_or_dictionary_id_and_records_its_size() {
    let json = event(2);
    let stored = encode(&json, None);
    let frame = &stored[1..];
    assert_eq!(
        zstd::zstd_safe::get_frame_content_size(frame).unwrap(),
        Some(json.len() as u64)
    );
    // Frame header descriptor: bit 2 is the checksum flag, bits 0-1 the
    // dictionary id size.
    assert_eq!(frame[4] & 0b111, 0, "descriptor {:08b}", frame[4]);
}

#[test]
fn an_unknown_format_is_an_error() {
    let mut stored = encode(&event(3), None);
    stored[0] = 9;
    let error = decode_with(&stored, None).unwrap_err().to_string();
    assert!(error.contains("format 9"), "{error}");
    assert!(dictionary_id(&stored).is_err());
}

#[test]
fn a_dictionary_payload_without_its_dictionary_is_an_error() {
    let dictionary = dictionary();
    let stored = encode(
        &event(4),
        Some((77, &EncoderDictionary::copy(&dictionary, 3))),
    );
    let error = decode_with(&stored, None).unwrap_err().to_string();
    assert!(error.contains("dictionary 77"), "{error}");
}

#[test]
fn empty_truncated_or_corrupt_input_is_an_error() {
    assert!(decode_with(&[], None).is_err(), "empty");
    let stored = encode(&event(5), None);
    assert!(
        decode_with(&stored[..stored.len() / 2], None).is_err(),
        "truncated"
    );
    let mut corrupt = stored.clone();
    for byte in &mut corrupt[1..9] {
        *byte ^= 0xA5;
    }
    assert!(decode_with(&corrupt, None).is_err(), "corrupt frame header");
    let dictionary = encode(
        &event(5),
        Some((1, &EncoderDictionary::copy(&dictionary(), 3))),
    );
    assert!(
        dictionary_id(&dictionary[..3]).is_err(),
        "cut dictionary id"
    );
}
