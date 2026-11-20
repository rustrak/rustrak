//! Spans an SDK streams instead of sending a transaction (#552): the root of
//! a segment is what Performance shows as a transaction, and the segment's
//! other spans are its waterfall, in whatever order the two arrive.

use crate::common::{create_test_project, create_test_token, TestDb};
use actix_web::test;
use chrono::Utc;
use rustrak::db::DbPool;
use rustrak::digest::processors::{ProcessorCtx, SpanV2Processor};
use serde_json::{json, Value};
use uuid::Uuid;

const TRACE: &str = "da245123602b428cb8f92a34062b63c3";
const ROOT: &str = "8632fabd5206435c";

fn ctx(pool: &DbPool, project_id: i32) -> ProcessorCtx {
    ProcessorCtx {
        pool: pool.clone(),
        project_id,
        event_id: Uuid::new_v4(),
        ingested_at: Utc::now(),
        remote_addr: None,
        keep_ai_spans: true,
    }
}

fn typed(attributes: Value) -> Value {
    attributes
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, value)| {
            let kind = match value {
                Value::String(_) => "string",
                Value::Bool(_) => "boolean",
                Value::Number(n) if n.is_f64() => "double",
                _ => "integer",
            };
            (key.clone(), json!({ "value": value, "type": kind }))
        })
        .collect::<serde_json::Map<_, _>>()
        .into()
}

fn now() -> f64 {
    Utc::now().timestamp() as f64
}

/// The root of a segment, as `@sentry/nextjs` 11 sends a request's.
fn root(span_id: &str, attributes: Value) -> Value {
    let mut all = json!({
        "sentry.op": "http.server", "sentry.segment.id": span_id,
        "sentry.segment.name": "GET /items/[id]", "sentry.segment.name.source": "route",
        "sentry.release": "web@1.0.0", "sentry.environment": "production",
        "sentry.sdk.name": "sentry.javascript.nextjs", "sentry.sdk.version": "11.14.0",
        "http.response.status_code": 200, "url.full": "http://localhost/items/6",
    });
    all.as_object_mut()
        .unwrap()
        .extend(attributes.as_object().unwrap().clone());
    let start = now();
    json!({
        "trace_id": TRACE, "span_id": span_id, "name": "GET /items/[id]", "status": "ok",
        "is_segment": true, "start_timestamp": start, "end_timestamp": start + 0.5,
        "attributes": typed(all),
    })
}

fn child(span_id: &str, segment: &str, name: &str, attributes: Value) -> Value {
    let mut all = json!({
        "sentry.segment.id": segment, "sentry.segment.name": "GET",
        "sentry.release": "web@1.0.0", "sentry.environment": "production",
    });
    all.as_object_mut()
        .unwrap()
        .extend(attributes.as_object().unwrap().clone());
    let start = now();
    json!({
        "trace_id": TRACE, "span_id": span_id, "parent_span_id": segment, "name": name,
        "status": "ok", "is_segment": false,
        "start_timestamp": start + 0.1, "end_timestamp": start + 0.2,
        "attributes": typed(all),
    })
}

async fn send(pool: &DbPool, project_id: i32, items: Vec<Value>) {
    let body = json!({ "version": 2, "items": items });
    SpanV2Processor
        .process(
            serde_json::to_vec(&body).unwrap().into(),
            &ctx(pool, project_id),
        )
        .await
        .unwrap();
}

async fn get(pool: &DbPool, token: &str, uri: String) -> Value {
    let app = crate::common::app(pool).await;
    let req = test::TestRequest::get()
        .uri(&uri)
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200, "{uri}");
    test::read_body_json(resp).await
}

async fn transactions(pool: &DbPool, token: &str, project_id: i32) -> Vec<Value> {
    get(
        pool,
        token,
        format!("/api/projects/{project_id}/transactions"),
    )
    .await["items"]
        .as_array()
        .unwrap()
        .clone()
}

async fn detail(pool: &DbPool, token: &str, project_id: i32, id: &Value) -> Value {
    let id = id.as_str().unwrap();
    get(
        pool,
        token,
        format!("/api/projects/{project_id}/transactions/{id}"),
    )
    .await
}

