//! A span's description stored once per project (#558). On a PHP app the SQL
//! text is about half of a database span's row and has a few hundred distinct
//! values, so a description seen again is kept in `span_descriptions` and the
//! span only refers to it. Sentry keeps the raw description as well
//! (`sentry.raw_description`): every reader gets the same text as before.

use crate::common::{create_test_project, create_test_token, TestDb};
use actix_web::test;
use chrono::Utc;
use rustrak::db::DbPool;
use rustrak::digest::processors::{
    ProcessorCtx, SpanProcessor, SpanV2Processor, TransactionProcessor,
};
use rustrak::services::StorageService;
use serde_json::{json, Value};
use uuid::Uuid;

/// What Doctrine writes for one entity: long, and the same on every call.
const SQL: &str =
    "SELECT t0.id AS id_1, t0.name AS name_2, t0.email AS email_3, t0.status AS status_4, \
                   t0.created_at AS created_at_5 FROM app_customer t0 WHERE t0.id = ?";

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

/// One transaction whose only span has `description`.
async fn transaction(pool: &DbPool, project_id: i32, description: &str) -> Uuid {
    let now = Utc::now().timestamp() as f64;
    let trace = Uuid::new_v4().simple().to_string();
    let payload = json!({
        "type": "transaction", "transaction": "GET /customers",
        "start_timestamp": now, "timestamp": now + 1.0,
        "contexts": { "trace": { "trace_id": trace, "span_id": "aaaaaaaaaaaaaaaa", "op": "http.server" } },
        "spans": [{
            "span_id": "bbbbbbbbbbbbbbbb", "parent_span_id": "aaaaaaaaaaaaaaaa", "trace_id": trace,
            "op": "db.sql.query", "description": description,
            "start_timestamp": now, "timestamp": now + 0.1,
        }],
    });
    TransactionProcessor
        .process(
            serde_json::to_vec(&payload).unwrap().into(),
            &ctx(pool, project_id),
        )
        .await
        .unwrap();
    sqlx::query_scalar("SELECT id FROM transactions WHERE project_id = $1 AND trace_id = $2")
        .bind(project_id)
        .bind(trace)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// `(kept in the row, refers to a stored description)` for each span of the
/// project, oldest first.
async fn stored(pool: &DbPool, project_id: i32) -> Vec<(bool, bool)> {
    sqlx::query_as::<_, (Option<String>, Option<i64>)>(
        "SELECT description, description_id FROM spans WHERE project_id = $1 \
         ORDER BY start_timestamp, id",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
    .unwrap()
    .into_iter()
    .map(|(text, id)| (text.is_some(), id.is_some()))
    .collect()
}

async fn descriptions(pool: &DbPool, project_id: i32) -> Vec<String> {
    sqlx::query_scalar("SELECT text FROM span_descriptions WHERE project_id = $1 ORDER BY id")
        .bind(project_id)
        .fetch_all(pool)
        .await
        .unwrap()
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

#[actix_web::test]
async fn a_description_seen_again_is_stored_once_and_read_as_before() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Repeats").await;
    let mut transactions = Vec::new();
    for _ in 0..4 {
        transactions.push(transaction(&db.pool, project.id, SQL).await);
    }

    // The first sight stays in its row: a description nobody sends again
    // (a URL with an id, SQL with its values inlined) never gets a row of
    // its own. From the second on, the spans refer to the one copy.
    assert_eq!(descriptions(&db.pool, project.id).await, [SQL]);
    let rows = stored(&db.pool, project.id).await;
    assert_eq!(rows.iter().filter(|(inline, _)| *inline).count(), 1);
    assert_eq!(rows.iter().filter(|(_, stored)| *stored).count(), 3);
    assert!(rows.iter().all(|(inline, stored)| inline != stored));

    // Every reader gets the text, whichever way its span keeps it.
    let p = format!("/api/projects/{}", project.id);
    let list = get(&db.pool, &token, format!("{p}/spans")).await;
    let items = list["items"].as_array().unwrap();
    assert_eq!(items.len(), 4);
    for item in items {
        assert_eq!(item["description"], SQL, "span list");
        let id = item["id"].as_str().unwrap();
        let detail = get(&db.pool, &token, format!("{p}/spans/{id}")).await;
        assert_eq!(detail["description"], SQL, "span detail");
    }
    for id in transactions {
        let detail = get(&db.pool, &token, format!("{p}/transactions/{id}")).await;
        assert_eq!(
            detail["data"]["spans"][0]["description"], SQL,
            "the waterfall"
        );
        let spans = get(&db.pool, &token, format!("{p}/transactions/{id}/spans")).await;
        assert_eq!(spans[0]["description"], SQL, "a transaction's spans");
    }
}

#[actix_web::test]
async fn short_and_one_off_descriptions_stay_in_their_rows() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "One-offs").await;
    // Short: a reference would save next to nothing.
    for _ in 0..3 {
        transaction(&db.pool, project.id, "GET /api/v1/users/42").await;
    }
    // Long, never the same twice.
    for n in 0..3 {
        transaction(&db.pool, project.id, &format!("{SQL} /* request {n} */")).await;
    }

    assert!(descriptions(&db.pool, project.id).await.is_empty());
    assert!(stored(&db.pool, project.id)
        .await
        .iter()
        .all(|(inline, stored)| *inline && !*stored));
}

