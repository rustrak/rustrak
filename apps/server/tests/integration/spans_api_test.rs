//! Integration tests for the Spans API
//!
//! Tests GET /api/projects/{id}/spans and GET /api/projects/{id}/spans/{id}
//! with a real database.

use crate::common::TestDb;
use actix_web::{test, web, App};
use chrono::Utc;
use rustrak::config::{Config, DashboardConfig, DatabaseConfig, RateLimitConfig};
use rustrak::digest::processors::{Processor, ProcessorCtx, SpanProcessor, SpanV2Processor};
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

async fn store_sample_spans(pool: &rustrak::db::DbPool, project_id: i32) {
    for (span_id, trace_id, op, status) in [
        ("aaaaaaaaaaaaaaaa", "trace-a", "http.client", "ok"),
        ("bbbbbbbbbbbbbbbb", "trace-b", "db.query", "internal_error"),
    ] {
        let payload = serde_json::to_vec(&json!({
            "span_id": span_id,
            "trace_id": trace_id,
            "op": op,
            "status": status,
            "start_timestamp": 1.0,
            "timestamp": 2.0
        }))
        .unwrap();
        let ctx = ProcessorCtx {
            pool: pool.clone(),
            project_id,
            event_id: Uuid::nil(),
            ingested_at: Utc::now(),
            remote_addr: None,
        };
        SpanProcessor
            .process(bytes::Bytes::from(payload), &ctx)
            .await
            .unwrap();
    }
}

#[actix_web::test]
async fn test_list_spans_returns_stored_spans() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let config = create_test_config();
    let token = create_test_token(&pool).await;
    let project = ProjectService::create(
        &pool,
        CreateProject {
            name: "Spans List Test".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    store_sample_spans(&pool, project.id).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(routes::spans::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/spans", project.id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total_count"], 2);
    let items = body["items"].as_array().expect("items is array");
    assert_eq!(items.len(), 2);
}

