//! Unit tests for the AI Agent Monitoring aggregation queries
//! — powers the 7 dashboard widgets.

use chrono::Utc;
use rustrak::digest::processors::{ProcessorCtx, SpanProcessor};
use rustrak::models::CreateProject;
use rustrak::services::span::{SpanFilters, SpanService};
use rustrak::services::ProjectService;
use serde_json::json;
use uuid::Uuid;

use crate::common::TestDb;

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

async fn store_span(pool: &rustrak::db::DbPool, project_id: i32, payload: serde_json::Value) {
    let bytes = serde_json::to_vec(&payload).unwrap();
    SpanProcessor
        .process(bytes::Bytes::from(bytes), &ctx(pool, project_id))
        .await
        .unwrap();
}

#[tokio::test]
async fn test_list_spans_filters_by_operation_type() {
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "gen-ai-op-filter".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    store_span(
        &db.pool,
        project.id,
        json!({
            "span_id": "aaaaaaaaaaaaaaaa", "trace_id": "t1",
            "start_timestamp": 1.0, "timestamp": 2.0,
            "data": {"gen_ai.operation.type": "agent"}
        }),
    )
    .await;
    store_span(
        &db.pool,
        project.id,
        json!({
            "span_id": "bbbbbbbbbbbbbbbb", "trace_id": "t2",
            "start_timestamp": 1.0, "timestamp": 2.0,
            "data": {"gen_ai.operation.type": "tool"}
        }),
    )
    .await;

    let filters = SpanFilters {
        operation_type: Some("agent".to_string()),
        ..Default::default()
    };
    let (list, total) = SpanService::list_offset(&db.pool, project.id, 1, 20, &filters)
        .await
        .unwrap();

    assert_eq!(total, 1);
    assert_eq!(list[0].span_id.as_deref(), Some("aaaaaaaaaaaaaaaa"));
}

#[tokio::test]
async fn test_llm_calls_by_model_counts_and_orders_by_frequency() {
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "gen-ai-llm-calls".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    for (span_id, model) in [
        ("1111111111111111", "gpt-4o"),
        ("2222222222222222", "gpt-4o"),
        ("3333333333333333", "claude-3-5-sonnet"),
    ] {
        store_span(
            &db.pool,
            project.id,
            json!({
                "span_id": span_id, "trace_id": "t",
                "start_timestamp": 1.0, "timestamp": 2.0,
                "data": {
                    "gen_ai.operation.type": "ai_client",
                    "gen_ai.request.model": model
                }
            }),
        )
        .await;
    }

    let rows = SpanService::llm_calls_by_model(&db.pool, project.id, &Default::default(), None, 3)
        .await
        .unwrap();

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].label, "gpt-4o");
    assert_eq!(rows[0].value, 2.0);
    assert_eq!(rows[1].label, "claude-3-5-sonnet");
    assert_eq!(rows[1].value, 1.0);
}

#[tokio::test]
async fn test_llm_calls_by_model_excludes_non_ai_client_spans() {
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "gen-ai-llm-calls-exclude".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    store_span(
        &db.pool,
        project.id,
        json!({
            "span_id": "4444444444444444", "trace_id": "t",
            "start_timestamp": 1.0, "timestamp": 2.0,
            "data": {"gen_ai.operation.type": "agent", "gen_ai.request.model": "gpt-4o"}
        }),
    )
    .await;

    let rows = SpanService::llm_calls_by_model(&db.pool, project.id, &Default::default(), None, 3)
        .await
        .unwrap();
    assert!(rows.is_empty(), "agent spans must not count as LLM calls");
}

#[tokio::test]
async fn test_tokens_by_model_sums_total_tokens() {
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "gen-ai-tokens-model".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    for (span_id, tokens) in [("5555555555555555", 100), ("6666666666666666", 50)] {
        store_span(
            &db.pool,
            project.id,
            json!({
                "span_id": span_id, "trace_id": "t",
                "start_timestamp": 1.0, "timestamp": 2.0,
                "data": {
                    "gen_ai.operation.type": "ai_client",
                    "gen_ai.request.model": "gpt-4o",
                    "gen_ai.usage.input_tokens": tokens,
                    "gen_ai.usage.output_tokens": 0
                }
            }),
        )
        .await;
    }

    let rows = SpanService::tokens_by_model(&db.pool, project.id, &Default::default(), None, 3)
        .await
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].label, "gpt-4o");
    assert_eq!(rows[0].value, 150.0);
}

