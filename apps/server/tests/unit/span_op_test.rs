//! A span that arrives without an `op` (#559). Relay never stores one without:
//! a transaction's spans get `"default"`
//! (relay-event-normalization/src/transactions/processor.rs), and a span item
//! gets one inferred from its attributes (relay-spans/src/op.rs).

use crate::common::TestDb;
use chrono::Utc;
use rustrak::digest::processors::{ProcessorCtx, TransactionProcessor};
use rustrak::models::CreateProject;
use rustrak::services::ProjectService;
use serde_json::{json, Value};
use uuid::Uuid;

async fn project(db: &TestDb) -> ProcessorCtx {
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "span-op".to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();
    ProcessorCtx {
        pool: db.pool.clone(),
        project_id: project.id,
        event_id: Uuid::new_v4(),
        ingested_at: Utc::now(),
        remote_addr: None,
        keep_ai_spans: true,
    }
}

/// The stored `op` of each span of the project, by `span_id`.
async fn ops(ctx: &ProcessorCtx) -> std::collections::BTreeMap<String, Option<String>> {
    sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT span_id, op FROM spans WHERE project_id = $1",
    )
    .bind(ctx.project_id)
    .fetch_all(&ctx.pool)
    .await
    .unwrap()
    .into_iter()
    .collect()
}

fn transaction(trace: Value, spans: Value) -> bytes::Bytes {
    let payload = json!({
        "type": "transaction",
        "transaction": "/items/[id]",
        "start_timestamp": 946684800.0,
        "timestamp": 946684810.0,
        "contexts": { "trace": trace },
        "spans": spans,
    });
    bytes::Bytes::from(serde_json::to_vec(&payload).unwrap())
}

fn child(span_id: &str, op: Option<Value>) -> Value {
    let mut span = json!({
        "span_id": span_id,
        "parent_span_id": "fa90fdead5f74053",
        "trace_id": "4c79f60c11214eb38604f4ae0781bfb2",
        "description": "build component tree",
        "start_timestamp": 946684800.0,
        "timestamp": 946684810.0,
        // What `@sentry/nextjs` sends in place of an op.
        "data": { "next.span_type": "NextNodeServer.getLayoutOrPageModule" },
    });
    if let Some(op) = op {
        span["op"] = op;
    }
    span
}

fn trace_with_op() -> Value {
    json!({
        "trace_id": "4c79f60c11214eb38604f4ae0781bfb2",
        "span_id": "fa90fdead5f74053",
        "op": "http.server",
    })
}

#[tokio::test]
async fn a_transaction_span_without_an_op_is_stored_as_default() {
    // Relay's `test_defaults_transaction_event_with_span_with_missing_op`,
    // plus the two other shapes its `span.op.value().is_none()` covers: a
    // null and a value that is not a string. An empty string is a value.
    let db = TestDb::new().await;
    let ctx = project(&db).await;

    let spans = json!([
        child("aaaaaaaaaaaaaaa1", None),
        child("aaaaaaaaaaaaaaa2", Some(Value::Null)),
        child("aaaaaaaaaaaaaaa3", Some(json!(7))),
        child("aaaaaaaaaaaaaaa4", Some(json!(""))),
        child("aaaaaaaaaaaaaaa5", Some(json!("function"))),
    ]);
    TransactionProcessor
        .process(transaction(trace_with_op(), spans), &ctx)
        .await
        .unwrap();

    let ops = ops(&ctx).await;
    let op = |id: &str| ops[id].as_deref();
    assert_eq!(op("aaaaaaaaaaaaaaa1"), Some("default"), "absent");
    assert_eq!(op("aaaaaaaaaaaaaaa2"), Some("default"), "null");
    assert_eq!(op("aaaaaaaaaaaaaaa3"), Some("default"), "not a string");
    assert_eq!(op("aaaaaaaaaaaaaaa4"), Some(""), "empty is kept");
    assert_eq!(op("aaaaaaaaaaaaaaa5"), Some("function"), "sent is kept");
}