#[actix_web::test]
async fn span_items_share_descriptions_with_transactions_and_projects_do_not() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Mixed").await;
    let other = create_test_project(&db.pool, "Other").await;
    let now = Utc::now().timestamp() as f64;
    let item = |n: usize| {
        serde_json::to_vec(&json!({
            "span_id": format!("{n:016x}"), "trace_id": "cccccccccccccccccccccccccccccccc",
            "op": "db.sql.query", "description": SQL,
            "start_timestamp": now, "timestamp": now + 0.1,
        }))
        .unwrap()
        .into()
    };
    transaction(&db.pool, project.id, SQL).await;
    SpanProcessor
        .process_batch(vec![item(1), item(2)], &ctx(&db.pool, project.id))
        .await
        .unwrap();
    transaction(&db.pool, other.id, SQL).await;
    transaction(&db.pool, other.id, SQL).await;

    assert_eq!(descriptions(&db.pool, project.id).await, [SQL]);
    assert_eq!(descriptions(&db.pool, other.id).await, [SQL]);
    assert_eq!(
        stored(&db.pool, project.id)
            .await
            .iter()
            .filter(|(_, stored)| *stored)
            .count(),
        2
    );
}

#[actix_web::test]
async fn a_project_at_its_limit_keeps_new_descriptions_in_their_rows() {
    // The limit is what bounds the table when an app's long descriptions
    // repeat but never settle (SQL with a few inlined values, say).
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Full").await;
    let limit = rustrak::services::span_description::MAX_PER_PROJECT;
    // A thousand rows a statement: SQLite takes 32,766 binds.
    for batch in 0..limit / 1_000 {
        let mut insert = sqlx::QueryBuilder::<rustrak::db::Db>::new(
            "INSERT INTO span_descriptions (project_id, hash, text, last_used_at) ",
        );
        insert.push_values(batch * 1_000..(batch + 1) * 1_000, |mut row, n| {
            row.push_bind(project.id)
                .push_bind(n.to_be_bytes().to_vec())
                .push_bind(format!("filler {n}"))
                .push_bind(Utc::now());
        });
        insert.build().execute(&db.pool).await.unwrap();
    }

    for _ in 0..3 {
        transaction(&db.pool, project.id, SQL).await;
    }

    assert!(!descriptions(&db.pool, project.id)
        .await
        .contains(&SQL.to_string()));
    assert!(stored(&db.pool, project.id)
        .await
        .iter()
        .all(|(inline, stored)| *inline && !*stored));
}