#[actix_web::test]
async fn test_list_spans_filters_by_trace_id() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let config = create_test_config();
    let token = create_test_token(&pool).await;
    let project = ProjectService::create(
        &pool,
        CreateProject {
            name: "Spans Filter Test".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    store_sample_spans(&pool, project.id).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(routes::spans::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{}/spans?trace_id=trace-a",
            project.id
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total_count"], 1);
    let items = body["items"].as_array().expect("items is array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["trace_id"], "trace-a");
    assert!(
        items[0]["transaction_id"].is_null(),
        "standalone span must report no transaction_id"
    );
}

#[actix_web::test]
async fn test_list_spans_filters_by_op() {
    // Task 9 explicitly calls for an HTTP-level op filter test — the
    // service-level equivalent (SpanService::list_offset) doesn't exercise
    // the `ListSpansQuery` deserialization/query-param boundary.
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let config = create_test_config();
    let token = create_test_token(&pool).await;
    let project = ProjectService::create(
        &pool,
        CreateProject {
            name: "Spans Op Filter Test".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    store_sample_spans(&pool, project.id).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(routes::spans::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/spans?op=db.query", project.id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total_count"], 1);
    let items = body["items"].as_array().expect("items is array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["op"], "db.query");
}

#[actix_web::test]
async fn test_list_spans_filters_by_status() {
    // SpanFilters::status had zero test coverage anywhere before this —
    // closing that gap alongside the op-filter HTTP test.
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let config = create_test_config();
    let token = create_test_token(&pool).await;
    let project = ProjectService::create(
        &pool,
        CreateProject {
            name: "Spans Status Filter Test".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    store_sample_spans(&pool, project.id).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(routes::spans::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{}/spans?status=internal_error",
            project.id
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["total_count"], 1);
    let items = body["items"].as_array().expect("items is array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["status"], "internal_error");
}

#[actix_web::test]
async fn test_list_spans_returns_401_without_token() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let config = create_test_config();
    let project = ProjectService::create(
        &pool,
        CreateProject {
            name: "Spans Auth Test".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(routes::spans::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/spans", project.id))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

/// Stores one AI span through the Spans Protocol v2 producer — the wire format
/// real SDKs use for OTel-instrumented AI spans — and returns its row id.
///
/// The attribute bag carries the payload the agents UI needs to be useful
/// (prompt, response, model) and which today never leaves the server.
async fn store_v2_ai_span(pool: &rustrak::db::DbPool, project_id: i32) -> Uuid {
    fn attr(value: Value, ty: &str) -> Value {
        json!({ "value": value, "type": ty })
    }

    let container = json!({
        "version": 2,
        "items": [{
            "trace_id": "trace-v2-ai",
            "span_id": "cccccccccccccccc",
            "name": "chat claude-opus-5",
            "status": "ok",
            "is_segment": false,
            "start_timestamp": 1784231017.89,
            "end_timestamp": 1784231018.89,
            "attributes": {
                "sentry.op": attr(json!("gen_ai.chat"), "string"),
                "gen_ai.operation.name": attr(json!("chat"), "string"),
                "gen_ai.request.model": attr(json!("claude-opus-5"), "string"),
                "gen_ai.request.messages": attr(
                    json!(r#"[{"role":"user","content":"what is the weather"}]"#),
                    "string",
                ),
                "gen_ai.response.text": attr(json!("it is sunny"), "string"),
                "gen_ai.usage.input_tokens": attr(json!(10), "integer"),
                "gen_ai.usage.output_tokens": attr(json!(5), "integer"),
            }
        }]
    });

    let ctx = ProcessorCtx {
        pool: pool.clone(),
        project_id,
        event_id: Uuid::nil(),
        ingested_at: Utc::now(),
        remote_addr: None,
    };
    SpanV2Processor
        .process(
            bytes::Bytes::from(serde_json::to_vec(&container).unwrap()),
            &ctx,
        )
        .await
        .unwrap();

    sqlx::query_scalar::<_, Uuid>("SELECT id FROM spans WHERE span_id = $1")
        .bind("cccccccccccccccc")
        .fetch_one(pool)
        .await
        .expect("stored v2 span")
}

#[actix_web::test]
async fn test_get_span_exposes_gen_ai_attributes_of_a_v2_span() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let config = create_test_config();
    let token = create_test_token(&pool).await;
    let project = ProjectService::create(
        &pool,
        CreateProject {
            name: "Span Detail V2 Test".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    let span_id = store_v2_ai_span(&pool, project.id).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(routes::spans::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/spans/{}", project.id, span_id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["id"], span_id.to_string());
    assert_eq!(body["gen_ai_request_model"], "claude-opus-5");
    assert_eq!(
        body["attributes"]["gen_ai.request.messages"],
        r#"[{"role":"user","content":"what is the weather"}]"#,
        "the prompt is stored and must reach the UI, exactly as LogResponse exposes log attributes"
    );
    assert_eq!(body["attributes"]["gen_ai.response.text"], "it is sunny");
}

/// Stores one AI span through the legacy standalone producer, whose `data`
/// column holds the whole span object rather than the flat attribute bag.
async fn store_legacy_ai_span(pool: &rustrak::db::DbPool, project_id: i32) -> Uuid {
    let payload = serde_json::to_vec(&json!({
        "span_id": "dddddddddddddddd",
        "trace_id": "trace-legacy-ai",
        "op": "gen_ai.chat",
        "status": "ok",
        "start_timestamp": 1.0,
        "timestamp": 2.0,
        "tags": { "environment": "production" },
        "data": {
            "gen_ai.operation.name": "chat",
            "gen_ai.request.model": "claude-opus-5",
            "gen_ai.request.messages": r#"[{"role":"user","content":"what is the weather"}]"#,
            "gen_ai.response.text": "it is sunny"
        }
    }))
    .unwrap();

    let ctx = ProcessorCtx {
        pool: pool.clone(),
        project_id,
        event_id: Uuid::nil(),
        ingested_at: Utc::now(),
        remote_addr: None,
    };
    SpanProcessor
        .process(bytes::Bytes::from(payload), &ctx)
        .await
        .unwrap();

    sqlx::query_scalar::<_, Uuid>("SELECT id FROM spans WHERE span_id = $1")
        .bind("dddddddddddddddd")
        .fetch_one(pool)
        .await
        .expect("stored legacy span")
}

#[actix_web::test]
async fn test_get_span_exposes_gen_ai_attributes_of_a_legacy_span_in_the_same_shape() {
    // The two producers write different shapes into one column: v2 stores the
    // flat attribute bag, the legacy paths store the whole span object with
    // the attributes under its own `data` key. The response must not leak
    // that difference — a client cannot know which produced a given row.
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let config = create_test_config();
    let token = create_test_token(&pool).await;
    let project = ProjectService::create(
        &pool,
        CreateProject {
            name: "Span Detail Legacy Test".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    let span_id = store_legacy_ai_span(&pool, project.id).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(routes::spans::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/spans/{}", project.id, span_id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["attributes"]["gen_ai.request.messages"],
        r#"[{"role":"user","content":"what is the weather"}]"#,
        "attributes must be flat for a legacy-origin span too, not nested under a second `data` key"
    );
    assert!(
        body["attributes"]["data"].is_null(),
        "the raw span object must not leak through as an attribute"
    );
}

#[actix_web::test]
async fn test_get_span_from_another_project_is_not_found() {
    // Span ids are UUIDs, so this is not guessable — but the id travels in a
    // URL and this endpoint returns prompts and model responses, which is the
    // most sensitive payload in the system. The project scope is the guard.
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let config = create_test_config();
    let token = create_test_token(&pool).await;

    let owner = ProjectService::create(
        &pool,
        CreateProject {
            name: "Span Detail Owner".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();
    let other = ProjectService::create(
        &pool,
        CreateProject {
            name: "Span Detail Other".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    let span_id = store_v2_ai_span(&pool, owner.id).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(routes::spans::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/spans/{}", other.id, span_id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        404,
        "a span must not be readable through a project that does not own it"
    );
}

#[actix_web::test]
async fn test_get_span_exposes_tags_when_the_producer_stored_them() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let config = create_test_config();
    let token = create_test_token(&pool).await;
    let project = ProjectService::create(
        &pool,
        CreateProject {
            name: "Span Detail Tags Test".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    let span_id = store_legacy_ai_span(&pool, project.id).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(routes::spans::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/spans/{}", project.id, span_id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["tags"]["environment"], "production");
}

#[actix_web::test]
async fn test_get_span_returns_401_without_token() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let config = create_test_config();
    let project = ProjectService::create(
        &pool,
        CreateProject {
            name: "Span Detail Auth Test".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();

    let span_id = store_v2_ai_span(&pool, project.id).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .configure(routes::spans::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/spans/{}", project.id, span_id))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}
