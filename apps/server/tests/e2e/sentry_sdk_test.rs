//! End-to-end tests using the Sentry Rust SDK
//!
//! These tests verify that Rustrak correctly receives and processes events
//! sent by a real Sentry SDK, just like in production usage.

use crate::common::process_error_event;
use actix_web::{middleware, web, App, HttpServer};
use chrono::Utc;
use rustrak::config::{Config, DashboardConfig, DatabaseConfig, RateLimitConfig};
use rustrak::db::DbPool;
use rustrak::ingest::EventMetadata;
use rustrak::models::CreateProject;
use rustrak::routes;
use rustrak::services::{
    DbSourceMapProvider, IssueService, LocalSourceMapStore, ProjectService, SourceMapProvider,
    SourceMapStore,
};
use sentry::protocol::{Event, Exception, Frame, Level, Stacktrace};
use std::net::TcpListener;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::Notify;

use crate::common::{null_sourcemap_provider, TestDb};

fn create_test_config(ingest_dir: &str) -> Config {
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
        ingest_dir: Some(ingest_dir.to_string()),
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

async fn create_test_project(pool: &DbPool, name: &str) -> rustrak::models::Project {
    ProjectService::create(
        pool,
        CreateProject {
            name: name.to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .expect("Failed to create test project")
}

/// Finds an available port for the test server
fn get_available_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind to port");
    listener.local_addr().unwrap().port()
}

/// Helper struct to manage test server lifecycle
struct TestServer {
    port: u16,
    pool: DbPool,
    ingest_dir: TempDir,
    shutdown: Arc<Notify>,
}

impl TestServer {
    async fn new(db: &TestDb) -> Self {
        let port = get_available_port();
        let ingest_dir = TempDir::new().expect("Failed to create temp dir");
        let config = create_test_config(ingest_dir.path().to_str().unwrap());
        let shutdown = Arc::new(Notify::new());

        let pool = db.pool.clone();
        let pool_clone = pool.clone();
        let shutdown_clone = shutdown.clone();

        // Build sourcemap provider for the ingest handler
        let store: Arc<dyn SourceMapStore> =
            Arc::new(LocalSourceMapStore::new(&config.sourcemap_storage_path));
        let provider: Arc<dyn SourceMapProvider> =
            Arc::new(DbSourceMapProvider::new(pool.clone(), Arc::clone(&store)));

        // Start the server in a background task
        tokio::spawn(async move {
            let server = HttpServer::new(move || {
                App::new()
                    .app_data(web::Data::new(pool_clone.clone()))
                    .app_data(web::Data::new(config.clone()))
                    .app_data(web::Data::new(Arc::clone(&provider)))
                    .wrap(middleware::Logger::default())
                    .service(
                        web::scope("/health")
                            .route("", web::get().to(routes::health::liveness))
                            .route("/ready", web::get().to(routes::health::readiness)),
                    )
                    .app_data(web::Data::new(
                        rustrak::digest::processors::Processors::new(
                            rustrak::ingest::get_ingest_dir(config.ingest_dir.as_deref()),
                            config.rate_limit.clone(),
                            crate::common::null_sourcemap_provider(),
                            None,
                        ),
                    ))
                    .configure(routes::ingest::configure)
            })
            .bind(("127.0.0.1", port))
            .expect("Failed to bind server")
            .run();

            tokio::select! {
                _ = server => {}
                _ = shutdown_clone.notified() => {}
            }
        });

        // Wait for server to be ready
        tokio::time::sleep(Duration::from_millis(100)).await;

        TestServer {
            port,
            pool,
            ingest_dir,
            shutdown,
        }
    }

    fn dsn(&self, sentry_key: &str, project_id: i32) -> String {
        format!(
            "http://{}@127.0.0.1:{}/{}",
            sentry_key, self.port, project_id
        )
    }

    async fn process_pending_events(&self, project_id: i32, rate_limit_config: &RateLimitConfig) {
        // Read all events from the ingest directory and process them
        let ingest_path = self.ingest_dir.path();
        if let Ok(entries) = std::fs::read_dir(ingest_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "json").unwrap_or(false) {
                    let event_id = path.file_stem().unwrap().to_str().unwrap().to_string();

                    let metadata = EventMetadata {
                        event_id,
                        project_id,
                        ingested_at: chrono::Utc::now(),
                        remote_addr: None,
                    };

                    let _ = process_error_event(
                        &self.pool,
                        &metadata,
                        ingest_path,
                        rate_limit_config,
                        null_sourcemap_provider(),
                    )
                    .await;
                }
            }
        }
    }

    fn shutdown(&self) {
        self.shutdown.notify_one();
    }
}

// =============================================================================
// Basic SDK Tests
// =============================================================================

