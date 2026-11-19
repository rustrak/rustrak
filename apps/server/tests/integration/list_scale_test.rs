//! The span and transaction lists on a table too large to count (#557): the
//! total stops at a cap, as Sentry's `count_hits` does, the span list takes a
//! window, and every page is still reachable.

use crate::common::{create_test_project, create_test_token, TestDb};
use actix_web::test;
use chrono::{DateTime, Duration, Utc};
use rustrak::db::DbPool;
use rustrak::digest::processors::{ProcessorCtx, SpanProcessor};
use rustrak::pagination::MAX_HITS;
use serde_json::{json, Value};
use uuid::Uuid;

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

/// `count` spans of one trace, the first the oldest, a second apart, the
/// newest starting at `newest`.
async fn spans(pool: &DbPool, project_id: i32, count: usize, newest: DateTime<Utc>) {
    let ctx = ProcessorCtx {
        pool: pool.clone(),
        project_id,
        event_id: Uuid::nil(),
        ingested_at: Utc::now(),
        remote_addr: None,
        keep_ai_spans: true,
    };
    let newest = newest.timestamp() as f64;
    let items = (0..count)
        .map(|n| {
            let start = newest - (count - 1 - n) as f64;
            serde_json::to_vec(&json!({
                "span_id": format!("{n:016x}"),
                "trace_id": format!("{:032x}", newest as u64),
                "op": "db.query",
                "start_timestamp": start,
                "timestamp": start + 0.5,
            }))
            .unwrap()
            .into()
        })
        .collect();
    SpanProcessor.process_batch(items, &ctx).await.unwrap();
}

async fn transaction(pool: &DbPool, project_id: i32, duration_ms: Option<f64>) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO transactions (id, event_id, project_id, transaction_name, op, status, \
         start_timestamp, timestamp, duration_ms, data) \
         VALUES ($1, $2, $3, 'GET /items', 'http.server', 'ok', $4, $4, $5, '{}')",
    )
    .bind(id)
    .bind(Uuid::new_v4())
    .bind(project_id)
    .bind(Utc::now())
    .bind(duration_ms)
    .execute(pool)
    .await
    .unwrap();
    id
}

#[actix_web::test]
async fn a_span_list_past_the_cap_reports_a_capped_total() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Capped spans").await;
    let cap = MAX_HITS as usize;
    spans(&db.pool, project.id, cap + 5, Utc::now()).await;

    let base = format!("/api/projects/{}/spans", project.id);
    let first = get(&db.pool, &token, base.clone()).await;
    assert_eq!(first["total_count"], MAX_HITS);
    assert_eq!(first["total_capped"], true);
    assert_eq!(first["total_pages"], MAX_HITS / 20);
    assert_eq!(first["items"].as_array().unwrap().len(), 20);
    // Newest first, as before.
    assert_eq!(first["items"][0]["span_id"], format!("{:016x}", cap + 4));

    // The rows past the cap are not counted, and are still there to read.
    let past = get(
        &db.pool,
        &token,
        format!("{base}?page={}", MAX_HITS / 20 + 1),
    )
    .await;
    let items = past["items"].as_array().unwrap();
    assert_eq!(items.len(), 5);
    assert_eq!(items[4]["span_id"], format!("{:016x}", 0));
}

#[actix_web::test]
async fn a_span_list_under_the_cap_reports_its_exact_total() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Few spans").await;
    spans(&db.pool, project.id, 3, Utc::now()).await;

    let body = get(
        &db.pool,
        &token,
        format!("/api/projects/{}/spans", project.id),
    )
    .await;
    assert_eq!(body["total_count"], 3);
    assert_eq!(body["total_capped"], false);
    assert_eq!(body["total_pages"], 1);
}

#[actix_web::test]
async fn a_span_list_keeps_to_its_period() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Windowed spans").await;
    spans(&db.pool, project.id, 2, Utc::now()).await;
    spans(&db.pool, project.id, 3, Utc::now() - Duration::days(3)).await;

    let base = format!("/api/projects/{}/spans", project.id);
    let day = get(&db.pool, &token, format!("{base}?period=24h")).await;
    assert_eq!(day["total_count"], 2);
    assert_eq!(day["items"].as_array().unwrap().len(), 2);
    let week = get(&db.pool, &token, format!("{base}?period=7d")).await;
    assert_eq!(week["total_count"], 5);
}

/// A transaction that ended `days` ago.
async fn old_transaction(pool: &DbPool, project_id: i32, days: i64) {
    let at = Utc::now() - Duration::days(days);
    sqlx::query(
        "INSERT INTO transactions (id, event_id, project_id, transaction_name, op, status, \
         start_timestamp, timestamp, duration_ms, data) \
         VALUES ($1, $2, $3, 'GET /items', 'http.server', 'ok', $4, $4, 10.0, '{}')",
    )
    .bind(Uuid::new_v4())
    .bind(Uuid::new_v4())
    .bind(project_id)
    .bind(at)
    .execute(pool)
    .await
    .unwrap();
}

