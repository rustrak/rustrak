//! `events.data` stored compressed (issue #532): a new row keeps its payload
//! in `payload`, zstd-compressed with the instance's own dictionary once it
//! has one, and reads back exactly as a row in the old format does.

use crate::common::{
    create_test_project, create_test_token, make_legacy, stored_payload as stored, TestDb,
};
use actix_web::test;
use chrono::{DateTime, Utc};
use rustrak::db::payload::{self, Column};
use rustrak::db::DbPool;
use rustrak::services::event::{EventLink, EventRow};
use rustrak::services::grouping::DenormalizedFields;
use rustrak::services::{EventService, IssueService};
use serde_json::{json, Value};
use uuid::Uuid;

const BROWSERS: [&str; 3] = ["Chrome", "Firefox", "Safari"];

/// An error event the way an SDK sends it, varied by `i`.
pub(crate) fn event(i: usize) -> Value {
    json!({
        "event_id": format!("{i:032x}"),
        "timestamp": 1_700_000_000.0 + i as f64,
        "platform": "python",
        "level": "error",
        "release": format!("shop@1.{}.0", i % 7),
        "environment": "production",
        "server_name": "web-1",
        "tags": { "browser": BROWSERS[i % 3], "route": format!("/orders/{i}") },
        "user": { "id": format!("user-{}", i % 50), "email": format!("u{}@example.com", i % 50) },
        "contexts": {
            "os": { "name": "Linux", "version": "6.8" },
            "runtime": { "name": "CPython", "version": "3.12.4" },
            "trace": { "trace_id": format!("{:032x}", i * 7919), "span_id": format!("{:016x}", i) },
        },
        "request": { "method": "GET", "url": format!("https://shop.example.com/orders/{i}"),
                     "headers": { "User-Agent": "Mozilla/5.0", "Accept": "application/json" } },
        "sdk": { "name": "sentry.python", "version": "2.19.0",
                 "integrations": ["django", "logging", "stdlib", "threading"] },
        "exception": { "values": [{
            "type": "KeyError",
            "value": format!("'order_{i}'"),
            "mechanism": { "type": "django", "handled": false },
            "stacktrace": { "frames": (0..8).map(|f| json!({
                "filename": format!("shop/views/orders_{f}.py"),
                "abs_path": format!("/srv/shop/views/orders_{f}.py"),
                "module": format!("shop.views.orders_{f}"),
                "function": format!("handle_{f}"),
                "lineno": 10 + f * 3 + i % 5,
                "in_app": f > 3,
                "context_line": "    order = orders[request.args['id']]",
                "pre_context": ["def handle(request):", "    orders = load()"],
                "post_context": ["    return render(order)"],
            })).collect::<Vec<_>>() }
        }] },
        "breadcrumbs": { "values": (0..10).map(|b| json!({
            "type": "default", "category": "query", "level": "info",
            "message": format!("SELECT * FROM orders WHERE id = {}", i + b),
            "timestamp": 1_700_000_000.0 + (i + b) as f64,
        })).collect::<Vec<_>>() },
    })
}

fn denormalized() -> DenormalizedFields {
    DenormalizedFields {
        calculated_type: "KeyError".into(),
        calculated_value: "'order'".into(),
        transaction: "/orders".into(),
        last_frame_filename: "orders.py".into(),
        last_frame_module: "shop.views".into(),
        last_frame_function: "handle".into(),
        culprit: "handle".into(),
        logger: String::new(),
        release: String::new(),
    }
}

