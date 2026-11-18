//! A span is stored once (issue #532): `transactions.data` without its
//! `spans`, and `spans.data` without what the row already has in columns.
//! The API answers exactly as it did when every span was stored three times.

use crate::common::TestDb;
use crate::common::{
    create_test_project, create_test_token, make_legacy, stored_json, stored_payload,
};
use actix_web::test;
use chrono::Utc;
use rustrak::digest::processors::{ProcessorCtx, TransactionProcessor};
use serde_json::{json, Value};
use uuid::Uuid;

const TRACE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// Top-level span keys the `spans` row also holds in a column.
const COLUMN_KEYS: [&str; 10] = [
    "span_id",
    "parent_span_id",
    "op",
    "description",
    "status",
    "start_timestamp",
    "timestamp",
    "exclusive_time",
    "tags",
    "segment_id",
];

/// What SDKs send for a transaction: every span field, an unknown key, a
/// timestamp no column can give back exactly and an op longer than its column.
fn transaction_payload() -> Value {
    json!({
        "event_id": Uuid::new_v4().simple().to_string(),
        "type": "transaction",
        "transaction": "GET /checkout",
        "start_timestamp": 1700000000.0,
        "timestamp": 1700000002.0,
        "platform": "python",
        "environment": "production",
        "release": "shop@1.0.0",
        "transaction_info": { "source": "route" },
        "contexts": { "trace": {
            "trace_id": TRACE, "span_id": "1000000000000000",
            "op": "http.server", "status": "ok", "origin": "auto.http.django"
        } },
        "measurements": { "lcp": { "value": 1200.0, "unit": "millisecond" } },
        "tags": { "browser": "Chrome" },
        "spans": [
            {
                "span_id": "2000000000000000",
                "parent_span_id": "1000000000000000",
                "trace_id": TRACE,
                "segment_id": "1000000000000000",
                "op": "db",
                "description": "SELECT * FROM orders WHERE id = %s",
                "status": "ok",
                "start_timestamp": 1700000000.25,
                "timestamp": 1700000000.75,
                "exclusive_time": 500.0,
                "origin": "auto.db.django",
                "tags": { "db.system": "postgresql" },
                "data": { "db.system": "postgresql", "db.name": "shop", "thread.id": 7 },
                "measurements": { "rows": { "value": 3, "unit": "none" } },
                "sentry_tags": { "category": "db" },
                "hash": "c5d2b6e0b0a4",
                "x_unknown": [1, { "nested": true }]
            },
            {
                "span_id": "3000000000000000",
                "parent_span_id": "1000000000000000",
                "op": "http.client",
                "description": "GET https://api.example.com/rates",
                "start_timestamp": 1_700_000_000.123_456_7,
                "timestamp": 1700000001.5,
                "exclusive_time": 3,
                "links": [{ "trace_id": TRACE, "span_id": "2000000000000000", "sampled": true }]
            },
            {
                "span_id": "4000000000000000",
                "parent_span_id": "3000000000000000",
                "op": "x".repeat(200),
                "start_timestamp": 1700000001.0,
                "timestamp": 1700000001.5,
                "is_segment": false
            }
        ]
    })
}

async fn store(pool: &rustrak::db::DbPool, project_id: i32, payload: &Value) -> Uuid {
    store_as(pool, project_id, payload, true).await
}

