//! Integration tests for the AI Agent Monitoring dashboard API
//!.

use crate::common::create_test_token;
use crate::common::TestDb;
use actix_web::test;
use chrono::Utc;
use rustrak::digest::processors::{ProcessorCtx, SpanProcessor};
use rustrak::models::CreateProject;
use rustrak::services::ProjectService;
use serde_json::{json, Value};
use uuid::Uuid;

async fn store_agent_run_with_llm_call(pool: &rustrak::db::DbPool, project_id: i32) {
    let ctx = ProcessorCtx {
        pool: pool.clone(),
        project_id,
        event_id: Uuid::nil(),
        ingested_at: Utc::now(),
        remote_addr: None,
        keep_ai_spans: true,
    };
    let agent = serde_json::to_vec(&json!({
        "span_id": "1111111111111111", "trace_id": "agent-trace",
        "start_timestamp": 1.0, "timestamp": 2.0,
        "data": {"gen_ai.operation.type": "agent", "gen_ai.agent.name": "planner"}
    }))
    .unwrap();
    SpanProcessor
        .process(bytes::Bytes::from(agent), &ctx)
        .await
        .unwrap();

    let llm = serde_json::to_vec(&json!({
        "span_id": "2222222222222222", "trace_id": "agent-trace",
        "parent_span_id": "1111111111111111",
        "start_timestamp": 1.2, "timestamp": 1.8,
        "data": {
            "gen_ai.operation.type": "ai_client",
            "gen_ai.request.model": "gpt-4o",
            "gen_ai.usage.input_tokens": 100,
            "gen_ai.usage.output_tokens": 50
        }
    }))
    .unwrap();
    SpanProcessor
        .process(bytes::Bytes::from(llm), &ctx)
        .await
        .unwrap();

    let tool = serde_json::to_vec(&json!({
        "span_id": "3333333333333333", "trace_id": "agent-trace",
        "parent_span_id": "1111111111111111",
        "start_timestamp": 1.85, "timestamp": 1.95,
        "data": {"gen_ai.operation.type": "tool", "gen_ai.tool.name": "search"}
    }))
    .unwrap();
    SpanProcessor
        .process(bytes::Bytes::from(tool), &ctx)
        .await
        .unwrap();
}

async fn create_test_project(pool: &rustrak::db::DbPool) -> i32 {
    ProjectService::create(
        pool,
        CreateProject {
            name: format!("Agents API Test {}", Uuid::new_v4()),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap()
    .id
}

#[actix_web::test]
async fn test_agent_runs_endpoint_returns_timeseries() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let config = crate::common::config();
    let token = create_test_token(&pool).await;
    let project_id = create_test_project(&pool).await;
    store_agent_run_with_llm_call(&pool, project_id).await;

    let app = crate::common::app_with_config(&pool, config.clone()).await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/agents/runs", project_id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    let points = body.as_array().expect("array response");
    let total: f64 = points.iter().map(|p| p["value"].as_f64().unwrap()).sum();
    assert_eq!(total, 1.0, "one agent-type span was stored");
}

#[actix_web::test]
async fn test_agent_duration_endpoint_returns_timeseries() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let config = crate::common::config();
    let token = create_test_token(&pool).await;
    let project_id = create_test_project(&pool).await;
    store_agent_run_with_llm_call(&pool, project_id).await;

    let app = crate::common::app_with_config(&pool, config.clone()).await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/agents/duration", project_id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    let points = body.as_array().expect("array response");
    assert!(!points.is_empty());
    assert!(points[0].get("avg_ms").is_some());
    assert!(points[0].get("p95_ms").is_some());
}

#[actix_web::test]
async fn test_agent_models_calls_endpoint_returns_breakdown() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let config = crate::common::config();
    let token = create_test_token(&pool).await;
    let project_id = create_test_project(&pool).await;
    store_agent_run_with_llm_call(&pool, project_id).await;

    let app = crate::common::app_with_config(&pool, config.clone()).await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/agents/models/calls", project_id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    let rows = body.as_array().expect("array response");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["label"], "gpt-4o");
    assert_eq!(rows[0]["value"], 1.0);
}