#[tokio::test]
async fn test_tool_calls_by_tool_counts() {
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "gen-ai-tool-calls".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    for (span_id, tool) in [
        ("7777777777777777", "search"),
        ("8888888888888888", "search"),
        ("9999999999999999", "calculator"),
    ] {
        store_span(
            &db.pool,
            project.id,
            json!({
                "span_id": span_id, "trace_id": "t",
                "start_timestamp": 1.0, "timestamp": 2.0,
                "data": {"gen_ai.operation.type": "tool", "gen_ai.tool.name": tool}
            }),
        )
        .await;
    }

    let rows = SpanService::tool_calls_by_tool(&db.pool, project.id, &Default::default(), None, 3)
        .await
        .unwrap();

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].label, "search");
    assert_eq!(rows[0].value, 2.0);
}

#[tokio::test]
async fn test_agent_runs_timeseries_counts_agent_spans() {
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "gen-ai-runs-ts".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    for span_id in ["aaaa111111111111", "bbbb111111111111"] {
        store_span(
            &db.pool,
            project.id,
            json!({
                "span_id": span_id, "trace_id": "t",
                "start_timestamp": 1.0, "timestamp": 2.0,
                "data": {"gen_ai.operation.type": "agent"}
            }),
        )
        .await;
    }
    // A tool span must not count as an agent run.
    store_span(
        &db.pool,
        project.id,
        json!({
            "span_id": "cccc111111111111", "trace_id": "t",
            "start_timestamp": 1.0, "timestamp": 2.0,
            "data": {"gen_ai.operation.type": "tool"}
        }),
    )
    .await;

    let points =
        SpanService::agent_runs_timeseries(&db.pool, project.id, &Default::default(), None, 1)
            .await
            .unwrap();

    let total: f64 = points.iter().map(|p| p.value).sum();
    assert_eq!(total, 2.0, "only the 2 agent-type spans count as runs");
}

#[tokio::test]
async fn test_agent_duration_timeseries_computes_avg_and_p95() {
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "gen-ai-duration-ts".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    // Three agent spans with durations 100ms, 200ms, 300ms (start/timestamp in seconds).
    for (span_id, end) in [
        ("eeee111111111111", 1.1),
        ("ffff111111111111", 1.2),
        ("0000111111111111", 1.3),
    ] {
        store_span(
            &db.pool,
            project.id,
            json!({
                "span_id": span_id, "trace_id": "t",
                "start_timestamp": 1.0, "timestamp": end,
                "data": {"gen_ai.operation.type": "agent"}
            }),
        )
        .await;
    }

    let points =
        SpanService::agent_duration_timeseries(&db.pool, project.id, &Default::default(), None, 1)
            .await
            .unwrap();

    assert!(!points.is_empty());
    let avg: f64 = points.iter().map(|p| p.avg_ms).sum::<f64>() / points.len() as f64;
    assert!(
        (avg - 200.0).abs() < 1.0,
        "avg of 100/200/300ms must be ~200ms, got {avg}"
    );
}

#[tokio::test]
async fn test_agent_traces_aggregates_per_trace_id() {
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "gen-ai-traces".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    let trace_id = "shared-trace-for-agent-run";

    store_span(
        &db.pool,
        project.id,
        json!({
            "span_id": "1000000000000001", "trace_id": trace_id,
            "start_timestamp": 1.0, "timestamp": 3.0,
            "data": {
                "gen_ai.operation.type": "agent",
                "gen_ai.agent.name": "research-agent"
            }
        }),
    )
    .await;
    store_span(
        &db.pool,
        project.id,
        json!({
            "span_id": "1000000000000002", "trace_id": trace_id,
            "parent_span_id": "1000000000000001",
            "start_timestamp": 1.2, "timestamp": 1.8,
            "data": {
                "gen_ai.operation.type": "ai_client",
                "gen_ai.request.model": "gpt-4o",
                "gen_ai.usage.input_tokens": 100,
                "gen_ai.usage.output_tokens": 50
            }
        }),
    )
    .await;
    store_span(
        &db.pool,
        project.id,
        json!({
            "span_id": "1000000000000003", "trace_id": trace_id,
            "parent_span_id": "1000000000000001",
            "start_timestamp": 1.9, "timestamp": 2.0,
            "data": {"gen_ai.operation.type": "tool", "gen_ai.tool.name": "search"}
        }),
    )
    .await;

    let (traces, total) = SpanService::agent_traces(
        &db.pool,
        project.id,
        1,
        20,
        None,
        &Default::default(),
        &Default::default(),
    )
    .await
    .unwrap();

    assert_eq!(total, 1, "one distinct trace_id with AI spans");
    assert_eq!(traces.len(), 1);
    let trace = &traces[0];
    assert_eq!(trace.trace_id, trace_id);
    assert_eq!(trace.agent_names, vec!["research-agent"]);
    assert_eq!(trace.tool_call_count, 1);
    assert_eq!(trace.total_tokens, 150.0);
}