/// [`store`], with the project's agents feature on or off.
async fn store_as(
    pool: &rustrak::db::DbPool,
    project_id: i32,
    payload: &Value,
    keep_ai_spans: bool,
) -> Uuid {
    let ctx = ProcessorCtx {
        pool: pool.clone(),
        project_id,
        event_id: Uuid::new_v4(),
        ingested_at: Utc::now(),
        remote_addr: None,
        keep_ai_spans,
    };
    TransactionProcessor
        .process(
            bytes::Bytes::from(serde_json::to_vec(payload).unwrap()),
            &ctx,
        )
        .await
        .unwrap();
    sqlx::query_scalar("SELECT id FROM transactions WHERE project_id = $1")
        .bind(project_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn get(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    token: &str,
    uri: String,
) -> Value {
    let req = test::TestRequest::get()
        .uri(&uri)
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let resp = test::call_service(app, req).await;
    assert_eq!(resp.status(), 200, "{uri}");
    test::read_body_json(resp).await
}

/// The span detail endpoint's `attributes` and `tags` for each of `spans`,
/// matched by span id: what the endpoint returned before the change.
async fn assert_span_details(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    pool: &rustrak::db::DbPool,
    token: &str,
    project_id: i32,
    spans: &[Value],
) {
    for span in spans {
        let row_id: Uuid = sqlx::query_scalar("SELECT id FROM spans WHERE span_id = $1")
            .bind(span["span_id"].as_str().unwrap())
            .fetch_one(pool)
            .await
            .unwrap();
        let detail = get(
            app,
            token,
            format!("/api/projects/{project_id}/spans/{row_id}"),
        )
        .await;
        let attributes = match span.get("data") {
            Some(data) if data.is_object() => data.clone(),
            _ => span.clone(),
        };
        assert_eq!(detail["attributes"], attributes, "attributes of {span}");
        assert_eq!(
            detail["tags"],
            span.get("tags").cloned().unwrap_or(Value::Null)
        );
    }
}

#[actix_web::test]
async fn transaction_and_span_payloads_are_stored_once_and_read_back_whole() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let token = create_test_token(&pool).await;
    let project = create_test_project(&pool, "Span storage").await;
    let payload = transaction_payload();
    let txn_id = store(&pool, project.id, &payload).await;
    let app = crate::common::app(&pool).await;

    let body = get(
        &app,
        &token,
        format!("/api/projects/{}/transactions/{txn_id}", project.id),
    )
    .await;
    assert_eq!(
        body["data"], payload,
        "the transaction payload, spans in order"
    );
    let spans = payload["spans"].as_array().unwrap();
    assert_span_details(&app, &pool, &token, project.id, spans).await;

    // Stored once: no spans in the transaction, no column copies in a span.
    let stored = stored_json(&pool, "transactions", txn_id).await;
    assert!(
        stored.get("spans").is_none(),
        "transactions.data keeps spans"
    );
    let stored: Vec<(String, Uuid)> =
        sqlx::query_as("SELECT span_id, id FROM spans WHERE transaction_id = $1")
            .bind(txn_id)
            .fetch_all(&pool)
            .await
            .unwrap();
    for (span_id, id) in stored {
        let data = stored_json(&pool, "spans", id).await;
        // Only what a column cannot give back exactly stays: an integer
        // exclusive time, a sub-microsecond timestamp, an op cut to 128, a
        // float on a whole second (the column gives back an integer).
        let kept: &[&str] = match span_id.as_str() {
            "3000000000000000" => &["start_timestamp", "exclusive_time"],
            "4000000000000000" => &["op", "start_timestamp"],
            _ => &[],
        };
        for key in COLUMN_KEYS {
            assert_eq!(
                data.get(key).is_some(),
                kept.contains(&key),
                "{span_id}: spans.data.{key} in {data}"
            );
        }
    }
}

/// The timestamp forms SDKs send, as JSON text: float seconds with
/// milliseconds or microseconds, whole seconds, RFC 3339, and floats past
/// the microsecond. `n` varies the instant.
fn timestamp_form(form: usize, n: u64) -> String {
    let secs = 1_600_000_000 + n % 300_000_000;
    // Never a whole second: `1700000000.000` is a float an integer cannot
    // give back, and stays in `data` like the RFC 3339 strings do.
    let fraction = n.wrapping_mul(2_654_435_761) % 999_000 + 1_000;
    match form {
        0 => format!("{secs}.{:03}", fraction / 1000),
        1 => format!("{secs}.{fraction:06}"),
        2 => secs.to_string(),
        3 => format!(
            "\"{}\"",
            chrono::DateTime::from_timestamp(secs as i64, fraction as u32 * 1000)
                .unwrap()
                .to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
        ),
        _ => format!("{secs}.{fraction:06}7"),
    }
}

/// Every span's timestamps come back exactly as sent, and the common float
/// forms are given back by the columns rather than kept in `spans.data`.
#[actix_web::test]
async fn span_timestamps_round_trip_and_the_common_forms_are_compacted() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let token = create_test_token(&pool).await;
    let project = create_test_project(&pool, "Span timestamps").await;
    const FORMS: usize = 5;
    const PER_FORM: u64 = 200;
    let mut seed = 0x9E37_79B9_7F4A_7C15_u64;
    let spans: Vec<String> = (0..FORMS)
        .flat_map(|form| (0..PER_FORM).map(move |i| (form, i)))
        .map(|(form, i)| {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let n = seed >> 16;
            format!(
                r#"{{"span_id":"{form}{i:015x}","op":"db","start_timestamp":{},"timestamp":{}}}"#,
                timestamp_form(form, n),
                timestamp_form(form, n + 1)
            )
        })
        .collect();
    let payload: Value = serde_json::from_str(&format!(
        r#"{{"event_id":"{}","type":"transaction","transaction":"GET /t",
            "start_timestamp":1600000000.0,"timestamp":1900000001.0,
            "contexts":{{"trace":{{"trace_id":"{TRACE}","span_id":"1000000000000000"}}}},
            "spans":[{}]}}"#,
        Uuid::new_v4().simple(),
        spans.join(",")
    ))
    .unwrap();
    let txn_id = store(&pool, project.id, &payload).await;
    let app = crate::common::app(&pool).await;

    let body = get(
        &app,
        &token,
        format!("/api/projects/{}/transactions/{txn_id}", project.id),
    )
    .await;
    assert_eq!(
        serde_json::to_string(&body["data"]["spans"]).unwrap(),
        serde_json::to_string(&payload["spans"]).unwrap()
    );

    let stored: Vec<(String, Uuid)> =
        sqlx::query_as("SELECT span_id, id FROM spans WHERE transaction_id = $1")
            .bind(txn_id)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(stored.len(), FORMS * PER_FORM as usize);
    for (span_id, id) in stored {
        let data = stored_json(&pool, "spans", id).await;
        // Floats with up to six decimals and integers come back from the
        // columns; a string or a finer float stays as sent.
        let compacted = span_id.starts_with(['0', '1', '2']);
        for key in ["start_timestamp", "timestamp"] {
            assert_eq!(
                data.get(key).is_none(),
                compacted,
                "{span_id}: spans.data.{key} in {data}"
            );
        }
    }
}