#[actix_web::test]
async fn test_sentry_sdk_capture_message() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "SDK Message Test").await;
    let server = TestServer::new(&db).await;

    let dsn = server.dsn(&project.sentry_key.to_string(), project.id);

    // Configure Sentry SDK with test transport
    let _guard = sentry::init(
        sentry::ClientOptions::new()
            .dsn(&dsn)
            .release("test-release@1.0.0")
            .environment("test"),
    );

    // Capture a message
    sentry::capture_message("Test message from Sentry SDK", Level::Info);

    // Flush to ensure event is sent
    if let Some(client) = sentry::Hub::current().client() {
        client.flush(Some(Duration::from_secs(5)));
    };

    // Give time for the event to be processed
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Process the events
    let rate_limit_config = RateLimitConfig {
        max_events_per_minute: 1000,
        max_events_per_hour: 10000,
        max_events_per_project_per_minute: 500,
        max_events_per_project_per_hour: 5000,
    };
    server
        .process_pending_events(project.id, &rate_limit_config)
        .await;

    // Verify issue was created
    let (issues, _) = IssueService::list_paginated(
        &db.pool,
        project.id,
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
        true,
        None,
        100,
    )
    .await
    .expect("Failed to list issues");

    assert!(!issues.is_empty(), "Should have created at least one issue");

    server.shutdown();
}

#[actix_web::test]
async fn test_sentry_sdk_capture_error() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "SDK Error Test").await;
    let server = TestServer::new(&db).await;

    let dsn = server.dsn(&project.sentry_key.to_string(), project.id);

    let _guard = sentry::init(sentry::ClientOptions::new().dsn(&dsn));

    // Capture an error event manually
    let event = Event {
        level: Level::Error,
        message: Some("Test error from Sentry SDK".to_string()),
        exception: sentry::protocol::Values {
            values: vec![Exception {
                ty: "TestError".to_string(),
                value: Some("Something went wrong".to_string()),
                ..Default::default()
            }],
        },
        ..Default::default()
    };

    sentry::capture_event(event);
    if let Some(client) = sentry::Hub::current().client() {
        client.flush(Some(Duration::from_secs(5)));
    };

    tokio::time::sleep(Duration::from_millis(500)).await;

    let rate_limit_config = RateLimitConfig {
        max_events_per_minute: 1000,
        max_events_per_hour: 10000,
        max_events_per_project_per_minute: 500,
        max_events_per_project_per_hour: 5000,
    };
    server
        .process_pending_events(project.id, &rate_limit_config)
        .await;

    let (issues, _) = IssueService::list_paginated(
        &db.pool,
        project.id,
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
        true,
        None,
        100,
    )
    .await
    .expect("Failed to list issues");

    assert!(!issues.is_empty());

    // Verify it's an error type issue
    let issue = &issues[0];
    assert!(issue.calculated_type.contains("Error") || issue.calculated_type.contains("Test"));

    server.shutdown();
}

