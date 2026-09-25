//! Integration tests for the AI Agent Monitoring dashboard API
//! (story-ai-agent-monitoring.md, GH #180).

use crate::common::TestDb;
use actix_web::{test, web, App};
use chrono::Utc;
use rustrak::config::{Config, DashboardConfig, DatabaseConfig, RateLimitConfig};
use rustrak::digest::processors::{Processor, ProcessorCtx, SpanProcessor};
use rustrak::models::CreateProject;
use rustrak::routes;
use rustrak::services::{AuthTokenService, ProjectService};
use serde_json::{json, Value};
use std::time::Duration as StdDuration;
use uuid::Uuid;

fn create_test_config() -> Config {
    Config {
        host: "127.0.0.1".to_string(),
        port: 0,
        database: DatabaseConfig {
            url: "postgres://test:test@localhost/test".to_string(),
            max_connections: 5,
            min_connections: 1,
            acquire_timeout: StdDuration::from_secs(5),
            idle_timeout: StdDuration::from_secs(60),
            max_lifetime: StdDuration::from_secs(300),
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
        ingest_dir: None,
        public_url: None,
        sourcemap_storage_path: "/tmp/test_sourcemaps".to_string(),
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

async fn create_test_token(pool: &rustrak::db::DbPool) -> String {
    AuthTokenService::create(
        pool,
        rustrak::models::CreateAuthToken {
            description: Some("Test token".to_string()),
        },
    )
    .await
    .expect("Failed to create test token")
    .token
}

async fn store_agent_run_with_llm_call(pool: &rustrak::db::DbPool, project_id: i32) {
    let ctx = ProcessorCtx {
        pool: pool.clone(),
        project_id,
        event_id: Uuid::nil(),
        ingested_at: Utc::now(),
        remote_addr: None,
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
    let config = create_test_config();
    let token = create_test_token(&pool).await;
    let project_id = create_test_project(&pool).await;
    store_agent_run_with_llm_call(&pool, project_id).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(routes::agents::configure),
    )
    .await;

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
    let config = create_test_config();
    let token = create_test_token(&pool).await;
    let project_id = create_test_project(&pool).await;
    store_agent_run_with_llm_call(&pool, project_id).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(routes::agents::configure),
    )
    .await;

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
    let config = create_test_config();
    let token = create_test_token(&pool).await;
    let project_id = create_test_project(&pool).await;
    store_agent_run_with_llm_call(&pool, project_id).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(routes::agents::configure),
    )
    .await;

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
    let config = create_test_config();
    let token = create_test_token(&pool).await;
    let project_id = create_test_project(&pool).await;
    store_agent_run_with_llm_call(&pool, project_id).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(routes::agents::configure),
    )
    .await;

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
    let config = create_test_config();
    let token = create_test_token(&pool).await;
    let project_id = create_test_project(&pool).await;
    store_agent_run_with_llm_call(&pool, project_id).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(routes::agents::configure),
    )
    .await;

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
    let config = create_test_config();
    let token = create_test_token(&pool).await;
    let project_id = create_test_project(&pool).await;
    store_agent_run_with_llm_call(&pool, project_id).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(routes::agents::configure),
    )
    .await;

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

#[actix_web::test]
async fn test_agents_endpoints_return_401_without_token() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let config = create_test_config();
    let project_id = create_test_project(&pool).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(routes::agents::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/agents/runs", project_id))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
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
    let config = create_test_config();
    let token = create_test_token(&pool).await;
    let project_id = create_test_project(&pool).await;

    store_agent_run_with_llm_call(&pool, project_id).await;
    store_failing_staging_trace(&pool, project_id).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(routes::agents::configure),
    )
    .await;

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
    let config = create_test_config();
    let token = create_test_token(&pool).await;
    let project_id = create_test_project(&pool).await;

    store_agent_run_with_llm_call(&pool, project_id).await;
    store_failing_staging_trace(&pool, project_id).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(routes::agents::configure),
    )
    .await;

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
    let config = create_test_config();
    let token = create_test_token(&pool).await;
    let project_id = create_test_project(&pool).await;

    store_agent_run_with_llm_call(&pool, project_id).await;
    store_failing_staging_trace(&pool, project_id).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(routes::agents::configure),
    )
    .await;

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
    let config = create_test_config();
    let token = create_test_token(&pool).await;
    let project_id = create_test_project(&pool).await;

    store_agent_run_with_llm_call(&pool, project_id).await;
    store_failing_staging_trace(&pool, project_id).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(routes::agents::configure),
    )
    .await;

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
    let config = create_test_config();
    let token = create_test_token(&pool).await;
    let project_id = create_test_project(&pool).await;

    store_agent_run_with_llm_call(&pool, project_id).await;
    store_failing_staging_trace(&pool, project_id).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(routes::agents::configure),
    )
    .await;

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
    let config = create_test_config();
    let token = create_test_token(&pool).await;
    let project_id = create_test_project(&pool).await;

    store_failing_staging_trace(&pool, project_id).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(routes::agents::configure),
    )
    .await;

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
