//! Sentry envelope generator for benchmarking.
//!
//! Generates valid Sentry envelope format payloads for load testing.

use chrono::{DateTime, Utc};
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use std::io::Write;
use uuid::Uuid;

/// Sentry event level
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Fatal,
    #[default]
    Error,
    Warning,
    Info,
    Debug,
}

/// Stack frame in a stack trace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackFrame {
    pub filename: String,
    pub function: String,
    pub lineno: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub colno: Option<u32>,
    pub in_app: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_line: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pre_context: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_context: Option<Vec<String>>,
}

/// Stack trace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stacktrace {
    pub frames: Vec<StackFrame>,
}

/// Exception value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExceptionValue {
    #[serde(rename = "type")]
    pub type_name: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stacktrace: Option<Stacktrace>,
}

/// Exception container
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Exception {
    pub values: Vec<ExceptionValue>,
}

/// Breadcrumb
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Breadcrumb {
    pub timestamp: f64,
    #[serde(rename = "type")]
    pub crumb_type: String,
    pub category: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// Breadcrumbs container
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Breadcrumbs {
    pub values: Vec<Breadcrumb>,
}

/// User context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<String>,
}

/// SDK information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sdk {
    pub name: String,
    pub version: String,
}

/// Sentry event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub event_id: String,
    pub timestamp: f64,
    pub platform: String,
    pub level: Level,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exception: Option<Exception>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub breadcrumbs: Option<Breadcrumbs>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<User>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<Vec<String>>,
    pub sdk: Sdk,
}

/// Envelope header
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvelopeHeader {
    pub event_id: String,
    pub sent_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dsn: Option<String>,
    pub sdk: Sdk,
}

/// Item header
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemHeader {
    #[serde(rename = "type")]
    pub item_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
}

/// Configuration for event generation
#[derive(Debug, Clone)]
pub struct EventConfig {
    /// Number of breadcrumbs to include
    pub breadcrumb_count: usize,
    /// Number of stack frames to include
    pub stack_depth: usize,
    /// Include user context
    pub include_user: bool,
    /// Include tags
    pub include_tags: bool,
    /// Include extra data
    pub include_extra: bool,
    /// Environment name
    pub environment: String,
    /// Release version
    pub release: String,
    /// Error type name
    pub error_type: String,
    /// How many distinct issues the generated events should group into.
    ///
    /// Grouping keys derive from exception type + message + transaction, so a
    /// message carrying a unique counter makes every single event its own issue.
    /// That is a pathological shape for an error tracker — real traffic is many
    /// events collapsing onto few issues — and it changes what the database is
    /// asked to do: one issue per event is an INSERT-only workload on `issues`,
    /// while realistic grouping is dominated by UPDATEs to existing rows.
    ///
    /// `None` keeps the unique-per-event behaviour (worst case for issue-table
    /// growth); `Some(n)` cycles the message across `n` groups.
    pub distinct_groups: Option<u32>,
    /// Fraction of generated payloads that are transactions rather than error
    /// events, in `0.0..=1.0`.
    ///
    /// Real SDK traffic is not purely errors, and the two take materially
    /// different paths: an error event is grouped and upserted into `issues`,
    /// while a transaction writes one `transactions` row plus one `spans` row
    /// per child span and does no grouping at all. Measuring only errors
    /// characterises only half of what the database is asked to do.
    pub transaction_ratio: f64,
    /// Child spans per transaction.
    pub spans_per_transaction: usize,
}

impl Default for EventConfig {
    fn default() -> Self {
        Self {
            breadcrumb_count: 5,
            stack_depth: 10,
            include_user: true,
            include_tags: true,
            include_extra: false,
            environment: "benchmark".to_string(),
            release: "rustrak-bench@0.1.0".to_string(),
            error_type: "Error".to_string(),
            distinct_groups: None,
            transaction_ratio: 0.0,
            spans_per_transaction: 10,
        }
    }
}

/// Which pipeline a generated payload exercises.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadKind {
    /// Error event: grouped, upserted into `issues`, stored in `events`
    Error,
    /// Transaction: stored in `transactions`, with child rows in `spans`
    Transaction,
}

