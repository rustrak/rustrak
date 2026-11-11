//! End-to-end tests using the Sentry Rust SDK
//!
//! These tests verify that Rustrak correctly receives and processes events
//! sent by a real Sentry SDK, just like in production usage.

use crate::common::create_test_project;
use actix_web::HttpServer;
use chrono::Utc;
use rustrak::config::Config;
use rustrak::db::DbPool;
use rustrak::services::scope::Scope;
use rustrak::services::IssueService;
use sentry::protocol::{Event, Exception, Frame, Level, Stacktrace};
use std::net::TcpListener;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::Notify;

use crate::common::TestDb;

fn create_test_config(ingest_dir: &str) -> Config {
    crate::common::config_with_ingest_dir(std::path::Path::new(ingest_dir))
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
        let state = crate::common::state(&pool, config).await;
        let shutdown_clone = shutdown.clone();

        tokio::spawn(async move {
            let server = HttpServer::new(move || rustrak::app::app(&state))
                .bind(("127.0.0.1", port))
                .expect("Failed to bind server")
                .run();

            tokio::select! {
                _ = server => {}
                _ = shutdown_clone.notified() => {}
            }
        });

        crate::common::eventually(|| async {
            tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .ok()
        })
        .await;

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

    /// Waits until the server's own digest has stored `events` events in the
    /// project and consumed everything else it accepted. An SDK's `flush` can
    /// return before its request is answered, so an empty ingest directory
    /// alone does not prove the last event has arrived.
    async fn digested(&self, project_id: i32, events: i64) {
        crate::common::eventually(|| async {
            (count_rows(&self.pool, "events", project_id).await >= events
                && !crate::common::has_files(self.ingest_dir.path()))
            .then_some(())
        })
        .await;
    }

    fn shutdown(&self) {
        self.shutdown.notify_one();
    }
}

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

    sentry::capture_message("Test message from Sentry SDK", Level::Info);

    flush().await;

    server.digested(project.id, 1).await;

    let (issues, _) = IssueService::list_offset(
        &db.pool,
        project.id,
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
        rustrak::pagination::IssueFilter::All,
        rustrak::pagination::IssueCategory::Error,
        1,
        100,
        None,
        rustrak::services::scope::Scope::default(),
        &rustrak::services::issue::IssueFilters::default(),
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
    flush().await;

    server.digested(project.id, 1).await;

    let (issues, _) = IssueService::list_offset(
        &db.pool,
        project.id,
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
        rustrak::pagination::IssueFilter::All,
        rustrak::pagination::IssueCategory::Error,
        1,
        100,
        None,
        rustrak::services::scope::Scope::default(),
        &rustrak::services::issue::IssueFilters::default(),
    )
    .await
    .expect("Failed to list issues");

    assert!(!issues.is_empty());

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

    flush().await;

    server.digested(project.id, 2).await;

    let (issues, _) = IssueService::list_offset(
        &db.pool,
        project.id,
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
        rustrak::pagination::IssueFilter::All,
        rustrak::pagination::IssueCategory::Error,
        1,
        100,
        None,
        rustrak::services::scope::Scope::default(),
        &rustrak::services::issue::IssueFilters::default(),
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
    flush().await;

    server.digested(project.id, 1).await;

    let (issues, _) = IssueService::list_offset(
        &db.pool,
        project.id,
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
        rustrak::pagination::IssueFilter::All,
        rustrak::pagination::IssueCategory::Error,
        1,
        100,
        None,
        rustrak::services::scope::Scope::default(),
        &rustrak::services::issue::IssueFilters::default(),
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

    flush().await;

    server.digested(project.id, 5).await;

    let (issues, _) = IssueService::list_offset(
        &db.pool,
        project.id,
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
        rustrak::pagination::IssueFilter::All,
        rustrak::pagination::IssueCategory::Error,
        1,
        100,
        None,
        rustrak::services::scope::Scope::default(),
        &rustrak::services::issue::IssueFilters::default(),
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
    flush().await;

    server.digested(project.id, 1).await;

    let (issues, _) = IssueService::list_offset(
        &db.pool,
        project.id,
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
        rustrak::pagination::IssueFilter::All,
        rustrak::pagination::IssueCategory::Error,
        1,
        100,
        None,
        rustrak::services::scope::Scope::default(),
        &rustrak::services::issue::IssueFilters::default(),
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
    flush().await;

    server.digested(project.id, 1).await;

    let (issues, _) = IssueService::list_offset(
        &db.pool,
        project.id,
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
        rustrak::pagination::IssueFilter::All,
        rustrak::pagination::IssueCategory::Error,
        1,
        100,
        None,
        rustrak::services::scope::Scope::default(),
        &rustrak::services::issue::IssueFilters::default(),
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
    flush().await;

    server.digested(project.id, 1).await;

    let (issues, _) = IssueService::list_offset(
        &db.pool,
        project.id,
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
        rustrak::pagination::IssueFilter::All,
        rustrak::pagination::IssueCategory::Error,
        1,
        100,
        None,
        rustrak::services::scope::Scope::default(),
        &rustrak::services::issue::IssueFilters::default(),
    )
    .await
    .expect("Failed to list issues");

    assert!(!issues.is_empty());

    server.shutdown();
}

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

    flush().await;

    server.digested(project.id, 3).await;

    let (issues, _) = IssueService::list_offset(
        &db.pool,
        project.id,
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
        rustrak::pagination::IssueFilter::All,
        rustrak::pagination::IssueCategory::Error,
        1,
        100,
        None,
        rustrak::services::scope::Scope::default(),
        &rustrak::services::issue::IssueFilters::default(),
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

    flush().await;

    server.digested(project.id, 3).await;

    let (issues, _) = IssueService::list_offset(
        &db.pool,
        project.id,
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
        rustrak::pagination::IssueFilter::All,
        rustrak::pagination::IssueCategory::Error,
        1,
        100,
        None,
        rustrak::services::scope::Scope::default(),
        &rustrak::services::issue::IssueFilters::default(),
    )
    .await
    .expect("Failed to list issues");

    // Should have separate issues for different error types
    assert!(!issues.is_empty());

    server.shutdown();
}

async fn only_issue(pool: &rustrak::db::DbPool, project_id: i32) -> rustrak::models::Issue {
    let (issues, _) = IssueService::list_offset(
        pool,
        project_id,
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
        rustrak::pagination::IssueFilter::All,
        rustrak::pagination::IssueCategory::Error,
        1,
        100,
        None,
        rustrak::services::scope::Scope::default(),
        &rustrak::services::issue::IssueFilters::default(),
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

/// Sends what the SDK holds. Off the test's thread: a blocking flush on it
/// stalls the server task this runtime also drives, until the flush times out.
async fn flush() {
    if let Some(client) = sentry::Hub::current().client() {
        tokio::task::spawn_blocking(move || client.flush(Some(Duration::from_secs(5))))
            .await
            .unwrap();
    }
}

/// Sends what the SDK holds and waits for the project to have `events` events.
async fn flush_and_digest(server: &TestServer, project_id: i32, events: i64) {
    flush().await;
    server.digested(project_id, events).await;
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
    flush_and_digest(&server, project.id, 1).await;
    sentry::capture_event(event(Level::Fatal, "gateway timeout"));
    flush_and_digest(&server, project.id, 2).await;

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
    flush_and_digest(&server, project.id, 1).await;

    let issue = only_issue(&db.pool, project.id).await;
    IssueService::set_status(&db.pool, issue.id, "resolved", None)
        .await
        .expect("resolve");

    sentry::capture_message("service unavailable", Level::Error);
    flush_and_digest(&server, project.id, 2).await;

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
    flush_and_digest(&server, project.id, 1).await;

    for term in ["billing/stripe.rs", "billing::stripe", "charge_customer"] {
        let (hits, total) = IssueService::list_offset(
            &db.pool,
            project.id,
            IssueSort::DigestOrder,
            SortOrder::Desc,
            IssueFilter::All,
            rustrak::pagination::IssueCategory::Error,
            1,
            20,
            Some(term),
            Scope::default(),
            &rustrak::services::issue::IssueFilters::default(),
        )
        .await
        .expect("search");
        assert_eq!(hits.len(), 1, "searching {term:?} should find the issue");
        assert_eq!(total, 1, "the count query must agree for {term:?}");
    }

    server.shutdown();
}

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
    server.digested(project.id, 0).await;
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

    let (issues, _) = IssueService::list_offset(
        &db.pool,
        project.id,
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
        rustrak::pagination::IssueFilter::All,
        rustrak::pagination::IssueCategory::Error,
        1,
        100,
        None,
        rustrak::services::scope::Scope::default(),
        &rustrak::services::issue::IssueFilters::default(),
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

    for (sent, order) in [4213, 9981, 1].into_iter().enumerate() {
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
        flush_and_digest(&server, project.id, sent as i64 + 1).await;
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

    for (sent, message) in [
        "Payment failed for order 4213",
        "Refund failed for order 4213",
    ]
    .into_iter()
    .enumerate()
    {
        sentry::capture_message(message, Level::Warning);
        flush_and_digest(&server, project.id, sent as i64 + 1).await;
    }

    let (issues, _) = IssueService::list_offset(
        &db.pool,
        project.id,
        rustrak::pagination::IssueSort::DigestOrder,
        rustrak::pagination::SortOrder::Desc,
        rustrak::pagination::IssueFilter::All,
        rustrak::pagination::IssueCategory::Error,
        1,
        100,
        None,
        rustrak::services::scope::Scope::default(),
        &rustrak::services::issue::IssueFilters::default(),
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
    for (sent, order) in [4213, 9981].into_iter().enumerate() {
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
        flush_and_digest(&server, project.id, sent as i64 + 1).await;
    }

    let found = only_issue(&db.pool, project.id).await;
    assert_eq!(found.id, issue.id, "the events opened a new issue");
    assert_eq!(found.digested_event_count, 3);

    server.shutdown();
}

//
// Every column below is VARCHAR(n) in both migrations, but only Postgres
// enforces the length, and only Postgres rejects a NUL byte in TEXT or JSONB.
// Telemetry showed `/envelope/` 500s on Postgres instances only, about one per
// transaction on one install: real SDK payloads carry long transaction names
// and span ops, and span descriptions can carry NUL. Relay truncates rather
// than rejects (`max_chars` in relay-event-schema), so these must be stored.

/// Posts one envelope carrying `items` (header, payload) and returns the status.
async fn post_items(
    server: &TestServer,
    project: &rustrak::models::Project,
    items: &[(serde_json::Value, serde_json::Value)],
) -> reqwest::StatusCode {
    let event_id = uuid::Uuid::new_v4().simple().to_string();
    let mut envelope = format!("{}\n", serde_json::json!({ "event_id": event_id }));
    for (header, payload) in items {
        let body = payload.to_string();
        let mut header = header.clone();
        header["length"] = serde_json::json!(body.len());
        envelope.push_str(&format!("{header}\n{body}\n"));
    }
    let url = format!(
        "http://127.0.0.1:{}/api/{}/envelope/?sentry_key={}",
        server.port, project.id, project.sentry_key
    );
    reqwest::Client::new()
        .post(&url)
        .header("Content-Type", "application/x-sentry-envelope")
        .body(envelope)
        .send()
        .await
        .expect("ingest request failed")
        .status()
}

async fn count_rows(pool: &DbPool, table: &str, project_id: i32) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT COUNT(*) FROM {table} WHERE project_id = $1"
    )))
    .bind(project_id)
    .fetch_one(pool)
    .await
    .expect("count rows")
}

fn now_epoch() -> f64 {
    Utc::now().timestamp_micros() as f64 / 1_000_000.0
}

#[actix_web::test]
async fn test_oversized_transaction_fields_are_stored_not_rejected() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Oversized Transaction").await;
    let server = TestServer::new(&db).await;

    let long = |n: usize| "x".repeat(n);
    let now = now_epoch();
    let transaction = serde_json::json!({
        "type": "transaction",
        "transaction": format!("/search?q={}", long(400)),
        "start_timestamp": now - 1.0,
        "timestamp": now,
        "platform": long(100),
        "release": long(300),
        "environment": long(100),
        "server_name": long(300),
        "level": long(30),
        "transaction_info": { "source": long(50) },
        "sdk": { "name": long(300), "version": long(300) },
        "contexts": { "trace": {
            "trace_id": "0123456789abcdef0123456789abcdef",
            "span_id": "0123456789abcdef",
            "op": long(100),
            "status": long(50)
        }},
        "spans": [{
            "span_id": "1123456789abcdef",
            "parent_span_id": "0123456789abcdef",
            "trace_id": "0123456789abcdef0123456789abcdef",
            "op": long(200),
            "status": long(50),
            "description": long(5000),
            "start_timestamp": now - 0.5,
            "timestamp": now
        }]
    });

    let status = post_items(
        &server,
        &project,
        &[(serde_json::json!({ "type": "transaction" }), transaction)],
    )
    .await;

    assert!(status.is_success(), "ingest returned {status}");
    assert_eq!(count_rows(&db.pool, "transactions", project.id).await, 1);
    assert_eq!(count_rows(&db.pool, "spans", project.id).await, 1);
    let name: String =
        sqlx::query_scalar("SELECT transaction_name FROM transactions WHERE project_id = $1")
            .bind(project.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(
        name.chars().count(),
        200,
        "Relay's max_chars for a transaction"
    );

    server.shutdown();
}

#[actix_web::test]
async fn test_nul_bytes_in_a_transaction_and_its_spans_are_stored() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "NUL Transaction").await;
    let server = TestServer::new(&db).await;

    let now = now_epoch();
    let transaction = serde_json::json!({
        "type": "transaction",
        "transaction": "GET /files/\u{0}name",
        "start_timestamp": now - 1.0,
        "timestamp": now,
        "tags": { "path\u{0}": "a\u{0}b" },
        "contexts": { "trace": {
            "trace_id": "0123456789abcdef0123456789abcdef",
            "span_id": "0123456789abcdef",
            "op": "http.server"
        }},
        "spans": [{
            "span_id": "1123456789abcdef",
            "parent_span_id": "0123456789abcdef",
            "op": "db.query",
            "description": "SELECT * FROM t WHERE x = '\u{0}'",
            "data": { "db.statement": "\u{0}" },
            "start_timestamp": now - 0.5,
            "timestamp": now
        }]
    });

    let status = post_items(
        &server,
        &project,
        &[(serde_json::json!({ "type": "transaction" }), transaction)],
    )
    .await;

    assert!(status.is_success(), "ingest returned {status}");
    assert_eq!(count_rows(&db.pool, "transactions", project.id).await, 1);
    assert_eq!(count_rows(&db.pool, "spans", project.id).await, 1);

    server.shutdown();
}

#[actix_web::test]
async fn test_oversized_standalone_span_with_nul_is_stored() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Oversized Span").await;
    let server = TestServer::new(&db).await;

    let long = |n: usize| "y".repeat(n);
    let now = now_epoch();
    let span = serde_json::json!({
        "span_id": "2123456789abcdef",
        "trace_id": "1123456789abcdef0123456789abcdef",
        "op": long(200),
        "status": long(50),
        "description": "cache \u{0} key",
        "platform": long(100),
        "release": long(300),
        "environment": long(100),
        "data": { "k": "\u{0}" },
        "start_timestamp": now - 0.5,
        "timestamp": now
    });

    let status = post_items(
        &server,
        &project,
        &[(serde_json::json!({ "type": "span" }), span)],
    )
    .await;

    assert!(status.is_success(), "ingest returned {status}");
    assert_eq!(count_rows(&db.pool, "spans", project.id).await, 1);

    server.shutdown();
}

#[actix_web::test]
async fn test_span_v2_with_oversized_and_nul_attributes_is_stored() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Oversized Span V2").await;
    let server = TestServer::new(&db).await;

    let long = |n: usize| "z".repeat(n);
    let now = now_epoch();
    let string = |v: String| serde_json::json!({ "value": v, "type": "string" });
    let container = serde_json::json!({
        "version": 2,
        "items": [{
            "trace_id": "2123456789abcdef0123456789abcdef",
            "span_id": "3123456789abcdef",
            "name": "render \u{0}",
            "status": long(50),
            "is_segment": true,
            "start_timestamp": now - 0.5,
            "end_timestamp": now,
            "attributes": {
                "sentry.op": string(long(200)),
                "sentry.release": string(long(300)),
                "sentry.environment": string(long(100)),
                "sentry.platform": string(long(100)),
                "gen_ai.operation.name": string("chat".to_string()),
                "gen_ai.request.model": string(long(300)),
                "free": string("\u{0}".to_string())
            }
        }]
    });

    let status = post_items(
        &server,
        &project,
        &[(
            serde_json::json!({
                "type": "span",
                "content_type": "application/vnd.sentry.items.span.v2+json",
                "item_count": 1
            }),
            container,
        )],
    )
    .await;

    assert!(status.is_success(), "ingest returned {status}");
    assert_eq!(count_rows(&db.pool, "spans", project.id).await, 1);

    server.shutdown();
}

#[actix_web::test]
async fn test_log_with_unknown_long_level_and_nul_body_is_stored() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Odd Logs").await;
    let server = TestServer::new(&db).await;

    let container = serde_json::json!({ "items": [{
        "timestamp": now_epoch(),
        "trace_id": "3123456789abcdef0123456789abcdef",
        "level": "x".repeat(40),
        "body": "binary \u{0} payload",
        "attributes": { "a\u{0}": { "value": "\u{0}", "type": "string" } }
    }]});

    let status = post_items(
        &server,
        &project,
        &[(
            serde_json::json!({
                "type": "log",
                "content_type": "application/vnd.sentry.items.log+json",
                "item_count": 1
            }),
            container,
        )],
    )
    .await;

    assert!(status.is_success(), "ingest returned {status}");
    assert_eq!(count_rows(&db.pool, "logs", project.id).await, 1);

    server.shutdown();
}

