//! Unit tests for Sentry Spans Protocol v2 ingestion
//! (application/vnd.sentry.items.span.v2+json).
//!
//! Discovered via a real @sentry/node 10.65 + Vercel AI SDK integration
//! test (packages/test-sentry/demo/src/ai-agent.ts) — modern Sentry SDKs
//! send AI-instrumented spans in this batched, typed-attribute wire format,
//! not the legacy one-span-per-item format `span_test.rs` covers.

use rustrak::ingest::envelope::{EnvelopeItemKind, ItemHeaders};
use rustrak::models::span_v2::{parse_span_v2_container, SpanV2Entry};
use serde_json::json;

#[test]
fn test_enum_dispatch_span_with_v2_content_type_maps_to_span_v2_batch() {
    let headers = ItemHeaders {
        item_type: "span".into(),
        length: None,
        content_type: Some("application/vnd.sentry.items.span.v2+json".into()),
        filename: None,
        attachment_type: None,
    };
    let kind = EnvelopeItemKind::from((headers, bytes::Bytes::from(b"{}".to_vec())));
    assert!(matches!(kind, EnvelopeItemKind::SpanV2Batch(_)));
}

#[test]
fn test_enum_dispatch_span_without_v2_content_type_stays_legacy() {
    let headers = ItemHeaders {
        item_type: "span".into(),
        length: None,
        content_type: None,
        filename: None,
        attachment_type: None,
    };
    let kind = EnvelopeItemKind::from((headers, bytes::Bytes::from(b"{}".to_vec())));
    assert!(matches!(kind, EnvelopeItemKind::Span(_)));
}

/// Real wire fixture (trimmed), captured off the wire from an actual
/// @sentry/node 10.65 + Vercel AI SDK generateText() + tool call, via a
/// local echo server acting as the DSN target — not a hand-rolled guess.
const REAL_WIRE_FIXTURE: &str = r#"{
    "version": 2,
    "items": [
        {
            "trace_id": "800087bbed8c481faaabed73e41e5d4b",
            "span_id": "8a743a442038cceb",
            "parent_span_id": "8efc25d3729c267c",
            "name": "generate_content gpt-4o",
            "start_timestamp": 1784231017.8907192,
            "end_timestamp": 1784231017.893409,
            "status": "ok",
            "is_segment": false,
            "attributes": {
                "sentry.origin": { "value": "auto.vercelai.otel", "type": "string" },
                "sentry.op": { "value": "gen_ai.generate_content", "type": "string" },
                "gen_ai.operation.name": { "value": "generate_content", "type": "string" },
                "gen_ai.request.model": { "value": "gpt-4o", "type": "string" },
                "gen_ai.function_id": { "value": "research_agent", "type": "string" },
                "gen_ai.usage.input_tokens": { "value": 210, "type": "integer" },
                "gen_ai.usage.output_tokens": { "value": 34, "type": "integer" }
            }
        },
        {
            "trace_id": "800087bbed8c481faaabed73e41e5d4b",
            "span_id": "a8aa8bae6afce239",
            "parent_span_id": "8efc25d3729c267c",
            "name": "execute_tool webSearch",
            "start_timestamp": 1784231017.894036,
            "end_timestamp": 1784231017.8943124,
            "status": "ok",
            "is_segment": false,
            "attributes": {
                "sentry.origin": { "value": "auto.vercelai.otel", "type": "string" },
                "sentry.op": { "value": "gen_ai.execute_tool", "type": "string" },
                "gen_ai.operation.name": { "value": "execute_tool", "type": "string" },
                "gen_ai.tool.name": { "value": "webSearch", "type": "string" }
            }
        }
    ]
}"#;

#[test]
fn test_parse_span_v2_container_real_fixture() {
    let entries = parse_span_v2_container(REAL_WIRE_FIXTURE.as_bytes()).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].span_id, "8a743a442038cceb");
    assert_eq!(entries[0].trace_id, "800087bbed8c481faaabed73e41e5d4b");
    assert_eq!(
        entries[0].parent_span_id.as_deref(),
        Some("8efc25d3729c267c")
    );
    assert_eq!(entries[0].name.as_deref(), Some("generate_content gpt-4o"));
    assert_eq!(entries[0].status.as_deref(), Some("ok"));
    assert!(!entries[0].is_segment);
    assert_eq!(entries[1].span_id, "a8aa8bae6afce239");
}