/// A 16-hex-character Sentry span id.
fn short_id() -> String {
    Uuid::new_v4().to_string().replace('-', "")[..16].to_string()
}

/// Assemble a Sentry envelope from a header and a list of items.
///
/// Wire format is newline-delimited: envelope header, then for each item its
/// header followed by its payload, each on its own line.
fn build_envelope(header: &EnvelopeHeader, items: &[(ItemHeader, Vec<u8>)]) -> Vec<u8> {
    let mut envelope = Vec::new();

    let header_json = serde_json::to_string(header).expect("Failed to serialize envelope header");
    envelope.extend_from_slice(header_json.as_bytes());
    envelope.push(b'\n');

    for (item_header, payload) in items {
        let item_header_json =
            serde_json::to_string(item_header).expect("Failed to serialize item header");
        envelope.extend_from_slice(item_header_json.as_bytes());
        envelope.push(b'\n');
        envelope.extend_from_slice(payload);
        envelope.push(b'\n');
    }

    envelope
}

/// Gzip a payload with the same settings the ingest path expects.
fn gzip(data: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder
        .write_all(data)
        .expect("Failed to compress envelope");
    encoder.finish().expect("Failed to finish compression")
}

/// Envelope generator for creating Sentry-compatible payloads
pub struct EnvelopeGenerator {
    config: EventConfig,
    sdk: Sdk,
    counter: u64,
    /// Fractional accumulator driving the error/transaction mix.
    mix_accumulator: f64,
}

impl EnvelopeGenerator {
    /// Create a new envelope generator with the given configuration
    pub fn new(config: EventConfig) -> Self {
        Self {
            config,
            sdk: Sdk {
                name: "rustrak-bench".to_string(),
                version: "0.1.0".to_string(),
            },
            counter: 0,
            mix_accumulator: 0.0,
        }
    }

    /// Generate a unique event ID
    fn generate_event_id(&mut self) -> String {
        self.counter += 1;
        Uuid::new_v4().to_string().replace('-', "")
    }

    /// Generate stack frames
    fn generate_stack_frames(&self) -> Vec<StackFrame> {
        let mut frames = Vec::with_capacity(self.config.stack_depth);

        for i in 0..self.config.stack_depth {
            frames.push(StackFrame {
                filename: format!("/app/src/module_{}.rs", i),
                function: format!("process_request_{}", i),
                lineno: 100 + (i as u32 * 10),
                colno: Some(5),
                in_app: i < 5, // First 5 frames are in-app
                context_line: Some(format!("    let result = handle_event({});", i)),
                pre_context: Some(vec![
                    format!("    // Processing step {}", i),
                    "    let data = prepare_data();".to_string(),
                ]),
                post_context: Some(vec![
                    "    log::info!(\"Step completed\");".to_string(),
                    "    return result;".to_string(),
                ]),
            });
        }

        frames
    }

    /// Generate breadcrumbs
    fn generate_breadcrumbs(&self, now: f64) -> Vec<Breadcrumb> {
        let mut crumbs = Vec::with_capacity(self.config.breadcrumb_count);

        for i in 0..self.config.breadcrumb_count {
            let offset = (self.config.breadcrumb_count - i) as f64;
            crumbs.push(Breadcrumb {
                timestamp: now - offset,
                crumb_type: "default".to_string(),
                category: match i % 4 {
                    0 => "http".to_string(),
                    1 => "navigation".to_string(),
                    2 => "ui.click".to_string(),
                    _ => "console".to_string(),
                },
                message: Some(format!("Breadcrumb event #{}", i + 1)),
                level: Some("info".to_string()),
                data: Some(serde_json::json!({
                    "step": i,
                    "action": format!("action_{}", i)
                })),
            });
        }

        crumbs
    }