#[actix_web::test]
async fn test_agent_models_tokens_endpoint_returns_breakdown() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let config = crate::common::config();
    let token = create_test_token(&pool).await;
    let project_id = create_test_project(&pool).await;
    store_agent_run_with_llm_call(&pool, project_id).await;

    let app = crate::common::app_with_config(&pool, config.clone()).await;

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{}/agents/models/tokens",
            project_id
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    let rows = body.as_array().expect("array response");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["label"], "gpt-4o");
    assert_eq!(rows[0]["value"], 150.0);
}

#[actix_web::test]
async fn test_agent_tools_endpoint_returns_breakdown() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let config = crate::common::config();
    let token = create_test_token(&pool).await;
    let project_id = create_test_project(&pool).await;
    store_agent_run_with_llm_call(&pool, project_id).await;

    let app = crate::common::app_with_config(&pool, config.clone()).await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/agents/tools", project_id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    let rows = body.as_array().expect("array response");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["label"], "search");
    assert_eq!(rows[0]["value"], 1.0);
}

#[actix_web::test]
async fn test_agent_traces_endpoint_returns_paginated_traces() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let config = crate::common::config();
    let token = create_test_token(&pool).await;
    let project_id = create_test_project(&pool).await;
    store_agent_run_with_llm_call(&pool, project_id).await;

    let app = crate::common::app_with_config(&pool, config.clone()).await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/agents/traces", project_id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total_count"], 1);
    let items = body["items"].as_array().expect("items is array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["trace_id"], "agent-trace");
    assert_eq!(items[0]["agent_names"], serde_json::json!(["planner"]));
    assert_eq!(items[0]["tool_call_count"], 1);
}

/// A second trace that exercises what the dashboard has to be able to show:
/// a failing tool call, a second model, prompt-cache and reasoning tokens,
/// and an environment to filter on.
async fn store_failing_staging_trace(pool: &rustrak::db::DbPool, project_id: i32) {
    let ctx = ProcessorCtx {
        pool: pool.clone(),
        project_id,
        event_id: Uuid::nil(),
        ingested_at: Utc::now(),
        remote_addr: None,
        keep_ai_spans: true,
    };

    let agent = serde_json::to_vec(&json!({
        "span_id": "4444444444444444", "trace_id": "staging-trace",
        "start_timestamp": 1.0, "timestamp": 4.0, "environment": "staging",
        "data": {"gen_ai.operation.type": "agent", "gen_ai.agent.name": "billing"}
    }))
    .unwrap();
    SpanProcessor
        .process(bytes::Bytes::from(agent), &ctx)
        .await
        .unwrap();

    let llm = serde_json::to_vec(&json!({
        "span_id": "5555555555555555", "trace_id": "staging-trace",
        "parent_span_id": "4444444444444444",
        "start_timestamp": 1.2, "timestamp": 3.2, "environment": "staging",
        "data": {
            "gen_ai.operation.type": "ai_client",
            "gen_ai.request.model": "claude-opus-5",
            "gen_ai.usage.input_tokens": 1000,
            "gen_ai.usage.cache_read.input_tokens": 400,
            "gen_ai.usage.output_tokens": 300,
            "gen_ai.usage.reasoning.output_tokens": 120
        }
    }))
    .unwrap();
    SpanProcessor
        .process(bytes::Bytes::from(llm), &ctx)
        .await
        .unwrap();

    let tool = serde_json::to_vec(&json!({
        "span_id": "6666666666666666", "trace_id": "staging-trace",
        "parent_span_id": "4444444444444444",
        "start_timestamp": 3.3, "timestamp": 3.9, "environment": "staging",
        "status": "internal_error",
        "data": {"gen_ai.operation.type": "tool", "gen_ai.tool.name": "lookup_invoice"}
    }))
    .unwrap();
    SpanProcessor
        .process(bytes::Bytes::from(tool), &ctx)
        .await
        .unwrap();
}

#[actix_web::test]
async fn test_agent_traces_carry_llm_call_and_error_counts() {
    // Sentry's Traces table has both columns. Without them a reader cannot
    // tell a cheap one-shot trace from an expensive multi-call one, nor spot
    // the failing traces without opening each.
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let config = crate::common::config();
    let token = create_test_token(&pool).await;
    let project_id = create_test_project(&pool).await;

    store_agent_run_with_llm_call(&pool, project_id).await;
    store_failing_staging_trace(&pool, project_id).await;

    let app = crate::common::app_with_config(&pool, config.clone()).await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/agents/traces", project_id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    let items = body["items"].as_array().expect("items is array");

    let staging = items
        .iter()
        .find(|t| t["trace_id"] == "staging-trace")
        .expect("staging trace present");
    assert_eq!(staging["llm_call_count"], 1);
    assert_eq!(staging["error_count"], 1);

    let clean = items
        .iter()
        .find(|t| t["trace_id"] == "agent-trace")
        .expect("clean trace present");
    assert_eq!(clean["llm_call_count"], 1);
    assert_eq!(
        clean["error_count"], 0,
        "a trace with no failing span must report zero, not null"
    );
}