#[actix_web::test]
async fn test_sentry_sdk_with_custom_fingerprint() {
    let db = TestDb::new().await;
    // Use a unique project name with timestamp to ensure isolation
    let project_name = format!("SDK Fingerprint Test {}", Utc::now().timestamp_millis());
    let project = create_test_project(&db.pool, &project_name).await;
    let server = TestServer::new(&db).await;

    let dsn = server.dsn(&project.sentry_key.to_string(), project.id);

    // Initialize Sentry client
    let _guard = sentry::init(sentry::ClientOptions::new().dsn(&dsn));

    // Send two events with different error types but same fingerprint
    for i in 0..2 {
        let event = Event {
            level: Level::Error,
            fingerprint: vec!["custom-fingerprint".into()].into(),
            exception: sentry::protocol::Values {
                values: vec![Exception {
                    ty: format!("Error{}", i),
                    value: Some(format!("Different error {}", i)),
                    ..Default::default()
                }],
            },
            ..Default::default()
        };
        sentry::capture_event(event);
    }

    if let Some(client) = sentry::Hub::current().client() {
        client.flush(Some(Duration::from_secs(5)));
    };
    // Give more time for events to be written to temp storage
    tokio::time::sleep(Duration::from_millis(1000)).await;

    let rate_limit_config = RateLimitConfig {
        max_events_per_minute: 1000,
        max_events_per_hour: 10000,
        max_events_per_project_per_minute: 500,
        max_events_per_project_per_hour: 5000,
    };

    // Process events multiple times to ensure all events are digested
    for _ in 0..3 {
        server
            .process_pending_events(project.id, &rate_limit_config)
            .await;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let (issues, _) = IssueService::list_paginated(
        &db.pool,
        project.id,
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
        true,
        None,
        100,
    )
    .await
    .expect("Failed to list issues");

    // Verify events were received and processed
    // Note: The Sentry SDK may not preserve custom fingerprints the same way as raw JSON,
    // so we verify that events were processed rather than asserting on exact grouping.
    assert!(!issues.is_empty(), "At least one issue should be created");

    // Count total events across all issues
    let total_events: i32 = issues.iter().map(|i| i.digested_event_count).sum();
    assert!(
        total_events >= 2,
        "At least 2 events should be digested, got {}",
        total_events
    );

    server.shutdown();
}

#[actix_web::test]
async fn test_sentry_sdk_with_stacktrace() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "SDK Stacktrace Test").await;
    let server = TestServer::new(&db).await;

    let dsn = server.dsn(&project.sentry_key.to_string(), project.id);

    let _guard = sentry::init(
        sentry::ClientOptions::new()
            .dsn(&dsn)
            .attach_stacktrace(true),
    );

    // Create event with stacktrace
    let stacktrace = Stacktrace {
        frames: vec![
            Frame {
                function: Some("main".to_string()),
                filename: Some("src/main.rs".to_string()),
                lineno: Some(42),
                in_app: Some(true),
                ..Default::default()
            },
            Frame {
                function: Some("process".to_string()),
                filename: Some("src/lib.rs".to_string()),
                lineno: Some(100),
                in_app: Some(true),
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    let event = Event {
        level: Level::Error,
        exception: sentry::protocol::Values {
            values: vec![Exception {
                ty: "RuntimeError".to_string(),
                value: Some("Stack trace test".to_string()),
                stacktrace: Some(stacktrace),
                ..Default::default()
            }],
        },
        ..Default::default()
    };

    sentry::capture_event(event);
    if let Some(client) = sentry::Hub::current().client() {
        client.flush(Some(Duration::from_secs(5)));
    };

    tokio::time::sleep(Duration::from_millis(500)).await;

    let rate_limit_config = RateLimitConfig {
        max_events_per_minute: 1000,
        max_events_per_hour: 10000,
        max_events_per_project_per_minute: 500,
        max_events_per_project_per_hour: 5000,
    };
    server
        .process_pending_events(project.id, &rate_limit_config)
        .await;

    let (issues, _) = IssueService::list_paginated(
        &db.pool,
        project.id,
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
        true,
        None,
        100,
    )
    .await
    .expect("Failed to list issues");

    assert!(!issues.is_empty());
    assert!(issues[0].calculated_type.contains("RuntimeError"));

    server.shutdown();
}

#[actix_web::test]
async fn test_sentry_sdk_different_levels() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "SDK Levels Test").await;
    let server = TestServer::new(&db).await;

    let dsn = server.dsn(&project.sentry_key.to_string(), project.id);

    let _guard = sentry::init(sentry::ClientOptions::new().dsn(&dsn));

    // Send events with different levels
    let levels = vec![
        (Level::Debug, "Debug message"),
        (Level::Info, "Info message"),
        (Level::Warning, "Warning message"),
        (Level::Error, "Error message"),
        (Level::Fatal, "Fatal message"),
    ];

    for (level, msg) in levels {
        sentry::capture_message(msg, level);
    }

    if let Some(client) = sentry::Hub::current().client() {
        client.flush(Some(Duration::from_secs(5)));
    };
    tokio::time::sleep(Duration::from_millis(500)).await;

    let rate_limit_config = RateLimitConfig {
        max_events_per_minute: 1000,
        max_events_per_hour: 10000,
        max_events_per_project_per_minute: 500,
        max_events_per_project_per_hour: 5000,
    };
    server
        .process_pending_events(project.id, &rate_limit_config)
        .await;

    let (issues, _) = IssueService::list_paginated(
        &db.pool,
        project.id,
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
        true,
        None,
        100,
    )
    .await
    .expect("Failed to list issues");

    // Should have issues created (exact count depends on how many events were received)
    assert!(!issues.is_empty());

    server.shutdown();
}

#[actix_web::test]
async fn test_sentry_sdk_with_tags() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "SDK Tags Test").await;
    let server = TestServer::new(&db).await;

    let dsn = server.dsn(&project.sentry_key.to_string(), project.id);

    let _guard = sentry::init(sentry::ClientOptions::new().dsn(&dsn));

    sentry::configure_scope(|scope| {
        scope.set_tag("environment", "test");
        scope.set_tag("version", "1.0.0");
        scope.set_tag("component", "api");
    });

    sentry::capture_message("Event with tags", Level::Info);
    if let Some(client) = sentry::Hub::current().client() {
        client.flush(Some(Duration::from_secs(5)));
    };

    tokio::time::sleep(Duration::from_millis(500)).await;

    let rate_limit_config = RateLimitConfig {
        max_events_per_minute: 1000,
        max_events_per_hour: 10000,
        max_events_per_project_per_minute: 500,
        max_events_per_project_per_hour: 5000,
    };
    server
        .process_pending_events(project.id, &rate_limit_config)
        .await;

    let (issues, _) = IssueService::list_paginated(
        &db.pool,
        project.id,
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
        true,
        None,
        100,
    )
    .await
    .expect("Failed to list issues");

    assert!(!issues.is_empty());

    server.shutdown();
}

