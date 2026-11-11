//! Full HTTP → dispatch → DB integration test for Sentry Spans Protocol v2
//! ingestion (application/vnd.sentry.items.span.v2+json).
//!
//! Uses a real wire fixture captured off the wire from an actual
//! @sentry/node 10.65 + Vercel AI SDK `generateText()` + tool call (via a
//! local echo server acting as the DSN target).

use crate::common::create_ingest_project;
use crate::common::TestDb;
use actix_web::test;
use rustrak::config::Config;
use serde_json::json;
use uuid::Uuid;

fn create_test_config() -> Config {
    Config {
        sourcemap_storage_path: "/tmp/test_sourcemaps_span_v2".to_string(),
        ..crate::common::config_with_ingest_dir(std::path::Path::new(
            "/tmp/rustrak_test_ingest_span_v2",
        ))
    }
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
    let (project_id, sentry_key) = create_ingest_project(&db.pool, "Span V2 Ingest Test").await;
    let config = create_test_config();

    let app = crate::common::app_with_config(&db.pool, config.clone()).await;

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

    let stored: i64 = crate::common::eventually(|| async {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM spans WHERE project_id = $1")
            .bind(project_id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        (count >= 4).then_some(count)
    })
    .await;
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