#[actix_web::test]
async fn test_agent_endpoints_filter_by_environment() {
    // Without this the dashboard mixes staging noise into production numbers,
    // which is exactly what makes an aggregate untrustworthy.
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let config = crate::common::config();
    let token = create_test_token(&pool).await;
    let project_id = create_test_project(&pool).await;

    store_agent_run_with_llm_call(&pool, project_id).await;
    store_failing_staging_trace(&pool, project_id).await;

    let app = crate::common::app_with_config(&pool, config.clone()).await;

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{}/agents/traces?environment=staging",
            project_id
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let body: Value = test::read_body_json(test::call_service(&app, req).await).await;
    let items = body["items"].as_array().expect("items is array");

    assert_eq!(items.len(), 1, "only the staging trace matches");
    assert_eq!(items[0]["trace_id"], "staging-trace");
}

#[actix_web::test]
async fn test_agent_summary_endpoint_returns_headline_numbers() {
    // The dashboard opened on six charts and no totals: a reader could see
    // shapes but not answer "how much did this cost me and how often did it
    // break". These are the numbers that answer that.
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let config = crate::common::config();
    let token = create_test_token(&pool).await;
    let project_id = create_test_project(&pool).await;

    store_agent_run_with_llm_call(&pool, project_id).await;
    store_failing_staging_trace(&pool, project_id).await;

    let app = crate::common::app_with_config(&pool, config.clone()).await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/agents/summary", project_id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["agent_runs"], 2);
    assert_eq!(body["llm_calls"], 2);
    assert_eq!(body["tool_calls"], 2);
    assert_eq!(body["error_count"], 1);
    // 150 from the clean trace + 1300 from the staging one. Agent spans are
    // excluded for the same reason the Traces table excludes them: their
    // usage is a rollup of their children.
    assert_eq!(body["total_tokens"], 1450.0);
}

#[actix_web::test]
async fn test_agent_models_table_breaks_tokens_down_by_type() {
    // "Tokens used" as one number hides the thing that actually drives cost:
    // how much of the input was served from cache, and how much of the output
    // was reasoning.
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let config = crate::common::config();
    let token = create_test_token(&pool).await;
    let project_id = create_test_project(&pool).await;

    store_agent_run_with_llm_call(&pool, project_id).await;
    store_failing_staging_trace(&pool, project_id).await;

    let app = crate::common::app_with_config(&pool, config.clone()).await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/agents/models", project_id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let rows: Value = test::read_body_json(resp).await;
    let rows = rows.as_array().expect("array of model rows");

    let opus = rows
        .iter()
        .find(|r| r["model"] == "claude-opus-5")
        .expect("claude-opus-5 row present");
    assert_eq!(opus["requests"], 1);
    assert_eq!(opus["input_tokens"], 1000.0);
    assert_eq!(opus["cached_input_tokens"], 400.0);
    assert_eq!(opus["output_tokens"], 300.0);
    assert_eq!(opus["reasoning_output_tokens"], 120.0);

    let gpt = rows
        .iter()
        .find(|r| r["model"] == "gpt-4o")
        .expect("gpt-4o row present");
    assert_eq!(
        gpt["cached_input_tokens"], 0.0,
        "a model reporting no caching sums to zero, not null"
    );
}

#[actix_web::test]
async fn test_agent_tools_table_counts_failures_per_tool() {
    // Sentry has a whole "Tool Errors" widget: a tool that fails half the
    // time is the single most actionable thing on the page.
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let config = crate::common::config();
    let token = create_test_token(&pool).await;
    let project_id = create_test_project(&pool).await;

    store_agent_run_with_llm_call(&pool, project_id).await;
    store_failing_staging_trace(&pool, project_id).await;

    let app = crate::common::app_with_config(&pool, config.clone()).await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/agents/tools/stats", project_id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let rows: Value = test::read_body_json(resp).await;
    let rows = rows.as_array().expect("array of tool rows");

    let failing = rows
        .iter()
        .find(|r| r["tool"] == "lookup_invoice")
        .expect("lookup_invoice row present");
    assert_eq!(failing["calls"], 1);
    assert_eq!(failing["errors"], 1);

    let healthy = rows
        .iter()
        .find(|r| r["tool"] == "search")
        .expect("search row present");
    assert_eq!(healthy["errors"], 0);
}

