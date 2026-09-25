//! Full HTTP → dispatch → DB integration test for Sentry Spans Protocol v2
//! ingestion (application/vnd.sentry.items.span.v2+json).
//!
//! Uses a real wire fixture captured off the wire from an actual
//! @sentry/node 10.65 + Vercel AI SDK `generateText()` + tool call (via a
//! local echo server acting as the DSN target) — the exact envelope shape
//! that motivated story-span-v2-protocol.md. This is the closest automated
//! equivalent to running `packages/test-sentry/demo/src/ai-agent.ts`
//! against a live server (the story's Task 6).

use crate::common::TestDb;
use actix_web::{test, web, App};
use rustrak::config::{Config, DashboardConfig, DatabaseConfig, RateLimitConfig};
use rustrak::routes;
use rustrak::services::{DbSourceMapProvider, LocalSourceMapStore, ProjectService};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

fn create_test_config() -> Config {
    Config {
        host: "127.0.0.1".to_string(),
        port: 0,
        database: DatabaseConfig {
            url: "sqlite::memory:".to_string(),
            max_connections: 5,
            min_connections: 1,
            acquire_timeout: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(60),
            max_lifetime: Duration::from_secs(300),
        },
        rate_limit: RateLimitConfig {
            max_events_per_minute: 1000,
            max_events_per_hour: 10000,
            max_events_per_project_per_minute: 500,
            max_events_per_project_per_hour: 5000,
        },
        security: rustrak::config::SecurityConfig {
            ssl_proxy: false,
            session_secret_key: None,
        },
        ingest_dir: Some("/tmp/rustrak_test_ingest_span_v2".to_string()),
        public_url: None,
        sourcemap_storage_path: "/tmp/test_sourcemaps_span_v2".to_string(),
        sourcemap_cache_bytes: 64 * 1024 * 1024,
        max_chunk_size_bytes: 10 * 1024 * 1024,
        session_flush_interval_secs: 30,
        session_cardinality_cap: 10_000,
        dashboard: DashboardConfig {
            dir: "./static".to_string(),
            enabled: true,
            url: None,
        },
        telemetry: rustrak::config::TelemetryConfig {
            enabled: false,
            do_not_track: false,
        },
    }
}