async fn transaction_op(ctx: &ProcessorCtx) -> Option<String> {
    sqlx::query_scalar("SELECT op FROM transactions WHERE project_id = $1")
        .bind(ctx.project_id)
        .fetch_one(&ctx.pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_transaction_without_an_op_is_stored_as_default() {
    // Relay's `test_defaults_missing_op_in_context`.
    let db = TestDb::new().await;
    let ctx = project(&db).await;
    let trace = json!({
        "trace_id": "4c79f60c11214eb38604f4ae0781bfb2",
        "span_id": "fa90fdead5f74053",
    });
    TransactionProcessor
        .process(transaction(trace, json!([])), &ctx)
        .await
        .unwrap();
    assert_eq!(transaction_op(&ctx).await.as_deref(), Some("default"));
}

#[tokio::test]
async fn a_transaction_keeps_the_op_it_sent_even_when_empty() {
    for sent in ["http.server", ""] {
        let db = TestDb::new().await;
        let ctx = project(&db).await;
        let mut trace = trace_with_op();
        trace["op"] = json!(sent);
        TransactionProcessor
            .process(transaction(trace, json!([])), &ctx)
            .await
            .unwrap();
        assert_eq!(transaction_op(&ctx).await.as_deref(), Some(sent));
    }
}

#[tokio::test]
async fn a_transaction_without_a_trace_context_gets_no_op() {
    // Relay defaults the op of a trace context, and does not make one.
    let db = TestDb::new().await;
    let ctx = project(&db).await;
    let payload = json!({
        "type": "transaction",
        "transaction": "/items/[id]",
        "start_timestamp": 946684800.0,
        "timestamp": 946684810.0,
    });
    TransactionProcessor
        .process(serde_json::to_vec(&payload).unwrap().into(), &ctx)
        .await
        .unwrap();
    assert_eq!(transaction_op(&ctx).await, None);
}

/// What Relay infers for a span item with no op, by the fields that decide
/// it: `derive_op_for_v2_span` (relay-spans/src/op.rs), after the renames of
/// `normalize_attribute_names` it runs behind. The first cases are Relay's own
/// tests.
fn inferred() -> Vec<(Value, &'static str)> {
    vec![
        (json!({}), "default"),
        (json!({ "op": "" }), "default"),
        (json!({ "op": null }), "default"),
        (
            json!({ "data": { "gen_ai.operation.name": "chat", "gen_ai.provider.name": "openai" } }),
            "gen_ai.chat",
        ),
        (
            json!({ "data": { "gen_ai.operation.name": "execute_tool" } }),
            "gen_ai.execute_tool",
        ),
        (
            json!({ "data": { "gen_ai.provider.name": "openai" } }),
            "default",
        ),
        (
            json!({ "data": { "gen_ai.provider.name": "openai", "rpc.service": "example.Service" } }),
            "rpc",
        ),
        (
            json!({ "data": { "db.system.name": "mysql", "db.operation.name": "query" } }),
            "db",
        ),
        (
            json!({ "kind": "client", "data": { "http.request.method": "GET" } }),
            "http.client",
        ),
        (
            json!({ "kind": "server", "data": { "http.request.method": "GET" } }),
            "http.server",
        ),
        (json!({ "data": { "http.request.method": "GET" } }), "http"),
        (
            json!({ "kind": "internal", "data": { "http.request.method": "GET" } }),
            "http",
        ),
        (
            json!({ "data": { "http.request.method": "GET", "sentry.http.prefetch": true } }),
            "http.prefetch",
        ),
        // A kind decides before a prefetch does.
        (
            json!({ "kind": "client", "data": { "http.request.method": "GET", "sentry.http.prefetch": true } }),
            "http.client",
        ),
        // The deprecated names sentry-conventions renames first.
        (
            json!({ "kind": "server", "data": { "http.method": "GET" } }),
            "http.server",
        ),
        (json!({ "data": { "http.request_method": "GET" } }), "http"),
        (json!({ "data": { "method": "GET" } }), "http"),
        (json!({ "data": { "db.system": "postgresql" } }), "db"),
        (
            json!({ "data": { "otel.kind": "client", "http.request.method": "GET" } }),
            "http.client",
        ),
        // In Relay's order: HTTP, database, AI, RPC, messaging, FaaS.
        (
            json!({ "data": { "http.request.method": "GET", "db.system.name": "mysql" } }),
            "http",
        ),
        (
            json!({ "data": { "db.system.name": "mysql", "gen_ai.operation.name": "chat" } }),
            "db",
        ),
        (
            json!({ "data": { "rpc.service": "s", "messaging.system": "kafka" } }),
            "rpc",
        ),
        (
            json!({ "data": { "messaging.system": "kafka" } }),
            "message",
        ),
        (
            json!({ "data": { "messaging.system": "kafka", "faas.trigger": "pubsub" } }),
            "message",
        ),
        (json!({ "data": { "faas.trigger": "pubsub" } }), "pubsub"),
        // Read as strings, so another type is no match.
        (json!({ "data": { "faas.trigger": 5 } }), "default"),
        (json!({ "data": { "gen_ai.operation.name": 5 } }), "default"),
        // Relay drops an empty `data` value before it looks.
        (json!({ "data": { "db.system.name": null } }), "default"),
        (json!({ "data": { "db.system.name": "" } }), "default"),
        // A tag is an attribute where `data` has none of that name.
        (json!({ "tags": { "db.system.name": "mysql" } }), "db"),
        // An op the SDK sent is never replaced.
        (
            json!({ "op": "custom", "data": { "db.system.name": "mysql" } }),
            "custom",
        ),
    ]
}

#[tokio::test]
async fn a_span_item_without_an_op_gets_the_one_its_attributes_imply() {
    use rustrak::digest::processors::SpanProcessor;
    let db = TestDb::new().await;
    let ctx = project(&db).await;

    let cases = inferred();
    let items = cases
        .iter()
        .enumerate()
        .map(|(n, (fields, _))| {
            let mut span = json!({
                "span_id": format!("{n:016x}"),
                "trace_id": "4c79f60c11214eb38604f4ae0781bfb2",
                "start_timestamp": 946684800.0,
                "timestamp": 946684810.0,
            });
            for (key, value) in fields.as_object().unwrap() {
                span[key] = value.clone();
            }
            serde_json::to_vec(&span).unwrap().into()
        })
        .collect();
    SpanProcessor.process_batch(items, &ctx).await.unwrap();

    let ops = ops(&ctx).await;
    assert_eq!(ops.len(), cases.len());
    for (n, (fields, expected)) in cases.iter().enumerate() {
        assert_eq!(
            ops[&format!("{n:016x}")].as_deref(),
            Some(*expected),
            "{fields}"
        );
    }
}

/// One v2 span whose typed attributes are `attributes`.
fn v2(n: usize, attributes: &Value) -> Value {
    json!({
        "trace_id": "4c79f60c11214eb38604f4ae0781bfb2",
        "span_id": format!("{n:016x}"),
        "name": "render route (app) /items/[id]",
        "start_timestamp": 946684800.0,
        "end_timestamp": 946684810.0,
        "attributes": attributes,
    })
}

fn string(value: &str) -> Value {
    json!({ "value": value, "type": "string" })
}

#[tokio::test]
async fn a_v2_span_without_sentry_op_gets_the_one_its_attributes_imply() {
    use rustrak::digest::processors::SpanV2Processor;
    let db = TestDb::new().await;
    let ctx = project(&db).await;

    // Relay infers only where the `sentry.op` key is absent
    // (`normalize_sentry_op`, relay-event-normalization/src/eap/mod.rs).
    let cases: Vec<(Value, Option<&str>)> = vec![
        // What `@sentry/nextjs` 11 streams for a Next.js internal span.
        (
            json!({ "next.span_type": string("AppRender.getBodyResult") }),
            Some("default"),
        ),
        (json!({}), Some("default")),
        (
            json!({ "sentry.kind": string("client"), "http.request.method": string("GET") }),
            Some("http.client"),
        ),
        // Relay's `test_op_from_deprecated_http_method`.
        (
            json!({ "sentry.kind": string("server"), "http.method": string("GET") }),
            Some("http.server"),
        ),
        // Relay's `test_op_from_deprecated_db_system`.
        (json!({ "db.system": string("postgresql") }), Some("db")),
        (
            json!({ "gen_ai.operation.name": string("invoke_agent") }),
            Some("gen_ai.invoke_agent"),
        ),
        // Relay's `test_attributes_from_deprecated_gen_ai_system`.
        (
            json!({ "gen_ai.system": string("some system") }),
            Some("default"),
        ),
        (
            json!({ "sentry.op": string("custom"), "db.system.name": string("mysql") }),
            Some("custom"),
        ),
        (json!({ "sentry.op": string("") }), Some("")),
        // The key is there, so nothing is inferred, and its value is not one.
        (
            json!({ "sentry.op": { "value": 5, "type": "string" }, "db.system.name": string("mysql") }),
            None,
        ),
        (
            json!({ "sentry.op": { "value": 5, "type": "integer" }, "db.system.name": string("mysql") }),
            None,
        ),
    ];
    let items: Vec<Value> = cases
        .iter()
        .enumerate()
        .map(|(n, (attributes, _))| v2(n, attributes))
        .collect();
    let container = json!({ "version": 2, "items": items });
    SpanV2Processor
        .process(serde_json::to_vec(&container).unwrap().into(), &ctx)
        .await
        .unwrap();

    let ops = ops(&ctx).await;
    assert_eq!(ops.len(), cases.len());
    for (n, (attributes, expected)) in cases.iter().enumerate() {
        assert_eq!(
            ops[&format!("{n:016x}")].as_deref(),
            *expected,
            "{attributes}"
        );
    }
}