/// An issue and its grouping, with no event yet.
pub(crate) async fn issue(pool: &DbPool, project_id: i32) -> (rustrak::models::Issue, i32) {
    let issue = IssueService::create(
        pool,
        project_id,
        Utc::now(),
        &denormalized(),
        Some("error"),
        Some("python"),
    )
    .await
    .unwrap();
    let grouping: i32 = sqlx::query_scalar(
        "INSERT INTO groupings (project_id, issue_id, grouping_key, grouping_key_hash) \
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(project_id)
    .bind(issue.id)
    .bind(Uuid::new_v4().to_string())
    .bind(format!(
        "{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    ))
    .fetch_one(pool)
    .await
    .unwrap();
    (issue, grouping)
}

async fn create_event(
    pool: &DbPool,
    project_id: i32,
    (issue_id, grouping): (Uuid, i32),
    data: &Value,
    at: DateTime<Utc>,
) -> Uuid {
    EventService::create(
        pool,
        Uuid::new_v4(),
        project_id,
        issue_id,
        grouping,
        data,
        at,
        &denormalized(),
        None,
        None,
        None,
    )
    .await
    .unwrap()
}

/// `count` events through the digest's own batch insert.
pub(crate) async fn insert_events(
    pool: &DbPool,
    project_id: i32,
    (issue_id, grouping): (Uuid, i32),
    from: usize,
    count: usize,
) {
    let encoder = payload::Encoder::current(pool, Column::Events)
        .await
        .unwrap();
    let mut conn = pool.acquire().await.unwrap();
    let all: Vec<usize> = (from..from + count).collect();
    for chunk in all.chunks(250) {
        let rows: Vec<(EventRow, EventLink)> = chunk
            .iter()
            .map(|&i| {
                (
                    EventRow::new(
                        &event(i),
                        Utc::now(),
                        &denormalized(),
                        None,
                        Uuid::new_v4(),
                        project_id,
                        &encoder,
                    ),
                    EventLink {
                        id: Uuid::new_v4(),
                        issue_id,
                        grouping_id: grouping,
                        alert_type: None,
                        issue_event_count: None,
                    },
                )
            })
            .collect();
        EventRow::insert_all(&mut conn, rows.iter().map(|(r, l)| (r, l)))
            .await
            .unwrap();
    }
}

async fn get(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    token: &str,
    uri: &str,
) -> actix_web::web::Bytes {
    let req = test::TestRequest::get()
        .uri(uri)
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let resp = test::call_service(app, req).await;
    assert_eq!(resp.status(), 200, "{uri}");
    test::read_body(resp).await
}

#[actix_web::test]
async fn an_event_is_stored_compressed_and_reads_back_as_an_old_row_does() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let token = create_test_token(&pool).await;
    let project = create_test_project(&pool, "Compressed events").await;
    let (issue, grouping) = issue(&pool, project.id).await;
    let data = event(1);
    let id = create_event(&pool, project.id, (issue.id, grouping), &data, Utc::now()).await;

    let (payload, empty_data) = stored(&pool, "events", id).await;
    let payload = payload.expect("a new row stores its payload");
    let json = serde_json::to_vec(&data).unwrap();
    assert!(empty_data, "a new row also stores its JSON uncompressed");
    assert_eq!(payload[0], 2, "zstd, no dictionary yet");
    assert!(
        payload.len() * 3 < json.len(),
        "{} bytes for {} of JSON",
        payload.len(),
        json.len()
    );
    assert_eq!(payload::decode(&pool, &payload).await.unwrap(), json);

    let app = crate::common::app(&pool).await;
    let uris = [
        format!(
            "/api/projects/{}/issues/{}/events/{id}",
            project.id, issue.id
        ),
        format!("/api/0/issues/{}/events/latest/", issue.numeric_id),
    ];
    let mut new = Vec::new();
    for uri in &uris {
        new.push(get(&app, &token, uri).await);
    }
    let detail: Value = serde_json::from_slice(&new[0]).unwrap();
    assert_eq!(detail["data"], data);

    make_legacy(&pool, "events", id).await;
    for (uri, new) in uris.iter().zip(new) {
        assert_eq!(get(&app, &token, uri).await, new, "{uri}");
    }
}

/// Tags, users and feedback contexts are read from both formats side by side.
#[actix_web::test]
async fn issue_aggregates_read_new_and_old_rows_alike() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let project = create_test_project(&pool, "Aggregates").await;
    let (issue, grouping) = issue(&pool, project.id).await;
    let t = |s: i64| Utc::now() - chrono::Duration::seconds(s);
    let with = |browser: &str, user: &str, feedback: Option<&str>| {
        let mut data = event(0);
        data["tags"] = json!({ "browser": browser });
        data["user"] = json!({ "id": user });
        if let Some(message) = feedback {
            data["contexts"]["feedback"] = json!({ "message": message });
        }
        data
    };
    let a = create_event(
        &pool,
        project.id,
        (issue.id, grouping),
        &with("Chrome", "u1", Some("broken")),
        t(30),
    )
    .await;
    let b = create_event(
        &pool,
        project.id,
        (issue.id, grouping),
        &with("Firefox", "u2", None),
        t(20),
    )
    .await;
    create_event(
        &pool,
        project.id,
        (issue.id, grouping),
        &with("Chrome", "u1", None),
        t(10),
    )
    .await;
    make_legacy(&pool, "events", b).await;
    // The feedback context of an issue whose one event is in the old format.
    let (old_issue, old_grouping) = self::issue(&pool, project.id).await;
    let c = create_event(
        &pool,
        project.id,
        (old_issue.id, old_grouping),
        &with("Edge", "u3", Some("old")),
        t(5),
    )
    .await;
    make_legacy(&pool, "events", c).await;
    assert!(stored(&pool, "events", a).await.0.is_some());

    let aggregates = IssueService::aggregates(&pool, issue.id).await.unwrap();
    assert_eq!(
        serde_json::to_value(&aggregates).unwrap(),
        json!({
            "user_count": 2,
            "tags": [{"key": "browser", "total_values": 3,
                      "top_values": [{"value": "Chrome", "count": 2}, {"value": "Firefox", "count": 1}]}]
        })
    );
    let values = IssueService::tag_values(&pool, issue.id, "browser")
        .await
        .unwrap();
    let got: Vec<(String, i64)> = values.iter().map(|v| (v.value.clone(), v.count)).collect();
    assert_eq!(got, [("Chrome".to_string(), 2), ("Firefox".to_string(), 1)]);
    let contexts = IssueService::feedback_contexts(&pool, &[issue.id, old_issue.id])
        .await
        .unwrap();
    assert_eq!(contexts[&issue.id], json!({ "message": "broken" }));
    assert_eq!(contexts[&old_issue.id], json!({ "message": "old" }));
}

/// Postgres estimates a table's rows from its statistics, which autovacuum
/// keeps; a test refreshes them itself.
async fn analyze(pool: &DbPool) {
    #[cfg(feature = "postgres")]
    sqlx::query("ANALYZE events").execute(pool).await.unwrap();
    #[cfg(not(feature = "postgres"))]
    let _ = pool;
}

async fn dictionaries(pool: &DbPool) -> Vec<i64> {
    sqlx::query_scalar(
        "SELECT id FROM payload_dictionaries WHERE kind = 'events' ORDER BY row_count",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn newest_event(pool: &DbPool, project_id: i32) -> Uuid {
    sqlx::query_scalar(
        "SELECT id FROM events WHERE project_id = $1 ORDER BY ingested_at DESC LIMIT 1",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// No dictionary below 500 events; one trained from them at 500, which the
/// rows after it use without a restart (and another database does not); a
/// second once the table has grown tenfold. Every row
/// decodes whichever dictionary it was made with, in a registry that starts
/// empty as a new process's does.
#[actix_web::test]
async fn events_get_a_dictionary_of_their_own_and_a_new_one_at_ten_times_the_rows() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let token = create_test_token(&pool).await;
    let project = create_test_project(&pool, "Dictionaries").await;
    let (issue, grouping) = issue(&pool, project.id).await;
    let at = |i: i64| Utc::now() + chrono::Duration::seconds(i);

    let first = create_event(&pool, project.id, (issue.id, grouping), &event(0), at(0)).await;
    insert_events(&pool, project.id, (issue.id, grouping), 1, 498).await;
    analyze(&pool).await;
    payload::train_due(&pool).await.unwrap();
    assert!(
        dictionaries(&pool).await.is_empty(),
        "trained below 500 rows"
    );

    insert_events(&pool, project.id, (issue.id, grouping), 499, 1).await;
    analyze(&pool).await;
    // One row whose header is in no known format is left out of the sample;
    // it does not stop the training.
    let whole: Vec<u8> = sqlx::query_scalar("SELECT payload FROM events WHERE id = $1")
        .bind(first)
        .fetch_one(&pool)
        .await
        .unwrap();
    let set_payload = |bytes: Vec<u8>| {
        sqlx::query("UPDATE events SET payload = $1 WHERE id = $2")
            .bind(bytes)
            .bind(first)
            .execute(&pool)
    };
    set_payload(vec![9]).await.unwrap();
    let trained = payload::train_due(&pool).await.unwrap();
    set_payload(whole).await.unwrap();
    let ids = dictionaries(&pool).await;
    assert_eq!(ids.len(), 1);
    assert_eq!(trained, [(Column::Events, ids[0] as u32)]);

    let data = event(10_000);
    let second = create_event(&pool, project.id, (issue.id, grouping), &data, at(1)).await;
    let (with_dictionary, _) = stored(&pool, "events", second).await;
    let with_dictionary = with_dictionary.unwrap();
    assert_eq!(
        payload::dictionary_id(&with_dictionary).unwrap(),
        Some(ids[0] as u32)
    );
    // Another database in the same process has no dictionary of its own.
    let other = TestDb::new().await;
    let other_project = create_test_project(&other.pool, "Elsewhere").await;
    let (other_issue, other_grouping) = self::issue(&other.pool, other_project.id).await;
    let elsewhere = create_event(
        &other.pool,
        other_project.id,
        (other_issue.id, other_grouping),
        &data,
        at(1),
    )
    .await;
    let (elsewhere, _) = stored(&other.pool, "events", elsewhere).await;
    assert_eq!(elsewhere.unwrap()[0], 2, "zstd without a dictionary");

    let plain = payload::encode(&serde_json::to_vec(&data).unwrap(), None);
    assert!(
        with_dictionary.len() * 2 < plain.len(),
        "{} bytes with the dictionary, {} without",
        with_dictionary.len(),
        plain.len()
    );

    insert_events(&pool, project.id, (issue.id, grouping), 500, 4_500).await;
    analyze(&pool).await;
    let trained = payload::train_due(&pool).await.unwrap();
    let ids = dictionaries(&pool).await;
    assert_eq!(ids.len(), 2, "retrained at ten times the rows");
    assert_eq!(trained, [(Column::Events, ids[1] as u32)]);
    assert!(payload::train_due(&pool).await.unwrap().is_empty());
    let third = create_event(
        &pool,
        project.id,
        (issue.id, grouping),
        &event(20_000),
        at(2),
    )
    .await;
    assert_eq!(newest_event(&pool, project.id).await, third);
    let (newest, _) = stored(&pool, "events", third).await;
    assert_eq!(
        payload::dictionary_id(&newest.unwrap()).unwrap(),
        Some(ids[1] as u32)
    );

    payload::forget_dictionaries();
    let app = crate::common::app(&pool).await;
    for (id, data) in [(first, event(0)), (second, data), (third, event(20_000))] {
        let body = get(
            &app,
            &token,
            &format!(
                "/api/projects/{}/issues/{}/events/{id}",
                project.id, issue.id
            ),
        )
        .await;
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["data"], data);
    }
}

/// The storage page's size estimate counts the bytes stored, not the JSON.
#[actix_web::test]
async fn the_size_estimate_counts_compressed_bytes() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let project = create_test_project(&pool, "Sizes").await;
    let (issue, grouping) = issue(&pool, project.id).await;
    let id = create_event(
        &pool,
        project.id,
        (issue.id, grouping),
        &event(1),
        Utc::now(),
    )
    .await;
    let (payload, _) = stored(&pool, "events", id).await;

    let projects = rustrak::services::StorageService::by_project(&pool)
        .await
        .unwrap();
    let ours = projects
        .iter()
        .find(|p| p.project_id == project.id)
        .unwrap();
    assert_eq!(ours.events_bytes, payload.unwrap().len() as i64);
}

/// A payload no reader can decode: a format byte that does not exist.
async fn corrupt(pool: &DbPool, id: Uuid) {
    sqlx::query("UPDATE events SET payload = $1 WHERE id = $2")
        .bind(vec![9u8, 9])
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
}

/// One event whose payload cannot be read leaves its issue's aggregates, tag
/// values and feedback readable without it; the event alone is an error.
#[actix_web::test]
async fn an_unreadable_event_is_left_out_of_the_aggregates() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let token = create_test_token(&pool).await;
    let project = create_test_project(&pool, "Unreadable event").await;
    let (issue, grouping) = issue(&pool, project.id).await;
    let mut readable = event(0);
    readable["contexts"]["feedback"] = json!({ "message": "kept" });
    let t = |s: i64| Utc::now() - chrono::Duration::seconds(s);
    create_event(&pool, project.id, (issue.id, grouping), &readable, t(20)).await;
    let bad = create_event(&pool, project.id, (issue.id, grouping), &event(1), t(10)).await;
    corrupt(&pool, bad).await;

    let aggregates = IssueService::aggregates(&pool, issue.id).await.unwrap();
    assert_eq!(aggregates.user_count, 1);
    let values = IssueService::tag_values(&pool, issue.id, "browser")
        .await
        .unwrap();
    let got: Vec<(String, i64)> = values.iter().map(|v| (v.value.clone(), v.count)).collect();
    assert_eq!(got, [("Chrome".to_string(), 1)]);
    let contexts = IssueService::feedback_contexts(&pool, &[issue.id])
        .await
        .unwrap();
    assert_eq!(contexts[&issue.id], json!({ "message": "kept" }));

    let app = crate::common::app(&pool).await;
    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{}/issues/{}/events/{bad}",
            project.id, issue.id
        ))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 500);
}