#[actix_web::test]
async fn test_agent_environments_endpoint_lists_what_can_be_filtered() {
    // The environment filter needs its own options: hardcoding
    // production/staging would be wrong for anyone who names them otherwise.
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let config = crate::common::config();
    let token = create_test_token(&pool).await;
    let project_id = create_test_project(&pool).await;

    store_failing_staging_trace(&pool, project_id).await;

    let app = crate::common::app_with_config(&pool, config.clone()).await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/agents/environments", project_id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    let envs = body.as_array().expect("array of environments");
    assert!(
        envs.iter().any(|e| e == "staging"),
        "an environment present in the data must be offered as a filter"
    );
}

/// The traces list narrows to the traces that ran an agent, called a model
/// or used a tool -- what a click on a row of the models or tools table
/// asks -- or that failed, and ranks them by any of its totals.
#[actix_web::test]
async fn test_agent_traces_filter_and_sort() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let token = create_test_token(&pool).await;
    let project_id = create_test_project(&pool).await;
    store_agent_run_with_llm_call(&pool, project_id).await;
    store_failing_staging_trace(&pool, project_id).await;

    let app = crate::common::app_with_config(&pool, crate::common::config()).await;
    let ids = |body: &Value| -> Vec<String> {
        body["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["trace_id"].as_str().unwrap().to_string())
            .collect()
    };

    for (query, expected) in [
        ("agent=billing", vec!["staging-trace"]),
        ("model=gpt-4o", vec!["agent-trace"]),
        ("tool=lookup_invoice", vec!["staging-trace"]),
        ("errors=true", vec!["staging-trace"]),
        ("tool=search&errors=true", vec![]),
        ("sort=tokens", vec!["staging-trace", "agent-trace"]),
        ("sort=duration", vec!["staging-trace", "agent-trace"]),
        ("sort=errors", vec!["staging-trace", "agent-trace"]),
    ] {
        let req = test::TestRequest::get()
            .uri(&format!("/api/projects/{project_id}/agents/traces?{query}"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200, "{query}");
        let body: Value = test::read_body_json(resp).await;
        assert_eq!(ids(&body), expected, "{query}");
        assert_eq!(body["total_count"], expected.len(), "{query}");
    }
}

/// Each bucket of agent runs says how many of them failed, so the chart can
/// stack the failures in red on top of the runs that went well.
#[actix_web::test]
async fn test_agent_runs_carry_failed_runs_per_bucket() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let token = create_test_token(&pool).await;
    let project_id = create_test_project(&pool).await;
    store_agent_run_with_llm_call(&pool, project_id).await;
    let ctx = ProcessorCtx {
        pool: pool.clone(),
        project_id,
        event_id: Uuid::nil(),
        ingested_at: Utc::now(),
        remote_addr: None,
        keep_ai_spans: true,
    };
    let failed = serde_json::to_vec(&json!({
        "span_id": "7777777777777777", "trace_id": "failed-trace",
        "start_timestamp": 1.5, "timestamp": 2.5, "status": "internal_error",
        "data": {"gen_ai.operation.type": "agent", "gen_ai.agent.name": "planner"}
    }))
    .unwrap();
    SpanProcessor
        .process(bytes::Bytes::from(failed), &ctx)
        .await
        .unwrap();

    let app = crate::common::app_with_config(&pool, crate::common::config()).await;
    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{project_id}/agents/runs"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, req).await;
    let points = body.as_array().unwrap();
    let runs: f64 = points.iter().map(|p| p["value"].as_f64().unwrap()).sum();
    let errors: i64 = points.iter().map(|p| p["errors"].as_i64().unwrap()).sum();
    assert_eq!(runs, 2.0);
    assert_eq!(errors, 1);
}

