//! Web Vitals: what each SDK wire format leaves in `web_vitals`, and the
//! page scores the API makes of them.

use crate::common::{create_test_project, create_test_token, TestDb};
use actix_web::test;
use chrono::Utc;
use rustrak::db::DbPool;
use rustrak::digest::processors::{
    ProcessorCtx, SpanProcessor, SpanV2Processor, TransactionProcessor,
};
use rustrak::services::web_vitals::{score, Vital};
use serde_json::{json, Value};
use uuid::Uuid;

const CHROME: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
                      (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
const TRACE: &str = "da245123602b428cb8f92a34062b63c3";

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

fn now() -> f64 {
    Utc::now().timestamp() as f64
}

/// A page load as `@sentry/browser` sends it when it does not stream spans.
fn pageload(page: &str, measurements: Value) -> Value {
    json!({
        "type": "transaction",
        "transaction": page,
        "transaction_info": { "source": "route" },
        "start_timestamp": now() - 2.0,
        "timestamp": now(),
        "platform": "javascript",
        "environment": "production",
        "release": "web@1.0.0",
        "request": { "url": "https://example.com/", "headers": { "User-Agent": CHROME } },
        "contexts": { "trace": {
            "trace_id": TRACE, "span_id": "8632fabd5206435c", "op": "pageload",
            "data": {
                "lcp.element": "body > img.hero", "lcp.url": "https://cdn.example.com/hero.webp",
                "cls.source.1": "body > div.ads", "effectiveConnectionType": "4g",
            },
        } },
        "measurements": measurements,
    })
}

async fn send_transaction(pool: &DbPool, project_id: i32, payload: Value) {
    TransactionProcessor
        .process(
            serde_json::to_vec(&payload).unwrap().into(),
            &ctx(pool, project_id),
        )
        .await
        .unwrap();
}

/// A span `@sentry/browser` sends on its own, as it sends an interaction's
/// INP and, with standalone spans on, a page's LCP and CLS.
fn standalone(span_id: &str, op: &str, measurements: Value) -> Value {
    json!({
        "trace_id": TRACE, "span_id": span_id, "op": op,
        "description": "body > button.pay",
        "start_timestamp": now() - 0.2, "timestamp": now(),
        "is_segment": true,
        "data": {
            "transaction": "/checkout", "user_agent.original": CHROME,
            "release": "web@1.0.0", "environment": "production",
            "sentry.op": op, "lcp.element": "body > img.hero",
        },
        "measurements": measurements,
    })
}

async fn send_spans(pool: &DbPool, project_id: i32, spans: Vec<Value>) {
    let items = spans
        .iter()
        .map(|span| serde_json::to_vec(span).unwrap().into())
        .collect();
    SpanProcessor
        .process_batch(items, &ctx(pool, project_id))
        .await
        .unwrap();
}

type Attribution = (
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// `(vital, target, resource, url, device, connection)` of every stored
/// vital, in vital order.
async fn attribution(pool: &DbPool, project_id: i32) -> Vec<Attribution> {
    sqlx::query_as(
        "SELECT vital, target, resource, url, device, connection FROM web_vitals \
         WHERE project_id = $1 ORDER BY vital",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

fn some(value: &str) -> Option<String> {
    Some(value.to_string())
}

/// `(page, vital, value, browser)` of every stored vital with a score, in
/// vital order.
async fn stored(pool: &DbPool, project_id: i32) -> Vec<(String, String, f64, Option<String>)> {
    sqlx::query_as(
        "SELECT page, vital, value, browser FROM web_vitals \
         WHERE project_id = $1 AND score BETWEEN 0 AND 1 ORDER BY vital",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn a_pageload_transaction_stores_its_vitals_scored_for_its_browser() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "web").await;

    send_transaction(
        &db.pool,
        project.id,
        pageload(
            "/checkout",
            json!({
                "lcp": { "value": 2100.0, "unit": "millisecond" },
                "fcp": { "value": 900.0, "unit": "millisecond" },
                "cls": { "value": 0.05, "unit": "" },
                "ttfb": { "value": 300.0, "unit": "millisecond" },
                "fp": { "value": 850.0, "unit": "millisecond" },
            }),
        ),
    )
    .await;

    let chrome = Some("Chrome".to_string());
    let row = |vital: &str, value| {
        (
            "/checkout".to_string(),
            vital.to_string(),
            value,
            chrome.clone(),
        )
    };
    assert_eq!(
        stored(&db.pool, project.id).await,
        vec![
            row("cls", 0.05),
            row("fcp", 900.0),
            row("lcp", 2100.0),
            row("ttfb", 300.0),
        ]
    );
    // What caused each, where and on what.
    let at = |vital: &str, target: Option<&str>, resource: Option<&str>| {
        (
            vital.to_string(),
            target.map(str::to_string),
            resource.map(str::to_string),
            some("https://example.com/"),
            some("desktop"),
            some("4g"),
        )
    };
    assert_eq!(
        attribution(&db.pool, project.id).await,
        vec![
            at("cls", Some("body > div.ads"), None),
            at("fcp", None, None),
            at(
                "lcp",
                Some("body > img.hero"),
                Some("https://cdn.example.com/hero.webp")
            ),
            at("ttfb", None, None),
        ]
    );
}

#[tokio::test]
async fn standalone_interaction_and_vital_spans_store_theirs_for_their_page() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "web").await;

    send_spans(
        &db.pool,
        project.id,
        vec![
            standalone(
                "a1a1a1a1a1a1a1a1",
                "ui.interaction.click",
                json!({ "inp": { "value": 180.0 } }),
            ),
            standalone(
                "b2b2b2b2b2b2b2b2",
                "ui.webvital.lcp",
                json!({ "lcp": { "value": 2100.0 } }),
            ),
            // Not a span Web Vitals reads.
            standalone(
                "c3c3c3c3c3c3c3c3",
                "resource.script",
                json!({ "lcp": { "value": 1.0 } }),
            ),
        ],
    )
    .await;

    let chrome = Some("Chrome".to_string());
    assert_eq!(
        stored(&db.pool, project.id).await,
        vec![
            (
                "/checkout".to_string(),
                "inp".to_string(),
                180.0,
                chrome.clone()
            ),
            ("/checkout".to_string(), "lcp".to_string(), 2100.0, chrome),
        ]
    );
    let attributed: Vec<_> = attribution(&db.pool, project.id)
        .await
        .into_iter()
        .map(|(vital, target, ..)| (vital, target))
        .collect();
    assert_eq!(
        attributed,
        vec![
            ("inp".to_string(), some("body > button.pay")),
            ("lcp".to_string(), some("body > img.hero")),
        ]
    );
}

/// A v2 span's attributes, typed as the SDK sends them.
fn typed(attributes: Value) -> Value {
    attributes
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, value)| {
            let kind = match value {
                Value::String(_) => "string",
                Value::Number(n) if n.is_f64() => "double",
                _ => "integer",
            };
            (key.clone(), json!({ "value": value, "type": kind }))
        })
        .collect::<serde_json::Map<_, _>>()
        .into()
}