/// The aggregates decode each event as it streams in, on the one connection
/// the stream holds: a dictionary they need is loaded before, so a pool of
/// one connection does not wait on itself.
#[cfg(feature = "sqlite")]
#[actix_web::test]
async fn aggregates_decode_dictionary_rows_on_a_pool_of_one_connection() {
    let dir = tempfile::tempdir().unwrap();
    let config = rustrak::config::DatabaseConfig {
        url: format!("sqlite://{}?mode=rwc", dir.path().join("one.db").display()),
        max_connections: 1,
        min_connections: 1,
        acquire_timeout: std::time::Duration::from_secs(5),
        idle_timeout: std::time::Duration::from_secs(60),
        max_lifetime: std::time::Duration::from_secs(300),
    };
    let pool = rustrak::db::create_pool(&config).await.unwrap();
    rustrak::db::run_migrations(&pool).await.unwrap();
    let project = create_test_project(&pool, "One connection").await;
    let (issue, grouping) = issue(&pool, project.id).await;
    insert_events(&pool, project.id, (issue.id, grouping), 0, 500).await;
    assert_eq!(payload::train_due(&pool).await.unwrap().len(), 1);
    let mut data = event(9);
    data["user"] = json!({ "id": "only-in-the-dictionary-row" });
    let id = create_event(&pool, project.id, (issue.id, grouping), &data, Utc::now()).await;
    let (stored, _) = stored(&pool, "events", id).await;
    assert!(payload::dictionary_id(&stored.unwrap()).unwrap().is_some());

    payload::forget_dictionaries();
    let aggregates = IssueService::aggregates(&pool, issue.id).await.unwrap();
    assert_eq!(aggregates.user_count, 51, "the dictionary row is counted");
}