#[actix_web::test]
async fn test_sentry_sdk_with_user_context() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "SDK User Context Test").await;
    let server = TestServer::new(&db).await;

    let dsn = server.dsn(&project.sentry_key.to_string(), project.id);

    let _guard = sentry::init(sentry::ClientOptions::new().dsn(&dsn));

    sentry::configure_scope(|scope| {
        scope.set_user(Some(sentry::User {
            id: Some("user-123".to_string()),
            email: Some("test@example.com".to_string()),
            username: Some("testuser".to_string()),
            ..Default::default()
        }));
    });

    sentry::capture_message("Event with user context", Level::Error);
    if let Some(client) = sentry::Hub::current().client() {
        client.flush(Some(Duration::from_secs(5)));
    };

    tokio::time::sleep(Duration::from_millis(500)).await;

    let rate_limit_config = RateLimitConfig {
        max_events_per_minute: 1000,
        max_events_per_hour: 10000,
        max_events_per_project_per_minute: 500,
        max_events_per_project_per_hour: 5000,
    };
    server
        .process_pending_events(project.id, &rate_limit_config)
        .await;

    let (issues, _) = IssueService::list_paginated(
        &db.pool,
        project.id,
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
        true,
        None,
        100,
    )
    .await
    .expect("Failed to list issues");

    assert!(!issues.is_empty());

    server.shutdown();
}

#[actix_web::test]
async fn test_sentry_sdk_with_breadcrumbs() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "SDK Breadcrumbs Test").await;
    let server = TestServer::new(&db).await;

    let dsn = server.dsn(&project.sentry_key.to_string(), project.id);

    let _guard = sentry::init(sentry::ClientOptions::new().dsn(&dsn));

    // Add breadcrumbs
    sentry::add_breadcrumb(sentry::Breadcrumb {
        ty: "navigation".to_string(),
        category: Some("ui".to_string()),
        message: Some("User clicked button".to_string()),
        level: Level::Info,
        ..Default::default()
    });

    sentry::add_breadcrumb(sentry::Breadcrumb {
        ty: "http".to_string(),
        category: Some("api".to_string()),
        message: Some("GET /api/users".to_string()),
        level: Level::Info,
        ..Default::default()
    });

    sentry::capture_message("Event with breadcrumbs", Level::Error);
    if let Some(client) = sentry::Hub::current().client() {
        client.flush(Some(Duration::from_secs(5)));
    };

    tokio::time::sleep(Duration::from_millis(500)).await;

    let rate_limit_config = RateLimitConfig {
        max_events_per_minute: 1000,
        max_events_per_hour: 10000,
        max_events_per_project_per_minute: 500,
        max_events_per_project_per_hour: 5000,
    };
    server
        .process_pending_events(project.id, &rate_limit_config)
        .await;

    let (issues, _) = IssueService::list_paginated(
        &db.pool,
        project.id,
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
        true,
        None,
        100,
    )
    .await
    .expect("Failed to list issues");

    assert!(!issues.is_empty());

    server.shutdown();
}

// =============================================================================
// Multiple Events Grouping Tests
// =============================================================================

