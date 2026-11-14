use bytes::Bytes;

use crate::error::{AppError, AppResult};
use crate::ingest::envelope::{EnvelopeHeaders, EnvelopeItemKind, ItemHeaders, ParsedEnvelope};

/// Maximum header size (8KB)
const MAX_HEADER_SIZE: usize = 8 * 1024;

/// Maximum size for non-"event" items (session, transaction, attachment, etc.)
const MAX_ITEM_SIZE: usize = 1024 * 1024;

/// Bound per-envelope metadata amplification from many tiny items.
const MAX_ITEM_COUNT: usize = 1024;

/// Do not retain a large envelope allocation for a tiny selected payload.
pub(crate) const RETAINED_ENVELOPE_OVERHEAD_COPY_FLOOR: usize = 16 * 1024;
/// Copy only when the retained envelope overhead is at least four times the
/// selected payload, avoiding an expensive copy of a large payload for a
/// comparatively small memory saving.
pub(crate) const RETAINED_ENVELOPE_OVERHEAD_COPY_RATIO: usize = 4;

/// The digest's trimming pass (`services::event_trim`) targets shrinking event
/// payloads under this size, matching the original per-item limit.
pub(crate) const TARGET_EVENT_SIZE: usize = 1024 * 1024;

/// Hard abuse ceiling for "event" items specifically — raised above
/// `TARGET_EVENT_SIZE` so a verbose-but-legitimate event (deep stack traces,
/// frame `vars`, large breadcrumb trails) survives ingest long enough for the
/// digest pipeline to trim it down, instead of being rejected outright for
/// being over budget before anyone got a chance to shrink it.
pub(crate) const MAX_RAW_EVENT_SIZE: usize = 4 * 1024 * 1024;

/// Sentry envelope parser.
///
/// Owns the envelope buffer and hands out [`Bytes`] zero-copy slices for item
/// payloads, so the body is never duplicated — every item shares the one
/// allocation until its slice is dropped.
pub struct EnvelopeParser {
    data: Bytes,
    position: usize,
}

pub(crate) fn detach_payload_if_needed(payload: Bytes, envelope_len: usize) -> Bytes {
    let retained_overhead = envelope_len.saturating_sub(payload.len());
    let should_detach = retained_overhead >= RETAINED_ENVELOPE_OVERHEAD_COPY_FLOOR
        && retained_overhead
            >= payload
                .len()
                .saturating_mul(RETAINED_ENVELOPE_OVERHEAD_COPY_RATIO);
    if should_detach {
        Bytes::copy_from_slice(&payload)
    } else {
        payload
    }
}

/// Removes every `\u0000` escape from a JSON document. PostgreSQL rejects a
/// NUL in TEXT and JSONB, so one in any string fails the whole insert there,
/// while SQLite stores it. A raw 0x00 cannot occur in valid JSON, so the
/// escape is the only form to remove.
pub(crate) fn strip_nul_escapes(json: &[u8]) -> std::borrow::Cow<'_, [u8]> {
    const NUL: &[u8] = br"\u0000";
    if !json.windows(NUL.len()).any(|w| w == NUL) {
        return std::borrow::Cow::Borrowed(json);
    }
    let mut out = Vec::with_capacity(json.len());
    let mut i = 0;
    while i < json.len() {
        if json[i] != b'\\' {
            out.push(json[i]);
            i += 1;
        } else if json[i..].starts_with(NUL) {
            i += NUL.len();
        } else {
            // Copy any other escape whole, so the `u0000` after an escaped
            // backslash (`\\u0000`, literal text) is never read as one.
            let end = (i + 2).min(json.len());
            out.extend_from_slice(&json[i..end]);
            i = end;
        }
    }
    std::borrow::Cow::Owned(out)
}

impl EnvelopeParser {
    pub fn new(data: Bytes) -> Self {
        Self { data, position: 0 }
    }