/// A retraining that comes up with a dictionary already stored (the same
/// samples train the same bytes, and its id is their hash) still records the
/// rows it was trained at, so it is not due again ten minutes later.
#[actix_web::test]
async fn a_retraining_that_yields_a_stored_dictionary_is_not_due_again() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let project = create_test_project(&pool, "Same dictionary").await;
    let (issue, grouping) = issue(&pool, project.id).await;
    insert_events(&pool, project.id, (issue.id, grouping), 0, 500).await;
    analyze(&pool).await;
    assert_eq!(payload::train_due(&pool).await.unwrap().len(), 1);

    // Due again over the same rows, as if ten times the rows had been
    // written since: it trains the same dictionary.
    sqlx::query("UPDATE payload_dictionaries SET row_count = row_count - 4500")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(payload::train_due(&pool).await.unwrap().len(), 1);
    assert_eq!(dictionaries(&pool).await.len(), 1, "the same dictionary");
    assert!(
        payload::train_due(&pool).await.unwrap().is_empty(),
        "trained again at the same rows"
    );
}

/// A dictionary's id hashes its content alone, not its entropy tables
/// (`ZDICT_finalizeDictionary`), so a retraining can name a stored id with
/// other bytes. The rows after it are compressed with the stored bytes, the
/// ones every reader decodes them with.
#[actix_web::test]
async fn a_retraining_under_a_stored_id_compresses_with_the_stored_bytes() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let project = create_test_project(&pool, "Stored bytes").await;
    let (issue, grouping) = issue(&pool, project.id).await;
    // Rows no other test trains on: the decoder registry is keyed by id
    // across the process, and this id is about to name other bytes.
    insert_events(&pool, project.id, (issue.id, grouping), 90_000, 500).await;
    analyze(&pool).await;
    assert_eq!(payload::train_due(&pool).await.unwrap().len(), 1);

    // The id now names bytes this training does not produce, and it is due
    // again over the same rows.
    sqlx::query("UPDATE payload_dictionaries SET dictionary = $1, row_count = row_count - 4500")
        .bind(b"raw dictionary content the training never made, ".repeat(32))
        .execute(&pool)
        .await
        .unwrap();
    payload::forget_dictionaries();
    assert_eq!(payload::train_due(&pool).await.unwrap().len(), 1);

    let data = event(10_000);
    let id = create_event(&pool, project.id, (issue.id, grouping), &data, Utc::now()).await;
    let (row, _) = stored(&pool, "events", id).await;
    payload::forget_dictionaries();
    let json = payload::decode(&pool, &row.unwrap()).await.unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&json).unwrap(), data);
}