fn span_ids(spans: &Value) -> Vec<&str> {
    spans
        .as_array()
        .unwrap()
        .iter()
        .map(|span| span["span_id"].as_str().unwrap())
        .collect()
}

#[actix_web::test]
async fn the_root_of_a_segment_is_a_transaction() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Streamed").await;
    send(
        &db.pool,
        project.id,
        vec![
            root(ROOT, json!({})),
            child(
                "b4bdeb38a16dfd36",
                ROOT,
                "SELECT 1",
                json!({ "sentry.op": "db", "db.system": "postgresql" }),
            ),
        ],
    )
    .await;

    let listed = transactions(&db.pool, &token, project.id).await;
    assert_eq!(listed.len(), 1, "{listed:?}");
    let txn = &listed[0];
    assert_eq!(txn["transaction_name"], "GET /items/[id]");
    assert_eq!(txn["status"], "ok");
    assert_eq!(txn["release"], "web@1.0.0");
    assert_eq!(txn["environment"], "production");
    assert_eq!(txn["platform"], "javascript");
    assert_eq!(txn["duration_ms"], 500.0);

    let one = detail(&db.pool, &token, project.id, &txn["id"]).await;
    let data = &one["data"];
    assert_eq!(data["transaction"], "GET /items/[id]");
    assert_eq!(data["transaction_info"]["source"], "route");
    assert_eq!(data["sdk"]["name"], "sentry.javascript.nextjs");
    let trace = &data["contexts"]["trace"];
    assert_eq!(trace["trace_id"], TRACE);
    assert_eq!(trace["span_id"], ROOT);
    assert_eq!(trace["op"], "http.server");
    assert_eq!(trace["status"], "ok");
    assert_eq!(trace["data"]["http.response.status_code"], 200);
    assert_eq!(data["request"]["url"], "http://localhost/items/6");

    // The waterfall: the segment's other spans, in the shape a transaction's
    // own are in. Its root is the transaction, not one of them.
    let spans = data["spans"].as_array().unwrap();
    assert_eq!(spans.len(), 1, "{spans:?}");
    let span = &spans[0];
    assert_eq!(span["span_id"], "b4bdeb38a16dfd36");
    assert_eq!(span["parent_span_id"], ROOT);
    assert_eq!(span["op"], "db");
    assert_eq!(span["description"], "SELECT 1");
    assert_eq!(span["status"], "ok");
    assert_eq!(span["data"]["db.system"], "postgresql");
    let (start, end) = (
        span["start_timestamp"].as_f64().unwrap(),
        span["timestamp"].as_f64().unwrap(),
    );
    assert!((end - start - 0.1).abs() < 1e-3, "{start} {end}");

    let id = txn["id"].as_str().unwrap();
    let of = get(
        &db.pool,
        &token,
        format!("/api/projects/{}/transactions/{id}/spans", project.id),
    )
    .await;
    assert_eq!(span_ids(&of), ["b4bdeb38a16dfd36"]);
}

#[actix_web::test]
async fn a_segment_is_whole_in_any_order_its_spans_arrive() {
    // The SDK sends each span as it ends: one that ends first goes out
    // before its root, and a web vital long after it.
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    for (n, order) in [[0, 1, 2], [1, 0, 2], [1, 2, 0]].iter().enumerate() {
        let project = create_test_project(&db.pool, &format!("Order {n}")).await;
        let batches = [
            vec![root(ROOT, json!({}))],
            vec![child("c000000000000001", ROOT, "early", json!({}))],
            vec![child("c000000000000002", ROOT, "late", json!({}))],
        ];
        for batch in order {
            send(&db.pool, project.id, batches[*batch].clone()).await;
        }

        let listed = transactions(&db.pool, &token, project.id).await;
        assert_eq!(listed.len(), 1, "{order:?}");
        let one = detail(&db.pool, &token, project.id, &listed[0]["id"]).await;
        let mut ids = span_ids(&one["data"]["spans"]);
        ids.sort_unstable();
        assert_eq!(ids, ["c000000000000001", "c000000000000002"], "{order:?}");
    }
}