/// Rows written before the change hold the whole span inside the transaction
/// and the whole span object in `spans.data`; they read back as they did.
#[actix_web::test]
async fn rows_in_the_old_format_read_back_unchanged() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let token = create_test_token(&pool).await;
    let project = create_test_project(&pool, "Legacy span storage").await;
    let payload = transaction_payload();
    let spans = payload["spans"].as_array().unwrap();

    let txn_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO transactions (id, event_id, project_id, trace_id, span_id, transaction_name, \
         start_timestamp, timestamp, duration_ms, data) \
         VALUES ($1, $2, $3, $4, '1000000000000000', 'GET /checkout', $5, $5, 2000.0, $6)",
    )
    .bind(txn_id)
    .bind(Uuid::new_v4())
    .bind(project.id)
    .bind(TRACE)
    .bind(Utc::now())
    .bind(&payload)
    .execute(&pool)
    .await
    .unwrap();
    for span in spans {
        sqlx::query(
            "INSERT INTO spans (id, transaction_id, project_id, span_id, trace_id, op, \
             description, tags, data) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(Uuid::new_v4())
        .bind(txn_id)
        .bind(project.id)
        .bind(span["span_id"].as_str())
        .bind(TRACE)
        .bind(span["op"].as_str().map(|op| &op[..op.len().min(128)]))
        .bind(span["description"].as_str())
        .bind(span.get("tags"))
        .bind(span)
        .execute(&pool)
        .await
        .unwrap();
    }
    let app = crate::common::app(&pool).await;

    let body = get(
        &app,
        &token,
        format!("/api/projects/{}/transactions/{txn_id}", project.id),
    )
    .await;
    assert_eq!(body["data"], payload);
    assert_span_details(&app, &pool, &token, project.id, spans).await;
}

/// Both payloads are stored compressed, nothing of them uncompressed, and
/// read back exactly as rows in the format before compression (whole JSON
/// in `data`, phase one's compact spans included) do.
#[actix_web::test]
async fn transaction_and_span_payloads_are_stored_compressed_and_read_as_old_rows() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let token = create_test_token(&pool).await;
    let project = create_test_project(&pool, "Compressed spans").await;
    let txn_id = store(&pool, project.id, &transaction_payload()).await;
    let spans: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM spans WHERE transaction_id = $1")
        .bind(txn_id)
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(spans.len(), 3);

    let (payload, empty) = stored_payload(&pool, "transactions", txn_id).await;
    let payload = payload.expect("compressed");
    let json = serde_json::to_vec(&stored_json(&pool, "transactions", txn_id).await).unwrap();
    assert!(empty, "transactions.data also holds the JSON");
    assert!(
        payload.len() < json.len(),
        "{} for {}",
        payload.len(),
        json.len()
    );
    for id in &spans {
        let (payload, empty) = stored_payload(&pool, "spans", *id).await;
        let json = serde_json::to_vec(&stored_json(&pool, "spans", *id).await).unwrap();
        assert!(empty, "spans.data also holds the JSON");
        assert!(payload.unwrap().len() <= json.len() + 1);
    }

    let app = crate::common::app(&pool).await;
    let mut uris = vec![format!(
        "/api/projects/{}/transactions/{txn_id}",
        project.id
    )];
    uris.extend(
        spans
            .iter()
            .map(|id| format!("/api/projects/{}/spans/{id}", project.id)),
    );
    let mut new = Vec::new();
    for uri in &uris {
        new.push(get(&app, &token, uri.clone()).await);
    }

    make_legacy(&pool, "transactions", txn_id).await;
    for id in &spans {
        make_legacy(&pool, "spans", *id).await;
    }
    for (uri, new) in uris.iter().zip(new) {
        assert_eq!(get(&app, &token, uri.clone()).await, new, "{uri}");
    }
}

