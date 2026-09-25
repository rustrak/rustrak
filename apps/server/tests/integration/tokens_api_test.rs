//! Integration tests for the Tokens API
//!
//! Tests the complete Tokens CRUD API with a real PostgreSQL database.

use crate::common::TestDb;
use actix_web::{test, web, App};
use rustrak::config::{Config, DashboardConfig, DatabaseConfig, RateLimitConfig};
use rustrak::routes;
use rustrak::services::AuthTokenService;
use serde_json::{json, Value};
use std::time::Duration;

/// Creates a test config
fn create_test_config() -> Config {
    Config {
        host: "127.0.0.1".to_string(),
        port: 0,
        database: DatabaseConfig {
            url: "postgres://test:test@localhost/test".to_string(),
            max_connections: 5,
            min_connections: 1,
            acquire_timeout: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(60),
            max_lifetime: Duration::from_secs(300),
        },
        rate_limit: RateLimitConfig {
            max_events_per_minute: 1000,
            max_events_per_hour: 10000,
            max_events_per_project_per_minute: 500,
            max_events_per_project_per_hour: 5000,
        },
        security: rustrak::config::SecurityConfig {
            ssl_proxy: false,
            session_secret_key: None,
        },
        ingest_dir: None,
        public_url: None,
        sourcemap_storage_path: "/tmp/test_sourcemaps".to_string(),
        sourcemap_cache_bytes: 64 * 1024 * 1024,
        max_chunk_size_bytes: 10 * 1024 * 1024,
        session_flush_interval_secs: 30,
        session_cardinality_cap: 10_000,
        dashboard: DashboardConfig {
            dir: "./static".to_string(),
            enabled: true,
            url: None,
        },
        telemetry: rustrak::config::TelemetryConfig {
            enabled: false,
            do_not_track: false,
        },
    }
}

/// Creates a test token and returns its value
async fn create_test_token(pool: &rustrak::db::DbPool) -> String {
    let result = AuthTokenService::create(
        pool,
        rustrak::models::CreateAuthToken {
            description: Some("Test token".to_string()),
        },
    )
    .await
    .expect("Failed to create test token");
    result.token
}

// =============================================================================
// List Tokens Tests
// =============================================================================

#[actix_web::test]
#[ignore = "Session cookies not preserved in actix test framework - use E2E tests"]
async fn test_list_tokens_empty() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config))
            .configure(routes::tokens::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/tokens")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body: Vec<Value> = test::read_body_json(resp).await;
    // Should have at least the token we used for auth
    assert!(!body.is_empty());
}

#[actix_web::test]
#[ignore = "Session cookies not preserved in actix test framework - use E2E tests"]
async fn test_list_tokens_unauthorized() {
    let db = TestDb::new().await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config))
            .configure(routes::tokens::configure),
    )
    .await;

    let req = test::TestRequest::get().uri("/api/tokens").to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
#[ignore = "Session cookies not preserved in actix test framework - use E2E tests"]
async fn test_list_tokens_invalid_token() {
    let db = TestDb::new().await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config))
            .configure(routes::tokens::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/tokens")
        .insert_header(("Authorization", "Bearer invalid_token_here"))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
#[ignore = "Session cookies not preserved in actix test framework - use E2E tests"]
async fn test_list_tokens_masks_token_values() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config))
            .configure(routes::tokens::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/tokens")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body: Vec<Value> = test::read_body_json(resp).await;

    // The token value should be masked (show token_prefix with first 8 chars + "...")
    for token_obj in &body {
        let token_prefix = token_obj["token_prefix"].as_str().unwrap();
        // Masked tokens should be "xxxxxxxx..." format (first 8 chars + "...")
        assert!(token_prefix.ends_with("..."));
        assert_eq!(token_prefix.len(), 11); // 8 chars + "..."
    }
}

// =============================================================================
// Create Token Tests
// =============================================================================

#[actix_web::test]
#[ignore = "Session cookies not preserved in actix test framework - use E2E tests"]
async fn test_create_token_success() {
    let db = TestDb::new().await;
    let auth_token = create_test_token(&db.pool).await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config))
            .configure(routes::tokens::configure),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/tokens")
        .insert_header(("Authorization", format!("Bearer {}", auth_token)))
        .insert_header(("Content-Type", "application/json"))
        .set_json(json!({
            "description": "New API token for testing"
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let body: Value = test::read_body_json(resp).await;

    // Created response should include the full token (only time it's visible)
    assert!(body["token"].is_string());
    let new_token = body["token"].as_str().unwrap();
    assert_eq!(new_token.len(), 40); // Full 40-char hex token
    assert!(body["id"].is_number());
    assert_eq!(body["description"], "New API token for testing");
}

#[actix_web::test]
#[ignore = "Session cookies not preserved in actix test framework - use E2E tests"]
async fn test_create_token_without_description() {
    let db = TestDb::new().await;
    let auth_token = create_test_token(&db.pool).await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config))
            .configure(routes::tokens::configure),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/tokens")
        .insert_header(("Authorization", format!("Bearer {}", auth_token)))
        .insert_header(("Content-Type", "application/json"))
        .set_json(json!({}))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let body: Value = test::read_body_json(resp).await;
    assert!(body["token"].is_string());
    assert!(body["description"].is_null());
}

