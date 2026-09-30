//! TDD tests for the typed envelope dispatch pipeline (spec: spec-transaction-processing.md)
//!
//! Level 1 — pure unit tests, no DB, no async.
//! Level 2 — processor DB behavior tests, #[tokio::test] + TestDb.

use rustrak::ingest::envelope::{EnvelopeItemKind, ItemHeaders};
use rustrak::ingest::parser::EnvelopeParser;

// =============================================================================
// Level 1 — Pure enum dispatch tests (no DB, no async)
// =============================================================================

#[test]
fn test_enum_dispatch_transaction_maps_to_transaction_kind() {
    let headers = ItemHeaders {
        item_type: "transaction".into(),
        length: None,
        content_type: None,
    };
    let kind = EnvelopeItemKind::from((headers, bytes::Bytes::from(b"{}".to_vec())));
    assert!(matches!(kind, EnvelopeItemKind::Transaction(_)));
}

#[test]
fn test_enum_dispatch_event_maps_to_event_kind() {
    let headers = ItemHeaders {
        item_type: "event".into(),
        length: None,
        content_type: None,
    };
    let kind = EnvelopeItemKind::from((headers, bytes::Bytes::from(b"{}".to_vec())));
    assert!(matches!(kind, EnvelopeItemKind::Event(_)));
}

#[test]
fn test_transaction_never_becomes_event_item() {
    // Regression guard for fb52e1e: transaction items must NEVER land in Event variant
    let raw = b"{\"event_id\":\"9ec79c33ec9942ab8353589fcb2e04dc\"}\n{\"type\":\"transaction\",\"length\":2}\n{}\n";
    let mut parser = EnvelopeParser::new(bytes::Bytes::copy_from_slice(raw));
    let items = parser.parse().unwrap().items;
    assert!(
        items
            .iter()
            .all(|i| !matches!(i, EnvelopeItemKind::Event(_))),
        "transaction item must not be classified as Event"
    );
}

#[test]
fn test_requires_event_true_for_transaction() {
    let kind = EnvelopeItemKind::Transaction(vec![].into());
    assert!(kind.requires_event());
}

#[test]
fn test_requires_event_true_for_event() {
    let kind = EnvelopeItemKind::Event(vec![].into());
    assert!(kind.requires_event());
}

#[test]
fn test_requires_event_false_for_session() {
    use rustrak::models::session::SessionUpdate;
    let kind = EnvelopeItemKind::Session(SessionUpdate::default());
    assert!(!kind.requires_event());
}

// =============================================================================
// Level 1 — Processor routing (pure: item -> which processor handles it)
// =============================================================================

#[test]
fn test_event_item_routes_to_error_processor() {
    use rustrak::digest::processors::{route, Route};
    assert_eq!(route(&EnvelopeItemKind::Event(vec![].into())), Route::Error);
}

#[test]
fn test_transaction_item_routes_to_transaction_processor() {
    use rustrak::digest::processors::{route, Route};
    assert_eq!(
        route(&EnvelopeItemKind::Transaction(vec![].into())),
        Route::Transaction
    );
}

#[test]
fn test_session_item_routes_to_session_processor() {
    use rustrak::digest::processors::{route, Route};
    use rustrak::models::session::SessionUpdate;
    assert_eq!(
        route(&EnvelopeItemKind::Session(SessionUpdate::default())),
        Route::Session
    );
}

#[test]
fn test_unknown_item_routes_to_ignored() {
    use rustrak::digest::processors::{route, Route};
    assert_eq!(
        route(&EnvelopeItemKind::Other("span".into(), vec![].into())),
        Route::Ignored
    );
}

// =============================================================================
// Level 2 — Processor DB behavior tests (#[tokio::test] + TestDb)
// =============================================================================