/// A span as `@sentry/browser` streams it: the page load is the segment
/// and its vitals come on spans of their own, named after its route.
fn streamed(span_id: &str, op: &str, segment: bool, attributes: Value) -> Value {
    let mut all = json!({
        "sentry.op": op, "sentry.segment.name": "/checkout",
        "sentry.segment.id": "8632fabd5206435c",
        "sentry.release": "web@1.0.0", "sentry.environment": "production",
        "sentry.sdk.name": "sentry.javascript.browser", "user_agent.original": CHROME,
    });
    all.as_object_mut()
        .unwrap()
        .extend(attributes.as_object().unwrap().clone());
    // An interaction's span is named after the element it was on.
    let name = match op.starts_with("ui.interaction.") {
        true => "body > button.pay",
        false => "/checkout",
    };
    json!({
        "trace_id": TRACE, "span_id": span_id, "name": name, "status": "ok",
        "is_segment": segment, "start_timestamp": now() - 1.0, "end_timestamp": now(),
        "attributes": typed(all),
    })
}

#[tokio::test]
async fn streamed_spans_store_their_vitals_tied_to_the_page_load() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "web").await;

    let body = json!({ "version": 2, "items": [
        streamed("8632fabd5206435c", "pageload", true, json!({
            "browser.web_vital.fcp.value": 900.0, "browser.web_vital.ttfb.value": 300.0,
            "url.full": "https://example.com/checkout", "network.connection.effective_type": "3g",
        })),
        streamed("d4d4d4d4d4d4d4d4", "ui.webvital.lcp", false, json!({
            "browser.web_vital.lcp.value": 2100.0, "sentry.pageload.span_id": "8632fabd5206435c",
            "browser.web_vital.lcp.element": "body > img.hero",
            "browser.web_vital.lcp.url": "https://cdn.example.com/hero.webp",
        })),
        streamed("e5e5e5e5e5e5e5e5", "ui.interaction.click", false, json!({
            "browser.web_vital.inp.value": 180.0,
        })),
    ] });
    SpanV2Processor
        .process(
            serde_json::to_vec(&body).unwrap().into(),
            &ctx(&db.pool, project.id),
        )
        .await
        .unwrap();

    let chrome = Some("Chrome".to_string());
    let row = |vital: &str, value| {
        (
            "/checkout".to_string(),
            vital.to_string(),
            value,
            chrome.clone(),
        )
    };
    assert_eq!(
        stored(&db.pool, project.id).await,
        vec![
            row("fcp", 900.0),
            row("inp", 180.0),
            row("lcp", 2100.0),
            row("ttfb", 300.0)
        ]
    );
    let attributed: Vec<_> = attribution(&db.pool, project.id)
        .await
        .into_iter()
        .map(|(vital, target, resource, url, _, connection)| {
            (vital, target, resource, url, connection)
        })
        .collect();
    let page = some("https://example.com/checkout");
    assert_eq!(
        attributed,
        vec![
            ("fcp".to_string(), None, None, page.clone(), some("3g")),
            (
                "inp".to_string(),
                some("body > button.pay"),
                None,
                None,
                None
            ),
            (
                "lcp".to_string(),
                some("body > img.hero"),
                some("https://cdn.example.com/hero.webp"),
                None,
                None,
            ),
            ("ttfb".to_string(), None, None, page, some("3g")),
        ]
    );
    // The page load's vitals point at the transaction it is stored as.
    let tied: Vec<(String, bool)> = sqlx::query_as(
        "SELECT v.vital, COALESCE(v.transaction_id = t.id, FALSE) FROM web_vitals v \
         LEFT JOIN transactions t ON t.project_id = v.project_id \
         WHERE v.project_id = $1 ORDER BY v.vital",
    )
    .bind(project.id)
    .fetch_all(&db.pool)
    .await
    .unwrap();
    assert_eq!(
        tied,
        vec![
            ("fcp".to_string(), true),
            ("inp".to_string(), false),
            ("lcp".to_string(), true),
            ("ttfb".to_string(), true),
        ]
    );
}