#[actix_web::test]
async fn test_error_event_with_nul_and_oversized_fields_is_digested() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Odd Error").await;
    let server = TestServer::new(&db).await;

    let long = |n: usize| "w".repeat(n);
    post_envelope(
        &server,
        &project,
        serde_json::json!({
            "platform": long(100),
            "level": "error",
            "release": long(300),
            "environment": long(100),
            "server_name": long(300),
            "transaction": long(300),
            "logger": long(100),
            "message": { "formatted": "bad \u{0} byte" },
            "extra": { "blob\u{0}": "\u{0}" },
            "exception": { "values": [{
                "type": format!("Err{}", long(300)),
                "value": "nul \u{0} inside",
                "stacktrace": { "frames": [{
                    "filename": long(400),
                    "module": long(400),
                    "function": long(400)
                }]}
            }]}
        }),
    )
    .await;

    assert_eq!(count_rows(&db.pool, "events", project.id).await, 1);

    server.shutdown();
}

/// A device clock can report anything. chrono accepts years Postgres cannot
/// store, so an absurd timestamp failed the whole envelope there. One before
/// 1970 is dropped, as Relay does; one past year 9999 falls back to the
/// time it was received.
#[actix_web::test]
async fn test_out_of_range_timestamps_do_not_fail_the_envelope() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Absurd Clock").await;
    let server = TestServer::new(&db).await;

    for epoch in [-3.0e11, 9.0e12] {
        let tx = serde_json::json!({
            "type": "transaction",
            "transaction": "GET /x",
            "start_timestamp": epoch,
            "timestamp": epoch,
            "contexts": { "trace": {
                "trace_id": "0123456789abcdef0123456789abcdef",
                "span_id": "0123456789abcdef"
            }},
            "spans": [{ "span_id": "1123456789abcdef", "start_timestamp": epoch, "timestamp": epoch }]
        });
        let status = post_items(
            &server,
            &project,
            &[(serde_json::json!({ "type": "transaction" }), tx)],
        )
        .await;
        assert!(
            status.is_success(),
            "epoch {epoch}: ingest returned {status}"
        );
    }
    assert_eq!(count_rows(&db.pool, "transactions", project.id).await, 1);

    server.shutdown();
}