#[test]
fn test_flat_attributes_unwraps_typed_value_type_pairs() {
    let entries = parse_span_v2_container(REAL_WIRE_FIXTURE.as_bytes()).unwrap();
    let flat = entries[0].flat_attributes();

    assert_eq!(flat["sentry.op"], "gen_ai.generate_content");
    assert_eq!(flat["gen_ai.request.model"], "gpt-4o");
    assert_eq!(flat["gen_ai.usage.input_tokens"], 210);
    assert_eq!(flat["gen_ai.usage.output_tokens"], 34);
    // The {"value":..., "type":...} wrapper must not leak through.
    assert!(flat["sentry.op"].is_string());
    assert!(flat.get("type").is_none());
}

#[test]
fn test_op_reads_sentry_op_attribute() {
    let flat = json!({ "sentry.op": "gen_ai.execute_tool" });
    assert_eq!(
        SpanV2Entry::op(&flat).as_deref(),
        Some("gen_ai.execute_tool")
    );
}

#[test]
fn test_op_falls_back_to_gen_ai_operation_name_when_sentry_op_absent() {
    let flat = json!({ "gen_ai.operation.name": "invoke_agent" });
    assert_eq!(SpanV2Entry::op(&flat).as_deref(), Some("invoke_agent"));
}

#[test]
fn test_op_is_none_when_neither_attribute_present() {
    let flat = json!({ "some.other.attr": "x" });
    assert_eq!(SpanV2Entry::op(&flat), None);
}

#[test]
fn test_parse_malformed_container_json_is_rejected() {
    let res = parse_span_v2_container(b"not json");
    assert!(res.is_err());
}