#[cfg(test)]
mod level2 {
    use crate::common::TestDb;
    use chrono::{DateTime, Utc};
    use rustrak::digest::processors::{Processor, ProcessorCtx, TransactionProcessor};
    use rustrak::models::CreateProject;
    use rustrak::services::ProjectService;
    use serde_json::json;
    use sqlx::Row;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_transaction_stored_in_dedicated_transactions_table() {
        // A transaction must land in the dedicated `transactions` table, not
        // be overloaded onto `events`. duration_ms is precomputed at write time
        // from (timestamp - start_timestamp), in milliseconds.
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "txn-dedicated".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let payload = serde_json::to_vec(&json!({
            "type": "transaction",
            "transaction": "/api/users",
            "start_timestamp": 1704801590.0,
            "timestamp": 1704801600.0,
            "spans": [{"op": "db", "description": "SELECT 1"}]
        }))
        .unwrap();

        let ctx = ProcessorCtx {
            pool: db.pool.clone(),
            project_id: project.id,
            event_id: Uuid::new_v4(),
            ingested_at: Utc::now(),
            remote_addr: None,
        };

        TransactionProcessor
            .process(bytes::Bytes::from(payload.clone()), &ctx)
            .await
            .unwrap();
        TransactionProcessor
            .process(bytes::Bytes::from(payload), &ctx)
            .await
            .unwrap();

        #[cfg(feature = "postgres")]
        const QUERY: &str =
            "SELECT transaction_name, duration_ms FROM transactions WHERE project_id = $1";
        #[cfg(not(feature = "postgres"))]
        const QUERY: &str =
            "SELECT transaction_name, duration_ms FROM transactions WHERE project_id = ?";

        let row = sqlx::query(QUERY)
            .bind(project.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();

        let name: String = row.get("transaction_name");
        let duration_ms: f64 = row.get("duration_ms");
        assert_eq!(name, "/api/users");
        assert_eq!(duration_ms, 10_000.0, "10s transaction → 10000ms");

        let transaction_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM transactions WHERE project_id = $1 AND event_id = $2",
        )
        .bind(project.id)
        .bind(ctx.event_id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(transaction_count, 1);
    }

    #[tokio::test]
    async fn test_transaction_normalizes_trace_context_to_columns() {
        // Relay denormalizes contexts.trace.{op,status,span_id} and the trace_id
        // into queryable columns. Rustrak must do the same so the list view can
        // filter by op/status without parsing JSON on every row.
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "txn-trace-ctx".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let payload = serde_json::to_vec(&json!({
            "type": "transaction",
            "transaction": "/api/checkout",
            "start_timestamp": 1.0,
            "timestamp": 2.0,
            "contexts": {
                "trace": {
                    "trace_id": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    "span_id": "bbbbbbbbbbbbbbbb",
                    "parent_span_id": "cccccccccccccccc",
                    "op": "http.server",
                    "status": "ok"
                }
            }
        }))
        .unwrap();

        let ctx = ProcessorCtx {
            pool: db.pool.clone(),
            project_id: project.id,
            event_id: Uuid::new_v4(),
            ingested_at: Utc::now(),
            remote_addr: None,
        };

        TransactionProcessor
            .process(bytes::Bytes::from(payload), &ctx)
            .await
            .unwrap();

        #[cfg(feature = "postgres")]
        const QUERY: &str = "SELECT op, status, span_id, parent_span_id, trace_id FROM transactions WHERE project_id = $1";
        #[cfg(not(feature = "postgres"))]
        const QUERY: &str = "SELECT op, status, span_id, parent_span_id, trace_id FROM transactions WHERE project_id = ?";

        let row = sqlx::query(QUERY)
            .bind(project.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();

        let op: Option<String> = row.get("op");
        let status: Option<String> = row.get("status");
        let span_id: Option<String> = row.get("span_id");
        let parent_span_id: Option<String> = row.get("parent_span_id");
        let trace_id: Option<String> = row.get("trace_id");

        assert_eq!(op.as_deref(), Some("http.server"));
        assert_eq!(status.as_deref(), Some("ok"));
        assert_eq!(span_id.as_deref(), Some("bbbbbbbbbbbbbbbb"));
        assert_eq!(parent_span_id.as_deref(), Some("cccccccccccccccc"));
        assert_eq!(
            trace_id.as_deref(),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
    }

    #[tokio::test]
    async fn test_transaction_source_known_value_stored_verbatim() {
        // transaction_info.source carries Relay's TransactionSource. Known values
        // (url/route/...) are stored verbatim into the `source` column.
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "txn-source-url".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let payload = serde_json::to_vec(&json!({
            "type": "transaction",
            "transaction": "/users/:id",
            "timestamp": 2.0,
            "transaction_info": { "source": "route" }
        }))
        .unwrap();

        let ctx = ProcessorCtx {
            pool: db.pool.clone(),
            project_id: project.id,
            event_id: Uuid::new_v4(),
            ingested_at: Utc::now(),
            remote_addr: None,
        };
        TransactionProcessor
            .process(bytes::Bytes::from(payload), &ctx)
            .await
            .unwrap();

        #[cfg(feature = "postgres")]
        const QUERY: &str = "SELECT source FROM transactions WHERE project_id = $1";
        #[cfg(not(feature = "postgres"))]
        const QUERY: &str = "SELECT source FROM transactions WHERE project_id = ?";

        let source: String = sqlx::query_scalar(QUERY)
            .bind(project.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(source, "route");
    }

    #[tokio::test]
    async fn test_transaction_source_defaults_to_unknown_when_absent() {
        // No transaction_info → Relay's TransactionSource default is Unknown.
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "txn-source-default".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let payload = serde_json::to_vec(&json!({
            "type": "transaction",
            "transaction": "/health",
            "timestamp": 2.0
        }))
        .unwrap();

        let ctx = ProcessorCtx {
            pool: db.pool.clone(),
            project_id: project.id,
            event_id: Uuid::new_v4(),
            ingested_at: Utc::now(),
            remote_addr: None,
        };
        TransactionProcessor
            .process(bytes::Bytes::from(payload), &ctx)
            .await
            .unwrap();

        #[cfg(feature = "postgres")]
        const QUERY: &str = "SELECT source FROM transactions WHERE project_id = $1";
        #[cfg(not(feature = "postgres"))]
        const QUERY: &str = "SELECT source FROM transactions WHERE project_id = ?";

        let source: String = sqlx::query_scalar(QUERY)
            .bind(project.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(source, "unknown");
    }

    #[tokio::test]
    async fn test_transaction_spans_extracted_to_indexed_rows() {
        // Relay extracts each span in the `spans` array into a standalone indexed
        // row (DataCategory::SpanIndexed). Rustrak stores them in the `spans`
        // table, linked to the transaction, individually queryable by op/trace.
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "txn-span-extract".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let payload = serde_json::to_vec(&json!({
            "type": "transaction",
            "transaction": "/api/x",
            "start_timestamp": 1.0,
            "timestamp": 2.0,
            "contexts": {
                "trace": { "trace_id": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "span_id": "bbbbbbbbbbbbbbbb" }
            },
            "spans": [
                { "span_id": "cccccccccccccccc", "parent_span_id": "bbbbbbbbbbbbbbbb",
                  "op": "db.query", "description": "SELECT 1", "status": "ok",
                  "start_timestamp": 1.0, "timestamp": 1.5 },
                { "span_id": "dddddddddddddddd", "parent_span_id": "cccccccccccccccc",
                  "op": "http.client", "description": "GET /x",
                  "start_timestamp": 1.5, "timestamp": 1.8 }
            ]
        }))
        .unwrap();

        let ctx = ProcessorCtx {
            pool: db.pool.clone(),
            project_id: project.id,
            event_id: Uuid::new_v4(),
            ingested_at: Utc::now(),
            remote_addr: None,
        };
        TransactionProcessor
            .process(bytes::Bytes::from(payload), &ctx)
            .await
            .unwrap();

        #[cfg(feature = "postgres")]
        const COUNT_QUERY: &str = "SELECT COUNT(*) FROM spans WHERE project_id = $1";
        #[cfg(not(feature = "postgres"))]
        const COUNT_QUERY: &str = "SELECT COUNT(*) FROM spans WHERE project_id = ?";

        let count: i64 = sqlx::query_scalar(COUNT_QUERY)
            .bind(project.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(
            count, 2,
            "both spans must be extracted into the spans table"
        );

        #[cfg(feature = "postgres")]
        const SPAN_QUERY: &str = "SELECT op, description, status, duration_ms, trace_id, parent_span_id FROM spans WHERE span_id = $1";
        #[cfg(not(feature = "postgres"))]
        const SPAN_QUERY: &str = "SELECT op, description, status, duration_ms, trace_id, parent_span_id FROM spans WHERE span_id = ?";

        let row = sqlx::query(SPAN_QUERY)
            .bind("cccccccccccccccc")
            .fetch_one(&db.pool)
            .await
            .unwrap();

        let op: Option<String> = row.get("op");
        let description: Option<String> = row.get("description");
        let status: Option<String> = row.get("status");
        let duration_ms: f64 = row.get("duration_ms");
        let trace_id: Option<String> = row.get("trace_id");
        let parent_span_id: Option<String> = row.get("parent_span_id");

        assert_eq!(op.as_deref(), Some("db.query"));
        assert_eq!(description.as_deref(), Some("SELECT 1"));
        assert_eq!(status.as_deref(), Some("ok"));
        assert_eq!(duration_ms, 500.0, "0.5s span → 500ms");
        assert_eq!(
            trace_id.as_deref(),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            "span inherits the transaction trace_id when not carried on the span"
        );
        assert_eq!(parent_span_id.as_deref(), Some("bbbbbbbbbbbbbbbb"));
    }

    #[tokio::test]
    async fn test_transaction_malformed_json_is_rejected_not_stored() {
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "txn-bad-json".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let ctx = ProcessorCtx {
            pool: db.pool.clone(),
            project_id: project.id,
            event_id: Uuid::new_v4(),
            ingested_at: Utc::now(),
            remote_addr: None,
        };

        // Binary garbage some misconfigured SDKs send — not valid JSON.
        let res = TransactionProcessor
            .process(bytes::Bytes::from(vec![0xff, 0x00, 0x01, b'n', b'o']), &ctx)
            .await;

        assert!(res.is_err(), "malformed transaction JSON must be rejected");
        #[cfg(feature = "postgres")]
        const QUERY: &str = "SELECT COUNT(*) FROM transactions WHERE project_id = $1";
        #[cfg(not(feature = "postgres"))]
        const QUERY: &str = "SELECT COUNT(*) FROM transactions WHERE project_id = ?";

        let count: i64 = sqlx::query_scalar(QUERY)
            .bind(project.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(count, 0, "no row may be stored for a malformed payload");
    }

    #[tokio::test]
    async fn test_transaction_pagination_no_skip_on_equal_timestamps() {
        use rustrak::services::TransactionService;

        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "txn-paging".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        // Three transactions that all share the SAME ingested_at (page boundary).
        let ts = Utc::now();
        for _ in 0..3 {
            let ctx = ProcessorCtx {
                pool: db.pool.clone(),
                project_id: project.id,
                event_id: Uuid::new_v4(),
                ingested_at: ts,
                remote_addr: None,
            };
            let payload = serde_json::to_vec(&json!({
                "type": "transaction", "transaction": "/x",
                "start_timestamp": 1.0, "timestamp": 2.0
            }))
            .unwrap();
            TransactionProcessor
                .process(bytes::Bytes::from(payload), &ctx)
                .await
                .unwrap();
        }

        let filters = rustrak::services::TransactionFilters::default();
        let (page1, _) = TransactionService::list_offset(&db.pool, project.id, 1, 2, &filters)
            .await
            .unwrap();
        assert_eq!(page1.len(), 2);

        let (page2, _) = TransactionService::list_offset(&db.pool, project.id, 2, 2, &filters)
            .await
            .unwrap();

        assert_eq!(
            page1.len() + page2.len(),
            3,
            "no transaction may be skipped at an equal-timestamp page boundary"
        );
    }

    #[tokio::test]
    async fn test_transaction_stores_to_db_with_correct_fields() {
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "txn-test".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let payload = serde_json::to_vec(&json!({
            "type": "transaction",
            "transaction": "/api/users",
            "start_timestamp": 1704801590.0,
            "timestamp": 1704801600.0,
            "spans": [{"op": "db", "description": "SELECT 1"}]
        }))
        .unwrap();

        let ctx = ProcessorCtx {
            pool: db.pool.clone(),
            project_id: project.id,
            event_id: Uuid::new_v4(),
            ingested_at: Utc::now(),
            remote_addr: None,
        };

        TransactionProcessor
            .process(bytes::Bytes::from(payload), &ctx)
            .await
            .unwrap();

        #[cfg(feature = "postgres")]
        const QUERY: &str =
            "SELECT transaction_name, start_timestamp FROM transactions WHERE project_id = $1";
        #[cfg(not(feature = "postgres"))]
        const QUERY: &str =
            "SELECT transaction_name, start_timestamp FROM transactions WHERE project_id = ?";

        let row = sqlx::query(QUERY)
            .bind(project.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();

        let transaction_name: String = row.get("transaction_name");
        // `start_timestamp` is TIMESTAMPTZ on Postgres, TEXT on SQLite; sqlx
        // decodes both into `DateTime<Utc>` (see src/services/transaction.rs,
        // which reads this same column the same way in production).
        let start_timestamp: Option<DateTime<Utc>> = row.get("start_timestamp");

        assert_eq!(transaction_name, "/api/users");
        assert!(
            start_timestamp.is_some(),
            "start_timestamp must not be NULL"
        );
    }

    #[tokio::test]
    async fn test_transaction_processor_impl_stores_via_trait() {
        use rustrak::digest::processors::Processor;

        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "txn-trait".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let payload = serde_json::to_vec(&json!({
            "type": "transaction",
            "transaction": "/api/trait",
            "start_timestamp": 1704801590.0,
            "timestamp": 1704801600.0,
            "spans": [{"op": "db", "description": "SELECT 1"}]
        }))
        .unwrap();

        let ctx = ProcessorCtx {
            pool: db.pool.clone(),
            project_id: project.id,
            event_id: Uuid::new_v4(),
            ingested_at: Utc::now(),
            remote_addr: None,
        };

        // Drive the trait into existence: dispatch through the Processor contract.
        TransactionProcessor
            .process(bytes::Bytes::from(payload), &ctx)
            .await
            .unwrap();

        #[cfg(feature = "postgres")]
        const QUERY: &str = "SELECT transaction_name FROM transactions WHERE project_id = $1";
        #[cfg(not(feature = "postgres"))]
        const QUERY: &str = "SELECT transaction_name FROM transactions WHERE project_id = ?";

        let name: String = sqlx::query_scalar(QUERY)
            .bind(project.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(name, "/api/trait");
    }

    #[tokio::test]
    async fn test_session_processor_forwards_update_to_aggregator() {
        use rustrak::digest::processors::{Processor, SessionItem, SessionProcessor};
        use rustrak::models::session::{SessionAttributes, SessionStatus, SessionUpdate};
        use rustrak::workers::session_aggregator::SessionAggregator;

        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "sess-fwd".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let handle = SessionAggregator::new(db.pool.clone(), 3600, 1000);
        let processor = SessionProcessor::new(Some(handle.clone()));

        let update = SessionUpdate {
            sid: Some("sid-1".to_string()),
            did: None,
            seq: Some(0),
            init: true,
            started: Some("2026-06-10T10:00:00.000Z".to_string()),
            timestamp: None,
            duration: None,
            status: Some(SessionStatus::Ok),
            errors: 0,
            attrs: Some(SessionAttributes {
                release: Some("1.0.0".to_string()),
                environment: Some("production".to_string()),
            }),
        };

        let ctx = ProcessorCtx {
            pool: db.pool.clone(),
            project_id: project.id,
            event_id: Uuid::new_v4(),
            ingested_at: Utc::now(),
            remote_addr: None,
        };

        processor
            .process(SessionItem::Update(update), &ctx)
            .await
            .unwrap();
        handle.flush().await.unwrap();

        #[cfg(feature = "postgres")]
        const QUERY: &str = "SELECT COUNT(*) FROM session_counts WHERE project_id = $1";
        #[cfg(not(feature = "postgres"))]
        const QUERY: &str = "SELECT COUNT(*) FROM session_counts WHERE project_id = ?";

        let count: i64 = sqlx::query_scalar(QUERY)
            .bind(project.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert!(
            count >= 1,
            "session must be aggregated and flushed to session_counts"
        );
    }

    #[tokio::test]
    async fn test_list_transactions_filters_by_op() {
        use rustrak::services::{TransactionFilters, TransactionService};

        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "txn-filter-op".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        for op in ["http.server", "db.query"] {
            let ctx = ProcessorCtx {
                pool: db.pool.clone(),
                project_id: project.id,
                event_id: Uuid::new_v4(),
                ingested_at: Utc::now(),
                remote_addr: None,
            };
            let payload = serde_json::to_vec(&json!({
                "type": "transaction", "transaction": "/x", "timestamp": 2.0,
                "contexts": { "trace": { "op": op } }
            }))
            .unwrap();
            TransactionProcessor
                .process(bytes::Bytes::from(payload), &ctx)
                .await
                .unwrap();
        }

        let filters = TransactionFilters {
            op: Some("http.server".to_string()),
            ..Default::default()
        };
        let (list, total) = TransactionService::list_offset(&db.pool, project.id, 1, 20, &filters)
            .await
            .unwrap();

        assert_eq!(
            total, 1,
            "only the http.server transaction matches the filter"
        );
        assert_eq!(list.len(), 1);
    }

    #[tokio::test]
    async fn test_list_transactions_filters_by_name() {
        // Drilling into a grouped row needs to list only that transaction's
        // samples — so the list must filter by exact transaction_name.
        use rustrak::services::{TransactionFilters, TransactionService};

        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "txn-filter-name".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        for name in ["/api/users", "/api/users", "/api/checkout"] {
            let ctx = ProcessorCtx {
                pool: db.pool.clone(),
                project_id: project.id,
                event_id: Uuid::new_v4(),
                ingested_at: Utc::now(),
                remote_addr: None,
            };
            let payload = serde_json::to_vec(&json!({
                "type": "transaction", "transaction": name, "timestamp": 2.0,
                "contexts": { "trace": { "op": "http.server" } }
            }))
            .unwrap();
            TransactionProcessor
                .process(bytes::Bytes::from(payload), &ctx)
                .await
                .unwrap();
        }

        let filters = TransactionFilters {
            name: Some("/api/users".to_string()),
            ..Default::default()
        };
        let (list, total) = TransactionService::list_offset(&db.pool, project.id, 1, 20, &filters)
            .await
            .unwrap();

        assert_eq!(total, 2, "only the two /api/users samples match");
        assert!(list.iter().all(|t| t.transaction_name == "/api/users"));
    }

    #[tokio::test]
    async fn test_list_spans_returns_extracted_spans_ordered() {
        use rustrak::services::TransactionService;

        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "txn-list-spans".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let ctx = ProcessorCtx {
            pool: db.pool.clone(),
            project_id: project.id,
            event_id: Uuid::new_v4(),
            ingested_at: Utc::now(),
            remote_addr: None,
        };
        let payload = serde_json::to_vec(&json!({
            "type": "transaction", "transaction": "/api/x",
            "start_timestamp": 1.0, "timestamp": 2.0,
            "contexts": { "trace": { "trace_id": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "span_id": "bbbbbbbbbbbbbbbb" } },
            "spans": [
                { "span_id": "dddddddddddddddd", "op": "http.client", "start_timestamp": 1.5, "timestamp": 1.8 },
                { "span_id": "cccccccccccccccc", "op": "db.query", "start_timestamp": 1.0, "timestamp": 1.5 }
            ]
        }))
        .unwrap();
        TransactionProcessor
            .process(bytes::Bytes::from(payload), &ctx)
            .await
            .unwrap();

        #[cfg(feature = "postgres")]
        const QUERY: &str = "SELECT id FROM transactions WHERE project_id = $1";
        #[cfg(not(feature = "postgres"))]
        const QUERY: &str = "SELECT id FROM transactions WHERE project_id = ?";

        let txn_id: Uuid = sqlx::query_scalar(QUERY)
            .bind(project.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();

        let spans = TransactionService::list_spans(&db.pool, project.id, txn_id)
            .await
            .unwrap();

        assert_eq!(spans.len(), 2);
        // Ordered by start_timestamp ASC → db.query (1.0) before http.client (1.5).
        assert_eq!(spans[0].op.as_deref(), Some("db.query"));
        assert_eq!(spans[1].op.as_deref(), Some("http.client"));
        assert_eq!(spans[0].duration_ms, Some(500.0));
    }

    #[tokio::test]
    async fn test_transaction_stats_aggregates_per_name_and_op() {
        use rustrak::services::TransactionService;

        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "txn-stats".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        // Group "/a" (http.server): 3 transactions, durations 100/200/300ms,
        // one failed (status != ok). Group "/b" (db): 1 transaction.
        let samples = [
            ("/a", "http.server", 0.0, 0.1, Some("ok")),
            ("/a", "http.server", 0.0, 0.2, Some("internal_error")),
            ("/a", "http.server", 0.0, 0.3, Some("ok")),
            ("/b", "db", 0.0, 0.05, Some("ok")),
        ];
        for (name, op, start, end, status) in samples {
            let ctx = ProcessorCtx {
                pool: db.pool.clone(),
                project_id: project.id,
                event_id: Uuid::new_v4(),
                ingested_at: Utc::now(),
                remote_addr: None,
            };
            let payload = serde_json::to_vec(&json!({
                "type": "transaction", "transaction": name,
                "start_timestamp": start, "timestamp": end,
                "contexts": { "trace": { "op": op, "status": status } }
            }))
            .unwrap();
            TransactionProcessor
                .process(bytes::Bytes::from(payload), &ctx)
                .await
                .unwrap();
        }

        let (stats, total) = TransactionService::stats(&db.pool, project.id, 1, 20, None)
            .await
            .unwrap();
        assert_eq!(total, 2, "two (name, op) groups total");
        assert_eq!(stats.len(), 2, "two (name, op) groups");

        let a = stats
            .iter()
            .find(|s| s.transaction_name == "/a")
            .expect("group /a present");
        assert_eq!(a.op.as_deref(), Some("http.server"));
        assert_eq!(a.count, 3);
        assert_eq!(a.p50_ms, 200.0, "median of 100/200/300");
        assert!(
            (a.failure_rate - 1.0 / 3.0).abs() < 1e-9,
            "1 of 3 failed → ~0.333"
        );
    }

    #[tokio::test]
    async fn test_transaction_stats_are_paginated() {
        use rustrak::services::TransactionService;

        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "txn-stats-paging".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        // Five distinct (name, op) groups.
        for i in 0..5 {
            let ctx = ProcessorCtx {
                pool: db.pool.clone(),
                project_id: project.id,
                event_id: Uuid::new_v4(),
                ingested_at: Utc::now(),
                remote_addr: None,
            };
            let payload = serde_json::to_vec(&json!({
                "type": "transaction", "transaction": format!("/g{i}"),
                "start_timestamp": 0.0, "timestamp": 0.1,
                "contexts": { "trace": { "op": "http.server" } }
            }))
            .unwrap();
            TransactionProcessor
                .process(bytes::Bytes::from(payload), &ctx)
                .await
                .unwrap();
        }

        let (page1, total) = TransactionService::stats(&db.pool, project.id, 1, 2, None)
            .await
            .unwrap();
        assert_eq!(total, 5, "total reflects all groups, not the page size");
        assert_eq!(page1.len(), 2, "page is limited to per_page groups");

        let (page3, _) = TransactionService::stats(&db.pool, project.id, 3, 2, None)
            .await
            .unwrap();
        assert_eq!(page3.len(), 1, "last page has the remaining group");
    }

    #[tokio::test]
    async fn test_list_spans_unknown_transaction_is_not_found() {
        use rustrak::services::TransactionService;

        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "txn-spans-404".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let res = TransactionService::list_spans(&db.pool, project.id, Uuid::new_v4()).await;
        assert!(
            matches!(res, Err(rustrak::error::AppError::NotFound(_))),
            "unknown transaction id must be NotFound, not an empty list"
        );
    }

    #[tokio::test]
    async fn test_stats_for_group_returns_single_aggregate() {
        // Powers the summary header regardless of how many groups exist — a
        // direct (name, op) lookup, not "fetch page 1 and find".
        use rustrak::services::TransactionService;

        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "txn-stats-group".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        for (name, end) in [("/a", 0.1), ("/a", 0.3), ("/b", 0.05)] {
            let ctx = ProcessorCtx {
                pool: db.pool.clone(),
                project_id: project.id,
                event_id: Uuid::new_v4(),
                ingested_at: Utc::now(),
                remote_addr: None,
            };
            let payload = serde_json::to_vec(&json!({
                "type": "transaction", "transaction": name,
                "start_timestamp": 0.0, "timestamp": end,
                "contexts": { "trace": { "op": "http.server" } }
            }))
            .unwrap();
            TransactionProcessor
                .process(bytes::Bytes::from(payload), &ctx)
                .await
                .unwrap();
        }

        let group = TransactionService::stats_for_group(
            &db.pool,
            project.id,
            "/a",
            Some("http.server"),
            None,
        )
        .await
        .unwrap()
        .expect("group /a present");
        assert_eq!(group.transaction_name, "/a");
        assert_eq!(group.op.as_deref(), Some("http.server"));
        assert_eq!(group.count, 2);

        let missing =
            TransactionService::stats_for_group(&db.pool, project.id, "/nope", None, None)
                .await
                .unwrap();
        assert!(missing.is_none(), "unknown group yields None");
    }

    #[tokio::test]
    async fn test_stats_ordering_is_stable_on_count_ties() {
        // Same name, two ops, same count → order must be deterministic across
        // requests so offset pagination doesn't reshuffle groups.
        use rustrak::services::TransactionService;

        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "txn-stats-tie".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        for op in ["db.query", "http.client", "db.query", "http.client"] {
            let ctx = ProcessorCtx {
                pool: db.pool.clone(),
                project_id: project.id,
                event_id: Uuid::new_v4(),
                ingested_at: Utc::now(),
                remote_addr: None,
            };
            let payload = serde_json::to_vec(&json!({
                "type": "transaction", "transaction": "/x",
                "start_timestamp": 0.0, "timestamp": 0.1,
                "contexts": { "trace": { "op": op } }
            }))
            .unwrap();
            TransactionProcessor
                .process(bytes::Bytes::from(payload), &ctx)
                .await
                .unwrap();
        }

        let (first, _) = TransactionService::stats(&db.pool, project.id, 1, 20, None)
            .await
            .unwrap();
        let (second, _) = TransactionService::stats(&db.pool, project.id, 1, 20, None)
            .await
            .unwrap();

        let order1: Vec<_> = first.iter().map(|s| s.op.clone()).collect();
        let order2: Vec<_> = second.iter().map(|s| s.op.clone()).collect();
        assert_eq!(order1, order2, "tie order must be stable across requests");
    }

    #[tokio::test]
    async fn test_transaction_does_not_create_grouping() {
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "txn-no-group".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let payload = serde_json::to_vec(&json!({
            "type": "transaction",
            "transaction": "/health",
            "start_timestamp": 1704801590.0,
            "timestamp": 1704801600.0,
            "spans": []
        }))
        .unwrap();

        let ctx = ProcessorCtx {
            pool: db.pool.clone(),
            project_id: project.id,
            event_id: Uuid::new_v4(),
            ingested_at: Utc::now(),
            remote_addr: None,
        };

        TransactionProcessor
            .process(bytes::Bytes::from(payload), &ctx)
            .await
            .unwrap();

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM groupings")
            .fetch_one(&db.pool)
            .await
            .unwrap();

        assert_eq!(
            count, 0,
            "TransactionProcessor must not create any groupings"
        );
    }

    // =========================================================================
    // gen_ai.* normalization on transaction-embedded spans
    // (story-ai-agent-monitoring.md) — must match standalone-span behavior
    // exactly (shared normalization function, not duplicated logic).
    // =========================================================================

    #[tokio::test]
    async fn test_transaction_embedded_ai_span_normalized() {
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "txn-gen-ai-child".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let payload = serde_json::to_vec(&json!({
            "type": "transaction",
            "transaction": "/api/chat",
            "start_timestamp": 1.0,
            "timestamp": 3.0,
            "spans": [
                {
                    "span_id": "8888888888888888",
                    "op": "gen_ai.chat.completions",
                    "start_timestamp": 1.5,
                    "timestamp": 2.5,
                    "data": {
                        "gen_ai.request.model": "claude-3-5-sonnet",
                        "gen_ai.usage.input_tokens": 200,
                        "gen_ai.usage.output_tokens": 100
                    }
                }
            ]
        }))
        .unwrap();

        let ctx = ProcessorCtx {
            pool: db.pool.clone(),
            project_id: project.id,
            event_id: Uuid::new_v4(),
            ingested_at: Utc::now(),
            remote_addr: None,
        };
        TransactionProcessor
            .process(bytes::Bytes::from(payload), &ctx)
            .await
            .unwrap();

        #[cfg(feature = "postgres")]
        const QUERY: &str = "SELECT gen_ai_operation_type, gen_ai_response_model, gen_ai_usage_total_tokens FROM spans WHERE project_id = $1 AND span_id = $2";
        #[cfg(not(feature = "postgres"))]
        const QUERY: &str = "SELECT gen_ai_operation_type, gen_ai_response_model, gen_ai_usage_total_tokens FROM spans WHERE project_id = ? AND span_id = ?";

        let row = sqlx::query(QUERY)
            .bind(project.id)
            .bind("8888888888888888")
            .fetch_one(&db.pool)
            .await
            .unwrap();

        let operation_type: Option<String> = row.get("gen_ai_operation_type");
        let response_model: Option<String> = row.get("gen_ai_response_model");
        let total_tokens: Option<f64> = row.get("gen_ai_usage_total_tokens");

        // op "gen_ai.chat.completions" doesn't match any infer_operation_type
        // pattern, and there's no operation.name/type set → defaults to "ai_client".
        assert_eq!(operation_type.as_deref(), Some("ai_client"));
        assert_eq!(response_model.as_deref(), Some("claude-3-5-sonnet"));
        assert_eq!(total_tokens, Some(300.0));
    }

    #[tokio::test]
    async fn test_transaction_ai_root_span_promoted_to_own_span_row() {
        // Real-SDK gap (story-span-v2-protocol.md follow-up): with
        // @sentry/node + Vercel AI SDK's vercelAIIntegration(), the trace's
        // ROOT span ("invoke_agent") is sent inline on the transaction
        // event's contexts.trace — never as its own span item — carrying
        // client-side-accumulated token/cost totals in contexts.trace.data.
        // Confirmed live: without promoting it to a `spans` row,
        // SpanService::agent_runs_timeseries (which counts
        // gen_ai_operation_type='agent' rows) sees nothing, even though the
        // Traces widget (which sums across ALL spans in a trace_id) works.
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "txn-ai-root-span".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let payload = serde_json::to_vec(&json!({
            "type": "transaction",
            "transaction": "invoke_agent research_agent",
            "start_timestamp": 1.0,
            "timestamp": 3.0,
            "contexts": {
                "trace": {
                    "trace_id": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    "span_id": "bbbbbbbbbbbbbbbb",
                    "op": "gen_ai.invoke_agent",
                    "status": "ok",
                    "data": {
                        "gen_ai.function_id": "research_agent",
                        "gen_ai.usage.input_tokens": 512,
                        "gen_ai.usage.output_tokens": 130
                    }
                }
            },
            "spans": []
        }))
        .unwrap();

        let ctx = ProcessorCtx {
            pool: db.pool.clone(),
            project_id: project.id,
            event_id: Uuid::new_v4(),
            ingested_at: Utc::now(),
            remote_addr: None,
        };
        TransactionProcessor
            .process(bytes::Bytes::from(payload), &ctx)
            .await
            .unwrap();

        #[cfg(feature = "postgres")]
        const QUERY: &str = "SELECT span_id, trace_id, op, description, status, is_segment, segment_id, transaction_id, start_timestamp, timestamp, gen_ai_operation_type, gen_ai_agent_name, gen_ai_usage_total_tokens FROM spans WHERE project_id = $1";
        #[cfg(not(feature = "postgres"))]
        const QUERY: &str = "SELECT span_id, trace_id, op, description, status, is_segment, segment_id, transaction_id, start_timestamp, timestamp, gen_ai_operation_type, gen_ai_agent_name, gen_ai_usage_total_tokens FROM spans WHERE project_id = ?";

        let row = sqlx::query(QUERY)
            .bind(project.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();

        let span_id: Option<String> = row.get("span_id");
        let trace_id: Option<String> = row.get("trace_id");
        let op: Option<String> = row.get("op");
        let description: Option<String> = row.get("description");
        let status: Option<String> = row.get("status");
        let is_segment: bool = row.get("is_segment");
        let segment_id: Option<String> = row.get("segment_id");
        let transaction_id: Option<Uuid> = row.get("transaction_id");
        let operation_type: Option<String> = row.get("gen_ai_operation_type");
        let agent_name: Option<String> = row.get("gen_ai_agent_name");
        let total_tokens: Option<f64> = row.get("gen_ai_usage_total_tokens");

        assert_eq!(span_id.as_deref(), Some("bbbbbbbbbbbbbbbb"));
        assert_eq!(
            trace_id.as_deref(),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
        assert_eq!(op.as_deref(), Some("gen_ai.invoke_agent"));
        assert_eq!(
            description.as_deref(),
            Some("invoke_agent research_agent"),
            "description must come from the transaction name"
        );
        assert_eq!(status.as_deref(), Some("ok"));
        assert!(
            is_segment,
            "the trace root must be marked as its own segment"
        );
        assert_eq!(segment_id.as_deref(), Some("bbbbbbbbbbbbbbbb"));
        assert!(
            transaction_id.is_some(),
            "the promoted root span is still linked to its parent transaction, unlike a standalone span"
        );
        assert_eq!(
            operation_type.as_deref(),
            Some("agent"),
            "gen_ai.invoke_agent infers to \"agent\""
        );
        assert_eq!(agent_name.as_deref(), Some("research_agent"));
        assert_eq!(total_tokens, Some(642.0), "512 input + 130 output");
    }

    #[tokio::test]
    async fn test_transaction_non_ai_contexts_trace_does_not_promote_extra_span_row() {
        // Regression guard: ordinary (non-AI) transactions must keep
        // today's exact span count — do not synthesize a root span row
        // unless contexts.trace is actually recognized as an AI span.
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "txn-non-ai-root-no-promote".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let payload = serde_json::to_vec(&json!({
            "type": "transaction",
            "transaction": "/api/checkout",
            "start_timestamp": 1.0,
            "timestamp": 2.0,
            "contexts": {
                "trace": {
                    "trace_id": "cccccccccccccccccccccccccccccc",
                    "span_id": "dddddddddddddddd",
                    "op": "http.server",
                    "status": "ok"
                }
            }
        }))
        .unwrap();

        let ctx = ProcessorCtx {
            pool: db.pool.clone(),
            project_id: project.id,
            event_id: Uuid::new_v4(),
            ingested_at: Utc::now(),
            remote_addr: None,
        };
        TransactionProcessor
            .process(bytes::Bytes::from(payload), &ctx)
            .await
            .unwrap();

        #[cfg(feature = "postgres")]
        const COUNT: &str = "SELECT COUNT(*) FROM spans WHERE project_id = $1";
        #[cfg(not(feature = "postgres"))]
        const COUNT: &str = "SELECT COUNT(*) FROM spans WHERE project_id = ?";
        let count: i64 = sqlx::query_scalar(COUNT)
            .bind(project.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(
            count, 0,
            "a non-AI contexts.trace must not create any spans row (no spans[] either here)"
        );
    }

    #[tokio::test]
    async fn test_transaction_ai_root_span_promoted_without_trace_data() {
        // Regression guard: a tool-only agent run (or an agent span that
        // failed before accumulating token data) can send
        // contexts.trace.op = "gen_ai.invoke_agent" with no `data` key at
        // all. `is_ai_span` recognizes the op alone, so the root must still
        // be promoted even though there's nothing to normalize in `data`.
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "txn-ai-root-span-no-data".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let payload = serde_json::to_vec(&json!({
            "type": "transaction",
            "transaction": "invoke_agent research_agent",
            "start_timestamp": 1.0,
            "timestamp": 3.0,
            "contexts": {
                "trace": {
                    "trace_id": "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
                    "span_id": "ffffffffffffffff",
                    "op": "gen_ai.invoke_agent",
                    "status": "ok"
                }
            },
            "spans": []
        }))
        .unwrap();

        let ctx = ProcessorCtx {
            pool: db.pool.clone(),
            project_id: project.id,
            event_id: Uuid::new_v4(),
            ingested_at: Utc::now(),
            remote_addr: None,
        };
        TransactionProcessor
            .process(bytes::Bytes::from(payload), &ctx)
            .await
            .unwrap();

        #[cfg(feature = "postgres")]
        const QUERY: &str =
            "SELECT span_id, gen_ai_operation_type FROM spans WHERE project_id = $1";
        #[cfg(not(feature = "postgres"))]
        const QUERY: &str = "SELECT span_id, gen_ai_operation_type FROM spans WHERE project_id = ?";

        let row = sqlx::query(QUERY)
            .bind(project.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();

        let span_id: Option<String> = row.get("span_id");
        let operation_type: Option<String> = row.get("gen_ai_operation_type");

        assert_eq!(span_id.as_deref(), Some("ffffffffffffffff"));
        assert_eq!(
            operation_type.as_deref(),
            Some("agent"),
            "op alone (no contexts.trace.data) must still promote and classify as \"agent\""
        );
    }

    #[tokio::test]
    async fn test_agent_runs_timeseries_counts_promoted_root_span() {
        // The actual widget this fixes: before promotion, an AI trace whose
        // root only exists in contexts.trace was invisible to "Agent Runs".
        use rustrak::services::span::SpanService;

        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "txn-agent-runs-widget".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let payload = serde_json::to_vec(&json!({
            "type": "transaction",
            "transaction": "invoke_agent research_agent",
            "start_timestamp": 1.0,
            "timestamp": 2.0,
            "contexts": {
                "trace": {
                    "trace_id": "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
                    "span_id": "ffffffffffffffff",
                    "op": "gen_ai.invoke_agent",
                    "data": { "gen_ai.usage.input_tokens": 10, "gen_ai.usage.output_tokens": 5 }
                }
            }
        }))
        .unwrap();

        let ctx = ProcessorCtx {
            pool: db.pool.clone(),
            project_id: project.id,
            event_id: Uuid::new_v4(),
            ingested_at: Utc::now(),
            remote_addr: None,
        };
        TransactionProcessor
            .process(bytes::Bytes::from(payload), &ctx)
            .await
            .unwrap();

        let points =
            SpanService::agent_runs_timeseries(&db.pool, project.id, &Default::default(), None, 1)
                .await
                .unwrap();

        let total: f64 = points.iter().map(|p| p.value).sum();
        assert_eq!(
            total, 1.0,
            "the promoted root span must be counted by the Agent Runs widget"
        );
    }

    #[tokio::test]
    async fn test_transaction_non_ai_child_span_leaves_gen_ai_columns_null() {
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "txn-not-ai-child".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let payload = serde_json::to_vec(&json!({
            "type": "transaction",
            "transaction": "/api/x",
            "start_timestamp": 1.0,
            "timestamp": 2.0,
            "spans": [
                { "span_id": "9999999999999999", "op": "db.query",
                  "start_timestamp": 1.0, "timestamp": 1.5 }
            ]
        }))
        .unwrap();

        let ctx = ProcessorCtx {
            pool: db.pool.clone(),
            project_id: project.id,
            event_id: Uuid::new_v4(),
            ingested_at: Utc::now(),
            remote_addr: None,
        };
        TransactionProcessor
            .process(bytes::Bytes::from(payload), &ctx)
            .await
            .unwrap();

        #[cfg(feature = "postgres")]
        const QUERY: &str =
            "SELECT gen_ai_operation_type FROM spans WHERE project_id = $1 AND span_id = $2";
        #[cfg(not(feature = "postgres"))]
        const QUERY: &str =
            "SELECT gen_ai_operation_type FROM spans WHERE project_id = ? AND span_id = ?";

        let operation_type: Option<String> = sqlx::query_scalar(QUERY)
            .bind(project.id)
            .bind("9999999999999999")
            .fetch_one(&db.pool)
            .await
            .unwrap();

        assert!(
            operation_type.is_none(),
            "non-AI transaction-child span must not get gen_ai columns populated"
        );
    }

    #[tokio::test]
    async fn test_promoted_agent_root_inherits_the_transaction_environment() {
        // The root agent span is synthesized from contexts.trace, which carries no
        // environment of its own — the transaction event does. Without inheriting
        // it, filtering the agents dashboard by environment drops every agent span
        // while keeping its children: "Agent runs" reads 0 next to a non-zero
        // "LLM calls" for the same traces.
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "root-span-env".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let payload = serde_json::to_vec(&json!({
            "event_id": "aa11bb22cc33dd44ee55ff6677889900",
            "transaction": "invoke_agent planner",
            "environment": "production",
            "release": "web@1.0.0",
            "platform": "javascript",
            "start_timestamp": 1.0,
            "timestamp": 3.0,
            "contexts": {
                "trace": {
                    "trace_id": "ffffffffffffffffffffffffffffffff",
                    "span_id": "1234567890abcdef",
                    "op": "gen_ai.invoke_agent",
                    "data": {
                        "gen_ai.operation.name": "invoke_agent",
                        "gen_ai.agent.name": "planner"
                    }
                }
            },
            "spans": []
        }))
        .unwrap();

        let ctx = ProcessorCtx {
            pool: db.pool.clone(),
            project_id: project.id,
            event_id: Uuid::nil(),
            ingested_at: Utc::now(),
            remote_addr: None,
        };
        TransactionProcessor
            .process(bytes::Bytes::from(payload), &ctx)
            .await
            .unwrap();

        let row: (Option<String>, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT environment, release, platform FROM spans
             WHERE project_id = $1 AND gen_ai_operation_type = 'agent'",
        )
        .bind(project.id)
        .fetch_one(&db.pool)
        .await
        .expect("the promoted agent root span exists");

        assert_eq!(row.0.as_deref(), Some("production"));
        assert_eq!(row.1.as_deref(), Some("web@1.0.0"));
        assert_eq!(row.2.as_deref(), Some("javascript"));
    }

    #[tokio::test]
    async fn test_transaction_child_span_inherits_the_transaction_environment() {
        // Same gap as the promoted root, one level down. The schema comment
        // says a transaction-embedded span "inherits these from its parent
        // transaction row instead" -- true if you JOIN, but the agents
        // dashboard filters `spans` directly, so a NULL here silently drops
        // every transaction-embedded AI span from an environment-filtered view.
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "child-span-env".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let payload = serde_json::to_vec(&json!({
            "event_id": "bb22cc33dd44ee55ff66770088990011",
            "transaction": "POST /chat",
            "environment": "production",
            "release": "web@1.0.0",
            "platform": "javascript",
            "start_timestamp": 1.0,
            "timestamp": 3.0,
            "contexts": {"trace": {"trace_id": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "span_id": "1111111111111111"}},
            "spans": [{
                "span_id": "2222222222222222",
                "parent_span_id": "1111111111111111",
                "trace_id": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "op": "gen_ai.chat",
                "start_timestamp": 1.2,
                "timestamp": 2.8,
                "data": {
                    "gen_ai.operation.type": "ai_client",
                    "gen_ai.request.model": "gpt-4o"
                }
            }]
        }))
        .unwrap();

        let ctx = ProcessorCtx {
            pool: db.pool.clone(),
            project_id: project.id,
            event_id: Uuid::nil(),
            ingested_at: Utc::now(),
            remote_addr: None,
        };
        TransactionProcessor
            .process(bytes::Bytes::from(payload), &ctx)
            .await
            .unwrap();

        let row: (Option<String>, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT environment, release, platform FROM spans
             WHERE project_id = $1 AND span_id = '2222222222222222'",
        )
        .bind(project.id)
        .fetch_one(&db.pool)
        .await
        .expect("the child span exists");

        assert_eq!(row.0.as_deref(), Some("production"));
        assert_eq!(row.1.as_deref(), Some("web@1.0.0"));
        assert_eq!(row.2.as_deref(), Some("javascript"));
    }
}