#[actix_web::test]
async fn descriptions_no_span_uses_any_more_are_removed() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Aged out").await;
    for _ in 0..3 {
        transaction(&db.pool, project.id, SQL).await;
    }
    let long_ago = Utc::now() - chrono::Duration::days(30);
    let age = |pool: DbPool| async move {
        sqlx::query("UPDATE span_descriptions SET last_used_at = $1")
            .bind(long_ago)
            .execute(&pool)
            .await
            .unwrap();
    };

    // Old, and still referred to: kept.
    age(db.pool.clone()).await;
    assert_eq!(
        StorageService::gc_span_descriptions(&db.pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(descriptions(&db.pool, project.id).await, [SQL]);

    // Its spans gone, as retention leaves them, but used a moment ago: kept,
    // since a span that refers to it may be on its way in.
    sqlx::query("DELETE FROM spans")
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE span_descriptions SET last_used_at = $1")
        .bind(Utc::now())
        .execute(&db.pool)
        .await
        .unwrap();
    assert_eq!(
        StorageService::gc_span_descriptions(&db.pool)
            .await
            .unwrap(),
        0
    );

    // Old and referred to by nothing: removed, and only that one. Another,
    // as unused but stored a moment ago, stays.
    age(db.pool.clone()).await;
    let recent = format!("{SQL} ORDER BY t0.name");
    sqlx::query(
        "INSERT INTO span_descriptions (project_id, hash, text, last_used_at) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(project.id)
    .bind(vec![7u8; 16])
    .bind(&recent)
    .bind(Utc::now())
    .execute(&db.pool)
    .await
    .unwrap();
    assert_eq!(
        StorageService::gc_span_descriptions(&db.pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        descriptions(&db.pool, project.id).await,
        std::slice::from_ref(&recent)
    );

    // The same description arriving afterwards is stored again, and read.
    for _ in 0..3 {
        transaction(&db.pool, project.id, SQL).await;
    }
    assert_eq!(
        descriptions(&db.pool, project.id).await,
        [recent, SQL.to_string()]
    );
    let list = get(
        &db.pool,
        &token,
        format!("/api/projects/{}/spans", project.id),
    )
    .await;
    for item in list["items"].as_array().unwrap() {
        assert_eq!(item["description"], SQL);
    }
}

#[actix_web::test]
async fn a_restart_finds_the_descriptions_already_stored() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Restarted").await;
    for _ in 0..3 {
        transaction(&db.pool, project.id, SQL).await;
    }
    // What a new process starts with: nothing remembered.
    rustrak::services::span_description::forget(&db.pool);
    for _ in 0..3 {
        transaction(&db.pool, project.id, SQL).await;
    }

    assert_eq!(descriptions(&db.pool, project.id).await, [SQL], "one copy");
    let referring = stored(&db.pool, project.id)
        .await
        .iter()
        .filter(|(_, stored)| *stored)
        .count();
    // Two before the restart, and after it every one but the first sight.
    assert_eq!(referring, 4);
}

#[actix_web::test]
async fn streamed_spans_keep_their_names_once_too() {
    // A v2 span's `name` is what its description is.
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Streamed").await;
    let now = Utc::now().timestamp() as f64;
    for batch in 0..3 {
        let container = json!({ "version": 2, "items": [{
            "trace_id": "dddddddddddddddddddddddddddddddd",
            "span_id": format!("{batch:016x}"),
            "name": SQL,
            "status": "ok",
            "start_timestamp": now, "end_timestamp": now + 0.1,
            "attributes": { "sentry.op": { "value": "db", "type": "string" } },
        }]});
        SpanV2Processor
            .process(
                serde_json::to_vec(&container).unwrap().into(),
                &ctx(&db.pool, project.id),
            )
            .await
            .unwrap();
    }

    assert_eq!(descriptions(&db.pool, project.id).await, [SQL]);
    let list = get(
        &db.pool,
        &token,
        format!("/api/projects/{}/spans", project.id),
    )
    .await;
    let items = list["items"].as_array().unwrap();
    assert_eq!(items.len(), 3);
    assert!(items.iter().all(|item| item["description"] == SQL));
}

#[actix_web::test]
async fn a_project_with_agents_off_stores_no_description_of_an_ai_span() {
    // Its AI spans are not stored, and neither is anything they said.
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Agents off").await;
    let now = Utc::now().timestamp() as f64;
    let prompt = format!(
        "chat gpt-4o: {}",
        "summarise the customer's last order ".repeat(3)
    );
    let off = ProcessorCtx {
        keep_ai_spans: false,
        ..ctx(&db.pool, project.id)
    };
    for n in 0..3 {
        let item = json!({
            "span_id": format!("{n:016x}"), "trace_id": "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
            "op": "gen_ai.chat", "description": prompt,
            "data": { "gen_ai.operation.name": "chat" },
            "start_timestamp": now, "timestamp": now + 0.1,
        });
        SpanProcessor
            .process_batch(vec![serde_json::to_vec(&item).unwrap().into()], &off)
            .await
            .unwrap();
    }
    // An ordinary span of the same project still is.
    for _ in 0..2 {
        transaction(&db.pool, project.id, SQL).await;
    }

    assert_eq!(descriptions(&db.pool, project.id).await, [SQL]);
    assert_eq!(stored(&db.pool, project.id).await.len(), 2);
}
