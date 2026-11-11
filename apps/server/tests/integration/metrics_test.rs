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

#[actix_web::test]
async fn missing_spool_gauges_remain_present_when_the_directory_is_recreated() {
    let db = TestDb::new().await;
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("ingest");
    let counters = Box::leak(Box::new(Counters::new()));
    let app = metrics_app(&db.pool, MetricsEndpoint::new(true, dir.clone(), counters)).await;

    for recreate in [false, true] {
        if recreate {
            std::fs::create_dir(&dir).unwrap();
            std::fs::write(dir.join("project-1-a.pending.json"), "event").unwrap();
        }
        let response =
            test::call_service(&app, test::TestRequest::get().uri("/metrics").to_request()).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = String::from_utf8(test::read_body(response).await.to_vec()).unwrap();
        // The cached empty reading is valid until the normal gauge TTL expires.
        for name in ["pending", "bytes", "oldest_seconds"] {
            assert!(body.contains(&format!("\nrustrak_spool_{name} 0\n")));
        }
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
    std::fs::write(dir.path().join("project-1-a.pending.json"), "test").unwrap();
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
    assert!(body.contains("rustrak_spool_pending 1"));
    assert!(body.contains("rustrak_db_bytes "));

    // Anonymous telemetry drains its window, not Prometheus's lifetime totals.
    counters.snapshot_and_reset();
    counters.ingest_accepted(std::time::Duration::from_millis(20));
    // The spool reading is reused between scrapes; counters are always live.
    std::fs::write(dir.path().join("project-1-b.pending.json"), "test").unwrap();
    let response =
        test::call_service(&app, test::TestRequest::get().uri("/metrics").to_request()).await;
    let body = String::from_utf8(test::read_body(response).await.to_vec()).unwrap();
    assert!(body.contains("rustrak_ingest_accepted_total 2"));
    assert!(body.contains("rustrak_spool_pending 1"));
}