/// Turns every event of `issue_id` into the format rows had before
/// compression, as an in-place upgrade finds them.
async fn make_issue_legacy(pool: &DbPool, issue_id: Uuid) {
    sqlx::query("UPDATE events SET data = $1, payload = NULL WHERE issue_id = $2")
        .bind(sqlx::types::Json(event(0)))
        .bind(issue_id)
        .execute(pool)
        .await
        .unwrap();
}

/// The dictionary `train_due` trains for events on a database holding only
/// `count` new rows from `from`.
async fn fresh_dictionary(from: usize, count: usize) -> u32 {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Fresh").await;
    let (issue, grouping) = issue(&db.pool, project.id).await;
    insert_events(&db.pool, project.id, (issue.id, grouping), from, count).await;
    analyze(&db.pool).await;
    let trained = payload::train_due(&db.pool).await.unwrap();
    assert_eq!(trained.len(), 1);
    trained[0].1
}

/// After an in-place upgrade the table is mostly rows in the old format. The
/// first dictionary waits for 500 rows in the new one and learns only from
/// them: it is the very dictionary a database holding just those rows trains.
#[actix_web::test]
async fn after_an_upgrade_the_first_dictionary_waits_for_new_rows_and_learns_only_them() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let project = create_test_project(&pool, "Upgraded").await;
    let (old, old_grouping) = issue(&pool, project.id).await;
    insert_events(&pool, project.id, (old.id, old_grouping), 0, 2_000).await;
    make_issue_legacy(&pool, old.id).await;
    // The sample is read in page order and its order shapes the dictionary.
    // The UPDATE above leaves dead rows whose space pruning or autovacuum
    // frees at random times, and a new row placed there would be read out
    // of order: a different dictionary from the same rows. Rewritten, the
    // table has room only at its end, as the fresh one does.
    #[cfg(feature = "postgres")]
    sqlx::query("VACUUM FULL events")
        .execute(&pool)
        .await
        .unwrap();
    let (new, grouping) = issue(&pool, project.id).await;
    insert_events(&pool, project.id, (new.id, grouping), 2_000, 499).await;
    analyze(&pool).await;
    assert!(
        payload::train_due(&pool).await.unwrap().is_empty(),
        "trained with 499 rows in the new format"
    );
    assert!(dictionaries(&pool).await.is_empty());

    insert_events(&pool, project.id, (new.id, grouping), 2_499, 1).await;
    analyze(&pool).await;
    let trained = payload::train_due(&pool).await.unwrap();
    assert_eq!(trained.len(), 1);
    assert_eq!(
        trained[0].1,
        fresh_dictionary(2_000, 500).await,
        "learnt from rows in the old format"
    );
}

