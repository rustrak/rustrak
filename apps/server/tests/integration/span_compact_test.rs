//! What a span row no longer repeats (#558), and every reader still gets:
//! its parent and segment ids, 16 hex characters kept as the 8 bytes they
//! are, and the platform, release and environment of the transaction it
//! came in.

use crate::common::{create_test_project, create_test_token, TestDb};
use actix_web::test;
use chrono::Utc;
use rustrak::db::DbPool;
use rustrak::digest::processors::{ProcessorCtx, SpanProcessor, TransactionProcessor};
use serde_json::{json, Value};
use uuid::Uuid;

const TRACE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const ROOT: &str = "00000000000000a1";

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

/// A transaction of `release` whose spans are `spans`, and its id.
async fn transaction(pool: &DbPool, project_id: i32, release: &str, spans: Value) -> Uuid {
    let now = Utc::now().timestamp() as f64;
    let mut spans = spans;
    for span in spans.as_array_mut().unwrap() {
        span["trace_id"] = json!(TRACE);
        span["start_timestamp"] = json!(now);
        span["timestamp"] = json!(now + 0.1);
    }
    let payload = json!({
        "type": "transaction", "transaction": "GET /items", "platform": "php",
        "release": release, "environment": "production",
        "start_timestamp": now, "timestamp": now + 1.0,
        "contexts": { "trace": { "trace_id": TRACE, "span_id": ROOT, "op": "http.server" } },
        "spans": spans,
    });
    TransactionProcessor
        .process(
            serde_json::to_vec(&payload).unwrap().into(),
            &ctx(pool, project_id),
        )
        .await
        .unwrap();
    sqlx::query_scalar(
        "SELECT id FROM transactions WHERE project_id = $1 ORDER BY ingested_at DESC",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// The span's JSON in each place the API serves it: the list, its own page,
/// its transaction's spans and its transaction's payload.
async fn served(
    pool: &DbPool,
    token: &str,
    project_id: i32,
    txn: Uuid,
    span_id: &str,
) -> Vec<Value> {
    let p = format!("/api/projects/{project_id}");
    let of = |spans: &Value| {
        spans
            .as_array()
            .unwrap()
            .iter()
            .find(|span| span["span_id"] == span_id)
            .cloned()
            .unwrap_or_else(|| panic!("{span_id} not served"))
    };
    let listed = of(&get(pool, token, format!("{p}/spans?per_page=100")).await["items"]);
    let own = get(
        pool,
        token,
        format!("{p}/spans/{}", listed["id"].as_str().unwrap()),
    )
    .await;
    let of_transaction = of(&get(pool, token, format!("{p}/transactions/{txn}/spans")).await);
    let in_payload =
        of(&get(pool, token, format!("{p}/transactions/{txn}")).await["data"]["spans"]);
    vec![listed, own, of_transaction, in_payload]
}

#[actix_web::test]
async fn parent_and_segment_ids_are_kept_as_numbers_and_served_as_sent() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Ids").await;
    // (span, parent, segment): the common form, its extremes, and three
    // forms eight bytes cannot give back, which stay as text.
    let cases = [
        ("b000000000000001", "00000000000000a1", "00000000000000a1"),
        ("b000000000000002", "ffffffffffffffff", "0000000000000000"),
        ("b000000000000003", "8000000000000000", "7fffffffffffffff"),
        ("b000000000000004", "00000000000000A1", "ABCDEF0123456789"),
        ("b000000000000005", "a1", "not-hex-at-all!!"),
        ("b000000000000006", "00000000000000a1x", "+000000000000001"),
    ];
    let spans: Vec<Value> = cases
        .iter()
        .map(|(span_id, parent, segment)| {
            json!({ "span_id": span_id, "parent_span_id": parent, "segment_id": segment, "op": "db" })
        })
        .collect();
    let txn = transaction(&db.pool, project.id, "app@1", json!(spans)).await;

    for (n, (span_id, parent, segment)) in cases.iter().enumerate() {
        let (as_text, as_number): (Option<String>, Option<i64>) = sqlx::query_as(
            "SELECT parent_span_id, parent_span_id_int FROM spans WHERE span_id = $1",
        )
        .bind(span_id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(
            (as_text.is_none(), as_number.is_some()),
            (n < 3, n < 3),
            "{parent} stored as a number"
        );
        for span in served(&db.pool, &token, project.id, txn, span_id).await {
            assert_eq!(span["parent_span_id"], *parent, "{span}");
            assert_eq!(span["segment_id"], *segment, "{span}");
        }
    }
}

#[actix_web::test]
async fn a_span_with_no_parent_or_segment_has_none() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "No ids").await;
    let txn = transaction(
        &db.pool,
        project.id,
        "app@1",
        json!([{ "span_id": "e000000000000001", "op": "db" }]),
    )
    .await;

    let served = served(&db.pool, &token, project.id, txn, "e000000000000001").await;
    for span in &served[..3] {
        assert_eq!(span["parent_span_id"], Value::Null, "{span}");
        assert_eq!(span["segment_id"], Value::Null, "{span}");
    }
    // The payload is what the SDK sent: the keys are absent, not null.
    assert!(served[3].get("parent_span_id").is_none());
    assert!(served[3].get("segment_id").is_none());
}

