//! Unit tests for standalone Span (Sentry "span" item type) ingestion.
//!
//! Standalone spans are NOT containerized like logs — each envelope item
//! holds exactly one flat span JSON object (Relay's legacy `Span` schema).

#[cfg(test)]
mod level2 {
    use crate::common::TestDb;
    use chrono::Utc;
    use rustrak::digest::processors::{ProcessorCtx, SpanProcessor};
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
    async fn test_span_batch_skips_malformed_and_crosses_chunk_boundary() {
        // Invalid items are skipped, while valid items survive the 64-item
        // transaction boundary.
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "span-batch".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let valid = |span_id: String| {
            serde_json::to_vec(&json!({
                "op": "http.client",
                "span_id": span_id,
                "start_timestamp": 1234567890.0,
                "timestamp": 1234567890.5,
                "trace_id": "d3d20f000885466b8c8f947c9b92b8d3",
            }))
            .unwrap()
        };
        let malformed = serde_json::to_vec(&json!({
            "trace_id": "d3d20f000885466b8c8f947c9b92b8d3",
            "start_timestamp": 1.0,
            "timestamp": 2.0,
        }))
        .unwrap();
        let mut items = vec![
            bytes::Bytes::from(valid("0000000000000000".to_string())),
            bytes::Bytes::from(malformed),
        ];
        items.extend((1..65).map(|i| bytes::Bytes::from(valid(format!("{i:016x}")))));

        SpanProcessor
            .process_batch(items, &ctx(&db.pool, project.id))
            .await
            .unwrap();

        const SPANS_QUERY: &str = "SELECT COUNT(*) FROM spans WHERE project_id = $1";
        let count: i64 = sqlx::query_scalar(SPANS_QUERY)
            .bind(project.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(count, 65);
    }

    #[tokio::test]
    async fn test_chunk_failure_commits_earlier_chunks_only() {
        // Pins the tx granularity of process_batch — the observable contract
        // of chunking. A duplicate span_id in the SECOND chunk fails only
        // that chunk, leaving the first chunk (64 spans) committed. A
        // reverted parse-all/one-tx implementation rolls everything back
        // (count 0), so this test fails if chunking is ever removed.
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "span-chunk-failure".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        // The production schema has no uniqueness on span_id — inject it so
        // the duplicate below fails the INSERT instead of landing twice.
        sqlx::query("CREATE UNIQUE INDEX test_spans_uniq_span_id ON spans(project_id, span_id)")
            .execute(&db.pool)
            .await
            .unwrap();

        let span_json = |span_id: &str, trace_id: &str| {
            bytes::Bytes::from(
                serde_json::to_vec(&json!({
                    "op": "http.client",
                    "span_id": span_id,
                    "start_timestamp": 1234567890.0,
                    "timestamp": 1234567890.5,
                    "trace_id": trace_id,
                }))
                .unwrap(),
            )
        };

        let trace_id = "d3d20f000885466b8c8f947c9b92b8d3";
        let mut items: Vec<bytes::Bytes> = (0..64)
            .map(|i| span_json(&format!("{i:016x}"), trace_id))
            .collect();
        // Same span_id under another trace bypasses the application dedup key
        // but violates the test-only unique index.
        items.push(span_json(
            "0000000000000000",
            "e4e31f111996577c9f8a58d9c03c9e4",
        ));

        let result = SpanProcessor
            .process_batch(items, &ctx(&db.pool, project.id))
            .await;
        assert!(result.is_err(), "the duplicate must fail the second chunk");

        const SPANS_QUERY: &str = "SELECT COUNT(*) FROM spans WHERE project_id = $1";
        let count: i64 = sqlx::query_scalar(SPANS_QUERY)
            .bind(project.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(
            count, 64,
            "the first chunk committed before the duplicate failed the second"
        );
    }

    #[tokio::test]
    async fn test_valid_span_stored_with_null_transaction_id() {
        // A standalone span (real Relay wire-format fixture, see
        // tests/integration/test_spans_standalone.py in the story's Dev Notes)
        // must land in the shared `spans` table with transaction_id NULL —
        // it is NOT attached to any transaction.
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "span-standalone".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let payload = serde_json::to_vec(&json!({
            "op": "http.client",
            "data": {"http.request.method": "GET"},
            "description": "Test span",
            "parent_span_id": "8a6626cc9bdd5d9b",
            "span_id": "9fd17741416e8e4e",
            "start_timestamp": 1234567890.0,
            "timestamp": 1234567890.5,
            "trace_id": "d3d20f000885466b8c8f947c9b92b8d3",
            "origin": "manual",
            "segment_id": "8a6626cc9bdd5d9b",
            "is_segment": false
        }))
        .unwrap();

        SpanProcessor
            .process(
                bytes::Bytes::from(payload.clone()),
                &ctx(&db.pool, project.id),
            )
            .await
            .unwrap();
        SpanProcessor
            .process(bytes::Bytes::from(payload), &ctx(&db.pool, project.id))
            .await
            .unwrap();

        const QUERY: &str = "SELECT span_id, trace_id, parent_span_id, op, description, transaction_id, duration_ms FROM spans WHERE project_id = $1";

        let rows = sqlx::query(QUERY)
            .bind(project.id)
            .fetch_all(&db.pool)
            .await
            .unwrap();
        assert_eq!(
            rows.len(),
            1,
            "replaying a standalone span must not duplicate it"
        );
        let row = rows.into_iter().next().unwrap();

        let span_id: Option<String> = row.get("span_id");
        let trace_id: Option<String> = row.get("trace_id");
        let parent_span_id: Option<String> = row.get("parent_span_id");
        let op: Option<String> = row.get("op");
        let description: Option<String> = row.get("description");
        let transaction_id: Option<Uuid> = row.get("transaction_id");
        let duration_ms: Option<f64> = row.get("duration_ms");

        assert_eq!(span_id.as_deref(), Some("9fd17741416e8e4e"));
        assert_eq!(
            trace_id.as_deref(),
            Some("d3d20f000885466b8c8f947c9b92b8d3")
        );
        assert_eq!(parent_span_id.as_deref(), Some("8a6626cc9bdd5d9b"));
        assert_eq!(op.as_deref(), Some("http.client"));
        assert_eq!(description.as_deref(), Some("Test span"));
        assert!(
            transaction_id.is_none(),
            "standalone span must not be linked to a transaction"
        );
        assert_eq!(duration_ms, Some(500.0), "0.5s span -> 500ms");
    }

    #[tokio::test]
    async fn test_span_missing_span_id_is_rejected() {
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "span-missing-id".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let payload = serde_json::to_vec(&json!({
            "trace_id": "d3d20f000885466b8c8f947c9b92b8d3",
            "start_timestamp": 1.0,
            "timestamp": 2.0
        }))
        .unwrap();

        let res = SpanProcessor
            .process(bytes::Bytes::from(payload), &ctx(&db.pool, project.id))
            .await;
        assert!(res.is_err(), "span without span_id must be rejected");

        const COUNT: &str = "SELECT COUNT(*) FROM spans WHERE project_id = $1";
        let count: i64 = sqlx::query_scalar(COUNT)
            .bind(project.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(count, 0, "no row may be stored for a rejected span");
    }

    #[tokio::test]
    async fn test_span_missing_trace_id_is_rejected() {
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "span-missing-trace".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let payload = serde_json::to_vec(&json!({
            "span_id": "9fd17741416e8e4e",
            "start_timestamp": 1.0,
            "timestamp": 2.0
        }))
        .unwrap();

        let res = SpanProcessor
            .process(bytes::Bytes::from(payload), &ctx(&db.pool, project.id))
            .await;
        assert!(res.is_err(), "span without trace_id must be rejected");
    }

    #[tokio::test]
    async fn test_span_missing_start_timestamp_is_rejected() {
        // Relay's validate_standalone_span: "span is missing start_timestamp".
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "span-missing-start".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let payload = serde_json::to_vec(&json!({
            "span_id": "9fd17741416e8e4e",
            "trace_id": "d3d20f000885466b8c8f947c9b92b8d3",
            "timestamp": 2.0
        }))
        .unwrap();

        let res = SpanProcessor
            .process(bytes::Bytes::from(payload), &ctx(&db.pool, project.id))
            .await;
        assert!(
            res.is_err(),
            "span without start_timestamp must be rejected"
        );

        const COUNT: &str = "SELECT COUNT(*) FROM spans WHERE project_id = $1";
        let count: i64 = sqlx::query_scalar(COUNT)
            .bind(project.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(count, 0, "no row may be stored for a rejected span");
    }

    #[tokio::test]
    async fn test_span_missing_end_timestamp_is_rejected() {
        // Relay's validate_standalone_span: "span is missing timestamp".
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "span-missing-end".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let payload = serde_json::to_vec(&json!({
            "span_id": "9fd17741416e8e4e",
            "trace_id": "d3d20f000885466b8c8f947c9b92b8d3",
            "start_timestamp": 1.0
        }))
        .unwrap();

        let res = SpanProcessor
            .process(bytes::Bytes::from(payload), &ctx(&db.pool, project.id))
            .await;
        assert!(res.is_err(), "span without timestamp must be rejected");
    }

    #[tokio::test]
    async fn test_span_start_after_end_is_rejected() {
        // Relay: DiscardReason::Timestamp when start_timestamp > timestamp.
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "span-bad-timing".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let payload = serde_json::to_vec(&json!({
            "span_id": "9fd17741416e8e4e",
            "trace_id": "d3d20f000885466b8c8f947c9b92b8d3",
            "start_timestamp": 2.0,
            "timestamp": 1.0
        }))
        .unwrap();

        let res = SpanProcessor
            .process(bytes::Bytes::from(payload), &ctx(&db.pool, project.id))
            .await;
        assert!(
            res.is_err(),
            "span with start_timestamp after timestamp must be rejected"
        );
    }

    #[tokio::test]
    async fn test_span_malformed_json_is_rejected() {
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "span-bad-json".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let res = SpanProcessor
            .process(
                bytes::Bytes::from(vec![0xff, 0x00, b'n', b'o']),
                &ctx(&db.pool, project.id),
            )
            .await;
        assert!(res.is_err(), "malformed span JSON must be rejected");
    }

    #[tokio::test]
    async fn test_span_data_column_stores_full_raw_payload() {
        // Mirrors TransactionProcessor::insert_span: the `data` column holds
        // the entire raw span JSON (full fidelity for debugging/future
        // gen_ai.* normalization), not just the nested `data` attributes
        // sub-object — consistent whether a span arrives standalone or
        // embedded in a transaction, since both write into the same table.
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "span-data-catchall".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let payload = serde_json::to_vec(&json!({
            "span_id": "9fd17741416e8e4e",
            "trace_id": "d3d20f000885466b8c8f947c9b92b8d3",
            "start_timestamp": 1.0,
            "timestamp": 2.0,
            "data": {"gen_ai.request.model": "gpt-4", "some.unknown.attr": 42}
        }))
        .unwrap();

        SpanProcessor
            .process(bytes::Bytes::from(payload), &ctx(&db.pool, project.id))
            .await
            .unwrap();

        const QUERY: &str = "SELECT data FROM spans WHERE project_id = $1";
        let data: serde_json::Value = sqlx::query_scalar(QUERY)
            .bind(project.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();

        assert_eq!(data["span_id"], "9fd17741416e8e4e");
        assert_eq!(data["data"]["gen_ai.request.model"], "gpt-4");
        assert_eq!(data["data"]["some.unknown.attr"], 42);
    }

    async fn store_span(
        pool: &rustrak::db::DbPool,
        project_id: i32,
        span_id: &str,
        trace_id: &str,
        op: &str,
    ) {
        let payload = serde_json::to_vec(&json!({
            "span_id": span_id,
            "trace_id": trace_id,
            "op": op,
            "start_timestamp": 1.0,
            "timestamp": 2.0
        }))
        .unwrap();
        SpanProcessor
            .process(bytes::Bytes::from(payload), &ctx(pool, project_id))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_list_spans_by_trace_id_returns_both_standalone_and_transaction_spans() {
        // AC #7: a trace_id query returns spans regardless of origin, since
        // standalone spans and transaction-embedded spans share this table.
        use rustrak::digest::processors::TransactionProcessor;
        use rustrak::services::span::{SpanFilters, SpanService};

        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "span-svc-cross-origin".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let shared_trace = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";

        // A standalone span sharing the trace.
        store_span(
            &db.pool,
            project.id,
            "1111111111111111",
            shared_trace,
            "http.client",
        )
        .await;

        // A transaction whose embedded span carries the SAME trace_id.
        let txn_payload = serde_json::to_vec(&json!({
            "type": "transaction",
            "transaction": "/api/x",
            "start_timestamp": 1.0,
            "timestamp": 2.0,
            "contexts": { "trace": { "trace_id": shared_trace, "span_id": "2222222222222222" } },
            "spans": [
                { "span_id": "3333333333333333", "op": "db.query", "start_timestamp": 1.0, "timestamp": 1.5 }
            ]
        }))
        .unwrap();
        TransactionProcessor
            .process(bytes::Bytes::from(txn_payload), &ctx(&db.pool, project.id))
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
            "both the standalone span and the transaction-embedded span share this trace_id"
        );
        assert_eq!(list.len(), 2);
    }

    #[tokio::test]
    async fn test_list_spans_standalone_has_no_transaction_id() {
        use rustrak::services::span::{SpanFilters, SpanService};

        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "span-svc-txn-id".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        store_span(
            &db.pool,
            project.id,
            "4444444444444444",
            "trace4",
            "http.client",
        )
        .await;

        let (list, _) =
            SpanService::list_offset(&db.pool, project.id, 1, 20, &SpanFilters::default())
                .await
                .unwrap();

        assert_eq!(list.len(), 1);
        assert!(
            list[0].transaction_id.is_none(),
            "standalone span response must report no parent transaction"
        );
    }

    #[tokio::test]
    async fn test_standalone_ai_span_normalized() {
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "span-gen-ai".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let payload = serde_json::to_vec(&json!({
            "span_id": "5555555555555555",
            "trace_id": "d3d20f000885466b8c8f947c9b92b8d3",
            "start_timestamp": 1.0,
            "timestamp": 2.0,
            "data": {
                "gen_ai.operation.name": "invoke_agent",
                "gen_ai.request.model": "gpt-4o",
                "gen_ai.usage.input_tokens": 1000,
                "gen_ai.usage.output_tokens": 500
            }
        }))
        .unwrap();

        SpanProcessor
            .process(bytes::Bytes::from(payload), &ctx(&db.pool, project.id))
            .await
            .unwrap();

        const QUERY: &str = "SELECT gen_ai_operation_type, gen_ai_response_model, gen_ai_usage_total_tokens FROM spans WHERE project_id = $1";

        let row = sqlx::query(QUERY)
            .bind(project.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();

        let operation_type: Option<String> = row.get("gen_ai_operation_type");
        let response_model: Option<String> = row.get("gen_ai_response_model");
        let total_tokens: Option<f64> = row.get("gen_ai_usage_total_tokens");

        assert_eq!(operation_type.as_deref(), Some("agent"));
        assert_eq!(response_model.as_deref(), Some("gpt-4o"));
        assert_eq!(total_tokens, Some(1500.0));
    }

    #[tokio::test]
    async fn test_non_ai_standalone_span_leaves_gen_ai_columns_null() {
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "span-not-ai".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let payload = serde_json::to_vec(&json!({
            "span_id": "6666666666666666",
            "trace_id": "d3d20f000885466b8c8f947c9b92b8d3",
            "op": "http.client",
            "start_timestamp": 1.0,
            "timestamp": 2.0
        }))
        .unwrap();

        SpanProcessor
            .process(bytes::Bytes::from(payload), &ctx(&db.pool, project.id))
            .await
            .unwrap();

        const QUERY: &str = "SELECT gen_ai_operation_type FROM spans WHERE project_id = $1";

        let row = sqlx::query(QUERY)
            .bind(project.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();

        let operation_type: Option<String> = row.get("gen_ai_operation_type");

        assert!(
            operation_type.is_none(),
            "non-AI span must not get gen_ai columns populated"
        );
    }
}