#[tokio::test]
async fn the_same_span_delivered_again_adds_no_vitals() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "web").await;
    let inp = || {
        standalone(
            "a1a1a1a1a1a1a1a1",
            "ui.interaction.click",
            json!({ "inp": { "value": 180.0 } }),
        )
    };

    send_spans(&db.pool, project.id, vec![inp()]).await;
    send_spans(&db.pool, project.id, vec![inp()]).await;

    assert_eq!(stored(&db.pool, project.id).await.len(), 1);
}

#[tokio::test]
async fn the_cleanup_removes_vitals_with_the_transactions() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "web").await;
    send_transaction(
        &db.pool,
        project.id,
        pageload("/checkout", json!({ "lcp": { "value": 2100.0 } })),
    )
    .await;
    send_spans(
        &db.pool,
        project.id,
        vec![standalone(
            "a1a1a1a1a1a1a1a1",
            "ui.interaction.click",
            json!({ "inp": { "value": 180.0 } }),
        )],
    )
    .await;

    rustrak::services::StorageService::purge_project(&db.pool, project.id)
        .await
        .unwrap();

    assert!(stored(&db.pool, project.id).await.is_empty());
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

/// Each page's page loads, with only an LCP each.
async fn send_lcps(pool: &DbPool, project_id: i32, page: &str, lcps: &[f64]) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    for lcp in lcps {
        let mut load = pageload(page, json!({ "lcp": { "value": lcp } }));
        let span = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        load["contexts"]["trace"]["span_id"] = format!("{span:016x}").into();
        send_transaction(pool, project_id, load).await;
    }
}

fn chrome_lcp(value: f64) -> f64 {
    score(&[(Vital::Lcp, value)], Some("Chrome"))[0].1
}

