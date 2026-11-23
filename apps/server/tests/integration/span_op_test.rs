//! What the API answers for a transaction and spans sent without an `op`
//! (#559): `"default"`, as sentry.io does, everywhere an op is read.

use crate::common::{create_test_project, create_test_token, TestDb};
use actix_web::test;
use chrono::Utc;
use rustrak::digest::processors::{ProcessorCtx, SpanV2Processor, TransactionProcessor};
use serde_json::{json, Value};
use uuid::Uuid;

const TRACE: &str = "4c79f60c11214eb38604f4ae0781bfb2";

/// The start of this second: the lists read the last 90 days.
fn now() -> f64 {
    Utc::now().timestamp() as f64
}

/// The Next.js transaction of the issue, cut down: the root and two of its
/// spans carry no op, one span carries its own.
fn nextjs_transaction() -> Value {
    let span = |span_id: &str, description: &str| {
        json!({
            "span_id": span_id,
            "parent_span_id": "fa90fdead5f74053",
            "trace_id": TRACE,
            "description": description,
            "start_timestamp": now(),
            "timestamp": now() + 5.0,
            "data": { "next.span_type": "AppRender.getBodyResult" },
        })
    };
    let mut null_op = span("a000000000000002", "build component tree");
    null_op["op"] = Value::Null;
    let mut own_op = span("a000000000000003", "resolve page server component");
    own_op["op"] = json!("function");
    json!({
        "type": "transaction",
        "transaction": "GET /items/[id]",
        "start_timestamp": now(),
        "timestamp": now() + 10.0,
        "contexts": { "trace": { "trace_id": TRACE, "span_id": "fa90fdead5f74053" } },
        "spans": [span("a000000000000001", "render route (app) /items/[id]"), null_op, own_op],
    })
}

#[actix_web::test]
async fn a_transaction_sent_without_ops_reads_back_as_default_everywhere() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let token = create_test_token(&pool).await;
    let project = create_test_project(&pool, "Span op").await;
    let ctx = ProcessorCtx {
        pool: pool.clone(),
        project_id: project.id,
        event_id: Uuid::new_v4(),
        ingested_at: Utc::now(),
        remote_addr: None,
        keep_ai_spans: true,
    };
    TransactionProcessor
        .process(
            serde_json::to_vec(&nextjs_transaction()).unwrap().into(),
            &ctx,
        )
        .await
        .unwrap();
    // The same spans as JS SDK 11 streams them, in another trace.
    let streamed = json!({ "version": 2, "items": [{
        "trace_id": "5c79f60c11214eb38604f4ae0781bfb2",
        "span_id": "b000000000000001",
        "name": "start response",
        "status": "ok",
        "start_timestamp": now(),
        "end_timestamp": now() + 1.0,
        "attributes": { "next.span_type": { "value": "NextNodeServer.startResponse", "type": "string" } },
    }]});
    SpanV2Processor
        .process(serde_json::to_vec(&streamed).unwrap().into(), &ctx)
        .await
        .unwrap();

    let app = crate::common::app(&pool).await;
    let get = |path: String| {
        let req = test::TestRequest::get()
            .uri(&format!("/api/projects/{}/{path}", project.id))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let app = &app;
        async move {
            let resp = test::call_service(app, req).await;
            assert_eq!(resp.status(), 200, "{path}");
            test::read_body_json::<Value, _>(resp).await
        }
    };

    // The transaction list and its op filter.
    let list = get("transactions?op=default".into()).await;
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    let id = list["items"][0]["id"].as_str().unwrap().to_string();

    // The payload the waterfall is drawn from, spans in the order sent.
    let detail = get(format!("transactions/{id}")).await;
    assert_eq!(detail["data"]["contexts"]["trace"]["op"], "default");
    let ops: Vec<&Value> = detail["data"]["spans"]
        .as_array()
        .unwrap()
        .iter()
        .map(|span| &span["op"])
        .collect();
    assert_eq!(ops, ["default", "default", "function"]);

    // The indexed spans of the transaction.
    let spans = get(format!("transactions/{id}/spans")).await;
    let mut ops: Vec<&str> = spans
        .as_array()
        .unwrap()
        .iter()
        .map(|span| span["op"].as_str().unwrap())
        .collect();
    ops.sort_unstable();
    assert_eq!(ops, ["default", "default", "function"]);

    // The span list filters on it, streamed spans included.
    let by_op = get("spans?op=default".into()).await;
    let mut found: Vec<&str> = by_op["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|span| span["span_id"].as_str().unwrap())
        .collect();
    found.sort_unstable();
    assert_eq!(
        found,
        ["a000000000000001", "a000000000000002", "b000000000000001"]
    );

    // The endpoint the overview lists is the one its detail page asks for.
    let stats = get("transactions/stats".into()).await;
    assert_eq!(stats["total_count"], 1);
    assert_eq!(stats["items"][0]["op"], "default");
    let group = get("transactions/stats/group?name=GET%20/items/[id]&op=default".into()).await;
    assert_eq!(group["count"], 1);
}

#[actix_web::test]
async fn spans_kept_in_the_transaction_payload_get_the_default_op_too() {
    // One entry that is not an object keeps the whole array in the payload,
    // where the detail endpoint reads it from.
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let token = create_test_token(&pool).await;
    let project = create_test_project(&pool, "Span op kept").await;
    let ctx = ProcessorCtx {
        pool: pool.clone(),
        project_id: project.id,
        event_id: Uuid::new_v4(),
        ingested_at: Utc::now(),
        remote_addr: None,
        keep_ai_spans: true,
    };
    let mut payload = nextjs_transaction();
    payload["spans"].as_array_mut().unwrap().push(Value::Null);
    TransactionProcessor
        .process(serde_json::to_vec(&payload).unwrap().into(), &ctx)
        .await
        .unwrap();
    let id: Uuid = sqlx::query_scalar("SELECT id FROM transactions WHERE project_id = $1")
        .bind(project.id)
        .fetch_one(&pool)
        .await
        .unwrap();

    let app = crate::common::app(&pool).await;
    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/transactions/{id}", project.id))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let detail: Value = test::read_body_json(resp).await;

    let spans = detail["data"]["spans"].as_array().unwrap();
    let ops: Vec<&Value> = spans[..3].iter().map(|span| &span["op"]).collect();
    assert_eq!(ops, ["default", "default", "function"]);
    assert!(spans[3].is_null(), "what is not a span is left as sent");
}