    /// Generate a complete event
    pub fn generate_event(&mut self) -> Event {
        let now: DateTime<Utc> = Utc::now();
        let timestamp = now.timestamp() as f64 + (now.timestamp_subsec_millis() as f64 / 1000.0);
        let event_id = self.generate_event_id();

        // The discriminator in the message is what the grouping algorithm keys
        // on, so it decides how many issues these events collapse into.
        let group_discriminator = match self.config.distinct_groups {
            Some(groups) if groups > 0 => self.counter % groups as u64,
            _ => self.counter,
        };

        let exception = Exception {
            values: vec![ExceptionValue {
                type_name: self.config.error_type.clone(),
                value: format!(
                    "Benchmark error #{} - testing server performance",
                    group_discriminator
                ),
                stacktrace: Some(Stacktrace {
                    frames: self.generate_stack_frames(),
                }),
            }],
        };

        let breadcrumbs = if self.config.breadcrumb_count > 0 {
            Some(Breadcrumbs {
                values: self.generate_breadcrumbs(timestamp),
            })
        } else {
            None
        };

        let user = if self.config.include_user {
            Some(User {
                id: Some(format!("user-{}", self.counter % 100)),
                email: Some(format!("user{}@benchmark.test", self.counter % 100)),
                username: Some(format!("benchuser_{}", self.counter % 100)),
                ip_address: Some(format!("192.168.1.{}", self.counter % 255)),
            })
        } else {
            None
        };

        let tags = if self.config.include_tags {
            Some(serde_json::json!({
                "benchmark": "true",
                "iteration": self.counter.to_string(),
                "scenario": "load_test"
            }))
        } else {
            None
        };

        let extra = if self.config.include_extra {
            Some(serde_json::json!({
                "request_id": format!("req-{}", Uuid::new_v4()),
                "processing_time_ms": self.counter % 1000,
                "payload_size": 1024
            }))
        } else {
            None
        };

        Event {
            event_id,
            timestamp,
            platform: "rust".to_string(),
            level: Level::Error,
            transaction: Some("/api/benchmark".to_string()),
            release: Some(self.config.release.clone()),
            environment: Some(self.config.environment.clone()),
            exception: Some(exception),
            breadcrumbs,
            user,
            tags,
            extra,
            fingerprint: None,
            sdk: self.sdk.clone(),
        }
    }

    /// Generate the next compressed payload together with what kind it is.
    ///
    /// The kind matters to callers that wait for the digest to drain: error
    /// events land in `events`, transactions land in `transactions` (plus one
    /// `spans` row per child). A caller that assumed everything becomes an
    /// `events` row would wait forever for a target it can never reach.
    pub fn generate_compressed_payload_kinded(
        &mut self,
        dsn: Option<&str>,
    ) -> (PayloadKind, Vec<u8>) {
        if self.should_send_transaction() {
            let spans = self.config.spans_per_transaction;
            (
                PayloadKind::Transaction,
                self.generate_compressed_transaction_envelope(spans, dsn),
            )
        } else {
            (PayloadKind::Error, self.generate_compressed_envelope(dsn))
        }
    }

    /// Generate the next compressed payload, honouring `transaction_ratio`.
    ///
    /// This is the entry point scenarios should use: it keeps the error/
    /// transaction mix in one place instead of having every scenario decide.
    ///
    /// Selection is deterministic (every Nth payload is a transaction) rather
    /// than random. Two runs of the same config therefore send exactly the same
    /// mix, which matters when the whole point is comparing one run against
    /// another — a random mix would add variance the comparison has to see past.
    pub fn generate_compressed_payload(&mut self, dsn: Option<&str>) -> Vec<u8> {
        if self.should_send_transaction() {
            let spans = self.config.spans_per_transaction;
            self.generate_compressed_transaction_envelope(spans, dsn)
        } else {
            self.generate_compressed_envelope(dsn)
        }
    }

    /// Whether the next payload should be a transaction.
    fn should_send_transaction(&mut self) -> bool {
        let ratio = self.config.transaction_ratio;
        if ratio <= 0.0 {
            return false;
        }
        if ratio >= 1.0 {
            return true;
        }

        // Advance a separate accumulator by the ratio and emit a transaction
        // each time it crosses an integer boundary. This spreads transactions
        // evenly through the stream for any ratio, rather than clustering them.
        let before = self.mix_accumulator;
        self.mix_accumulator += ratio;
        self.mix_accumulator.floor() > before.floor()
    }