#[actix_web::test]
async fn a_batch_delivered_twice_is_stored_once() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Twice").await;
    let batch = vec![
        root(ROOT, json!({})),
        child("c000000000000001", ROOT, "a", json!({})),
    ];
    send(&db.pool, project.id, batch.clone()).await;
    send(&db.pool, project.id, batch).await;

    let listed = transactions(&db.pool, &token, project.id).await;
    assert_eq!(listed.len(), 1);
    let one = detail(&db.pool, &token, project.id, &listed[0]["id"]).await;
    assert_eq!(span_ids(&one["data"]["spans"]), ["c000000000000001"]);
}

/// The client checks a transaction's id with a strict `z.uuid()`, which
/// rejects the random version and variant bits of a bare hash: such a
/// transaction could be neither listed nor opened (#572). Eight roots,
/// since one bare hash in 64 already reads as a version 8 UUID.
#[actix_web::test]
async fn a_transaction_made_from_a_root_has_an_rfc_9562_id() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Ids").await;
    let roots: Vec<Value> = (1..=8)
        .map(|n| root(&format!("{n:016x}"), json!({})))
        .collect();
    send(&db.pool, project.id, roots).await;

    let listed = transactions(&db.pool, &token, project.id).await;
    assert_eq!(listed.len(), 8, "{listed:?}");
    for transaction in listed {
        let id = Uuid::parse_str(transaction["id"].as_str().unwrap()).unwrap();
        assert_eq!(id.get_version_num(), 8, "{id}");
        assert_eq!(id.get_variant(), uuid::Variant::RFC4122, "{id}");
    }
}

#[actix_web::test]
async fn each_segment_of_a_trace_has_its_own_spans() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Two").await;
    let other = "9999999999999999";
    send(
        &db.pool,
        project.id,
        vec![
            root(ROOT, json!({})),
            root(other, json!({ "sentry.segment.name": "GET /other" })),
            child("c000000000000001", ROOT, "a", json!({})),
            child("c000000000000002", other, "b", json!({})),
        ],
    )
    .await;

    let listed = transactions(&db.pool, &token, project.id).await;
    assert_eq!(listed.len(), 2);
    for txn in &listed {
        let one = detail(&db.pool, &token, project.id, &txn["id"]).await;
        let expected = if txn["transaction_name"] == "GET /other" {
            "c000000000000002"
        } else {
            "c000000000000001"
        };
        assert_eq!(span_ids(&one["data"]["spans"]), [expected]);
    }
}

#[actix_web::test]
async fn a_span_that_is_not_a_segments_root_is_no_transaction() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "None").await;
    let mut vital = root(
        "a000000000000001",
        json!({ "sentry.op": "ui.webvital.inp" }),
    );
    vital["name"] = json!("INP");
    send(
        &db.pool,
        project.id,
        vec![
            // Its root never came.
            child("c000000000000001", ROOT, "orphan", json!({})),
            // Relay takes a web vital's or an interaction's span out of
            // segments altogether.
            vital,
        ],
    )
    .await;

    assert_eq!(transactions(&db.pool, &token, project.id).await.len(), 0);
}

#[actix_web::test]
async fn web_vitals_are_the_transactions_measurements() {
    // Three are attributes of the root; LCP, CLS and INP are spans of their
    // own, sent when the page is left.
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Vitals").await;
    send(
        &db.pool,
        project.id,
        vec![root(
            ROOT,
            json!({
                "sentry.op": "pageload", "sentry.sdk.name": "sentry.javascript.react",
                "browser.web_vital.ttfb.value": 81.5, "browser.web_vital.fcp.value": 240.0,
            }),
        )],
    )
    .await;
    send(
        &db.pool,
        project.id,
        vec![
            child(
                "c000000000000001",
                ROOT,
                "LCP",
                json!({ "sentry.op": "ui.webvital.lcp", "browser.web_vital.lcp.value": 1200.0 }),
            ),
            child(
                "c000000000000002",
                ROOT,
                "CLS",
                json!({ "sentry.op": "ui.webvital.cls", "browser.web_vital.cls.value": 0.02 }),
            ),
        ],
    )
    .await;

    let listed = transactions(&db.pool, &token, project.id).await;
    let one = detail(&db.pool, &token, project.id, &listed[0]["id"]).await;
    let measurements = &one["data"]["measurements"];
    assert_eq!(measurements["ttfb"]["value"], 81.5);
    assert_eq!(measurements["fcp"]["value"], 240.0);
    assert_eq!(measurements["lcp"]["value"], 1200.0);
    assert_eq!(measurements["cls"]["value"], 0.02);
}