/// Two turns of one conversation: the agent span of each carries the
/// conversation id; the model and tool spans under it do not, as several
/// SDKs send it, and still belong to the conversation through their trace.
async fn store_conversation(pool: &rustrak::db::DbPool, project_id: i32) {
    let ctx = ProcessorCtx {
        pool: pool.clone(),
        project_id,
        event_id: Uuid::nil(),
        ingested_at: Utc::now(),
        remote_addr: None,
        keep_ai_spans: true,
    };
    let spans = [
        json!({"span_id": "c100000000000000", "trace_id": "turn-1", "start_timestamp": 10.0, "timestamp": 14.0,
               "data": {"gen_ai.operation.type": "agent", "gen_ai.agent.name": "support", "gen_ai.conversation.id": "conv-1"}}),
        json!({"span_id": "c100000000000001", "trace_id": "turn-1", "parent_span_id": "c100000000000000",
               "start_timestamp": 10.5, "timestamp": 11.5,
               "data": {"gen_ai.operation.type": "ai_client", "gen_ai.request.model": "gpt-4o", "gen_ai.usage.total_tokens": 100,
                        "gen_ai.request.messages": "[{\"role\":\"user\",\"content\":\"Where is my order?\"}]",
                        "gen_ai.response.tool_calls": "[{\"name\":\"get_order\"}]"}}),
        json!({"span_id": "c100000000000002", "trace_id": "turn-1", "parent_span_id": "c100000000000000",
               "start_timestamp": 12.0, "timestamp": 12.5, "status": "internal_error",
               "data": {"gen_ai.operation.type": "tool", "gen_ai.tool.name": "get_order",
                        "gen_ai.tool.input": "{\"id\":7}", "gen_ai.tool.output": "timeout"}}),
        json!({"span_id": "c100000000000003", "trace_id": "turn-1", "parent_span_id": "c100000000000000",
               "start_timestamp": 13.0, "timestamp": 14.0,
               "data": {"gen_ai.operation.type": "ai_client", "gen_ai.request.model": "gpt-4o", "gen_ai.usage.total_tokens": 50,
                        "gen_ai.response.text": "It ships tomorrow."}}),
        json!({"span_id": "c200000000000000", "trace_id": "turn-2", "start_timestamp": 60.0, "timestamp": 62.0,
               "data": {"gen_ai.operation.type": "agent", "gen_ai.agent.name": "billing", "gen_ai.conversation.id": "conv-1"}}),
        json!({"span_id": "c200000000000001", "trace_id": "turn-2", "parent_span_id": "c200000000000000",
               "start_timestamp": 60.5, "timestamp": 61.5,
               "data": {"gen_ai.operation.type": "ai_client", "gen_ai.request.model": "gpt-4o", "gen_ai.usage.total_tokens": 30,
                        "gen_ai.response.text": "Refund issued."}}),
    ];
    for span in spans {
        SpanProcessor
            .process(bytes::Bytes::from(serde_json::to_vec(&span).unwrap()), &ctx)
            .await
            .unwrap();
    }
}

/// Sentry's conversations: every trace sharing a `gen_ai.conversation.id`
/// is a turn of one conversation, listed with the totals of all its turns.
#[actix_web::test]
async fn test_agent_conversations_list_and_transcript() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let token = create_test_token(&pool).await;
    let project_id = create_test_project(&pool).await;
    store_agent_run_with_llm_call(&pool, project_id).await;
    store_conversation(&pool, project_id).await;

    let app = crate::common::app_with_config(&pool, crate::common::config()).await;
    let get = |uri: String| {
        test::TestRequest::get()
            .uri(&uri)
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request()
    };

    let body: Value = test::call_and_read_body_json(
        &app,
        get(format!("/api/projects/{project_id}/agents/conversations")),
    )
    .await;
    assert_eq!(
        body["total_count"], 1,
        "the trace without an id is no conversation"
    );
    let conversation = &body["items"][0];
    assert_eq!(conversation["conversation_id"], "conv-1");
    assert_eq!(conversation["turn_count"], 2);
    assert_eq!(conversation["agent_names"], json!(["support", "billing"]));
    assert_eq!(conversation["llm_call_count"], 3);
    assert_eq!(conversation["tool_call_count"], 1);
    assert_eq!(conversation["total_tokens"], 180.0);
    assert_eq!(conversation["error_count"], 1);

    let resp = test::call_service(
        &app,
        get(format!(
            "/api/projects/{project_id}/agents/conversations/conv-1"
        )),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let spans = body["spans"].as_array().unwrap();
    assert_eq!(spans.len(), 6, "every AI span of both turns, in order");
    assert_eq!(spans[0]["operation_type"], "agent");
    assert_eq!(
        spans[1]["input"],
        "[{\"role\":\"user\",\"content\":\"Where is my order?\"}]"
    );
    assert_eq!(spans[1]["tool_calls"], "[{\"name\":\"get_order\"}]");
    assert_eq!(spans[2]["tool_name"], "get_order");
    assert_eq!(spans[2]["status"], "internal_error");
    assert_eq!(spans[2]["output"], "timeout");
    assert_eq!(spans[3]["output"], "It ships tomorrow.");
    assert_eq!(spans[4]["trace_id"], "turn-2");

    let resp = test::call_service(
        &app,
        get(format!(
            "/api/projects/{project_id}/agents/conversations/missing"
        )),
    )
    .await;
    assert_eq!(resp.status(), 404);
}