    /// Generate a transaction payload with `span_count` child spans.
    ///
    /// Transactions take a different path through the server than error events:
    /// no grouping, no issue upsert, but one insert per child span. A payload
    /// with ten spans is eleven rows, which loads the database quite differently
    /// from an error event, and neither shape stands in for the other.
    pub fn generate_transaction(&mut self, span_count: usize) -> serde_json::Value {
        self.counter += 1;
        let event_id = Uuid::new_v4().to_string().replace('-', "");
        let trace_id = Uuid::new_v4().to_string().replace('-', "");

        let now: DateTime<Utc> = Utc::now();
        let end = now.timestamp() as f64 + (now.timestamp_subsec_millis() as f64 / 1000.0);
        // A plausible transaction duration; the exact value does not matter, but
        // it must be positive so the server's duration_ms is not clamped to zero.
        let start = end - 0.250;

        let root_span_id = short_id();

        let spans: Vec<serde_json::Value> = (0..span_count)
            .map(|i| {
                // Fan the children out across the parent's window so their
                // timestamps are ordered and non-degenerate.
                let fraction = i as f64 / span_count.max(1) as f64;
                let span_start = start + fraction * 0.2;
                const SPAN_OPS: [&str; 3] = ["db.query", "http.client", "cache.get"];
                serde_json::json!({
                    "span_id": short_id(),
                    "parent_span_id": root_span_id,
                    "trace_id": trace_id,
                    "op": SPAN_OPS[i % 3],
                    "description": format!("SELECT * FROM table_{}", i % 7),
                    "status": "ok",
                    "start_timestamp": span_start,
                    "timestamp": span_start + 0.02,
                    "exclusive_time": 20.0,
                })
            })
            .collect();

        serde_json::json!({
            "event_id": event_id,
            "type": "transaction",
            "transaction": format!("/api/endpoint/{}", self.counter % 20),
            "transaction_info": { "source": "route" },
            "platform": "rust",
            "level": "info",
            "start_timestamp": start,
            "timestamp": end,
            "release": self.config.release,
            "environment": self.config.environment,
            "contexts": {
                "trace": {
                    "trace_id": trace_id,
                    "span_id": root_span_id,
                    "op": "http.server",
                    "status": "ok",
                }
            },
            "spans": spans,
            "sdk": { "name": self.sdk.name, "version": self.sdk.version },
        })
    }

    /// Generate an envelope carrying a single transaction item.
    pub fn generate_transaction_envelope(
        &mut self,
        span_count: usize,
        dsn: Option<&str>,
    ) -> Vec<u8> {
        let transaction = self.generate_transaction(span_count);
        let payload = serde_json::to_string(&transaction).expect("Failed to serialize transaction");
        let event_id = transaction
            .get("event_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        build_envelope(
            &EnvelopeHeader {
                event_id,
                sent_at: Utc::now().to_rfc3339(),
                dsn: dsn.map(String::from),
                sdk: self.sdk.clone(),
            },
            &[(
                ItemHeader {
                    item_type: "transaction".to_string(),
                    length: Some(payload.len()),
                    content_type: Some("application/json".to_string()),
                },
                payload.into_bytes(),
            )],
        )
    }

    /// Gzip-compressed transaction envelope.
    pub fn generate_compressed_transaction_envelope(
        &mut self,
        span_count: usize,
        dsn: Option<&str>,
    ) -> Vec<u8> {
        gzip(&self.generate_transaction_envelope(span_count, dsn))
    }