/// The old rows of an upgraded table never grow: a new dictionary is due
/// once the rows in the new format have grown tenfold, not the whole table.
#[actix_web::test]
async fn after_an_upgrade_a_new_dictionary_is_due_at_ten_times_the_new_rows() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let project = create_test_project(&pool, "Upgraded growth").await;
    let (old, old_grouping) = issue(&pool, project.id).await;
    insert_events(&pool, project.id, (old.id, old_grouping), 0, 2_000).await;
    make_issue_legacy(&pool, old.id).await;
    let (new, grouping) = issue(&pool, project.id).await;
    insert_events(&pool, project.id, (new.id, grouping), 2_000, 500).await;
    analyze(&pool).await;
    assert_eq!(payload::train_due(&pool).await.unwrap().len(), 1);

    insert_events(&pool, project.id, (new.id, grouping), 2_500, 4_499).await;
    analyze(&pool).await;
    assert!(
        payload::train_due(&pool).await.unwrap().is_empty(),
        "due before the new rows grew tenfold"
    );
    insert_events(&pool, project.id, (new.id, grouping), 6_999, 1).await;
    analyze(&pool).await;
    assert_eq!(
        payload::train_due(&pool).await.unwrap().len(),
        1,
        "not due at ten times the new rows"
    );
}