fn vital<'a>(vitals: &'a Value, name: &str) -> &'a Value {
    vitals
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["vital"] == name)
        .unwrap_or_else(|| panic!("no {name} in {vitals}"))
}

fn approx(actual: &Value, expected: f64) {
    let actual = actual
        .as_f64()
        .unwrap_or_else(|| panic!("{actual} is not a number"));
    assert!(
        (actual - expected).abs() < 1e-9,
        "{actual} is not {expected}"
    );
}

#[tokio::test]
async fn the_summary_has_each_vitals_p75_and_average_score() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "web").await;
    let token = create_test_token(&db.pool).await;
    send_lcps(&db.pool, project.id, "/a", &[1000.0, 3000.0]).await;

    let summary = get(
        &db.pool,
        &token,
        format!("/api/projects/{}/web-vitals", project.id),
    )
    .await;

    let lcp = vital(&summary["vitals"], "lcp");
    // Interpolated as percentile_cont: 1000 + 0.75 * (3000 - 1000).
    approx(&lcp["value"], 2500.0);
    assert_eq!(lcp["count"], 2);
    let average = (chrome_lcp(1000.0) + chrome_lcp(3000.0)) / 2.0;
    approx(&lcp["score"], average);
    // LCP is the only vital with data, so it is the whole score.
    approx(&summary["score"], average);
    assert_eq!(summary["browsers"], json!(["Chrome"]));
}

#[tokio::test]
async fn pages_are_ranked_by_the_score_they_cost_the_project() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "web").await;
    let token = create_test_token(&db.pool).await;
    send_lcps(&db.pool, project.id, "/fast", &[900.0, 1000.0, 1100.0]).await;
    send_lcps(&db.pool, project.id, "/slow", &[5000.0]).await;

    let pages = get(
        &db.pool,
        &token,
        format!("/api/projects/{}/web-vitals/pages", project.id),
    )
    .await;
    let summary = get(
        &db.pool,
        &token,
        format!("/api/projects/{}/web-vitals", project.id),
    )
    .await;

    let items = pages["items"].as_array().unwrap();
    // One slow load costs more than three fast ones.
    assert_eq!(items[0]["page"], "/slow");
    assert_eq!(items[1]["page"], "/fast");
    assert_eq!(items[1]["pageloads"], 3);
    approx(&items[0]["score"], chrome_lcp(5000.0));
    // What every page costs is what the project is missing.
    let opportunity: f64 = items
        .iter()
        .map(|p| p["opportunity"].as_f64().unwrap())
        .sum();
    approx(
        &json!(opportunity),
        1.0 - summary["score"].as_f64().unwrap(),
    );
}

#[tokio::test]
async fn the_trend_rates_a_vitals_measurements_bucket_by_bucket() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "web").await;
    let token = create_test_token(&db.pool).await;
    send_lcps(&db.pool, project.id, "/fast", &[1000.0]).await;
    send_lcps(&db.pool, project.id, "/slow", &[5000.0]).await;
    let trend = |query: &str| {
        format!(
            "/api/projects/{}/web-vitals/timeseries?vital=lcp&period=24h{query}",
            project.id
        )
    };

    let all = get(&db.pool, &token, trend("")).await;
    let slow = get(&db.pool, &token, trend("&page=%2Fslow")).await;

    let points = all.as_array().unwrap();
    assert!(points.len() >= 24, "every hour of the day is a bucket");
    let now = |points: &Value| points.as_array().unwrap().last().unwrap()["distribution"].clone();
    assert_eq!(now(&all), json!({ "good": 1, "meh": 0, "poor": 1 }));
    assert_eq!(now(&slow), json!({ "good": 0, "meh": 0, "poor": 1 }));
    assert_eq!(
        points[0]["distribution"],
        json!({ "good": 0, "meh": 0, "poor": 0 })
    );
}

/// A page load of `page` with its LCP on `element`, from `user_agent`.
async fn send_lcp_on(
    pool: &DbPool,
    project_id: i32,
    page: &str,
    lcp: f64,
    element: &str,
    user_agent: &str,
) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1 << 40);
    let mut load = pageload(page, json!({ "lcp": { "value": lcp } }));
    let span = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    load["contexts"]["trace"]["span_id"] = format!("{span:016x}").into();
    load["contexts"]["trace"]["data"]["lcp.element"] = element.into();
    load["request"]["headers"]["User-Agent"] = user_agent.into();
    send_transaction(pool, project_id, load).await;
}