    /// Generate a complete envelope (uncompressed)
    pub fn generate_envelope(&mut self, dsn: Option<&str>) -> Vec<u8> {
        let event = self.generate_event();
        let event_json = serde_json::to_string(&event).expect("Failed to serialize event");

        let now: DateTime<Utc> = Utc::now();
        let envelope_header = EnvelopeHeader {
            event_id: event.event_id.clone(),
            sent_at: now.to_rfc3339(),
            dsn: dsn.map(String::from),
            sdk: self.sdk.clone(),
        };

        let item_header = ItemHeader {
            item_type: "event".to_string(),
            length: Some(event_json.len()),
            content_type: Some("application/json".to_string()),
        };

        let envelope_header_json =
            serde_json::to_string(&envelope_header).expect("Failed to serialize envelope header");
        let item_header_json =
            serde_json::to_string(&item_header).expect("Failed to serialize item header");

        // Format: envelope_header\nitem_header\nitem_payload\n
        let mut envelope = Vec::new();
        envelope.extend_from_slice(envelope_header_json.as_bytes());
        envelope.push(b'\n');
        envelope.extend_from_slice(item_header_json.as_bytes());
        envelope.push(b'\n');
        envelope.extend_from_slice(event_json.as_bytes());
        envelope.push(b'\n');

        envelope
    }