#[test]
fn test_parse_container_with_no_items_returns_empty_vec() {
    let entries = parse_span_v2_container(br#"{"version":2}"#).unwrap();
    assert!(entries.is_empty());
}

#[test]
fn test_parse_container_missing_version_is_still_accepted() {
    // `version` is optional and advisory in the real protocol (Relay's
    // ContainerMetadata.version: Option<u16>) — a missing version is a
    // documented-valid state, not a rejection condition.
    let entries = parse_span_v2_container(br#"{"items":[]}"#).unwrap();
    assert!(entries.is_empty());
}

#[test]
fn test_parse_container_with_unrecognized_version_is_still_accepted() {
    // Relay's own tests round-trip arbitrary version values (e.g. 123)
    // unrejected — version never gates whether items[] parses.
    let entries = parse_span_v2_container(br#"{"version":99,"items":[]}"#).unwrap();
    assert!(entries.is_empty());
}

#[cfg(test)]
mod level2 {
    use super::REAL_WIRE_FIXTURE;
    use crate::common::TestDb;
    use chrono::Utc;
    use rustrak::digest::processors::{ProcessorCtx, SpanV2Processor};
    use rustrak::models::CreateProject;
    use rustrak::services::ProjectService;
    use serde_json::json;
    use sqlx::Row;
    use uuid::Uuid;

    fn ctx(pool: &rustrak::db::DbPool, project_id: i32) -> ProcessorCtx {
        ProcessorCtx {
            pool: pool.clone(),
            project_id,
            event_id: Uuid::nil(),
            ingested_at: Utc::now(),
            remote_addr: None,
            keep_ai_spans: true,
        }
    }

    #[tokio::test]
    async fn test_real_fixture_batch_stores_one_row_per_entry() {
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "span-v2-real-fixture".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        SpanV2Processor
            .process(
                bytes::Bytes::from(REAL_WIRE_FIXTURE.as_bytes().to_vec()),
                &ctx(&db.pool, project.id),
            )
            .await
            .unwrap();
        SpanV2Processor
            .process(
                bytes::Bytes::from(REAL_WIRE_FIXTURE.as_bytes().to_vec()),
                &ctx(&db.pool, project.id),
            )
            .await
            .unwrap();

        const COUNT: &str = "SELECT COUNT(*) FROM spans WHERE project_id = $1";
        let count: i64 = sqlx::query_scalar(COUNT)
            .bind(project.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(
            count, 2,
            "the fixture's 2 items must each become their own row"
        );
    }

    #[tokio::test]
    async fn test_v2_entry_maps_op_from_sentry_op_attribute_not_top_level() {
        // The key structural difference from the legacy format: `op` has no
        // top-level field in v2, it must be read out of attributes["sentry.op"].
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "span-v2-op-mapping".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        SpanV2Processor
            .process(
                bytes::Bytes::from(REAL_WIRE_FIXTURE.as_bytes().to_vec()),
                &ctx(&db.pool, project.id),
            )
            .await
            .unwrap();

        const QUERY: &str = "SELECT span_id, trace_id, parent_span_id, op, description, status, transaction_id, duration_ms FROM spans WHERE project_id = $1 AND span_id = $2";

        let row = sqlx::query(QUERY)
            .bind(project.id)
            .bind("8a743a442038cceb")
            .fetch_one(&db.pool)
            .await
            .unwrap();

        let trace_id: Option<String> = row.get("trace_id");
        let parent_span_id: Option<String> = row.get("parent_span_id");
        let op: Option<String> = row.get("op");
        let description: Option<String> = row.get("description");
        let status: Option<String> = row.get("status");
        let transaction_id: Option<Uuid> = row.get("transaction_id");
        let duration_ms: Option<f64> = row.get("duration_ms");

        assert_eq!(
            trace_id.as_deref(),
            Some("800087bbed8c481faaabed73e41e5d4b")
        );
        assert_eq!(parent_span_id.as_deref(), Some("8efc25d3729c267c"));
        assert_eq!(
            op.as_deref(),
            Some("gen_ai.generate_content"),
            "op must come from attributes[\"sentry.op\"]"
        );
        assert_eq!(
            description.as_deref(),
            Some("generate_content gpt-4o"),
            "description must come from `name`"
        );
        assert_eq!(status.as_deref(), Some("ok"));
        assert!(
            transaction_id.is_none(),
            "standalone v2 span must not be linked to a transaction"
        );
        assert!(
            duration_ms.is_some_and(|d| d > 0.0),
            "duration must be derived from end_timestamp - start_timestamp"
        );
    }

    #[tokio::test]
    async fn test_v2_ai_span_normalized() {
        // AC #4: extract_gen_ai_columns is the same shared entry point as
        // the legacy/transaction producers — the AI span in the real
        // fixture must get gen_ai columns populated.
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "span-v2-gen-ai".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        SpanV2Processor
            .process(
                bytes::Bytes::from(REAL_WIRE_FIXTURE.as_bytes().to_vec()),
                &ctx(&db.pool, project.id),
            )
            .await
            .unwrap();

        const QUERY: &str = "SELECT gen_ai_operation_type, gen_ai_agent_name, gen_ai_response_model, gen_ai_usage_total_tokens FROM spans WHERE project_id = $1 AND span_id = $2";

        let row = sqlx::query(QUERY)
            .bind(project.id)
            .bind("8a743a442038cceb")
            .fetch_one(&db.pool)
            .await
            .unwrap();

        let operation_type: Option<String> = row.get("gen_ai_operation_type");
        let agent_name: Option<String> = row.get("gen_ai_agent_name");
        let response_model: Option<String> = row.get("gen_ai_response_model");
        let total_tokens: Option<f64> = row.get("gen_ai_usage_total_tokens");

        // "gen_ai.generate_content" isn't in infer_operation_type's match
        // table (only the JS-raw "ai.generateText.doGenerate" form is) so it
        // falls through to the ai_client default — same behavior Relay's
        // own eap::ai normalization has (see story-span-v2-protocol.md).
        assert_eq!(operation_type.as_deref(), Some("ai_client"));
        assert_eq!(
            agent_name.as_deref(),
            Some("research_agent"),
            "must default from gen_ai.function_id"
        );
        assert_eq!(
            response_model.as_deref(),
            Some("gpt-4o"),
            "must default from gen_ai.request.model"
        );
        assert_eq!(total_tokens, Some(244.0), "210 input + 34 output");
    }

    #[tokio::test]
    async fn test_v2_tool_span_gets_tool_operation_type() {
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "span-v2-tool".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        SpanV2Processor
            .process(
                bytes::Bytes::from(REAL_WIRE_FIXTURE.as_bytes().to_vec()),
                &ctx(&db.pool, project.id),
            )
            .await
            .unwrap();

        const QUERY: &str = "SELECT gen_ai_operation_type, gen_ai_tool_name FROM spans WHERE project_id = $1 AND span_id = $2";

        let row = sqlx::query(QUERY)
            .bind(project.id)
            .bind("a8aa8bae6afce239")
            .fetch_one(&db.pool)
            .await
            .unwrap();

        let operation_type: Option<String> = row.get("gen_ai_operation_type");
        let tool_name: Option<String> = row.get("gen_ai_tool_name");

        assert_eq!(operation_type.as_deref(), Some("tool"));
        assert_eq!(tool_name.as_deref(), Some("webSearch"));
    }

    #[tokio::test]
    async fn test_v2_entry_missing_span_id_is_skipped_not_whole_batch() {
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "span-v2-partial-batch".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let payload = json!({
            "version": 2,
            "items": [
                {
                    "trace_id": "d3d20f000885466b8c8f947c9b92b8d3",
                    "start_timestamp": 1.0,
                    "end_timestamp": 2.0,
                    "attributes": {}
                },
                {
                    "trace_id": "d3d20f000885466b8c8f947c9b92b8d3",
                    "span_id": "dddddddddddddddd",
                    "start_timestamp": 1.0,
                    "end_timestamp": 2.0,
                    "attributes": {}
                }
            ]
        })
        .to_string();

        SpanV2Processor
            .process(
                bytes::Bytes::from(payload.into_bytes()),
                &ctx(&db.pool, project.id),
            )
            .await
            .unwrap();

        const COUNT: &str = "SELECT COUNT(*) FROM spans WHERE project_id = $1";
        let count: i64 = sqlx::query_scalar(COUNT)
            .bind(project.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(
            count, 1,
            "the entry missing span_id must be skipped, the valid one still stored"
        );
    }

    #[tokio::test]
    async fn test_v2_is_segment_true_sets_segment_id_to_own_span_id() {
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "span-v2-segment".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let payload = json!({
            "version": 2,
            "items": [{
                "trace_id": "d3d20f000885466b8c8f947c9b92b8d3",
                "span_id": "eeeeeeeeeeeeeeee",
                "start_timestamp": 1.0,
                "end_timestamp": 2.0,
                "is_segment": true,
                "attributes": {}
            }]
        })
        .to_string();

        SpanV2Processor
            .process(
                bytes::Bytes::from(payload.into_bytes()),
                &ctx(&db.pool, project.id),
            )
            .await
            .unwrap();

        const QUERY: &str = "SELECT is_segment, segment_id FROM spans WHERE project_id = $1";

        let row = sqlx::query(QUERY)
            .bind(project.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();

        let is_segment: bool = row.get("is_segment");
        let segment_id: Option<String> = row.get("segment_id");

        assert!(is_segment);
        assert_eq!(segment_id.as_deref(), Some("eeeeeeeeeeeeeeee"));
    }

    #[tokio::test]
    async fn test_v2_and_legacy_spans_sharing_trace_id_are_both_queryable() {
        // AC #6: a trace with mixed-origin spans (v2-sourced + legacy) must
        // be queryable as one coherent trace.
        use rustrak::digest::processors::SpanProcessor;
        use rustrak::services::span::{SpanFilters, SpanService};

        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "span-v2-mixed-origin".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let shared_trace = "ffffffffffffffffffffffffffffffff";

        // Legacy standalone span.
        let legacy_payload = json!({
            "span_id": "1111111111111111",
            "trace_id": shared_trace,
            "op": "http.client",
            "start_timestamp": 1.0,
            "timestamp": 2.0
        });
        SpanProcessor
            .process(
                bytes::Bytes::from(serde_json::to_vec(&legacy_payload).unwrap()),
                &ctx(&db.pool, project.id),
            )
            .await
            .unwrap();

        // v2 span sharing the same trace_id.
        let v2_payload = json!({
            "version": 2,
            "items": [{
                "trace_id": shared_trace,
                "span_id": "2222222222222222",
                "start_timestamp": 1.0,
                "end_timestamp": 1.5,
                "attributes": {}
            }]
        })
        .to_string();
        SpanV2Processor
            .process(
                bytes::Bytes::from(v2_payload.into_bytes()),
                &ctx(&db.pool, project.id),
            )
            .await
            .unwrap();

        let filters = SpanFilters {
            trace_id: Some(shared_trace.to_string()),
            ..Default::default()
        };
        let (list, total) = SpanService::list_offset(&db.pool, project.id, 1, 20, &filters)
            .await
            .unwrap();

        assert_eq!(
            total, 2,
            "both the legacy and v2-sourced span share this trace_id"
        );
        assert_eq!(list.len(), 2);
    }

    #[tokio::test]
    async fn test_v2_malformed_container_json_is_rejected() {
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "span-v2-bad-json".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let res = SpanV2Processor
            .process(
                bytes::Bytes::from(vec![0xff, 0x00, b'n', b'o']),
                &ctx(&db.pool, project.id),
            )
            .await;
        assert!(
            res.is_err(),
            "malformed span v2 container JSON must be rejected"
        );
    }

    #[tokio::test]
    async fn test_v2_entry_missing_timestamps_is_skipped() {
        // Relay's `validate_timestamps` discards a span whose start or end is
        // absent (DiscardReason::Timestamp) — a missing timestamp is not a
        // zero timestamp. Without this, serde's default collapses both to 0.0,
        // `0.0 > 0.0` is false, and the entry is stored with NULL timestamps.
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "span-v2-no-timestamps".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let payload = json!({
            "version": 2,
            "items": [
                {
                    "trace_id": "d3d20f000885466b8c8f947c9b92b8d3",
                    "span_id": "aaaaaaaaaaaaaaaa",
                    "attributes": {}
                },
                {
                    "trace_id": "d3d20f000885466b8c8f947c9b92b8d3",
                    "span_id": "bbbbbbbbbbbbbbbb",
                    "start_timestamp": 1.0,
                    "attributes": {}
                },
                {
                    "trace_id": "d3d20f000885466b8c8f947c9b92b8d3",
                    "span_id": "cccccccccccccccc",
                    "start_timestamp": 1.0,
                    "end_timestamp": 2.0,
                    "attributes": {}
                },
                {
                    "trace_id": "d3d20f000885466b8c8f947c9b92b8d3",
                    "span_id": "dddddddddddddddd",
                    "start_timestamp": -1.0,
                    "end_timestamp": 1.0,
                    "attributes": {}
                },
                {
                    "trace_id": "d3d20f000885466b8c8f947c9b92b8d3",
                    "span_id": "eeeeeeeeeeeeeeee",
                    "start_timestamp": 1.0,
                    "end_timestamp": 1.7976931348623157e308,
                    "attributes": {}
                },
                {
                    "trace_id": "d3d20f000885466b8c8f947c9b92b8d3",
                    "span_id": "ffffffffffffffff",
                    "start_timestamp": 2.0,
                    "end_timestamp": 1.0,
                    "attributes": {}
                }
            ]
        })
        .to_string();

        SpanV2Processor
            .process(
                bytes::Bytes::from(payload.into_bytes()),
                &ctx(&db.pool, project.id),
            )
            .await
            .unwrap();

        const QUERY: &str = "SELECT span_id FROM spans WHERE project_id = $1";
        let rows = sqlx::query(QUERY)
            .bind(project.id)
            .fetch_all(&db.pool)
            .await
            .unwrap();

        let stored: Vec<String> = rows.iter().map(|r| r.get("span_id")).collect();
        assert_eq!(
            stored,
            vec!["cccccccccccccccc"],
            "entries with a missing start or end timestamp must be skipped, \
             not stored with NULL timestamps"
        );
    }

    #[tokio::test]
    async fn test_v2_entry_start_equal_to_end_is_accepted() {
        // Relay's check is `start <= end` — a zero-duration span is valid.
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "span-v2-zero-duration".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let payload = json!({
            "version": 2,
            "items": [{
                "trace_id": "d3d20f000885466b8c8f947c9b92b8d3",
                "span_id": "0f0f0f0f0f0f0f0f",
                "start_timestamp": 1784231017.5,
                "end_timestamp": 1784231017.5,
                "attributes": {}
            }]
        })
        .to_string();

        SpanV2Processor
            .process(
                bytes::Bytes::from(payload.into_bytes()),
                &ctx(&db.pool, project.id),
            )
            .await
            .unwrap();

        const QUERY: &str = "SELECT duration_ms FROM spans WHERE project_id = $1";
        let row = sqlx::query(QUERY)
            .bind(project.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();

        let duration_ms: Option<f64> = row.get("duration_ms");
        assert_eq!(
            duration_ms,
            Some(0.0),
            "start == end is a valid zero-duration span, not a discard"
        );
    }

    #[tokio::test]
    async fn test_v2_sentry_attributes_are_promoted_to_columns() {
        // v2 carries as `sentry.*` attributes what the legacy schema kept as
        // top-level span fields — Relay's OTel->v2 path passes these exact keys
        // through (relay-spans/src/otel_to_sentry_v2.rs), so a real
        // OTel-instrumented SDK span arrives with them.
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "span-v2-sentry-attrs".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let payload = json!({
            "version": 2,
            "items": [{
                "trace_id": "d3d20f000885466b8c8f947c9b92b8d3",
                "span_id": "1a1a1a1a1a1a1a1a",
                "start_timestamp": 1.0,
                "end_timestamp": 3.0,
                "attributes": {
                    "sentry.exclusive_time": { "value": 1000.0, "type": "double" },
                    "sentry.platform": { "value": "javascript", "type": "string" },
                    "sentry.release": { "value": "v1.2.3", "type": "string" },
                    "sentry.environment": { "value": "production", "type": "string" }
                }
            }]
        })
        .to_string();

        SpanV2Processor
            .process(
                bytes::Bytes::from(payload.into_bytes()),
                &ctx(&db.pool, project.id),
            )
            .await
            .unwrap();

        const QUERY: &str =
            "SELECT exclusive_time_ms, platform, release, environment FROM spans WHERE project_id = $1";
        let row = sqlx::query(QUERY)
            .bind(project.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();

        let exclusive_time_ms: Option<f64> = row.get("exclusive_time_ms");
        let platform: Option<String> = row.get("platform");
        let release: Option<String> = row.get("release");
        let environment: Option<String> = row.get("environment");

        assert_eq!(
            exclusive_time_ms,
            Some(1000.0),
            "sentry.exclusive_time is the v2 wire equivalent of legacy exclusive_time"
        );
        assert_eq!(platform.as_deref(), Some("javascript"));
        assert_eq!(release.as_deref(), Some("v1.2.3"));
        assert_eq!(environment.as_deref(), Some("production"));
    }

    #[tokio::test]
    async fn test_v2_child_span_takes_segment_id_from_attribute() {
        // v2 has no `segment_id` field: a non-root span's link to its segment
        // lives only in the `sentry.segment.id` attribute. Deriving segment_id
        // from `is_segment` alone drops that link for every child.
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "span-v2-child-segment".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let payload = json!({
            "version": 2,
            "items": [{
                "trace_id": "d3d20f000885466b8c8f947c9b92b8d3",
                "span_id": "2b2b2b2b2b2b2b2b",
                "parent_span_id": "9999999999999999",
                "start_timestamp": 1.0,
                "end_timestamp": 2.0,
                "is_segment": false,
                "attributes": {
                    "sentry.segment.id": { "value": "9999999999999999", "type": "string" }
                }
            }]
        })
        .to_string();

        SpanV2Processor
            .process(
                bytes::Bytes::from(payload.into_bytes()),
                &ctx(&db.pool, project.id),
            )
            .await
            .unwrap();

        const QUERY: &str = "SELECT is_segment, segment_id FROM spans WHERE project_id = $1";
        let row = sqlx::query(QUERY)
            .bind(project.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();

        let is_segment: bool = row.get("is_segment");
        let segment_id: Option<String> = row.get("segment_id");

        assert!(!is_segment, "this span is a child, not a segment root");
        assert_eq!(
            segment_id.as_deref(),
            Some("9999999999999999"),
            "a child span must keep its link to the segment root"
        );
    }
}

// ── Attribute type/value agreement ──────────────────────────────────────────
//
// Relay drops any attribute whose declared `type` disagrees with its `value`,
// rather than repairing it (`normalize_attribute_types`,
// relay-event-normalization/src/eap/mod.rs). Its own comment gives the reason:
// downstream consumers require consistent shapes. Rustrak has such a consumer
// (`models::transaction::span_attributes`), so it drops them too.

#[test]
fn test_flat_attributes_drops_a_value_that_contradicts_its_declared_type() {
    // `AttributeType` has no object variant at all — boolean, integer, double,
    // string, array. An object value can only ever arrive on a malformed
    // payload, and storing it lets one attribute masquerade as a nested bag.
    let container = json!({
        "version": 2,
        "items": [{
            "trace_id": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "span_id": "1111111111111111",
            "start_timestamp": 1.0,
            "end_timestamp": 2.0,
            "attributes": {
                "data": { "value": { "nested": 1 }, "type": "string" },
                "gen_ai.request.model": { "value": "gpt-4o", "type": "string" }
            }
        }]
    });

    let entries = parse_span_v2_container(container.to_string().as_bytes()).unwrap();
    let flat = entries[0].flat_attributes();

    assert!(
        flat.get("data").is_none(),
        "an object value under a string type must be dropped, not stored"
    );
    assert_eq!(
        flat["gen_ai.request.model"], "gpt-4o",
        "dropping one bad attribute must not disturb its siblings"
    );
}

#[test]
fn test_flat_attributes_keeps_every_type_the_protocol_defines() {
    let container = json!({
        "version": 2,
        "items": [{
            "trace_id": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "span_id": "1111111111111111",
            "start_timestamp": 1.0,
            "end_timestamp": 2.0,
            "attributes": {
                "s": { "value": "text", "type": "string" },
                "i": { "value": 42, "type": "integer" },
                "d": { "value": 1.5, "type": "double" },
                "b": { "value": true, "type": "boolean" },
                "a": { "value": ["x", "y"], "type": "array" },
                // Relay accepts an integer under `double` — a JSON number that
                // happens to be whole must not be mistaken for a mismatch.
                "whole_double": { "value": 3, "type": "double" }
            }
        }]
    });

    let entries = parse_span_v2_container(container.to_string().as_bytes()).unwrap();
    let flat = entries[0].flat_attributes();

    assert_eq!(flat["s"], "text");
    assert_eq!(flat["i"], 42);
    assert_eq!(flat["d"], 1.5);
    assert_eq!(flat["b"], true);
    assert_eq!(flat["a"], json!(["x", "y"]));
    assert_eq!(flat["whole_double"], 3);
}

#[test]
fn test_flat_attributes_drops_an_unknown_declared_type() {
    // Relay removes these outright rather than passing them through, even
    // though `AttributeType::Unknown` exists for forward compatibility.
    let container = json!({
        "version": 2,
        "items": [{
            "trace_id": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "span_id": "1111111111111111",
            "start_timestamp": 1.0,
            "end_timestamp": 2.0,
            "attributes": {
                "future": { "value": {"a": 1}, "type": "object" },
                "kept": { "value": "ok", "type": "string" }
            }
        }]
    });

    let entries = parse_span_v2_container(container.to_string().as_bytes()).unwrap();
    let flat = entries[0].flat_attributes();

    assert!(flat.get("future").is_none());
    assert_eq!(flat["kept"], "ok");
}

/// Relay's `SpanV2.start_timestamp`/`end_timestamp` are `Annotated<Timestamp>`,
/// which accept an RFC 3339 string as well as epoch seconds (issue #427).
#[test]
fn test_parse_container_accepts_rfc3339_timestamps() {
    let body = br#"{"version":2,"items":[{
        "trace_id":"d3d20f000885466b8c8f947c9b92b8d3",
        "span_id":"dddddddddddddddd",
        "start_timestamp":"2026-10-19T09:50:34.5Z",
        "end_timestamp":1792403435.0
    }]}"#;
    let spans = parse_span_v2_container(body).unwrap();
    assert!(spans[0].start_timestamp.is_some());
    assert!(spans[0].end_timestamp.is_some());
}