#[actix_web::test]
async fn a_failed_segment_counts_as_a_failed_transaction() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Failed").await;
    let mut failed = root(ROOT, json!({ "sentry.status.message": "internal_error" }));
    failed["status"] = json!("error");
    send(
        &db.pool,
        project.id,
        vec![failed, root("9999999999999999", json!({}))],
    )
    .await;

    let stats = get(
        &db.pool,
        &token,
        format!("/api/projects/{}/transactions/stats", project.id),
    )
    .await;
    let group = &stats["items"][0];
    assert_eq!(group["count"], 2, "{stats}");
    assert_eq!(group["failure_rate"], 0.5, "{stats}");
}

#[actix_web::test]
async fn a_transaction_sent_as_one_keeps_its_own_spans() {
    // A span item of the same trace that names the transaction as its
    // segment is not among them: what the SDK sent is given back as sent.
    use rustrak::digest::processors::TransactionProcessor;
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Static").await;
    let start = now();
    let payload = json!({
        "type": "transaction", "transaction": "GET /static", "platform": "node",
        "start_timestamp": start, "timestamp": start + 1.0,
        "contexts": { "trace": { "trace_id": TRACE, "span_id": ROOT, "op": "http.server" } },
        "spans": [{ "span_id": "d000000000000001", "parent_span_id": ROOT, "op": "db",
                    "trace_id": TRACE, "start_timestamp": start, "timestamp": start + 0.1 }],
    });
    TransactionProcessor
        .process(
            serde_json::to_vec(&payload).unwrap().into(),
            &ctx(&db.pool, project.id),
        )
        .await
        .unwrap();
    send(
        &db.pool,
        project.id,
        vec![child("c000000000000001", ROOT, "chat", json!({}))],
    )
    .await;

    let listed = transactions(&db.pool, &token, project.id).await;
    assert_eq!(listed.len(), 1);
    let one = detail(&db.pool, &token, project.id, &listed[0]["id"]).await;
    assert_eq!(span_ids(&one["data"]["spans"]), ["d000000000000001"]);
}

#[actix_web::test]
async fn an_attribute_named_span_id_is_an_attribute() {
    // Not the span's id, and not a sign that the bag is a whole span.
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Named").await;
    send(
        &db.pool,
        project.id,
        vec![
            root(ROOT, json!({ "span_id": "custom" })),
            child(
                "c000000000000001",
                ROOT,
                "SELECT 1",
                json!({ "span_id": "custom", "sentry.op": "db" }),
            ),
        ],
    )
    .await;

    let listed = transactions(&db.pool, &token, project.id).await;
    let one = detail(&db.pool, &token, project.id, &listed[0]["id"]).await;
    let data = &one["data"];
    assert_eq!(data["request"]["url"], "http://localhost/items/6");
    assert_eq!(data["contexts"]["trace"]["data"]["span_id"], "custom");
    let spans = data["spans"].as_array().unwrap();
    assert_eq!(spans.len(), 1, "{spans:?}");
    assert_eq!(spans[0]["span_id"], "c000000000000001");
    assert_eq!(spans[0]["op"], "db");
    assert_eq!(spans[0]["description"], "SELECT 1");
    assert!(spans[0]["timestamp"].is_number());
    assert_eq!(spans[0]["data"]["span_id"], "custom");
}