/// Another writer that commits while a transaction's digest waits for
/// SQLite's write lock does not fail it. A deferred transaction that read
/// first (the payload dictionary, once per write) holds a snapshot that
/// cannot be upgraded: `SQLITE_BUSY` at once, or `SQLITE_BUSY_SNAPSHOT`
/// (517), neither retried by `busy_timeout`. Nothing is read inside it now,
/// so it waits for the lock and writes.
#[cfg(feature = "sqlite")]
#[actix_web::test]
async fn a_commit_while_a_digest_waits_for_the_write_lock_does_not_fail_it() {
    let directory = tempfile::TempDir::new().unwrap();
    let config = rustrak::config::DatabaseConfig {
        url: format!(
            "sqlite://{}?mode=rwc",
            directory.path().join("busy.db").display()
        ),
        max_connections: 5,
        min_connections: 1,
        acquire_timeout: std::time::Duration::from_secs(10),
        idle_timeout: std::time::Duration::from_secs(60),
        max_lifetime: std::time::Duration::from_secs(300),
    };
    let pool = rustrak::db::create_pool(&config).await.unwrap();
    rustrak::db::run_migrations(&pool).await.unwrap();
    let project = create_test_project(&pool, "Busy").await;

    // Open the connections first: opening one sets pragmas, which waits for
    // the write lock too.
    let open: Vec<_> = futures_util::future::join_all((0..5).map(|_| pool.acquire())).await;
    let mut other = open.into_iter().next().unwrap().unwrap();
    let mut writer = sqlx::Connection::begin_with(&mut *other, "BEGIN IMMEDIATE")
        .await
        .unwrap();
    sqlx::query("UPDATE projects SET name = name || 'x' WHERE id = $1")
        .bind(project.id)
        .execute(&mut *writer)
        .await
        .unwrap();
    let ctx = ProcessorCtx {
        pool: pool.clone(),
        project_id: project.id,
        event_id: Uuid::new_v4(),
        ingested_at: Utc::now(),
        remote_addr: None,
        keep_ai_spans: true,
    };
    let body = bytes::Bytes::from(serde_json::to_vec(&transaction_payload()).unwrap());
    let digest = tokio::spawn(async move { TransactionProcessor.process(body, &ctx).await });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    writer.commit().await.unwrap();

    digest
        .await
        .unwrap()
        .expect("the digest waits for the lock");
    let spans: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM spans")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(spans > 0);
}

/// A payload no reader can decode: a format byte that does not exist.
async fn corrupt(pool: &rustrak::db::DbPool, table: &str, id: Uuid) {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "UPDATE {table} SET payload = $1 WHERE id = $2"
    )))
    .bind(vec![9u8, 9])
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
}