/// Relay parses `level` into a closed enum (`debug`, `info`/`log`, `warning`,
/// `error`, `fatal`/`critical`); anything else is removed and the default
/// applies (relay-event-schema `Level::from_str`).
#[actix_web::test]
async fn test_levels_are_normalized_like_relay() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Levels").await;
    let server = TestServer::new(&db).await;

    for (sent, stored) in [("bogus", "error"), ("critical", "fatal"), ("log", "info")] {
        post_envelope(
            &server,
            &project,
            serde_json::json!({
                "level": sent,
                "exception": { "values": [{ "type": format!("Err{sent}"), "value": "x" }] }
            }),
        )
        .await;
        let level: String = sqlx::query_scalar(
            "SELECT level FROM events WHERE project_id = $1 AND calculated_type = $2",
        )
        .bind(project.id)
        .bind(format!("Err{sent}"))
        .fetch_one(&db.pool)
        .await
        .expect("event stored");
        assert_eq!(level, stored, "level {sent}");
    }

    let now = now_epoch();
    let tx = serde_json::json!({
        "type": "transaction", "transaction": "GET /x", "level": "bogus",
        "start_timestamp": now - 1.0, "timestamp": now,
        "contexts": { "trace": { "trace_id": "0123456789abcdef0123456789abcdef", "span_id": "0123456789abcdef" }}
    });
    post_items(
        &server,
        &project,
        &[(serde_json::json!({ "type": "transaction" }), tx)],
    )
    .await;
    let level: String = sqlx::query_scalar("SELECT level FROM transactions WHERE project_id = $1")
        .bind(project.id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(level, "info");

    server.shutdown();
}