/// The list holds up against what SDKs really send: a turn per environment,
/// a conversation whose only tagged span is no AI span, and a span with no
/// start time.
#[actix_web::test]
async fn test_agent_conversations_agree_with_their_filters_and_count() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let token = create_test_token(&pool).await;
    let project_id = create_test_project(&pool).await;
    let ctx = ProcessorCtx {
        pool: pool.clone(),
        project_id,
        event_id: Uuid::nil(),
        ingested_at: Utc::now(),
        remote_addr: None,
        keep_ai_spans: true,
    };
    let spans = [
        // One conversation, a turn in each environment, by different agents.
        json!({"span_id": "d100000000000000", "trace_id": "prod-turn", "start_timestamp": 10.0, "timestamp": 11.0,
               "environment": "production",
               "data": {"gen_ai.operation.type": "agent", "gen_ai.agent.name": "billing", "gen_ai.conversation.id": "conv-2"}}),
        json!({"span_id": "d200000000000000", "trace_id": "staging-turn", "start_timestamp": 20.0, "timestamp": 21.0,
               "environment": "staging",
               "data": {"gen_ai.operation.type": "agent", "gen_ai.agent.name": "support", "gen_ai.conversation.id": "conv-2"}}),
        // Tagged, but no AI span anywhere in its trace: not a listed row.
        json!({"span_id": "d300000000000000", "trace_id": "plain-trace", "op": "http.client",
               "start_timestamp": 30.0, "timestamp": 31.0,
               "data": {"gen_ai.conversation.id": "conv-3"}}),
    ];
    for span in spans {
        SpanProcessor
            .process(bytes::Bytes::from(serde_json::to_vec(&span).unwrap()), &ctx)
            .await
            .unwrap();
    }
    // An agent run that never sent its start, as a transaction's child span
    // can be stored.
    sqlx::query(
        "INSERT INTO spans (id, project_id, span_id, trace_id, op, timestamp, data, \
         gen_ai_operation_type, gen_ai_agent_name, gen_ai_conversation_id) \
         VALUES ($1, $2, 'd400000000000000', 'no-start', 'gen_ai.invoke_agent', $3, $4, \
         'agent', 'triage', 'conv-4')",
    )
    .bind(Uuid::new_v4())
    .bind(project_id)
    .bind(Utc::now())
    .bind(json!({}))
    .execute(&pool)
    .await
    .unwrap();

    let app = crate::common::app_with_config(&pool, crate::common::config()).await;
    let get = |uri: String| {
        test::TestRequest::get()
            .uri(&uri)
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request()
    };

    let resp = test::call_service(
        &app,
        get(format!("/api/projects/{project_id}/agents/conversations")),
    )
    .await;
    assert_eq!(
        resp.status(),
        200,
        "a span with no start time breaks nothing"
    );
    let body: Value = test::read_body_json(resp).await;
    let items = body["items"].as_array().unwrap();
    assert_eq!(
        body["total_count"].as_u64().unwrap() as usize,
        items.len(),
        "the count is of the rows the page can list"
    );
    let no_start = items
        .iter()
        .find(|c| c["conversation_id"] == "conv-4")
        .unwrap();
    assert!(no_start["last_seen"].is_string());

    let body: Value = test::call_and_read_body_json(
        &app,
        get(format!(
            "/api/projects/{project_id}/agents/conversations?environment=staging"
        )),
    )
    .await;
    assert_eq!(body["items"][0]["conversation_id"], "conv-2");
    assert_eq!(
        body["items"][0]["agent_names"],
        json!(["support"]),
        "only the agents of the turns the filter keeps"
    );
}