/// A training reads a bounded part of the table, however many old rows it
/// holds: on SQLite its newest rows, on Postgres 16 MB of its pages, which
/// these 20,000 old rows outgrow. The 500 new-format rows written before
/// them are never all reached (a whole-table search would train on them).
#[actix_web::test]
async fn a_table_of_old_rows_trains_nothing_and_is_read_only_in_part() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let project = create_test_project(&pool, "Old rows").await;
    let (new, grouping) = issue(&pool, project.id).await;
    insert_events(&pool, project.id, (new.id, grouping), 0, 500).await;
    let (old, old_grouping) = issue(&pool, project.id).await;
    insert_events(&pool, project.id, (old.id, old_grouping), 500, 20_000).await;
    make_issue_legacy(&pool, old.id).await;
    analyze(&pool).await;

    assert!(payload::train_due(&pool).await.unwrap().is_empty());
    assert!(dictionaries(&pool).await.is_empty());
}

/// The size estimate of a table mixing old rows (JSON) and new ones
/// (compressed) samples both as the table holds them, the same rows every
/// time: not the newest rows only, nor whichever a plan yields first.
#[actix_web::test]
async fn the_size_estimate_of_old_and_new_rows_is_stable_and_near_the_stored_bytes() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let project = create_test_project(&pool, "Mixed sizes").await;
    let (old, old_grouping) = issue(&pool, project.id).await;
    insert_events(&pool, project.id, (old.id, old_grouping), 0, 300).await;
    make_issue_legacy(&pool, old.id).await;
    let (new, grouping) = issue(&pool, project.id).await;
    insert_events(&pool, project.id, (new.id, grouping), 300, 300).await;
    // Standalone spans, old ones first, each in a trace of its own.
    for i in 0..600u128 {
        let span = json!({ "op": "db",
                           "description": format!("SELECT id, total FROM orders WHERE id = {i} ").repeat(20),
                           "data": { "db.system": "postgresql", "thread.name": format!("worker-{i}") } });
        let insert = sqlx::query(
            "INSERT INTO spans (id, project_id, span_id, trace_id, op, data, payload) \
             VALUES ($1, $2, $3, $4, 'db', $5, $6)",
        )
        .bind(Uuid::new_v4())
        .bind(project.id)
        .bind(format!("{:016x}", i))
        .bind(format!(
            "{:032x}",
            i.wrapping_mul(0x9e37_79b9_7f4a_7c15_f39c_c060_5ced_c835)
        ));
        if i < 300 {
            insert.bind(sqlx::types::Json(&span)).bind(None::<Vec<u8>>)
        } else {
            insert
                .bind(payload::NO_DATA)
                .bind(Some(payload::encode(span.to_string().as_bytes(), None)))
        }
        .execute(&pool)
        .await
        .unwrap();
    }
    #[cfg(feature = "postgres")]
    sqlx::query("ANALYZE spans").execute(&pool).await.unwrap();

    let stored = |table: &str| {
        sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(format!(
            "SELECT CAST(SUM(CASE WHEN payload IS NULL THEN length(CAST(data AS TEXT)) \
             ELSE length(payload) END) AS BIGINT) FROM {table} WHERE project_id = $1"
        )))
        .bind(project.id)
        .fetch_one(&pool)
    };
    let (events, spans) = (
        stored("events").await.unwrap(),
        stored("spans").await.unwrap(),
    );

    let estimate = |pool: DbPool| async move {
        let projects = rustrak::services::StorageService::by_project(&pool)
            .await
            .unwrap();
        let ours = projects
            .into_iter()
            .find(|p| p.project_id == project.id)
            .unwrap();
        (ours.events_bytes, ours.transactions_bytes)
    };
    let first = estimate(pool.clone()).await;
    for _ in 0..3 {
        assert_eq!(
            estimate(pool.clone()).await,
            first,
            "the same rows, another estimate"
        );
    }
    // Another plan, as after a restart with other statistics: no index.
    #[cfg(feature = "postgres")]
    {
        sqlx::query(
            "DO $$ BEGIN \
             EXECUTE format('ALTER DATABASE %I SET enable_indexscan = off', current_database()); \
             EXECUTE format('ALTER DATABASE %I SET enable_bitmapscan = off', current_database()); \
             END $$",
        )
        .execute(&pool)
        .await
        .unwrap();
        let restarted = sqlx::postgres::PgPoolOptions::new()
            .connect_with((*pool.connect_options()).clone())
            .await
            .unwrap();
        assert_eq!(
            estimate(restarted).await,
            first,
            "another plan, another estimate"
        );
    }
    for (what, estimated, stored) in [("events", first.0, events), ("spans", first.1, spans)] {
        let off = (estimated - stored).abs() as f64 / stored as f64;
        assert!(off < 0.2, "{what}: estimated {estimated}, stored {stored}");
    }
}

/// Rolling payload compression back would drop every compressed payload
/// with its column, and SQL cannot decompress them into `data`: it refuses
/// while any exists, and the payloads stay.
#[actix_web::test]
async fn rolling_payload_compression_back_refuses_while_payloads_exist() {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    let project = create_test_project(&pool, "Rollback").await;
    let (issue, grouping) = issue(&pool, project.id).await;
    let id = create_event(
        &pool,
        project.id,
        (issue.id, grouping),
        &event(1),
        Utc::now(),
    )
    .await;

    #[cfg(feature = "postgres")]
    let migrator = sqlx::migrate!("./migrations/postgres");
    #[cfg(not(feature = "postgres"))]
    let migrator = sqlx::migrate!("./migrations/sqlite");
    // Every migration of issue #532 after the index one.
    let err = migrator.undo(&pool, 20261116000000).await.unwrap_err();
    assert!(err.to_string().contains("payload_compression"), "{err}");
    assert!(stored(&pool, "events", id).await.0.is_some());
}