async fn create_test_project(pool: &rustrak::db::DbPool, name: &str) -> (i32, String) {
    let project = ProjectService::create(
        pool,
        rustrak::models::CreateProject {
            name: name.to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .expect("Failed to create test project");
    (project.id, project.sentry_key.to_string())
}

/// Real wire fixture: a `research_agent` trace, as actually sent by
/// @sentry/node 10.65 + Vercel AI SDK's `vercelAIIntegration()` — the trace
/// root ("invoke_agent") arrives inline on a `transaction` item's
/// `contexts.trace` (never as its own span item, see
/// `digest/processors/transaction.rs`'s root-span promotion), while its two
/// `generate_content` LLM-call spans and one `execute_tool` span arrive
/// batched in a single Spans Protocol v2 container item. Both items share
/// one envelope and one `trace_id`, and the root's `span_id`
/// (`8efc25d3729c267c`) matches the children's `parent_span_id`.
fn span_v2_envelope(trace_id: &str) -> Vec<u8> {
    let transaction = json!({
        "type": "transaction",
        "transaction": "invoke_agent research_agent",
        "start_timestamp": 1784231017.89,
        "timestamp": 1784231017.90,
        "contexts": {
            "trace": {
                "trace_id": trace_id,
                "span_id": "8efc25d3729c267c",
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
    });
    let transaction_str = transaction.to_string();
    let transaction_header = json!({
        "type": "transaction",
        "length": transaction_str.len()
    });

    let container = json!({
        "version": 2,
        "items": [
            {
                "trace_id": trace_id,
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
                "trace_id": trace_id,
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
            },
            {
                "trace_id": trace_id,
                "span_id": "815d683f6b3697cc",
                "parent_span_id": "8efc25d3729c267c",
                "name": "generate_content gpt-4o",
                "start_timestamp": 1784231017.8947513,
                "end_timestamp": 1784231017.894881,
                "status": "ok",
                "is_segment": false,
                "attributes": {
                    "sentry.origin": { "value": "auto.vercelai.otel", "type": "string" },
                    "sentry.op": { "value": "gen_ai.generate_content", "type": "string" },
                    "gen_ai.operation.name": { "value": "generate_content", "type": "string" },
                    "gen_ai.request.model": { "value": "gpt-4o", "type": "string" },
                    "gen_ai.function_id": { "value": "research_agent", "type": "string" },
                    "gen_ai.usage.input_tokens": { "value": 302, "type": "integer" },
                    "gen_ai.usage.output_tokens": { "value": 96, "type": "integer" }
                }
            }
        ]
    });
    let container_str = container.to_string();

    let item_header = json!({
        "type": "span",
        "content_type": "application/vnd.sentry.items.span.v2+json",
        "item_count": 3,
        "length": container_str.len()
    });

    format!(
        "{}\n{}\n{}\n{}\n{}\n",
        json!({}),
        transaction_header,
        transaction_str,
        item_header,
        container_str
    )
    .into_bytes()
}

/// Full pipeline: real captured wire format -> POST /envelope/ -> spawned
/// SpanV2Processor -> `spans` table, with gen_ai.* normalization applied
/// identically to the legacy/transaction producers, then queryable through
/// the same Agents API the dashboard uses.
#[actix_web::test]
async fn test_real_sdk_span_v2_envelope_populates_agents_dashboard() {
    let db = TestDb::new().await;
    let (project_id, sentry_key) = create_test_project(&db.pool, "Span V2 Ingest Test").await;
    let config = create_test_config();

    let sourcemap_store = Arc::new(LocalSourceMapStore::new(&config.sourcemap_storage_path))
        as Arc<dyn rustrak::services::SourceMapStore>;
    let sourcemap_provider = Arc::new(DbSourceMapProvider::new(db.pool.clone(), sourcemap_store))
        as Arc<dyn rustrak::services::SourceMapProvider>;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data(web::Data::new(sourcemap_provider))
            .app_data(web::Data::new(
                rustrak::digest::processors::Processors::new(
                    rustrak::ingest::get_ingest_dir(config.ingest_dir.as_deref()),
                    config.rate_limit.clone(),
                    crate::common::null_sourcemap_provider(),
                    None,
                ),
            ))
            .configure(routes::ingest::configure),
    )
    .await;

    let trace_id = Uuid::new_v4().to_string().replace("-", "");
    let body = span_v2_envelope(&trace_id);

    let req = test::TestRequest::post()
        .uri(&format!("/api/{}/envelope/", project_id))
        .insert_header(("X-Sentry-Auth", format!("Sentry sentry_key={}", sentry_key)))
        .insert_header(("Content-Type", "application/x-sentry-envelope"))
        .set_payload(body)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status().as_u16(),
        200,
        "span v2 envelope must return 200"
    );

    // The processor runs in a spawned task; poll (bounded) instead of a
    // fixed sleep so the assertion isn't flaky on slower CI runners.
    #[cfg(feature = "postgres")]
    const COUNT_QUERY: &str = "SELECT COUNT(*) FROM spans WHERE project_id = $1";
    #[cfg(not(feature = "postgres"))]
    const COUNT_QUERY: &str = "SELECT COUNT(*) FROM spans WHERE project_id = ?";

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    let stored: i64 = loop {
        let count: i64 = sqlx::query_scalar(COUNT_QUERY)
            .bind(project_id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        if count >= 4 || tokio::time::Instant::now() >= deadline {
            break count;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    };
    assert_eq!(
        stored, 4,
        "3 spans from the v2 batch plus the promoted transaction root span"
    );

    // Before the fix, this exact wire format hit `SpanProcessor` (wrong
    // format) and was rejected with "span missing span_id" — the regression
    // this test guards against. Now confirm the actual consumer of this
    // data — the same query the Agents API/dashboard uses — sees a
    // coherent trace.
    let (traces, total) = rustrak::services::span::SpanService::agent_traces(
        &db.pool,
        project_id,
        1,
        20,
        None,
        &Default::default(),
    )
    .await
    .unwrap();

    assert_eq!(total, 1, "all 3 spans share one trace_id -> one agent run");
    assert_eq!(traces.len(), 1);
    assert_eq!(
        traces[0].agent_names,
        vec!["research_agent".to_string()],
        "agent name must come from gen_ai.function_id, defaulted onto gen_ai.agent.name"
    );
    assert_eq!(
        traces[0].tool_call_count, 1,
        "exactly one of the 3 spans is a tool call"
    );
    assert_eq!(
        traces[0].total_tokens,
        (210.0 + 34.0) + (302.0 + 96.0),
        "total tokens summed across both generate_content spans (tool span has none)"
    );
}