#[actix_web::test]
#[ignore = "Session cookies not preserved in actix test framework - use E2E tests"]
async fn test_create_token_unauthorized() {
    let db = TestDb::new().await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config))
            .configure(routes::tokens::configure),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/tokens")
        .insert_header(("Content-Type", "application/json"))
        .set_json(json!({"description": "Test"}))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
#[ignore = "Session cookies not preserved in actix test framework - use E2E tests"]
async fn test_create_token_generates_unique_tokens() {
    let db = TestDb::new().await;
    let auth_token = create_test_token(&db.pool).await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config))
            .configure(routes::tokens::configure),
    )
    .await;

    // Create multiple tokens and verify they're all unique
    let mut tokens = Vec::new();
    for i in 0..5 {
        let req = test::TestRequest::post()
            .uri("/api/tokens")
            .insert_header(("Authorization", format!("Bearer {}", auth_token)))
            .insert_header(("Content-Type", "application/json"))
            .set_json(json!({"description": format!("Token {}", i)}))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);

        let body: Value = test::read_body_json(resp).await;
        let token = body["token"].as_str().unwrap().to_string();
        tokens.push(token);
    }

    // Verify all tokens are unique
    let unique_tokens: std::collections::HashSet<_> = tokens.iter().collect();
    assert_eq!(unique_tokens.len(), tokens.len());
}

#[actix_web::test]
#[ignore = "Session cookies not preserved in actix test framework - use E2E tests"]
async fn test_create_token_is_valid_hex() {
    let db = TestDb::new().await;
    let auth_token = create_test_token(&db.pool).await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config))
            .configure(routes::tokens::configure),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/tokens")
        .insert_header(("Authorization", format!("Bearer {}", auth_token)))
        .insert_header(("Content-Type", "application/json"))
        .set_json(json!({"description": "Hex test"}))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let body: Value = test::read_body_json(resp).await;
    let new_token = body["token"].as_str().unwrap();

    // Verify it's valid lowercase hex
    assert!(new_token.chars().all(|c| c.is_ascii_hexdigit()));
    assert!(new_token.chars().all(|c| !c.is_uppercase()));
}

// =============================================================================
// Delete Token Tests
// =============================================================================