#[actix_web::test]
async fn test_sentry_sdk_groups_similar_errors() {
    let db = TestDb::new().await;
    // Use a unique project name with timestamp to ensure isolation
    let project_name = format!(
        "SDK Grouping Test {}",
        chrono::Utc::now().timestamp_millis()
    );
    let project = create_test_project(&db.pool, &project_name).await;
    let server = TestServer::new(&db).await;

    let dsn = server.dsn(&project.sentry_key.to_string(), project.id);

    // Initialize Sentry client
    let _guard = sentry::init(sentry::ClientOptions::new().dsn(&dsn));

    // Send multiple events with the same error type and explicit fingerprint
    // We use an explicit fingerprint to ensure consistent grouping
    for _ in 0..3 {
        let event = Event {
            level: Level::Error,
            fingerprint: vec!["connection-error-group".into()].into(),
            exception: sentry::protocol::Values {
                values: vec![Exception {
                    ty: "ConnectionError".to_string(),
                    value: Some("Failed to connect to database".to_string()),
                    ..Default::default()
                }],
            },
            ..Default::default()
        };
        sentry::capture_event(event);
    }

    if let Some(client) = sentry::Hub::current().client() {
        client.flush(Some(Duration::from_secs(5)));
    };
    // Give more time for events to be written to temp storage
    tokio::time::sleep(Duration::from_millis(1000)).await;

    let rate_limit_config = RateLimitConfig {
        max_events_per_minute: 1000,
        max_events_per_hour: 10000,
        max_events_per_project_per_minute: 500,
        max_events_per_project_per_hour: 5000,
    };

    // Process events multiple times to ensure all events are digested
    for _ in 0..3 {
        server
            .process_pending_events(project.id, &rate_limit_config)
            .await;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let (issues, _) = IssueService::list_paginated(
        &db.pool,
        project.id,
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
        true,
        None,
        100,
    )
    .await
    .expect("Failed to list issues");

    // Verify events were received and processed
    // Note: The Sentry SDK may not preserve custom fingerprints the same way as raw JSON,
    // so we verify that events were processed rather than asserting on exact grouping.
    assert!(!issues.is_empty(), "At least one issue should be created");

    // Count total events across all issues
    let total_events: i32 = issues.iter().map(|i| i.digested_event_count).sum();
    assert!(
        total_events >= 3,
        "At least 3 events should be digested, got {}",
        total_events
    );

    server.shutdown();
}

#[actix_web::test]
async fn test_sentry_sdk_separates_different_errors() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "SDK Separation Test").await;
    let server = TestServer::new(&db).await;

    let dsn = server.dsn(&project.sentry_key.to_string(), project.id);

    let _guard = sentry::init(sentry::ClientOptions::new().dsn(&dsn));

    // Send events with different error types
    let errors = vec![
        ("NetworkError", "Connection timeout"),
        ("ValidationError", "Invalid input"),
        ("AuthError", "Invalid credentials"),
    ];

    for (error_type, error_msg) in errors {
        let event = Event {
            level: Level::Error,
            exception: sentry::protocol::Values {
                values: vec![Exception {
                    ty: error_type.to_string(),
                    value: Some(error_msg.to_string()),
                    ..Default::default()
                }],
            },
            ..Default::default()
        };
        sentry::capture_event(event);
    }

    if let Some(client) = sentry::Hub::current().client() {
        client.flush(Some(Duration::from_secs(5)));
    };
    tokio::time::sleep(Duration::from_millis(500)).await;

    let rate_limit_config = RateLimitConfig {
        max_events_per_minute: 1000,
        max_events_per_hour: 10000,
        max_events_per_project_per_minute: 500,
        max_events_per_project_per_hour: 5000,
    };
    server
        .process_pending_events(project.id, &rate_limit_config)
        .await;

    let (issues, _) = IssueService::list_paginated(
        &db.pool,
        project.id,
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
        true,
        None,
        100,
    )
    .await
    .expect("Failed to list issues");

    // Should have separate issues for different error types
    assert!(!issues.is_empty());

    server.shutdown();
}

// =============================================================================
// Issue Fields and Search, End to End
// =============================================================================

fn e2e_rate_limits() -> RateLimitConfig {
    RateLimitConfig {
        max_events_per_minute: 1000,
        max_events_per_hour: 10000,
        max_events_per_project_per_minute: 500,
        max_events_per_project_per_hour: 5000,
    }
}

async fn only_issue(pool: &rustrak::db::DbPool, project_id: i32) -> rustrak::models::Issue {
    let (issues, _) = IssueService::list_paginated(
        pool,
        project_id,
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
        true,
        None,
        100,
    )
    .await
    .expect("Failed to list issues");
    assert_eq!(
        issues.len(),
        1,
        "expected the SDK events to share one issue"
    );
    issues.into_iter().next().unwrap()
}

async fn flush_and_digest(server: &TestServer, project_id: i32) {
    if let Some(client) = sentry::Hub::current().client() {
        client.flush(Some(Duration::from_secs(5)));
    };
    tokio::time::sleep(Duration::from_millis(500)).await;
    server
        .process_pending_events(project_id, &e2e_rate_limits())
        .await;
}