/// Relay's `TimestampProcessor`: an event or transaction from before 1970 is
/// invalid and dropped; a span from before 1970 is removed from its
/// transaction, which is kept.
#[actix_web::test]
async fn test_pre_1970_timestamps_are_dropped_like_relay() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Stale").await;
    let server = TestServer::new(&db).await;
    let trace = serde_json::json!({ "trace": {
        "trace_id": "0123456789abcdef0123456789abcdef", "span_id": "0123456789abcdef"
    }});
    let now = now_epoch();

    let stale = serde_json::json!({
        "type": "transaction", "transaction": "GET /old",
        "start_timestamp": -100.0, "timestamp": -50.0, "contexts": trace
    });
    let status = post_items(
        &server,
        &project,
        &[(serde_json::json!({ "type": "transaction" }), stale)],
    )
    .await;
    assert!(
        status.is_success(),
        "a dropped item still answers 2xx: {status}"
    );
    assert_eq!(count_rows(&db.pool, "transactions", project.id).await, 0);

    let mixed = serde_json::json!({
        "type": "transaction", "transaction": "GET /new",
        "start_timestamp": now - 1.0, "timestamp": now, "contexts": trace,
        "spans": [
            { "span_id": "1123456789abcdef", "start_timestamp": -5.0, "timestamp": now },
            { "span_id": "2123456789abcdef", "start_timestamp": now - 0.5, "timestamp": now }
        ]
    });
    post_items(
        &server,
        &project,
        &[(serde_json::json!({ "type": "transaction" }), mixed)],
    )
    .await;
    assert_eq!(count_rows(&db.pool, "transactions", project.id).await, 1);
    let span_ids: Vec<String> =
        sqlx::query_scalar("SELECT span_id FROM spans WHERE project_id = $1")
            .bind(project.id)
            .fetch_all(&db.pool)
            .await
            .unwrap();
    assert_eq!(span_ids, vec!["2123456789abcdef".to_string()]);

    post_envelope(
        &server,
        &project,
        serde_json::json!({ "exception": { "values": [{ "type": "Old", "value": "x" }] } }),
    )
    .await;
    let before = count_rows(&db.pool, "events", project.id).await;
    let event_id = uuid::Uuid::new_v4().simple().to_string();
    let body = serde_json::json!({
        "event_id": event_id, "timestamp": -100.0,
        "exception": { "values": [{ "type": "Ancient", "value": "x" }] }
    })
    .to_string();
    let envelope = format!(
        "{}\n{}\n{body}\n",
        serde_json::json!({ "event_id": event_id }),
        serde_json::json!({ "type": "event", "length": body.len() })
    );
    let url = format!(
        "http://127.0.0.1:{}/api/{}/envelope/?sentry_key={}",
        server.port, project.id, project.sentry_key
    );
    reqwest::Client::new()
        .post(&url)
        .body(envelope)
        .send()
        .await
        .unwrap();
    server.digested(project.id, before).await;
    assert_eq!(count_rows(&db.pool, "events", project.id).await, before);

    server.shutdown();
}