    /// Generate a gzip-compressed envelope
    pub fn generate_compressed_envelope(&mut self, dsn: Option<&str>) -> Vec<u8> {
        let envelope = self.generate_envelope(dsn);

        let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
        encoder
            .write_all(&envelope)
            .expect("Failed to compress envelope");
        encoder.finish().expect("Failed to finish compression")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_event() {
        let config = EventConfig::default();
        let mut generator = EnvelopeGenerator::new(config);

        let event = generator.generate_event();

        assert!(!event.event_id.is_empty());
        assert!(event.timestamp > 0.0);
        assert_eq!(event.platform, "rust");
        assert!(event.exception.is_some());
    }

    /// Collect the exception messages of `count` generated events. The message
    /// is what the server's grouping algorithm keys on, so distinct messages
    /// mean distinct issues.
    fn generated_messages(config: EventConfig, count: usize) -> Vec<String> {
        let mut generator = EnvelopeGenerator::new(config);
        (0..count)
            .map(|_| {
                generator.generate_event().exception.unwrap().values[0]
                    .value
                    .clone()
            })
            .collect()
    }

    #[test]
    fn distinct_groups_none_gives_one_group_per_event() {
        let config = EventConfig::default();
        assert_eq!(config.distinct_groups, None);

        let messages = generated_messages(config, 20);
        let unique: std::collections::HashSet<_> = messages.iter().collect();

        // Default behaviour: every event lands in its own issue.
        assert_eq!(unique.len(), 20);
    }

    #[test]
    fn distinct_groups_caps_the_number_of_groups() {
        let config = EventConfig {
            distinct_groups: Some(5),
            ..EventConfig::default()
        };

        let messages = generated_messages(config, 50);
        let unique: std::collections::HashSet<_> = messages.iter().collect();

        // 50 events must collapse onto exactly 5 issues.
        assert_eq!(unique.len(), 5);
    }

    #[test]
    fn distinct_groups_of_zero_falls_back_to_unique_messages() {
        // Guards the modulo: `counter % 0` would panic.
        let config = EventConfig {
            distinct_groups: Some(0),
            ..EventConfig::default()
        };

        let messages = generated_messages(config, 10);
        let unique: std::collections::HashSet<_> = messages.iter().collect();

        assert_eq!(unique.len(), 10);
    }

    fn kinds_for_ratio(ratio: f64, count: usize) -> Vec<PayloadKind> {
        let config = EventConfig {
            transaction_ratio: ratio,
            ..EventConfig::default()
        };
        let mut generator = EnvelopeGenerator::new(config);
        (0..count)
            .map(|_| generator.generate_compressed_payload_kinded(None).0)
            .collect()
    }

    #[test]
    fn transaction_ratio_zero_sends_only_errors() {
        let kinds = kinds_for_ratio(0.0, 50);
        assert!(kinds.iter().all(|k| *k == PayloadKind::Error));
    }

    #[test]
    fn transaction_ratio_one_sends_only_transactions() {
        let kinds = kinds_for_ratio(1.0, 50);
        assert!(kinds.iter().all(|k| *k == PayloadKind::Transaction));
    }

    #[test]
    fn transaction_ratio_produces_the_requested_proportion() {
        let kinds = kinds_for_ratio(0.25, 100);
        let transactions = kinds
            .iter()
            .filter(|k| **k == PayloadKind::Transaction)
            .count();

        // 25% of 100, allowing one for where the accumulator lands.
        assert!(
            (24..=26).contains(&transactions),
            "expected ~25 transactions, got {}",
            transactions
        );
    }

    #[test]
    fn transaction_mix_is_spread_out_not_clustered() {
        // A ratio of 0.5 should alternate rather than send 50 errors then 50
        // transactions; clustering would make the load pattern unrepresentative.
        let kinds = kinds_for_ratio(0.5, 20);
        let first_half = kinds[..10]
            .iter()
            .filter(|k| **k == PayloadKind::Transaction)
            .count();

        assert!(
            (4..=6).contains(&first_half),
            "transactions clustered: {} in the first half",
            first_half
        );
    }

    #[test]
    fn generated_transaction_has_the_fields_the_server_reads() {
        let mut generator = EnvelopeGenerator::new(EventConfig::default());
        let transaction = generator.generate_transaction(5);

        // Mirrors what digest/processors/transaction.rs extracts.
        assert_eq!(transaction["type"], "transaction");
        assert!(transaction["transaction"].is_string());
        assert!(transaction["start_timestamp"].is_number());
        assert!(transaction["timestamp"].is_number());

        let trace = &transaction["contexts"]["trace"];
        assert!(trace["trace_id"].is_string());
        assert!(trace["span_id"].is_string());
        assert!(trace["op"].is_string());

        let spans = transaction["spans"].as_array().unwrap();
        assert_eq!(spans.len(), 5);
        for span in spans {
            assert!(span["span_id"].is_string());
            assert_eq!(span["parent_span_id"], trace["span_id"]);
            assert_eq!(span["trace_id"], trace["trace_id"]);
        }
    }

    #[test]
    fn transaction_ends_after_it_starts() {
        // A non-positive duration would make the server clamp duration_ms to
        // zero, quietly removing the thing the scenario means to measure.
        let mut generator = EnvelopeGenerator::new(EventConfig::default());
        let transaction = generator.generate_transaction(3);

        let start = transaction["start_timestamp"].as_f64().unwrap();
        let end = transaction["timestamp"].as_f64().unwrap();
        assert!(end > start, "start={} end={}", start, end);
    }

    #[test]
    fn transaction_envelope_declares_the_transaction_item_type() {
        let mut generator = EnvelopeGenerator::new(EventConfig::default());
        let envelope = generator.generate_transaction_envelope(3, None);
        let text = String::from_utf8(envelope).expect("Invalid UTF-8");

        // The server dispatches on this header; "event" would route it into the
        // error pipeline instead.
        assert!(text.contains("\"type\":\"transaction\""));
        assert!(text.contains("event_id"));
    }

    #[test]
    fn span_ids_are_16_hex_characters() {
        // Sentry span ids are 8 bytes; a full UUID here is rejected downstream.
        let id = short_id();
        assert_eq!(id.len(), 16);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_generate_envelope() {
        let config = EventConfig::default();
        let mut generator = EnvelopeGenerator::new(config);

        let envelope = generator.generate_envelope(Some("http://key@localhost:8080/1"));

        // Check it's valid UTF-8 and contains expected parts
        let envelope_str = String::from_utf8(envelope).expect("Invalid UTF-8");
        assert!(envelope_str.contains("event_id"));
        assert!(envelope_str.contains("sent_at"));
        assert!(envelope_str.contains("\"type\":\"event\""));
    }

    #[test]
    fn test_generate_compressed_envelope() {
        let config = EventConfig::default();
        let mut generator = EnvelopeGenerator::new(config);

        let compressed = generator.generate_compressed_envelope(None);

        // Gzip magic bytes
        assert!(compressed.len() >= 2);
        assert_eq!(compressed[0], 0x1f);
        assert_eq!(compressed[1], 0x8b);
    }

    #[test]
    fn test_counter_increments() {
        let config = EventConfig::default();
        let mut generator = EnvelopeGenerator::new(config);

        assert_eq!(generator.counter, 0);
        generator.generate_event();
        assert_eq!(generator.counter, 1);
        generator.generate_event();
        assert_eq!(generator.counter, 2);
    }
}