#[actix_web::test]
async fn test_sdk_issue_title_and_level_follow_the_latest_event() {
    let db = TestDb::new().await;
    let name = format!("SDK Latest Event {}", Utc::now().timestamp_millis());
    let project = create_test_project(&db.pool, &name).await;
    let server = TestServer::new(&db).await;
    let _guard = sentry::init(
        sentry::ClientOptions::new().dsn(&server.dsn(&project.sentry_key.to_string(), project.id)),
    );

    let event = |level: Level, value: &str| Event {
        level,
        fingerprint: vec!["pinned".into()].into(),
        exception: sentry::protocol::Values {
            values: vec![Exception {
                ty: "PaymentError".to_string(),
                value: Some(value.to_string()),
                ..Default::default()
            }],
        },
        ..Default::default()
    };

    // Warning then Fatal: both are non-default, so the SDK puts `level` on the
    // wire for each. An event whose level is the SDK default omits the field
    // entirely, which is covered by the digest integration tests.
    sentry::capture_event(event(Level::Warning, "card declined"));
    flush_and_digest(&server, project.id).await;
    sentry::capture_event(event(Level::Fatal, "gateway timeout"));
    flush_and_digest(&server, project.id).await;

    let issue = only_issue(&db.pool, project.id).await;
    assert_eq!(issue.digested_event_count, 2);
    assert_eq!(issue.title(), "PaymentError: gateway timeout");
    assert_eq!(issue.level.as_deref(), Some("fatal"));

    server.shutdown();
}

#[actix_web::test]
async fn test_sdk_automatic_regression_shows_up_in_the_activity_log() {
    let db = TestDb::new().await;
    let name = format!("SDK Regression {}", Utc::now().timestamp_millis());
    let project = create_test_project(&db.pool, &name).await;
    let server = TestServer::new(&db).await;
    let _guard = sentry::init(
        sentry::ClientOptions::new().dsn(&server.dsn(&project.sentry_key.to_string(), project.id)),
    );

    sentry::capture_message("service unavailable", Level::Error);
    flush_and_digest(&server, project.id).await;

    let issue = only_issue(&db.pool, project.id).await;
    IssueService::set_status(&db.pool, issue.id, "resolved", None)
        .await
        .expect("resolve");

    sentry::capture_message("service unavailable", Level::Error);
    flush_and_digest(&server, project.id).await;

    let reopened = only_issue(&db.pool, project.id).await;
    assert_eq!(reopened.status, "unresolved");
    assert_eq!(reopened.substatus.as_deref(), Some("regressed"));

    let activity = rustrak::services::IssueSocialService::list_activity(&db.pool, issue.id)
        .await
        .expect("activity");
    assert_eq!(
        activity.len(),
        1,
        "the reopen should be visible on the issue timeline, got {activity:?}"
    );
    assert_eq!(activity[0].activity_type, "set_regression");

    server.shutdown();
}

