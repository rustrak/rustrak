//! Integration tests for Health endpoints
//!
//! Tests the liveness and readiness health check endpoints.

use crate::common::TestDb;
use actix_web::test;
use rustrak::services::AuthTokenService;
use serde_json::Value;

#[actix_web::test]
async fn test_liveness_returns_ok() {
    let db = TestDb::new().await;

    let app = crate::common::app(&db.pool).await;

    let req = test::TestRequest::get().uri("/health").to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["status"], "ok");
}

#[actix_web::test]
async fn test_liveness_returns_correct_content_type() {
    let db = TestDb::new().await;

    let app = crate::common::app(&db.pool).await;

    let req = test::TestRequest::get().uri("/health").to_request();

    let resp = test::call_service(&app, req).await;
    let content_type = resp
        .headers()
        .get("content-type")
        .expect("Content-Type header missing");
    assert!(content_type.to_str().unwrap().contains("application/json"));
}

#[actix_web::test]
async fn test_liveness_does_not_require_auth() {
    let db = TestDb::new().await;

    let app = crate::common::app(&db.pool).await;

    // No auth header provided
    let req = test::TestRequest::get().uri("/health").to_request();

    let resp = test::call_service(&app, req).await;
    // Should succeed without auth
    assert!(resp.status().is_success());
}

#[actix_web::test]
async fn test_readiness_returns_ready_with_healthy_db() {
    let db = TestDb::new().await;

    let app = crate::common::app(&db.pool).await;

    let req = test::TestRequest::get().uri("/health/ready").to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["status"], "ready");
    assert_eq!(body["checks"]["database"], "ok");
}

#[actix_web::test]
async fn test_readiness_returns_not_ready_with_closed_pool() {
    let db = TestDb::new().await;

    // Close the pool to simulate unhealthy database
    db.pool.close().await;

    let app = crate::common::app(&db.pool).await;

    let req = test::TestRequest::get().uri("/health/ready").to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 503);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["status"], "not_ready");
    assert_eq!(body["checks"]["database"], "error");
}

#[actix_web::test]
async fn test_readiness_does_not_require_auth() {
    let db = TestDb::new().await;

    let app = crate::common::app(&db.pool).await;

    // No auth header provided
    let req = test::TestRequest::get().uri("/health/ready").to_request();

    let resp = test::call_service(&app, req).await;
    // Should succeed without auth
    assert!(resp.status().is_success());
}

#[actix_web::test]
async fn test_readiness_returns_correct_content_type() {
    let db = TestDb::new().await;

    let app = crate::common::app(&db.pool).await;

    let req = test::TestRequest::get().uri("/health/ready").to_request();

    let resp = test::call_service(&app, req).await;
    let content_type = resp
        .headers()
        .get("content-type")
        .expect("Content-Type header missing");
    assert!(content_type.to_str().unwrap().contains("application/json"));
}

#[actix_web::test]
async fn test_health_post_returns_error() {
    let db = TestDb::new().await;

    let app = crate::common::app(&db.pool).await;

    let req = test::TestRequest::post().uri("/health").to_request();

    let resp = test::call_service(&app, req).await;
    // Actix-web returns 404 when no route matches the method
    assert!(resp.status().is_client_error());
}

#[actix_web::test]
async fn test_readiness_post_returns_error() {
    let db = TestDb::new().await;

    let app = crate::common::app(&db.pool).await;

    let req = test::TestRequest::post().uri("/health/ready").to_request();

    let resp = test::call_service(&app, req).await;
    // Actix-web returns 404 when no route matches the method
    assert!(resp.status().is_client_error());
}

#[actix_web::test]
async fn test_version_returns_version_to_an_authenticated_caller() {
    let db = TestDb::new().await;
    let token = AuthTokenService::create(
        &db.pool,
        rustrak::models::CreateAuthToken {
            description: Some("Version test token".to_string()),
            ..Default::default()
        },
    )
    .await
    .expect("Failed to create test token")
    .token;

    let app = crate::common::app(&db.pool).await;

    let req = test::TestRequest::get()
        .uri("/health/version")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
}