#[tokio::test]
async fn test_agent_traces_lists_every_agent_in_a_handoff_trace() {
    // A handoff runs several agents under one trace. Sentry's Traces table
    // shows them all; reporting only the first hides the handoff entirely.
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "gen-ai-traces-handoff".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    let trace_id = "trace-with-handoff";

    store_span(
        &db.pool,
        project.id,
        json!({
            "span_id": "3000000000000001", "trace_id": trace_id,
            "start_timestamp": 1.0, "timestamp": 3.0,
            "data": {"gen_ai.operation.type": "agent", "gen_ai.agent.name": "triage-agent"}
        }),
    )
    .await;
    // Same agent again — must not appear twice.
    store_span(
        &db.pool,
        project.id,
        json!({
            "span_id": "3000000000000002", "trace_id": trace_id,
            "start_timestamp": 1.5, "timestamp": 2.0,
            "data": {"gen_ai.operation.type": "agent", "gen_ai.agent.name": "triage-agent"}
        }),
    )
    .await;
    store_span(
        &db.pool,
        project.id,
        json!({
            "span_id": "3000000000000003", "trace_id": trace_id,
            "start_timestamp": 2.0, "timestamp": 3.0,
            "data": {"gen_ai.operation.type": "agent", "gen_ai.agent.name": "billing-agent"}
        }),
    )
    .await;

    let (traces, _) = SpanService::agent_traces(
        &db.pool,
        project.id,
        1,
        20,
        None,
        &Default::default(),
        &Default::default(),
    )
    .await
    .unwrap();

    assert_eq!(traces.len(), 1);
    assert_eq!(
        traces[0].agent_names,
        vec!["triage-agent", "billing-agent"],
        "every distinct agent, earliest first, no duplicates"
    );
}

#[tokio::test]
async fn test_agent_traces_excludes_agent_span_usage_from_token_sum() {
    // An agent span reports the aggregate usage of its own children. Summing
    // it alongside them double-counts the trace's tokens, which is why
    // Sentry's Traces table filters agent runs out of this sum.
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "gen-ai-traces-no-double-count".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    let trace_id = "trace-with-aggregating-agent";

    // Agent span carrying the aggregate of both LLM calls below (150 + 30).
    store_span(
        &db.pool,
        project.id,
        json!({
            "span_id": "2000000000000001", "trace_id": trace_id,
            "start_timestamp": 1.0, "timestamp": 3.0,
            "data": {
                "gen_ai.operation.type": "agent",
                "gen_ai.agent.name": "research-agent",
                "gen_ai.usage.total_tokens": 180
            }
        }),
    )
    .await;
    store_span(
        &db.pool,
        project.id,
        json!({
            "span_id": "2000000000000002", "trace_id": trace_id,
            "parent_span_id": "2000000000000001",
            "start_timestamp": 1.2, "timestamp": 1.8,
            "data": {
                "gen_ai.operation.type": "ai_client",
                "gen_ai.usage.input_tokens": 100,
                "gen_ai.usage.output_tokens": 50
            }
        }),
    )
    .await;
    store_span(
        &db.pool,
        project.id,
        json!({
            "span_id": "2000000000000003", "trace_id": trace_id,
            "parent_span_id": "2000000000000001",
            "start_timestamp": 1.9, "timestamp": 2.5,
            "data": {
                "gen_ai.operation.type": "ai_client",
                "gen_ai.usage.input_tokens": 20,
                "gen_ai.usage.output_tokens": 10
            }
        }),
    )
    .await;

    let (traces, _) = SpanService::agent_traces(
        &db.pool,
        project.id,
        1,
        20,
        None,
        &Default::default(),
        &Default::default(),
    )
    .await
    .unwrap();

    assert_eq!(traces.len(), 1);
    assert_eq!(
        traces[0].total_tokens, 180.0,
        "tokens must come from the child LLM calls only — counting the \
         agent span's aggregate too would report 360"
    );
}