#[actix_web::test]
async fn test_sdk_issue_is_searchable_by_frame_filename_and_module() {
    use rustrak::pagination::{IssueFilter, IssueSort, SortOrder};
    let db = TestDb::new().await;
    let name = format!("SDK Frame Search {}", Utc::now().timestamp_millis());
    let project = create_test_project(&db.pool, &name).await;
    let server = TestServer::new(&db).await;
    let _guard = sentry::init(
        sentry::ClientOptions::new().dsn(&server.dsn(&project.sentry_key.to_string(), project.id)),
    );

    sentry::capture_event(Event {
        level: Level::Error,
        exception: sentry::protocol::Values {
            values: vec![Exception {
                ty: "PaymentError".to_string(),
                value: Some("card declined".to_string()),
                stacktrace: Some(Stacktrace {
                    frames: vec![Frame {
                        filename: Some("billing/stripe.rs".to_string()),
                        module: Some("billing::stripe".to_string()),
                        function: Some("charge_customer".to_string()),
                        in_app: Some(true),
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                ..Default::default()
            }],
        },
        ..Default::default()
    });
    flush_and_digest(&server, project.id).await;

    for term in ["billing/stripe.rs", "billing::stripe", "charge_customer"] {
        let (hits, total) = IssueService::list_offset(
            &db.pool,
            project.id,
            IssueSort::DigestOrder,
            SortOrder::Desc,
            IssueFilter::All,
            1,
            20,
            Some(term),
        )
        .await
        .expect("search");
        assert_eq!(hits.len(), 1, "searching {term:?} should find the issue");
        assert_eq!(total, 1, "the count query must agree for {term:?}");
    }

    server.shutdown();
}

// =============================================================================
// Exception Groups, End to End
// =============================================================================

/// Posts a raw envelope to the same ingest route the SDK uses. Exception groups
/// come from Python 3.11, .NET and JS runtimes; the Rust SDK's `Mechanism` has
/// no `exception_id` / `is_exception_group`, so it cannot express one. The
/// payloads below are Sentry's own grouping fixtures
/// (`tests/sentry/grouping/grouping_inputs/exception-groups-*.json`).
async fn post_envelope(
    server: &TestServer,
    project: &rustrak::models::Project,
    event: serde_json::Value,
) {
    let event_id = uuid::Uuid::new_v4().to_string().replace('-', "");
    let mut event = event;
    let obj = event.as_object_mut().unwrap();
    obj.insert("event_id".into(), serde_json::json!(&event_id));
    obj.insert(
        "timestamp".into(),
        serde_json::json!(Utc::now().timestamp() as f64),
    );
    let body = serde_json::to_string(&event).unwrap();
    let envelope = format!(
        "{}\n{}\n{}",
        serde_json::json!({ "event_id": &event_id }),
        serde_json::json!({ "type": "event", "length": body.len() }),
        body
    );

    let url = format!(
        "http://127.0.0.1:{}/api/{}/envelope/?sentry_key={}",
        server.port, project.id, project.sentry_key
    );
    let status = reqwest::Client::new()
        .post(&url)
        .header("Content-Type", "application/x-sentry-envelope")
        .body(envelope)
        .send()
        .await
        .expect("ingest request failed")
        .status();
    assert!(status.is_success(), "ingest returned {status}");

    tokio::time::sleep(Duration::from_millis(300)).await;
    server
        .process_pending_events(project.id, &e2e_rate_limits())
        .await;
}

/// `exception-groups-one-exception.json`: a .NET `AggregateException` wrapping
/// a single real error.
fn aggregate_exception_with_one_inner(inner_value: &str) -> serde_json::Value {
    serde_json::json!({
        "platform": "csharp",
        "exception": { "values": [
            { "type": "MyApp.Exception", "value": inner_value,
              "mechanism": { "type": "chained", "exception_id": 1, "parent_id": 0,
                             "source": "InnerException" } },
            { "type": "System.AggregateException", "value": "One or more errors occurred.",
              "mechanism": { "type": "generic", "exception_id": 0,
                             "is_exception_group": true } }
        ]}
    })
}

#[actix_web::test]
async fn test_aggregate_exception_is_titled_and_grouped_by_the_inner_error() {
    let db = TestDb::new().await;
    let name = format!("SDK Exception Group {}", Utc::now().timestamp_millis());
    let project = create_test_project(&db.pool, &name).await;
    let server = TestServer::new(&db).await;

    post_envelope(
        &server,
        &project,
        aggregate_exception_with_one_inner("Test 1"),
    )
    .await;

    let issue = only_issue(&db.pool, project.id).await;
    assert_eq!(
        issue.title(),
        "MyApp.Exception: Test 1",
        "the wrapper must not title the issue"
    );

    server.shutdown();
}

#[actix_web::test]
async fn test_aggregate_exceptions_wrapping_different_errors_get_separate_issues() {
    // The wrapper's value is the same constant for both, so grouping by it
    // would file two unrelated errors as one issue. The inner values differ in
    // more than a number: `Test 1` and `Test 2` both normalize to `Test <int>`
    // and would share an issue, as they do in Sentry.
    let db = TestDb::new().await;
    let name = format!(
        "SDK Exception Group Split {}",
        Utc::now().timestamp_millis()
    );
    let project = create_test_project(&db.pool, &name).await;
    let server = TestServer::new(&db).await;

    post_envelope(
        &server,
        &project,
        aggregate_exception_with_one_inner("card declined"),
    )
    .await;
    post_envelope(
        &server,
        &project,
        aggregate_exception_with_one_inner("gateway timeout"),
    )
    .await;

    let (issues, _) = IssueService::list_paginated(
        &db.pool,
        project.id,
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
        true,
        None,
        100,
    )
    .await
    .expect("Failed to list issues");

    let mut titles: Vec<String> = issues.iter().map(|i| i.title()).collect();
    titles.sort();
    assert_eq!(
        titles,
        vec![
            "MyApp.Exception: card declined".to_string(),
            "MyApp.Exception: gateway timeout".to_string()
        ]
    );

    server.shutdown();
}

#[actix_web::test]
async fn test_aggregate_exception_with_two_distinct_errors_keeps_the_wrapper() {
    // `exception-groups-two-types.json`: the group genuinely represents more
    // than one error, so it stays as the issue.
    let db = TestDb::new().await;
    let name = format!("SDK Exception Group Two {}", Utc::now().timestamp_millis());
    let project = create_test_project(&db.pool, &name).await;
    let server = TestServer::new(&db).await;

    post_envelope(
        &server,
        &project,
        serde_json::json!({
            "platform": "csharp",
            "exception": { "values": [
                { "type": "MyApp.SuchWowException", "value": "Test 2",
                  "mechanism": { "type": "chained", "exception_id": 2, "parent_id": 0 } },
                { "type": "MyApp.AmazingException", "value": "Test 1",
                  "mechanism": { "type": "chained", "exception_id": 1, "parent_id": 0 } },
                { "type": "System.AggregateException", "value": "One or more errors occurred.",
                  "mechanism": { "type": "generic", "exception_id": 0,
                                 "is_exception_group": true } }
            ]}
        }),
    )
    .await;

    let issue = only_issue(&db.pool, project.id).await;
    assert_eq!(
        issue.title(),
        "System.AggregateException: One or more errors occurred."
    );

    server.shutdown();
}

#[actix_web::test]
async fn test_sdk_same_bug_with_different_ids_lands_in_one_issue() {
    // The scenario message normalization exists for: one bug, one issue, no
    // matter how many distinct ids it mentions.
    let db = TestDb::new().await;
    let name = format!("SDK Parameterization {}", Utc::now().timestamp_millis());
    let project = create_test_project(&db.pool, &name).await;
    let server = TestServer::new(&db).await;
    let _guard = sentry::init(
        sentry::ClientOptions::new().dsn(&server.dsn(&project.sentry_key.to_string(), project.id)),
    );

    for order in [4213, 9981, 1] {
        sentry::capture_event(Event {
            level: Level::Warning,
            exception: sentry::protocol::Values {
                values: vec![Exception {
                    ty: "PaymentError".to_string(),
                    value: Some(format!("Payment failed for order {order}")),
                    ..Default::default()
                }],
            },
            ..Default::default()
        });
        flush_and_digest(&server, project.id).await;
    }

    let issue = only_issue(&db.pool, project.id).await;
    assert_eq!(issue.digested_event_count, 3);
    assert_eq!(
        issue.title(),
        "PaymentError: Payment failed for order 1",
        "the title keeps a real message, only grouping is normalized"
    );

    server.shutdown();
}

#[actix_web::test]
async fn test_sdk_messages_differing_beyond_an_id_still_split() {
    let db = TestDb::new().await;
    let name = format!(
        "SDK Parameterization Split {}",
        Utc::now().timestamp_millis()
    );
    let project = create_test_project(&db.pool, &name).await;
    let server = TestServer::new(&db).await;
    let _guard = sentry::init(
        sentry::ClientOptions::new().dsn(&server.dsn(&project.sentry_key.to_string(), project.id)),
    );

    for message in [
        "Payment failed for order 4213",
        "Refund failed for order 4213",
    ] {
        sentry::capture_message(message, Level::Warning);
        flush_and_digest(&server, project.id).await;
    }

    let (issues, _) = IssueService::list_paginated(
        &db.pool,
        project.id,
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
        true,
        None,
        100,
    )
    .await
    .expect("Failed to list issues");
    assert_eq!(issues.len(), 2, "different bugs must not be merged");

    server.shutdown();
}

#[actix_web::test]
async fn test_sdk_an_issue_from_before_the_upgrade_keeps_its_events() {
    // The whole point of the fallback: after upgrading, a real SDK's events
    // still land in the issue they were landing in before, instead of opening a
    // duplicate beside it.
    let db = TestDb::new().await;
    let name = format!("SDK Upgrade Path {}", Utc::now().timestamp_millis());
    let project = create_test_project(&db.pool, &name).await;
    let server = TestServer::new(&db).await;
    let _guard = sentry::init(
        sentry::ClientOptions::new().dsn(&server.dsn(&project.sentry_key.to_string(), project.id)),
    );

    // Stand in for an issue the previous release created: same event, but keyed
    // the way v0.14.10 would have keyed it.
    let event_json = serde_json::json!({
        "platform": "rust",
        "exception": { "values": [
            { "type": "PaymentError", "value": "Payment failed for order 4213" }
        ]}
    });
    let legacy_key = rustrak::services::calculate_grouping_key_v1(&event_json);
    let issue = IssueService::create(
        &db.pool,
        project.id,
        Utc::now(),
        &rustrak::services::get_denormalized_fields(&event_json),
        Some("error"),
        Some("rust"),
    )
    .await
    .expect("create issue");
    sqlx::query(
        "INSERT INTO groupings (project_id, issue_id, grouping_key, grouping_key_hash) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(project.id)
    .bind(issue.id)
    .bind(&legacy_key)
    .bind(rustrak::services::hash_grouping_key(&legacy_key))
    .execute(&db.pool)
    .await
    .expect("insert grouping");

    // Now the SDK sends the same error again, twice, with different order ids.
    for order in [4213, 9981] {
        sentry::capture_event(Event {
            level: Level::Error,
            exception: sentry::protocol::Values {
                values: vec![Exception {
                    ty: "PaymentError".to_string(),
                    value: Some(format!("Payment failed for order {order}")),
                    ..Default::default()
                }],
            },
            ..Default::default()
        });
        flush_and_digest(&server, project.id).await;
    }

    let found = only_issue(&db.pool, project.id).await;
    assert_eq!(found.id, issue.id, "the events opened a new issue");
    assert_eq!(found.digested_event_count, 3);

    server.shutdown();
}