const ANDROID: &str = "Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 \
                       (KHTML, like Gecko) Chrome/131.0.0.0 Mobile Safari/537.36";

#[tokio::test]
async fn the_summary_splits_each_vital_into_ratings_percentiles_and_devices() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "web").await;
    let token = create_test_token(&db.pool).await;
    // web.dev: good up to 2.5s, poor past 4s.
    for (lcp, ua) in [(1000.0, CHROME), (3000.0, CHROME), (5000.0, ANDROID)] {
        send_lcp_on(&db.pool, project.id, "/a", lcp, "img", ua).await;
    }
    let base = format!("/api/projects/{}/web-vitals", project.id);

    let summary = get(&db.pool, &token, base.clone()).await;
    let lcp = vital(&summary["vitals"], "lcp");
    assert_eq!(
        lcp["distribution"],
        json!({ "good": 1, "meh": 1, "poor": 1 })
    );

    // Every percentile at once, interpolated over 1000, 3000 and 5000.
    let percentiles: Vec<(u64, f64)> = lcp["percentiles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["percentile"].as_u64().unwrap(),
                p["value"].as_f64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        percentiles,
        vec![
            (50, 3000.0),
            (75, 4000.0),
            (90, 4600.0),
            (95, 4800.0),
            (99, 4960.0)
        ]
    );
    approx(&lcp["value"], 4000.0);

    let phones = get(&db.pool, &token, format!("{base}?device=mobile")).await;
    let lcp = vital(&phones["vitals"], "lcp");
    assert_eq!(lcp["count"], 1);
    approx(&lcp["value"], 5000.0);
}

#[tokio::test]
async fn a_vitals_causes_are_its_elements_ranked_by_the_score_they_lose() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "web").await;
    let token = create_test_token(&db.pool).await;
    for lcp in [5000.0, 6000.0] {
        send_lcp_on(&db.pool, project.id, "/a", lcp, "img.hero", CHROME).await;
    }
    for lcp in [900.0, 1000.0, 1100.0] {
        send_lcp_on(&db.pool, project.id, "/b", lcp, "h1.title", CHROME).await;
    }

    let causes = get(
        &db.pool,
        &token,
        format!("/api/projects/{}/web-vitals/targets?vital=lcp", project.id),
    )
    .await;

    let causes = causes.as_array().unwrap();
    assert_eq!(causes[0]["target"], "img.hero");
    assert_eq!(causes[0]["count"], 2);
    approx(&causes[0]["value"], 5750.0);
    assert_eq!(causes[1]["target"], "h1.title");
}

#[tokio::test]
async fn pages_rank_by_one_vital_when_asked() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "web").await;
    let token = create_test_token(&db.pool).await;
    // /lcp is slow to paint, /inp slow to answer.
    send_lcp_on(&db.pool, project.id, "/lcp", 6000.0, "img", CHROME).await;
    send_lcp_on(&db.pool, project.id, "/inp", 1000.0, "img", CHROME).await;
    send_spans(
        &db.pool,
        project.id,
        vec![{
            let mut span = standalone(
                "f6f6f6f6f6f6f6f6",
                "ui.interaction.click",
                json!({ "inp": { "value": 900.0 } }),
            );
            span["data"]["transaction"] = "/inp".into();
            span
        }],
    )
    .await;
    let pages = |vital: &str| {
        format!(
            "/api/projects/{}/web-vitals/pages?vital={vital}",
            project.id
        )
    };

    let by_lcp = get(&db.pool, &token, pages("lcp")).await;
    let by_inp = get(&db.pool, &token, pages("inp")).await;

    assert_eq!(by_lcp["items"][0]["page"], "/lcp");
    assert_eq!(by_inp["items"][0]["page"], "/inp");
}