#[actix_web::test]
async fn with_no_period_the_reads_cover_ninety_days() {
    // Sentry's fallback (`MAX_STATS_PERIOD`): no read is over all time.
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Default window").await;
    let p = format!("/api/projects/{}", project.id);
    old_transaction(&db.pool, project.id, 80).await;
    old_transaction(&db.pool, project.id, 100).await;
    spans(&db.pool, project.id, 2, Utc::now() - Duration::days(80)).await;
    spans(&db.pool, project.id, 3, Utc::now() - Duration::days(100)).await;
    let group = "name=GET%20/items&op=http.server";

    for period in ["", "?period=90d", "?period=365d", "?period=nonsense"] {
        let amp = period.replace('?', "&");
        let list = get(&db.pool, &token, format!("{p}/transactions{period}")).await;
        assert_eq!(list["total_count"], 1, "transactions{period}");
        let stats = get(&db.pool, &token, format!("{p}/transactions/stats{period}")).await;
        assert_eq!(stats["items"][0]["count"], 1, "stats{period}");
        let one = get(
            &db.pool,
            &token,
            format!("{p}/transactions/stats/group?{group}{amp}"),
        )
        .await;
        assert_eq!(one["count"], 1, "group{period}");
        let trend = get(
            &db.pool,
            &token,
            format!("{p}/transactions/stats/group/timeseries?{group}{amp}"),
        )
        .await;
        let calls: i64 = trend
            .as_array()
            .unwrap()
            .iter()
            .map(|point| point["count"].as_i64().unwrap())
            .sum();
        assert_eq!(calls, 1, "timeseries{period}");
        let listed = get(&db.pool, &token, format!("{p}/spans{period}")).await;
        assert_eq!(listed["total_count"], 2, "spans{period}");
    }
}

#[actix_web::test]
async fn the_spans_of_a_trace_are_listed_whatever_their_age() {
    // A trace is looked up by its id, which no window makes cheaper, and a
    // trace page with some of its spans missing draws a wrong picture.
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Old trace").await;
    let at = Utc::now() - Duration::days(100);
    spans(&db.pool, project.id, 3, at).await;

    let base = format!(
        "/api/projects/{}/spans?trace_id={:032x}",
        project.id,
        at.timestamp()
    );
    assert_eq!(get(&db.pool, &token, base.clone()).await["total_count"], 3);
    // Asked for a window, it is kept to it, and one that cannot be read is
    // the longest, as on every other list.
    for period in ["24h", "all", ""] {
        let windowed = get(&db.pool, &token, format!("{base}&period={period}")).await;
        assert_eq!(windowed["total_count"], 0, "period={period}");
    }
}

#[actix_web::test]
async fn a_transaction_list_past_the_cap_reports_a_capped_total() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Capped transactions").await;
    for n in 0..MAX_HITS + 5 {
        transaction(&db.pool, project.id, Some(n as f64)).await;
    }

    let base = format!("/api/projects/{}/transactions", project.id);
    for query in ["?period=24h", "?sort=slowest&name=GET%20/items"] {
        let body = get(&db.pool, &token, format!("{base}{query}")).await;
        assert_eq!(body["total_count"], MAX_HITS, "{query}");
        assert_eq!(body["total_capped"], true, "{query}");
        assert_eq!(body["items"].as_array().unwrap().len(), 20, "{query}");
    }
    let past = get(
        &db.pool,
        &token,
        format!("{base}?sort=slowest&page={}", MAX_HITS / 20 + 1),
    )
    .await;
    let durations: Vec<f64> = past["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["duration_ms"].as_f64().unwrap())
        .collect();
    assert_eq!(durations, [4.0, 3.0, 2.0, 1.0, 0.0]);
}

#[actix_web::test]
async fn the_slowest_transactions_page_in_one_order() {
    // Longest first, equal durations by id, the ones with no duration last:
    // the order every page has to agree on.
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Slowest").await;
    let mut tied = [
        transaction(&db.pool, project.id, Some(200.0)).await,
        transaction(&db.pool, project.id, Some(200.0)).await,
    ];
    tied.sort_unstable_by(|a, b| b.cmp(a));
    let fast = transaction(&db.pool, project.id, Some(100.0)).await;
    let unknown = transaction(&db.pool, project.id, None).await;
    let slow = transaction(&db.pool, project.id, Some(300.0)).await;

    let mut ids = Vec::new();
    for page in 1..=3 {
        let body = get(
            &db.pool,
            &token,
            format!(
                "/api/projects/{}/transactions?sort=slowest&per_page=2&page={page}",
                project.id
            ),
        )
        .await;
        assert_eq!(body["total_count"], 5);
        assert_eq!(body["total_capped"], false);
        for item in body["items"].as_array().unwrap() {
            ids.push(item["id"].as_str().unwrap().to_string());
        }
    }
    let expected: Vec<String> = [slow, tied[0], tied[1], fast, unknown]
        .iter()
        .map(Uuid::to_string)
        .collect();
    assert_eq!(ids, expected);
}

#[actix_web::test]
async fn a_stats_page_past_the_last_still_counts_the_endpoints() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project = create_test_project(&db.pool, "Stats pages").await;
    transaction(&db.pool, project.id, Some(10.0)).await;

    let body = get(
        &db.pool,
        &token,
        format!("/api/projects/{}/transactions/stats?page=4", project.id),
    )
    .await;
    assert_eq!(body["items"].as_array().unwrap().len(), 0);
    assert_eq!(body["total_count"], 1);
}