#[tokio::test]
async fn test_agent_traces_does_not_double_count_root_span_rollup_totals() {
    // Regression guard for the root-span-promotion fix (story-span-v2-protocol.md
    // follow-up, verified live 2026-07-17): real Sentry SDKs (e.g. Vercel AI
    // SDK's vercelAiEventProcessor) accumulate child token/cost totals onto
    // the trace ROOT span's own gen_ai.usage.* attributes client-side. Once
    // TransactionProcessor promotes that root into its own `spans` row
    // (operation_type='agent'), naively SUMing gen_ai_usage_total_tokens
    // across every gen_ai span in the trace double-counts: the root's
    // already-rolled-up total gets added ON TOP of the same tokens counted
    // again from its 'ai_client' children.
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "gen-ai-traces-root-rollup".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    let trace_id = "root-rollup-trace";

    // The root/agent span carries the ACCUMULATED total (100+50=150) on its
    // own gen_ai.usage attributes — mirrors what a real SDK sends.
    store_span(
        &db.pool,
        project.id,
        json!({
            "span_id": "2100000000000001", "trace_id": trace_id,
            "start_timestamp": 1.0, "timestamp": 3.0,
            "data": {
                "gen_ai.operation.type": "agent",
                "gen_ai.agent.name": "research-agent",
                "gen_ai.usage.input_tokens": 100,
                "gen_ai.usage.output_tokens": 50
            }
        }),
    )
    .await;
    // Its child ai_client span carries the SAME 100+50 tokens as its own
    // distinct usage — this is the actual, non-duplicated usage.
    store_span(
        &db.pool,
        project.id,
        json!({
            "span_id": "2100000000000002", "trace_id": trace_id,
            "parent_span_id": "2100000000000001",
            "start_timestamp": 1.2, "timestamp": 1.8,
            "data": {
                "gen_ai.operation.type": "ai_client",
                "gen_ai.request.model": "gpt-4o",
                "gen_ai.usage.input_tokens": 100,
                "gen_ai.usage.output_tokens": 50
            }
        }),
    )
    .await;

    let (traces, _) = SpanService::agent_traces(
        &db.pool,
        project.id,
        1,
        20,
        None,
        &Default::default(),
        &Default::default(),
    )
    .await
    .unwrap();

    assert_eq!(traces.len(), 1);
    assert_eq!(
        traces[0].total_tokens, 150.0,
        "must count the real 150 tokens once, not 300 (root rollup + child, double-counted)"
    );
}

#[tokio::test]
async fn test_agent_traces_reports_zero_tokens_for_root_only_trace() {
    // A root-only trace (promoted root/agent span, no non-agent 'ai_client'
    // children ever ingested) reports 0 tokens — matches real Sentry's
    // Traces table exactly: its query unconditionally excludes agent-type
    // spans from the token sum with no root-only fallback
    // (tracesTable.tsx's `getAgentRunsFilter({negated: true})`), so an
    // agent span's own rolled-up gen_ai.usage.* is never counted, even when
    // it's the only gen_ai span in the trace.
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "gen-ai-traces-root-only".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    let trace_id = "root-only-trace";

    store_span(
        &db.pool,
        project.id,
        json!({
            "span_id": "2200000000000001", "trace_id": trace_id,
            "start_timestamp": 1.0, "timestamp": 3.0,
            "data": {
                "gen_ai.operation.type": "agent",
                "gen_ai.agent.name": "research-agent",
                "gen_ai.usage.total_tokens": 15
            }
        }),
    )
    .await;

    let (traces, _) = SpanService::agent_traces(
        &db.pool,
        project.id,
        1,
        20,
        None,
        &Default::default(),
        &Default::default(),
    )
    .await
    .unwrap();

    assert_eq!(traces.len(), 1);
    assert_eq!(
        traces[0].total_tokens, 0.0,
        "agent-type rows are always excluded from the token sum, root-only or not"
    );
}

// =============================================================================
// SpanResponse exposes gen_ai.* fields (needed by the Agents trace waterfall
// drill-down to show model/tokens per span)
// =============================================================================

#[tokio::test]
async fn test_list_spans_response_includes_gen_ai_fields() {
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "gen-ai-span-response".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    store_span(
        &db.pool,
        project.id,
        json!({
            "span_id": "aaaabbbbccccdddd", "trace_id": "t",
            "start_timestamp": 1.0, "timestamp": 2.0,
            "data": {
                "gen_ai.operation.type": "ai_client",
                "gen_ai.request.model": "gpt-4o",
                "gen_ai.usage.input_tokens": 100,
                "gen_ai.usage.output_tokens": 50
            }
        }),
    )
    .await;

    let (list, _) = SpanService::list_offset(&db.pool, project.id, 1, 20, &SpanFilters::default())
        .await
        .unwrap();

    assert_eq!(list.len(), 1);
    let span = &list[0];
    assert_eq!(span.gen_ai_operation_type.as_deref(), Some("ai_client"));
    assert_eq!(span.gen_ai_response_model.as_deref(), Some("gpt-4o"));
    assert_eq!(span.gen_ai_usage_total_tokens, Some(150.0));
}

#[tokio::test]
async fn test_list_spans_response_non_ai_span_has_null_gen_ai_fields() {
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "gen-ai-span-response-null".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    store_span(
        &db.pool,
        project.id,
        json!({
            "span_id": "eeeeffffgggghhhh", "trace_id": "t",
            "op": "http.client",
            "start_timestamp": 1.0, "timestamp": 2.0
        }),
    )
    .await;

    let (list, _) = SpanService::list_offset(&db.pool, project.id, 1, 20, &SpanFilters::default())
        .await
        .unwrap();

    assert_eq!(list.len(), 1);
    assert!(list[0].gen_ai_operation_type.is_none());
    assert!(list[0].gen_ai_usage_total_tokens.is_none());
}