#[actix_web::test]
#[ignore = "Session cookies not preserved in actix test framework - use E2E tests"]
async fn test_delete_token_success() {
    let db = TestDb::new().await;
    let auth_token = create_test_token(&db.pool).await;
    let config = create_test_config();

    // Create a token to delete
    let token_to_delete = AuthTokenService::create(
        &db.pool,
        rustrak::models::CreateAuthToken {
            description: Some("Token to delete".to_string()),
        },
    )
    .await
    .expect("Failed to create token");

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config))
            .configure(routes::tokens::configure),
    )
    .await;

    let req = test::TestRequest::delete()
        .uri(&format!("/api/tokens/{}", token_to_delete.id))
        .insert_header(("Authorization", format!("Bearer {}", auth_token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 204);

    // Verify token is actually deleted
    let tokens = AuthTokenService::list(&db.pool).await.unwrap();
    assert!(!tokens.iter().any(|t| t.id == token_to_delete.id));
}

#[actix_web::test]
#[ignore = "Session cookies not preserved in actix test framework - use E2E tests"]
async fn test_delete_token_not_found() {
    let db = TestDb::new().await;
    let auth_token = create_test_token(&db.pool).await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config))
            .configure(routes::tokens::configure),
    )
    .await;

    let req = test::TestRequest::delete()
        .uri("/api/tokens/99999")
        .insert_header(("Authorization", format!("Bearer {}", auth_token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
#[ignore = "Session cookies not preserved in actix test framework - use E2E tests"]
async fn test_delete_token_unauthorized() {
    let db = TestDb::new().await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config))
            .configure(routes::tokens::configure),
    )
    .await;

    let req = test::TestRequest::delete()
        .uri("/api/tokens/1")
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
#[ignore = "Session cookies not preserved in actix test framework - use E2E tests"]
async fn test_delete_token_invalid_id_format() {
    let db = TestDb::new().await;
    let auth_token = create_test_token(&db.pool).await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config))
            .configure(routes::tokens::configure),
    )
    .await;

    let req = test::TestRequest::delete()
        .uri("/api/tokens/not-a-number")
        .insert_header(("Authorization", format!("Bearer {}", auth_token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404); // Path doesn't match
}

// =============================================================================
// Edge Cases and Security Tests
// =============================================================================

#[actix_web::test]
#[ignore = "Session cookies not preserved in actix test framework - use E2E tests"]
async fn test_cannot_delete_own_token_while_using_it() {
    let db = TestDb::new().await;
    let config = create_test_config();

    // Create a token
    let token = AuthTokenService::create(
        &db.pool,
        rustrak::models::CreateAuthToken {
            description: Some("Self-delete test".to_string()),
        },
    )
    .await
    .expect("Failed to create token");

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config))
            .configure(routes::tokens::configure),
    )
    .await;

    // Try to delete the token using itself for auth
    // Note: This is allowed in our implementation, but the deleted token
    // won't work for subsequent requests
    let req = test::TestRequest::delete()
        .uri(&format!("/api/tokens/{}", token.id))
        .insert_header(("Authorization", format!("Bearer {}", token.token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 204);

    // Verify the token no longer works
    let req = test::TestRequest::get()
        .uri("/api/tokens")
        .insert_header(("Authorization", format!("Bearer {}", token.token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
#[ignore = "Session cookies not preserved in actix test framework - use E2E tests"]
async fn test_token_last_used_at_updated() {
    let db = TestDb::new().await;
    let config = create_test_config();

    // Create a token
    let token = AuthTokenService::create(
        &db.pool,
        rustrak::models::CreateAuthToken {
            description: Some("Last used test".to_string()),
        },
    )
    .await
    .expect("Failed to create token");

    // Initially, last_used_at should be None
    let initial_token = AuthTokenService::get_by_token(&db.pool, &token.token)
        .await
        .unwrap()
        .unwrap();
    assert!(initial_token.last_used_at.is_none());

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config))
            .configure(routes::tokens::configure),
    )
    .await;

    // Use the token for a request
    let req = test::TestRequest::get()
        .uri("/api/tokens")
        .insert_header(("Authorization", format!("Bearer {}", token.token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    // Wait for the fire-and-forget async update to complete
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Now last_used_at should be set
    let updated_token = AuthTokenService::get_by_token(&db.pool, &token.token)
        .await
        .unwrap()
        .unwrap();
    assert!(updated_token.last_used_at.is_some());
}

#[actix_web::test]
#[ignore = "Session cookies not preserved in actix test framework - use E2E tests"]
async fn test_malformed_bearer_header() {
    let db = TestDb::new().await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config))
            .configure(routes::tokens::configure),
    )
    .await;

    // Missing "Bearer " prefix
    let req = test::TestRequest::get()
        .uri("/api/tokens")
        .insert_header(("Authorization", "abc123"))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
#[ignore = "Session cookies not preserved in actix test framework - use E2E tests"]
async fn test_empty_bearer_token() {
    let db = TestDb::new().await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config))
            .configure(routes::tokens::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/tokens")
        .insert_header(("Authorization", "Bearer "))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

// =============================================================================
// Get Token Tests
// =============================================================================

#[actix_web::test]
async fn test_get_token_returns_full_token() {
    let db = TestDb::new().await;
    let auth_token = create_test_token(&db.pool).await;
    let config = create_test_config();

    // Create a token to retrieve
    let token_to_get = AuthTokenService::create(
        &db.pool,
        rustrak::models::CreateAuthToken {
            description: Some("Token to retrieve".to_string()),
        },
    )
    .await
    .expect("Failed to create token");

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config))
            .configure(routes::tokens::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/tokens/{}", token_to_get.id))
        .insert_header(("Authorization", format!("Bearer {}", auth_token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["token"].as_str().unwrap(), token_to_get.token);
    assert_eq!(body["id"].as_i64().unwrap(), token_to_get.id as i64);
    assert_eq!(body["description"].as_str().unwrap(), "Token to retrieve");
}

#[actix_web::test]
async fn test_get_token_not_found() {
    let db = TestDb::new().await;
    let auth_token = create_test_token(&db.pool).await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config))
            .configure(routes::tokens::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/tokens/99999")
        .insert_header(("Authorization", format!("Bearer {}", auth_token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn test_get_token_unauthorized() {
    let db = TestDb::new().await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config))
            .configure(routes::tokens::configure),
    )
    .await;

    let req = test::TestRequest::get().uri("/api/tokens/1").to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}