#[actix_web::test]
async fn a_transactions_spans_read_their_scope_from_it() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Scope").await;
    let txn = transaction(
        &db.pool,
        project.id,
        "app@1.4.2",
        json!([
            { "span_id": "c000000000000001", "parent_span_id": ROOT, "op": "db.sql.query" },
            // An AI span: the agents pages filter spans by environment, so
            // it keeps its own copy.
            { "span_id": "c000000000000002", "parent_span_id": ROOT, "op": "gen_ai.chat",
              "data": { "gen_ai.operation.name": "chat", "gen_ai.request.model": "m" } },
        ]),
    )
    .await;
    // One with no release: its spans have none either.
    let bare = transaction(
        &db.pool,
        project.id,
        "",
        json!([{ "span_id": "c000000000000003", "parent_span_id": ROOT, "op": "db.sql.query" }]),
    )
    .await;

    let kept = |span_id: &'static str| {
        let pool = db.pool.clone();
        async move {
            sqlx::query_as::<_, (Option<String>, Option<String>, Option<String>)>(
                "SELECT platform, release, environment FROM spans WHERE span_id = $1",
            )
            .bind(span_id)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    assert_eq!(kept("c000000000000001").await, (None, None, None));
    assert_eq!(
        kept("c000000000000002").await,
        (
            Some("php".to_string()),
            Some("app@1.4.2".to_string()),
            Some("production".to_string())
        )
    );

    for (txn, span_id, release) in [
        (txn, "c000000000000001", json!("app@1.4.2")),
        (txn, "c000000000000002", json!("app@1.4.2")),
        (bare, "c000000000000003", Value::Null),
    ] {
        // Its transaction's payload carries no scope on a span.
        for span in &served(&db.pool, &token, project.id, txn, span_id).await[..3] {
            assert_eq!(span["platform"], "php", "{span}");
            assert_eq!(span["release"], release, "{span}");
            assert_eq!(span["environment"], "production", "{span}");
        }
    }
}

#[actix_web::test]
async fn a_scope_a_child_span_names_gives_way_to_its_transactions() {
    // As before, and as Relay does with the platform: a span in a
    // transaction is of that transaction's platform, release and environment.
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Conflict").await;
    let txn = transaction(
        &db.pool,
        project.id,
        "app@1.4.2",
        json!([{
            "span_id": "f000000000000001", "parent_span_id": ROOT, "op": "db.sql.query",
            "platform": "node", "release": "web@9", "environment": "staging",
        }]),
    )
    .await;

    let kept: (Option<String>, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT platform, release, environment FROM spans WHERE span_id = 'f000000000000001'",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(kept, (None, None, None));
    for span in &served(&db.pool, &token, project.id, txn, "f000000000000001").await[..3] {
        assert_eq!(span["platform"], "php", "{span}");
        assert_eq!(span["release"], "app@1.4.2", "{span}");
        assert_eq!(span["environment"], "production", "{span}");
    }
}

#[actix_web::test]
async fn a_span_item_keeps_the_scope_it_names() {
    // No transaction to read it from.
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Standalone").await;
    let now = Utc::now().timestamp() as f64;
    let item = json!({
        "span_id": "d000000000000001", "trace_id": TRACE, "parent_span_id": ROOT,
        "segment_id": ROOT, "op": "db", "platform": "node", "release": "web@2",
        "environment": "staging", "start_timestamp": now, "timestamp": now + 0.1,
    });
    SpanProcessor
        .process_batch(
            vec![serde_json::to_vec(&item).unwrap().into()],
            &ctx(&db.pool, project.id),
        )
        .await
        .unwrap();

    let list = get(
        &db.pool,
        &token,
        format!("/api/projects/{}/spans", project.id),
    )
    .await;
    let span = &list["items"][0];
    assert_eq!(span["platform"], "node");
    assert_eq!(span["release"], "web@2");
    assert_eq!(span["environment"], "staging");
    assert_eq!(span["parent_span_id"], ROOT);
    assert_eq!(span["segment_id"], ROOT);
}