#[tokio::test]
async fn samples_are_a_vitals_worst_measurements_with_where_they_happened() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "web").await;
    let token = create_test_token(&db.pool).await;
    for lcp in [1000.0, 7000.0, 3000.0] {
        send_lcp_on(&db.pool, project.id, "/a", lcp, "img.hero", ANDROID).await;
    }

    let samples = get(
        &db.pool,
        &token,
        format!(
            "/api/projects/{}/web-vitals/samples?vital=lcp&page=%2Fa",
            project.id
        ),
    )
    .await;

    let samples = samples.as_array().unwrap();
    let values: Vec<f64> = samples
        .iter()
        .map(|s| s["value"].as_f64().unwrap())
        .collect();
    assert_eq!(values, vec![7000.0, 3000.0, 1000.0]);
    let worst = &samples[0];
    assert_eq!(worst["target"], "img.hero");
    assert_eq!(worst["browser"], "Chrome Mobile");
    assert_eq!(worst["device"], "mobile");
    assert_eq!(worst["url"], "https://example.com/");
    assert!(worst["transaction_id"].is_string());
}

#[tokio::test]
async fn a_vital_breaks_down_by_device_and_by_browser() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "web").await;
    let token = create_test_token(&db.pool).await;
    for (lcp, ua) in [(1000.0, CHROME), (1200.0, CHROME), (6000.0, ANDROID)] {
        send_lcp_on(&db.pool, project.id, "/a", lcp, "img", ua).await;
    }
    let breakdown = |by: &str| {
        format!(
            "/api/projects/{}/web-vitals/breakdown?vital=lcp&by={by}",
            project.id
        )
    };

    let devices = get(&db.pool, &token, breakdown("device")).await;
    let browsers = get(&db.pool, &token, breakdown("browser")).await;

    // The most measured first.
    assert_eq!(devices[0]["key"], "desktop");
    assert_eq!(devices[0]["stats"]["count"], 2);
    assert_eq!(devices[1]["key"], "mobile");
    approx(&devices[1]["stats"]["value"], 6000.0);
    assert_eq!(browsers[0]["key"], "Chrome");
    assert_eq!(browsers[1]["key"], "Chrome Mobile");
}

#[tokio::test]
async fn a_breakdown_needs_a_vital_and_a_known_dimension() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "web").await;
    let token = create_test_token(&db.pool).await;
    let app = crate::common::app(&db.pool).await;
    for query in ["by=device", "vital=lcp&by=planet"] {
        let req = test::TestRequest::get()
            .uri(&format!(
                "/api/projects/{}/web-vitals/breakdown?{query}",
                project.id
            ))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        assert_eq!(test::call_service(&app, req).await.status(), 400, "{query}");
    }
}

#[tokio::test]
async fn overlong_and_nul_fields_are_cut_rather_than_failing_the_spans() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "web").await;
    let mut span = standalone(
        "a1a1a1a1a1a1a1a1",
        "ui.webvital.lcp",
        json!({ "lcp": { "value": 2100.0 } }),
    );
    span["data"]["environment"] = "e".repeat(70).into();
    span["data"]["release"] = "r".repeat(250).into();
    span["data"]["lcp.element"] = "body > img\u{0}.hero".into();

    send_spans(&db.pool, project.id, vec![span]).await;

    let (environment, release, target): (String, String, String) =
        sqlx::query_as("SELECT environment, release, target FROM web_vitals WHERE project_id = $1")
            .bind(project.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(environment.chars().count(), 64);
    assert_eq!(release.chars().count(), 200);
    assert_eq!(target, "body > img.hero");
}

#[tokio::test]
async fn a_standalone_vital_counts_with_the_page_load_it_names() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "web").await;
    let token = create_test_token(&db.pool).await;
    // The page load's FCP on its transaction, its LCP on a span of its own
    // that names it, as `@sentry/browser` 10 sends them.
    send_transaction(
        &db.pool,
        project.id,
        pageload("/checkout", json!({ "fcp": { "value": 900.0 } })),
    )
    .await;
    let mut lcp = standalone(
        "b2b2b2b2b2b2b2b2",
        "ui.webvital.lcp",
        json!({ "lcp": { "value": 2100.0 } }),
    );
    lcp["data"]["sentry.pageload.span_id"] = "8632fabd5206435c".into();
    send_spans(&db.pool, project.id, vec![lcp]).await;

    let pages = get(
        &db.pool,
        &token,
        format!("/api/projects/{}/web-vitals/pages", project.id),
    )
    .await;

    assert_eq!(pages["items"][0]["pageloads"], 1);
}