async fn span_row_id(pool: &rustrak::db::DbPool, span_id: &str) -> Uuid {
    sqlx::query_scalar("SELECT id FROM spans WHERE span_id = $1")
        .bind(span_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// One span whose payload cannot be read leaves the transaction and the
/// conversation it belongs to readable without it; the span alone is an
/// error.
#[actix_web::test]
async fn an_unreadable_span_is_left_out_of_the_pages_that_list_it() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let token = create_test_token(&pool).await;
    let project = create_test_project(&pool, "Unreadable span").await;
    let mut payload = transaction_payload();
    let ai = |span_id: &str| {
        json!({
            "span_id": span_id, "parent_span_id": "1000000000000000", "op": "gen_ai.chat",
            "start_timestamp": 1700000001.0, "timestamp": 1700000001.5,
            "data": { "gen_ai.operation.name": "chat", "gen_ai.conversation.id": "conv-1" }
        })
    };
    let spans = payload["spans"].as_array_mut().unwrap();
    spans.push(ai("5000000000000000"));
    spans.push(ai("6000000000000000"));
    let txn_id = store(&pool, project.id, &payload).await;
    let bad = span_row_id(&pool, "2000000000000000").await;
    corrupt(&pool, "spans", bad).await;
    corrupt(&pool, "spans", span_row_id(&pool, "5000000000000000").await).await;
    let app = crate::common::app(&pool).await;

    let body = get(
        &app,
        &token,
        format!("/api/projects/{}/transactions/{txn_id}", project.id),
    )
    .await;
    let ids: Vec<&str> = body["data"]["spans"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["span_id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        ["3000000000000000", "4000000000000000", "6000000000000000"]
    );

    let conversation =
        rustrak::services::SpanService::agent_conversation(&pool, project.id, "conv-1")
            .await
            .unwrap();
    let ids: Vec<_> = conversation
        .spans
        .iter()
        .map(|s| s.span_id.clone().unwrap())
        .collect();
    assert_eq!(ids, ["6000000000000000"]);

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/spans/{bad}", project.id))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 500);
}

/// A Spans v2 attribute named like the compact-row marker is an attribute:
/// span detail returns the bag as sent.
#[actix_web::test]
async fn a_v2_attribute_named_like_the_marker_is_returned_as_sent() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let project = create_test_project(&pool, "V2 marker").await;
    let ctx = ProcessorCtx {
        pool: pool.clone(),
        project_id: project.id,
        event_id: Uuid::new_v4(),
        ingested_at: Utc::now(),
        remote_addr: None,
        keep_ai_spans: true,
    };
    let item = json!({ "version": 2, "items": [{
        "trace_id": TRACE, "span_id": "7000000000000000", "name": "GET /rates",
        "start_timestamp": 1700000000.5, "end_timestamp": 1700000001.0,
        "status": "ok", "is_segment": false,
        "attributes": {
            "_rustrak_position": { "value": 3, "type": "integer" },
            "http.method": { "value": "GET", "type": "string" }
        }
    }]});
    rustrak::digest::processors::SpanV2Processor
        .process(bytes::Bytes::from(serde_json::to_vec(&item).unwrap()), &ctx)
        .await
        .unwrap();

    let id = span_row_id(&pool, "7000000000000000").await;
    let detail = rustrak::services::SpanService::get_by_id(&pool, project.id, id)
        .await
        .unwrap();
    assert_eq!(
        detail.attributes,
        json!({ "_rustrak_position": 3, "http.method": "GET" })
    );
}

/// With agents off, the AI spans are left out of the `spans` rows the
/// transaction's spans are rebuilt from, as they were left out of the stored
/// payload when it kept them.
#[actix_web::test]
async fn with_agents_off_the_rebuilt_spans_leave_the_ai_ones_out() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let token = create_test_token(&pool).await;
    let project = create_test_project(&pool, "Agents off").await;
    let mut payload = transaction_payload();
    payload["spans"].as_array_mut().unwrap().insert(
        1,
        json!({
            "span_id": "5000000000000000", "parent_span_id": "1000000000000000",
            "op": "gen_ai.chat", "start_timestamp": 1700000001.0, "timestamp": 1700000001.5,
            "data": { "gen_ai.operation.name": "chat", "gen_ai.request.model": "gpt-4o" }
        }),
    );
    let txn_id = store_as(&pool, project.id, &payload, false).await;
    let app = crate::common::app(&pool).await;

    let body = get(
        &app,
        &token,
        format!("/api/projects/{}/transactions/{txn_id}", project.id),
    )
    .await;
    assert!(stored_json(&pool, "transactions", txn_id)
        .await
        .get("spans")
        .is_none());
    assert_eq!(body["data"], without_ai_spans(&payload));
}

/// `payload` with only the spans an agents-off project keeps.
fn without_ai_spans(payload: &Value) -> Value {
    let mut expected = payload.clone();
    expected["spans"]
        .as_array_mut()
        .unwrap()
        .retain(|s| s["op"] != "gen_ai.chat");
    expected
}

/// Two child spans with the same id are two spans: both come back, each in
/// its place.
#[actix_web::test]
async fn child_spans_sharing_an_id_round_trip() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let token = create_test_token(&pool).await;
    let project = create_test_project(&pool, "Duplicate span ids").await;
    let mut payload = transaction_payload();
    let spans = payload["spans"].as_array_mut().unwrap();
    let mut twin = spans[0].clone();
    twin["description"] = json!("the second one");
    twin["start_timestamp"] = json!(1700000000.5);
    spans.push(twin);
    let txn_id = store(&pool, project.id, &payload).await;
    let app = crate::common::app(&pool).await;

    let body = get(
        &app,
        &token,
        format!("/api/projects/{}/transactions/{txn_id}", project.id),
    )
    .await;
    assert_eq!(body["data"], payload);
}
