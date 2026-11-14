use actix_web::dev::{Service, ServiceResponse};
use actix_web::{http::StatusCode, test, web};

use crate::common::TestDb;
use rustrak::db::DbPool;
use rustrak::routes::metrics::MetricsEndpoint;
use rustrak::telemetry::Counters;

/// The app with its scrape reading `metrics`, so counts are this test's own.
async fn metrics_app(
    pool: &DbPool,
    metrics: MetricsEndpoint,
) -> impl Service<actix_http::Request, Response = ServiceResponse, Error = actix_web::Error> {
    let mut state = crate::common::state(pool, crate::common::config()).await;
    state.metrics = web::Data::new(metrics);
    crate::common::app_with(&state).await
}

async fn scrape<S>(app: &S) -> String
where
    S: Service<actix_http::Request, Response = ServiceResponse, Error = actix_web::Error>,
{
    let response =
        test::call_service(app, test::TestRequest::get().uri("/metrics").to_request()).await;
    assert_eq!(response.status(), StatusCode::OK);
    String::from_utf8(test::read_body(response).await.to_vec()).unwrap()
}

/// The backlog gauge is the in-memory pending count, so a scrape never walks
/// the spool: under a backlog it holds millions of files and the walk took
/// minutes. The spool's size and oldest file come from a sample taken off the
/// request path, and are left out until the first one.
#[actix_web::test]
async fn the_backlog_gauge_is_read_from_memory_and_the_spool_size_from_a_sample() {
    let db = TestDb::new().await;
    let dir = tempfile::tempdir().unwrap();
    // Names the startup scan cannot read as events: on disk, not pending.
    std::fs::write(dir.path().join("project-1-a.pending.json"), "event").unwrap();
    std::fs::write(dir.path().join("project-1-b.pending.json"), "event").unwrap();
    let counters = Box::leak(Box::new(Counters::new()));
    let metrics = web::Data::new(MetricsEndpoint::new(
        true,
        dir.path().to_path_buf(),
        counters,
    ));
    let mut state =
        crate::common::state(&db.pool, crate::common::config_with_ingest_dir(dir.path())).await;
    state.metrics = metrics.clone();
    state
        .processors
        .errors
        .pending()
        .insert(1, uuid::Uuid::new_v4());
    let app = crate::common::app_with(&state).await;

    let body = scrape(&app).await;
    assert!(body.contains("\nrustrak_spool_pending 1\n"), "{body}");
    assert!(!body.contains("\nrustrak_spool_bytes "), "{body}");
    assert!(!body.contains("\nrustrak_spool_oldest_seconds "), "{body}");

    metrics.sample_spool().await;
    let body = scrape(&app).await;
    assert!(body.contains("\nrustrak_spool_pending 1\n"), "{body}");
    assert!(body.contains("\nrustrak_spool_bytes 10\n"), "{body}");
    assert!(body.contains("\nrustrak_spool_oldest_seconds "), "{body}");
}

#[actix_web::test]
async fn a_missing_spool_samples_as_empty_until_it_is_recreated() {
    let db = TestDb::new().await;
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("ingest");
    let counters = Box::leak(Box::new(Counters::new()));
    let metrics = web::Data::new(MetricsEndpoint::new(true, dir.clone(), counters));
    let mut state = crate::common::state(&db.pool, crate::common::config()).await;
    state.metrics = metrics.clone();
    let app = crate::common::app_with(&state).await;

    for (recreate, bytes) in [(false, 0), (true, 5)] {
        if recreate {
            std::fs::create_dir(&dir).unwrap();
            std::fs::write(dir.join("project-1-a.pending.json"), "event").unwrap();
        }
        metrics.sample_spool().await;
        let body = scrape(&app).await;
        assert!(
            body.contains(&format!("\nrustrak_spool_bytes {bytes}\n")),
            "{body}"
        );
        assert!(body.contains("\nrustrak_spool_oldest_seconds "), "{body}");
    }
}

/// The operational counters an operator watches during an incident.
#[actix_web::test]
async fn operational_counters_are_exposed() {
    let db = TestDb::new().await;
    let dir = tempfile::tempdir().unwrap();
    let counters = Box::leak(Box::new(Counters::new()));
    counters.ingest_shed(rustrak::telemetry::Shed::Backlog);
    counters.ingest_shed(rustrak::telemetry::Shed::Disk);
    counters.ingest_shed(rustrak::telemetry::Shed::Disk);
    counters.db_unavailable();
    counters.dead_lettered();
    counters.post_commit_dropped();
    let app = metrics_app(
        &db.pool,
        MetricsEndpoint::new(true, dir.path().to_path_buf(), counters),
    )
    .await;

    let body = scrape(&app).await;
    for line in [
        "rustrak_ingest_shed_total{reason=\"backlog\"} 1",
        "rustrak_ingest_shed_total{reason=\"disk\"} 2",
        "rustrak_db_unavailable_total 1",
        "rustrak_spool_dead_lettered_total 1",
        "rustrak_digest_post_commit_dropped_total 1",
        "rustrak_worker_restarts_total ",
        "rustrak_digest_total{result=\"rate_limited\"} 0",
    ] {
        assert!(body.contains(line), "missing {line}:\n{body}");
    }
}

#[actix_web::test]
async fn metrics_are_off_by_default_and_public_only_when_enabled() {
    let db = TestDb::new().await;
    let dir = tempfile::tempdir().unwrap();
    let counters = Box::leak(Box::new(Counters::new()));
    let app = metrics_app(
        &db.pool,
        MetricsEndpoint::new(false, dir.path().to_path_buf(), counters),
    )
    .await;
    let disabled =
        test::call_service(&app, test::TestRequest::get().uri("/metrics").to_request()).await;
    assert_eq!(disabled.status(), StatusCode::NOT_FOUND);

    counters.ingest_accepted(std::time::Duration::from_millis(20));
    let app = metrics_app(
        &db.pool,
        MetricsEndpoint::new(true, dir.path().to_path_buf(), counters),
    )
    .await;
    let request = test::TestRequest::get().uri("/metrics").to_request();
    let response = test::call_service(&app, request).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap(),
        "text/plain; version=0.0.4; charset=utf-8"
    );
    assert_eq!(
        response
            .headers()
            .get("cache-control")
            .unwrap()
            .to_str()
            .unwrap(),
        "no-store"
    );
    let body = String::from_utf8(test::read_body(response).await.to_vec()).unwrap();
    assert!(body.contains("rustrak_ingest_accepted_total 1"));
    assert!(body.contains("rustrak_spool_pending "));
    assert!(body.contains("rustrak_db_bytes "));

    // Anonymous telemetry drains its window, not Prometheus's lifetime totals.
    counters.snapshot_and_reset();
    counters.ingest_accepted(std::time::Duration::from_millis(20));
    let body = scrape(&app).await;
    assert!(body.contains("rustrak_ingest_accepted_total 2"));
}