    pub(crate) fn data(&self) -> &[u8] {
        &self.data
    }

    /// Parses the complete envelope
    pub fn parse(&mut self) -> AppResult<ParsedEnvelope> {
        // 1. Parse envelope headers (first line)
        let headers = self.parse_envelope_headers()?;

        // 2. Parse items
        let mut items = Vec::new();
        while !self.at_eof() {
            if items.len() >= MAX_ITEM_COUNT {
                return Err(AppError::PayloadTooLarge(format!(
                    "Envelope contains more than {} items",
                    MAX_ITEM_COUNT
                )));
            }
            if let Some(item) = self.parse_item()? {
                items.push(item);
            }
        }

        Ok(ParsedEnvelope { headers, items })
    }

    fn parse_envelope_headers(&mut self) -> AppResult<EnvelopeHeaders> {
        let line = self.read_line(MAX_HEADER_SIZE)?;

        if line.is_empty() {
            return Err(AppError::Validation("Empty envelope headers".to_string()));
        }

        serde_json::from_slice(&line)
            .map_err(|e| AppError::Validation(format!("Invalid envelope headers JSON: {}", e)))
    }

    fn parse_item(&mut self) -> AppResult<Option<EnvelopeItemKind>> {
        // Read item headers
        let header_line = self.read_line(MAX_HEADER_SIZE)?;

        if header_line.is_empty() {
            return Ok(None);
        }

        let headers: ItemHeaders = serde_json::from_slice(&header_line)
            .map_err(|e| AppError::Validation(format!("Invalid item headers JSON: {}", e)))?;

        // "event" items get the relaxed abuse ceiling — see MAX_RAW_EVENT_SIZE.
        // An attachment is bounded by the envelope alone, as Relay's limit
        // for one is far above its event limit: an oversized one is dropped
        // later on its own, not with the envelope (see MAX_ATTACHMENT_SIZE).
        let is_attachment = headers.item_type == "attachment";
        let max_size = match headers.item_type.as_str() {
            "event" => MAX_RAW_EVENT_SIZE,
            "attachment" => crate::ingest::decompression::MAX_DECOMPRESSED_SIZE,
            _ => MAX_ITEM_SIZE,
        };

        // Read payload
        let payload = if let Some(length) = headers.length {
            // Explicit length
            if length > max_size {
                return Err(AppError::PayloadTooLarge(format!(
                    "Item payload exceeds {} bytes",
                    max_size
                )));
            }
            let payload = self.read_bytes(length)?;

            // Consume newline after payload (if exists)
            self.skip_newline();

            payload
        } else {
            // Read until newline
            self.read_line(max_size)?
        };
        // Every item type is JSON except attachments, which are raw bytes.
        let payload = match is_attachment {
            true => payload,
            false => match strip_nul_escapes(&payload) {
                std::borrow::Cow::Borrowed(_) => payload,
                std::borrow::Cow::Owned(clean) => Bytes::from(clean),
            },
        };

        Ok(Some(EnvelopeItemKind::from((headers, payload))))
    }

    fn read_line(&mut self, max_size: usize) -> AppResult<Bytes> {
        let start = self.position;
        let mut end = self.position;

        while end < self.data.len() && self.data[end] != b'\n' {
            end += 1;
            if end - start > max_size {
                return Err(AppError::PayloadTooLarge(format!(
                    "Line exceeds {} bytes",
                    max_size
                )));
            }
        }

        let line = self.data.slice(start..end);
        self.position = if end < self.data.len() { end + 1 } else { end };

        Ok(line)
    }

    fn read_bytes(&mut self, length: usize) -> AppResult<Bytes> {
        if self.position + length > self.data.len() {
            return Err(AppError::Validation(
                "Unexpected EOF while reading item payload".to_string(),
            ));
        }

        let bytes = self.data.slice(self.position..self.position + length);
        self.position += length;

        Ok(bytes)
    }

    fn skip_newline(&mut self) {
        if self.position < self.data.len() && self.data[self.position] == b'\n' {
            self.position += 1;
        }
    }

    fn at_eof(&self) -> bool {
        self.position >= self.data.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nul_escapes_are_removed_and_everything_else_is_kept() {
        let strip =
            |s: &str| String::from_utf8(strip_nul_escapes(s.as_bytes()).into_owned()).unwrap();
        assert_eq!(strip(r#"{"a\u0000":"x\u0000y"}"#), r#"{"a":"xy"}"#);
        // An escaped backslash followed by `u0000` is literal text, not a NUL.
        assert_eq!(strip(r#"{"a":"\\u0000\u0000"}"#), r#"{"a":"\\u0000"}"#);
        assert_eq!(strip(r#"{"a":"\n\u00e9\""}"#), r#"{"a":"\n\u00e9\""}"#);
        assert!(matches!(
            strip_nul_escapes(br#"{"a":"b"}"#),
            std::borrow::Cow::Borrowed(_)
        ));
    }

    #[test]
    fn a_stripped_payload_parses_to_the_same_value_minus_nul() {
        let raw = serde_json::json!({ "k\u{0}": ["\u{0}", "a\u{0}b", "\\u0000"] }).to_string();
        let clean: serde_json::Value =
            serde_json::from_slice(&strip_nul_escapes(raw.as_bytes())).unwrap();
        assert_eq!(clean, serde_json::json!({ "k": ["", "ab", "\\u0000"] }));
    }

    /// Builds a minimal envelope with one item of the given type and payload size.
    fn envelope_with_item(item_type: &str, payload_len: usize) -> Vec<u8> {
        let mut data = b"{}\n".to_vec();
        data.extend_from_slice(
            format!(r#"{{"type":"{}","length":{}}}"#, item_type, payload_len).as_bytes(),
        );
        data.push(b'\n');
        data.extend(std::iter::repeat_n(b'a', payload_len));
        data
    }

    #[test]
    fn event_item_between_1mb_and_4mb_is_accepted() {
        let data = envelope_with_item("event", 2 * 1024 * 1024);
        let result = EnvelopeParser::new(Bytes::from(data)).parse();
        assert!(
            result.is_ok(),
            "a 2MB event item should be accepted (only >4MB event items are rejected): {:?}",
            result.err()
        );
    }

    #[test]
    fn event_item_over_4mb_is_rejected() {
        let data = envelope_with_item("event", 5 * 1024 * 1024);
        let result = EnvelopeParser::new(Bytes::from(data)).parse();
        assert!(
            matches!(result, Err(AppError::PayloadTooLarge(_))),
            "a 5MB event item should still hit the abuse ceiling: {:?}",
            result
        );
    }

    #[test]
    fn non_event_item_over_1mb_is_still_rejected() {
        let data = envelope_with_item("session", 2 * 1024 * 1024);
        let result = EnvelopeParser::new(Bytes::from(data)).parse();
        assert!(
            matches!(result, Err(AppError::PayloadTooLarge(_))),
            "non-event items must not get the relaxed event ceiling: {:?}",
            result
        );
    }

    #[test]
    fn envelope_with_too_many_tiny_items_is_rejected() {
        let mut data = b"{}\n".to_vec();
        for _ in 0..=MAX_ITEM_COUNT {
            data.extend_from_slice(br#"{"type":"other","length":0}"#);
            data.push(b'\n');
        }

        let result = EnvelopeParser::new(Bytes::from(data)).parse();

        assert!(matches!(result, Err(AppError::PayloadTooLarge(_))));
    }

    #[test]
    fn item_without_length_is_read_until_newline() {
        let mut data = b"{}\n".to_vec();
        data.extend_from_slice(b"{\"type\":\"event\"}\n");
        data.extend_from_slice(b"{\"message\":\"no length given\"}\n");
        let parsed = EnvelopeParser::new(Bytes::from(data)).parse().unwrap();

        assert_eq!(parsed.items.len(), 1);
        let EnvelopeItemKind::Event(payload) = &parsed.items[0] else {
            panic!("expected an event item");
        };
        assert_eq!(payload.as_ref(), b"{\"message\":\"no length given\"}");
    }

    #[test]
    fn item_without_length_is_rejected_when_the_line_exceeds_the_limit() {
        let mut data = b"{}\n{\"type\":\"session\"}\n".to_vec();
        data.extend(std::iter::repeat_n(b'a', MAX_ITEM_SIZE + 1));

        let result = EnvelopeParser::new(Bytes::from(data)).parse();
        assert!(matches!(result, Err(AppError::PayloadTooLarge(_))));
    }

    #[test]
    fn selected_payload_detaches_when_envelope_overhead_is_large() {
        let payload_len = 1024;
        let overhead = RETAINED_ENVELOPE_OVERHEAD_COPY_FLOOR
            .max(payload_len * RETAINED_ENVELOPE_OVERHEAD_COPY_RATIO)
            + 1;
        let envelope = Bytes::from(vec![b'x'; overhead + payload_len]);
        let payload = envelope.slice(..payload_len);

        let detached = detach_payload_if_needed(payload.clone(), envelope.len());

        assert_eq!(detached, payload);
        assert_ne!(detached.as_ptr(), payload.as_ptr());
    }

    #[test]
    fn selected_payload_stays_zero_copy_below_overhead_threshold() {
        let payload_len = 1024;
        let overhead = RETAINED_ENVELOPE_OVERHEAD_COPY_FLOOR - 1;
        let envelope = Bytes::from(vec![b'x'; overhead + payload_len]);
        let payload = envelope.slice(..payload_len);

        let retained = detach_payload_if_needed(payload.clone(), envelope.len());

        assert_eq!(retained, payload);
        assert_eq!(retained.as_ptr(), payload.as_ptr());
    }

    #[test]
    fn large_payload_stays_zero_copy_when_overhead_is_not_four_times_larger() {
        let payload_len = 128 * 1024;
        let overhead = RETAINED_ENVELOPE_OVERHEAD_COPY_FLOOR + 1;
        let envelope = Bytes::from(vec![b'x'; overhead + payload_len]);
        let payload = envelope.slice(..payload_len);

        let retained = detach_payload_if_needed(payload.clone(), envelope.len());

        assert_eq!(retained, payload);
        assert_eq!(retained.as_ptr(), payload.as_ptr());
    }

    #[test]
    fn payload_detaches_at_the_size_aware_boundary() {
        let payload_len = 16 * 1024;
        let overhead = payload_len * RETAINED_ENVELOPE_OVERHEAD_COPY_RATIO;
        let envelope = Bytes::from(vec![b'x'; overhead + payload_len]);
        let payload = envelope.slice(..payload_len);

        let detached = detach_payload_if_needed(payload.clone(), envelope.len());

        assert_eq!(detached, payload);
        assert_ne!(detached.as_ptr(), payload.as_ptr());
    }

    /// Property tests: whatever an SDK (or an attacker) sends, parsing ends in
    /// `Ok` or `Err`, never a panic. Payloads are slices of the body, so a
    /// declared length can only be honoured by bytes that are really there.
    mod properties {
        use super::*;
        use proptest::collection::vec;
        use proptest::prelude::*;

        /// Item types whose payload the parser hands over as it read it.
        const BYTE_TYPES: [&str; 9] = [
            "event",
            "transaction",
            "log",
            "check_in",
            "feedback",
            "user_report",
            "attachment",
            "span",
            "something_new",
        ];

        fn payload_len(item: &EnvelopeItemKind) -> Option<usize> {
            Some(match item {
                EnvelopeItemKind::Event(p)
                | EnvelopeItemKind::Transaction(p)
                | EnvelopeItemKind::Log(p)
                | EnvelopeItemKind::CheckIn(p)
                | EnvelopeItemKind::Feedback(p)
                | EnvelopeItemKind::UserReport(p)
                | EnvelopeItemKind::Span(p)
                | EnvelopeItemKind::SpanV2Batch(p)
                | EnvelopeItemKind::Other(_, p) => p.len(),
                EnvelopeItemKind::Attachment(a) => a.payload.len(),
                EnvelopeItemKind::Session(_) | EnvelopeItemKind::Sessions(_) => return None,
            })
        }

        /// A payload byte that is never `\`: a `\u0000` escape is stripped
        /// from JSON items and would change the length on purpose.
        fn payload_byte() -> impl Strategy<Value = u8> {
            any::<u8>().prop_filter("no escapes", |b| *b != b'\\')
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(256))]

            #[test]
            fn arbitrary_bytes_never_panic(data in vec(any::<u8>(), 0..2048)) {
                let _ = EnvelopeParser::new(Bytes::from(data)).parse();
            }

            /// Arbitrary lines after a valid header line: item headers that
            /// are not JSON, not UTF-8, or JSON of the wrong shape.
            #[test]
            fn arbitrary_items_never_panic(lines in vec(vec(any::<u8>(), 0..64), 0..8)) {
                let mut data = b"{}\n".to_vec();
                for line in lines {
                    data.extend(line);
                    data.push(b'\n');
                }
                let _ = EnvelopeParser::new(Bytes::from(data)).parse();
            }

            /// Every well-formed envelope comes back with the same items and
            /// the same payload lengths, with explicit lengths (any byte, a
            /// newline included) and without (up to the newline).
            #[test]
            fn valid_envelopes_round_trip(
                items in vec(
                    (
                        prop::sample::select(BYTE_TYPES.to_vec()),
                        vec(payload_byte(), 0..256),
                        any::<bool>(),
                    ),
                    0..12,
                ),
                trailing_newline in any::<bool>(),
            ) {
                let mut data = br#"{"event_id":"9ec79c33ec9942ab8353589fcb2e04dc"}"#.to_vec();
                let mut expected = Vec::new();
                for (ty, mut payload, explicit) in items {
                    data.push(b'\n');
                    if explicit {
                        data.extend(format!(r#"{{"type":"{ty}","length":{}}}"#, payload.len()).bytes());
                    } else {
                        payload.retain(|b| *b != b'\n');
                        data.extend(format!(r#"{{"type":"{ty}"}}"#).bytes());
                    }
                    data.push(b'\n');
                    data.extend(&payload);
                    expected.push(payload.len());
                }
                if trailing_newline {
                    data.push(b'\n');
                }

                let parsed = EnvelopeParser::new(Bytes::from(data)).parse().unwrap();
                let lengths: Vec<_> = parsed.items.iter().map(payload_len).collect();
                let expected: Vec<_> = expected.into_iter().map(Some).collect();
                prop_assert_eq!(lengths, expected);
            }

            /// A declared length the body cannot back, up to the largest any
            /// item type accepts and far past it, is an error, read from the
            /// bytes present: nothing is allocated for the declared size.
            #[test]
            fn lying_lengths_are_errors(
                ty in prop::sample::select(BYTE_TYPES.to_vec()),
                actual in vec(any::<u8>(), 0..128),
                extra in 1usize..=usize::MAX / 2,
                newline in any::<bool>(),
            ) {
                let declared = actual.len().saturating_add(extra);
                let mut data = format!("{{}}\n{{\"type\":\"{ty}\",\"length\":{declared}}}").into_bytes();
                if newline {
                    data.push(b'\n');
                }
                data.extend(&actual);
                prop_assert!(EnvelopeParser::new(Bytes::from(data)).parse().is_err());
            }
        }
    }
}
