//! Integration tests for the Alerts API
//!
//! Tests the notification channels, alert rules, and alert triggering
//! with a real PostgreSQL database.

use crate::common::TestDb;
use actix_session::{storage::CookieSessionStore, SessionMiddleware};
use actix_web::{cookie::Key, test, web, App};
use chrono::Utc;
use rustrak::config::{Config, DashboardConfig, DatabaseConfig, RateLimitConfig};
use rustrak::models::{
    AlertRuleChannelInput, AlertType, ChannelType, CreateAlertRule, CreateNotificationChannel,
    UpdateAlertRule, UpdateNotificationChannel,
};
use rustrak::routes;
use rustrak::services::grouping::DenormalizedFields;
use rustrak::services::{AlertService, IssueService, ProjectService};
use serde_json::json;
use std::time::Duration;
use uuid::Uuid;

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

/// Session key for tests
fn test_session_key() -> Key {
    Key::from(&[0u8; 64])
}

/// Creates a test project and returns its ID
async fn create_test_project(pool: &rustrak::db::DbPool) -> i32 {
    let project = ProjectService::create(
        pool,
        rustrak::models::CreateProject {
            name: format!("Test Project {}", Uuid::new_v4()),
            slug: None,
            platform: None,
        },
    )
    .await
    .expect("Failed to create test project");
    project.id
}

#[tokio::test]
async fn test_event_alert_history_is_scoped_to_project() {
    let db = TestDb::new().await;
    let first_project = create_test_project(&db.pool).await;
    let second_project = create_test_project(&db.pool).await;
    let event_id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO alert_history (project_id, alert_type, channel_type, channel_name, status, idempotency_key) VALUES ($1, 'new_issue', 'webhook', 'test', 'pending', $2)",
    )
    .bind(first_project)
    .bind(format!(
        "event-{first_project}-{event_id}-new_issue-integration"
    ))
    .execute(&db.pool)
    .await
    .expect("alert history insert must succeed");

    assert!(AlertService::event_alert_exists(
        &db.pool,
        first_project,
        event_id,
        AlertType::NewIssue
    )
    .await
    .expect("first project lookup must succeed"));

    sqlx::query(
        "INSERT INTO alert_history (project_id, alert_type, channel_type, channel_name, status, idempotency_key) VALUES ($1, 'new_issue', 'webhook', 'legacy', 'pending', $2)",
    )
    .bind(first_project)
    .bind(format!("event-{event_id}-new_issue-integration"))
    .execute(&db.pool)
    .await
    .expect("legacy alert history insert must succeed");

    assert!(AlertService::event_alert_exists(
        &db.pool,
        first_project,
        event_id,
        AlertType::NewIssue,
    )
    .await
    .expect("first project legacy lookup must succeed"));

    assert!(!AlertService::event_alert_exists(
        &db.pool,
        second_project,
        event_id,
        AlertType::NewIssue,
    )
    .await
    .expect("second project lookup must succeed"));
}

#[tokio::test]
async fn test_alert_retry_queue_processes_pending_history_without_retry_time() {
    let db = TestDb::new().await;
    let project_id = create_test_project(&db.pool).await;

    sqlx::query(
        "INSERT INTO alert_history (project_id, alert_type, channel_type, channel_name, status, idempotency_key) VALUES ($1, 'new_issue', 'webhook', 'unconfigured', 'pending', $2)",
    )
    .bind(project_id)
    .bind(format!("retry-null-next-{project_id}"))
    .execute(&db.pool)
    .await
    .expect("pending alert history insert must succeed");

    assert_eq!(
        AlertService::process_retry_queue(&db.pool, 5)
            .await
            .expect("retry queue processing must succeed"),
        1
    );

    let status: String =
        sqlx::query_scalar("SELECT status FROM alert_history WHERE idempotency_key = $1")
            .bind(format!("retry-null-next-{project_id}"))
            .fetch_one(&db.pool)
            .await
            .expect("retry status lookup must succeed");
    assert_eq!(status, "failed");
}

#[tokio::test]
async fn test_event_alert_cooldown_is_a_durable_replay_barrier() {
    let db = TestDb::new().await;
    let project_id = create_test_project(&db.pool).await;
    let project = ProjectService::get_by_id(&db.pool, project_id)
        .await
        .expect("project lookup must succeed");
    let channel = AlertService::create_channel(
        &db.pool,
        CreateNotificationChannel {
            name: "Cooldown Webhook".to_string(),
            provider_type: ChannelType::Webhook,
            credentials: json!({ "url": "https://example.com/webhook" }),
            is_enabled: true,
        },
    )
    .await
    .expect("channel creation must succeed");
    let rule = AlertService::create_rule(
        &db.pool,
        project_id,
        CreateAlertRule {
            name: "Cooldown Rule".to_string(),
            alert_type: AlertType::NewIssue,
            channels: vec![AlertRuleChannelInput {
                integration_id: channel.id,
                routing_override: json!({}),
            }],
            conditions: json!({}),
            cooldown_minutes: 60,
        },
    )
    .await
    .expect("rule creation must succeed");
    sqlx::query("UPDATE alert_rules SET last_triggered_at = $1 WHERE id = $2")
        .bind(Utc::now())
        .bind(rule.id)
        .execute(&db.pool)
        .await
        .expect("cooldown setup must succeed");

    let issue = IssueService::create(
        &db.pool,
        project_id,
        Utc::now(),
        &DenormalizedFields {
            calculated_type: "Error".to_string(),
            calculated_value: "cooldown replay".to_string(),
            transaction: "/test".to_string(),
            last_frame_filename: "test.rs".to_string(),
            last_frame_module: "test".to_string(),
            last_frame_function: "test".to_string(),
            culprit: "test".to_string(),
            logger: String::new(),
            release: String::new(),
        },
        Some("error"),
        Some("rust"),
    )
    .await
    .expect("issue creation must succeed");
    let event_id = Uuid::new_v4();

    AlertService::trigger_event_alert(
        &db.pool,
        &project,
        &issue,
        AlertType::NewIssue,
        event_id,
        "https://dashboard.example.com",
    )
    .await
    .expect("cooldown suppression must succeed");

    let status: String =
        sqlx::query_scalar("SELECT status FROM alert_history WHERE idempotency_key LIKE $1")
            .bind(format!("event-{project_id}-{event_id}-new_issue-%"))
            .fetch_one(&db.pool)
            .await
            .expect("suppressed alert history must exist");
    assert_eq!(status, "skipped");
    assert!(
        AlertService::event_alert_exists(&db.pool, project_id, event_id, AlertType::NewIssue,)
            .await
            .expect("replay barrier lookup must succeed")
    );
}

// =============================================================================
// Service-Level Tests (Direct Database)
// =============================================================================

// These tests bypass HTTP and test the AlertService directly

#[tokio::test]
async fn test_channel_crud_service_level() {
    let db = TestDb::new().await;

    // Create channel
    let create_input = CreateNotificationChannel {
        name: "Test Webhook".to_string(),
        provider_type: ChannelType::Webhook,
        credentials: json!({
            "url": "https://example.com/webhook"
        }),
        is_enabled: true,
    };

    let channel = AlertService::create_channel(&db.pool, create_input)
        .await
        .expect("Failed to create channel");

    assert_eq!(channel.name, "Test Webhook");
    assert_eq!(channel.provider_type, ChannelType::Webhook);
    assert!(channel.is_enabled);

    // List channels
    let channels = AlertService::list_channels(&db.pool)
        .await
        .expect("Failed to list channels");
    assert_eq!(channels.len(), 1);
    assert_eq!(channels[0].id, channel.id);

    // Get channel
    let fetched = AlertService::get_channel(&db.pool, channel.id)
        .await
        .expect("Failed to get channel");
    assert_eq!(fetched.name, "Test Webhook");

    // Update channel
    let update_input = UpdateNotificationChannel {
        name: Some("Updated Webhook".to_string()),
        credentials: None,
        is_enabled: Some(false),
    };

    let updated = AlertService::update_channel(&db.pool, channel.id, update_input)
        .await
        .expect("Failed to update channel");
    assert_eq!(updated.name, "Updated Webhook");
    assert!(!updated.is_enabled);

    // Delete channel
    AlertService::delete_channel(&db.pool, channel.id)
        .await
        .expect("Failed to delete channel");

    // Verify deleted
    let result = AlertService::get_channel(&db.pool, channel.id).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_channel_duplicate_name_fails() {
    let db = TestDb::new().await;

    let create_input1 = CreateNotificationChannel {
        name: "Unique Name".to_string(),
        provider_type: ChannelType::Webhook,
        credentials: json!({ "url": "https://example.com/webhook1" }),
        is_enabled: true,
    };

    AlertService::create_channel(&db.pool, create_input1)
        .await
        .expect("First channel should succeed");

    // Try to create another with the same name
    let create_input2 = CreateNotificationChannel {
        name: "Unique Name".to_string(),
        provider_type: ChannelType::Webhook,
        credentials: json!({ "url": "https://example.com/webhook2" }),
        is_enabled: true,
    };

    let result = AlertService::create_channel(&db.pool, create_input2).await;
    assert!(result.is_err());
    let err_msg = format!("{:?}", result.unwrap_err());
    assert!(err_msg.contains("already exists"));
}

#[tokio::test]
async fn test_channel_invalid_config_fails() {
    let db = TestDb::new().await;

    // Webhook with invalid URL format should fail
    let create_input = CreateNotificationChannel {
        name: "Invalid Webhook".to_string(),
        provider_type: ChannelType::Webhook,
        credentials: json!({ "url": "not-a-valid-url" }),
        is_enabled: true,
    };

    let result = AlertService::create_channel(&db.pool, create_input).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_slack_channel_config_validation() {
    let db = TestDb::new().await;

    // Invalid Slack webhook URL (wrong host, but method field present)
    let create_input = CreateNotificationChannel {
        name: "Invalid Slack".to_string(),
        provider_type: ChannelType::Slack,
        credentials: json!({
            "method": "webhook",
            "webhook_url": "https://example.com/not-slack"
        }),
        is_enabled: true,
    };

    let result = AlertService::create_channel(&db.pool, create_input).await;
    assert!(result.is_err());

    // Valid Slack webhook URL
    let valid_input = CreateNotificationChannel {
        name: "Valid Slack".to_string(),
        provider_type: ChannelType::Slack,
        credentials: json!({
            "method": "webhook",
            "webhook_url": "https://hooks.slack.com/services/T00000000/B00000000/XXXXXXXX"
        }),
        is_enabled: true,
    };

    let channel = AlertService::create_channel(&db.pool, valid_input)
        .await
        .expect("Valid Slack channel should succeed");
    assert_eq!(channel.name, "Valid Slack");
}

#[tokio::test]
async fn test_custom_webhook_channel_crud_and_validation() {
    let db = TestDb::new().await;
    let template = r#"{"msgtype":"text","text":{"content":"{{ issue.title }}"}}"#;

    // Missing template: rejected before any row is written.
    let result = AlertService::create_channel(
        &db.pool,
        CreateNotificationChannel {
            name: "Custom No Template".to_string(),
            provider_type: ChannelType::CustomWebhook,
            credentials: json!({ "url": "https://example.com/hook" }),
            is_enabled: true,
        },
    )
    .await;
    assert!(result.is_err());

    // Renders, but never to JSON: a trailing comma. Compiling the template
    // says nothing about this, so the save-time check has to render it, or
    // the integration only fails when an alert fires.
    let result = AlertService::create_channel(
        &db.pool,
        CreateNotificationChannel {
            name: "Custom Trailing Comma".to_string(),
            provider_type: ChannelType::CustomWebhook,
            credentials: json!({
                "url": "https://example.com/hook",
                "template": r#"{"a": "{{ issue.title }}",}"#,
            }),
            is_enabled: true,
        },
    )
    .await;
    assert!(
        result.is_err(),
        "a template that cannot render JSON must not be saved"
    );

    // A field that does not exist is a typo, and a typo is caught here in
    // words rather than shipped as a blank in a real message.
    let result = AlertService::create_channel(
        &db.pool,
        CreateNotificationChannel {
            name: "Custom Typo".to_string(),
            provider_type: ChannelType::CustomWebhook,
            credentials: json!({
                "url": "https://example.com/hook",
                "template": r#"{"a": "{{ issue.titel }}"}"#,
            }),
            is_enabled: true,
        },
    )
    .await;
    let err = result
        .expect_err("an unknown field must not be saved")
        .to_string();
    assert!(err.contains("undefined"), "got: {err}");

    // Broken template syntax: rejected at save time, not delivery time.
    let result = AlertService::create_channel(
        &db.pool,
        CreateNotificationChannel {
            name: "Custom Bad Syntax".to_string(),
            provider_type: ChannelType::CustomWebhook,
            credentials: json!({ "url": "https://example.com/hook", "template": "{% if %}" }),
            is_enabled: true,
        },
    )
    .await;
    assert!(result.is_err());

    // Valid: also proves the widened provider_type CHECK took effect in both
    // dialects, since the INSERT below would fail the old three-value CHECK.
    let channel = AlertService::create_channel(
        &db.pool,
        CreateNotificationChannel {
            name: "Ops chat bridge".to_string(),
            provider_type: ChannelType::CustomWebhook,
            credentials: json!({ "url": "https://example.com/hooks/incoming", "template": template }),
            is_enabled: true,
        },
    )
    .await
    .expect("Valid custom webhook channel should succeed");
    assert_eq!(channel.provider_type, ChannelType::CustomWebhook);

    // Update with a broken template is rejected; the row keeps the old credentials.
    let result = AlertService::update_channel(
        &db.pool,
        channel.id,
        UpdateNotificationChannel {
            name: None,
            credentials: Some(json!({ "template": "{% endif %}" })),
            is_enabled: None,
        },
    )
    .await;
    assert!(result.is_err());
    let fetched = AlertService::get_channel(&db.pool, channel.id)
        .await
        .expect("channel must survive the failed update");
    assert_eq!(fetched.credentials["template"], json!(template));

    // Update with a valid template round-trips.
    let updated = AlertService::update_channel(
        &db.pool,
        channel.id,
        UpdateNotificationChannel {
            name: None,
            credentials: Some(json!({ "template": "{\"a\": 1}" })),
            is_enabled: None,
        },
    )
    .await
    .expect("valid template update must succeed");
    assert_eq!(updated.credentials["template"], json!("{\"a\": 1}"));
}

/// The SQLite provider_type rebuild must not let SQLx's default foreign-key
/// enforcement cascade away the children of the integrations it is not
/// touching. Covers both child relations and both migration directions.
#[cfg(feature = "sqlite")]
#[tokio::test]
async fn test_custom_webhook_sqlite_migration_preserves_child_references() {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    let dir = tempfile::tempdir().expect("tempdir");
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(dir.path().join("fk-migration.sqlite"))
                .create_if_missing(true),
        )
        .await
        .expect("open sqlite");

    let enforced: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(&pool)
        .await
        .expect("pragma read");
    assert_eq!(
        enforced, 1,
        "SQLx enables foreign-key enforcement by default; this test is only \
         meaningful with it on"
    );

    // The state the migration receives: the pre-existing three-value CHECK
    // plus both children with their real ON DELETE actions. Columns the
    // migration does not touch are trimmed; the ones it moves are exact.
    sqlx::raw_sql(
        r#"
        CREATE TABLE alert_rules (id INTEGER PRIMARY KEY);
        CREATE TABLE alert_integrations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            provider_type TEXT NOT NULL CHECK (provider_type IN ('slack','email','webhook')),
            credentials TEXT NOT NULL DEFAULT '{}',
            is_enabled INTEGER NOT NULL DEFAULT 1,
            failure_count INTEGER NOT NULL DEFAULT 0,
            last_failure_at TEXT,
            last_failure_message TEXT,
            last_success_at TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE TABLE alert_rule_channels (
            alert_rule_id INTEGER NOT NULL REFERENCES alert_rules(id) ON DELETE CASCADE,
            integration_id INTEGER NOT NULL REFERENCES alert_integrations(id) ON DELETE CASCADE,
            routing_override TEXT NOT NULL DEFAULT '{}',
            PRIMARY KEY (alert_rule_id, integration_id)
        );
        CREATE TABLE alert_history (
            id INTEGER PRIMARY KEY,
            integration_id INTEGER REFERENCES alert_integrations(id) ON DELETE SET NULL
        );
        INSERT INTO alert_rules VALUES (1);
        INSERT INTO alert_integrations (id, name, provider_type) VALUES (7, 'chan', 'webhook');
        INSERT INTO alert_rule_channels VALUES (1, 7, '{"url":"keep-me"}');
        INSERT INTO alert_history VALUES (1, 7);
        INSERT INTO alert_history VALUES (2, NULL);
        "#,
    )
    .execute(&pool)
    .await
    .expect("prior-state schema");

    sqlx::raw_sql(include_str!(
        "../../migrations/sqlite/20260831000000_custom_webhook_provider.up.sql"
    ))
    .execute(&pool)
    .await
    .expect("up migration");

    let channels: Vec<(i64, i64, String)> = sqlx::query_as(
        "SELECT alert_rule_id, integration_id, routing_override FROM alert_rule_channels",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        channels,
        vec![(1, 7, r#"{"url":"keep-me"}"#.to_string())],
        "the junction row must survive the parent rebuild with its override"
    );
    let hist_ref: Option<i64> =
        sqlx::query_scalar("SELECT integration_id FROM alert_history WHERE id = 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(hist_ref, Some(7), "history reference must survive");

    sqlx::query("INSERT INTO alert_integrations (id, name, provider_type) VALUES (8, 'cw', 'custom_webhook')")
        .execute(&pool)
        .await
        .expect("CHECK widened: custom_webhook rows are accepted");
    sqlx::query("INSERT INTO alert_rule_channels VALUES (1, 8, '{}')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO alert_history VALUES (3, 8)")
        .execute(&pool)
        .await
        .unwrap();

    sqlx::raw_sql(include_str!(
        "../../migrations/sqlite/20260831000000_custom_webhook_provider.down.sql"
    ))
    .execute(&pool)
    .await
    .expect("down migration");

    let survivors: Vec<(i64, i64)> =
        sqlx::query_as("SELECT alert_rule_id, integration_id FROM alert_rule_channels")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        survivors,
        vec![(1, 7)],
        "down removes the custom_webhook integration's channel row and nothing else"
    );
    let history: Vec<(i64, Option<i64>)> =
        sqlx::query_as("SELECT id, integration_id FROM alert_history ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        history,
        vec![(1, Some(7)), (2, None), (3, None)],
        "down nulls only the removed integration's history reference (SET NULL semantics)"
    );

    let rejected = sqlx::query("INSERT INTO alert_integrations (id, name, provider_type) VALUES (9, 'cw2', 'custom_webhook')")
        .execute(&pool)
        .await;
    assert!(
        rejected.is_err(),
        "CHECK is restored to three values after down"
    );
}

#[tokio::test]
async fn test_rule_crud_service_level() {
    let db = TestDb::new().await;

    // First create a project and a channel
    let project_id = create_test_project(&db.pool).await;

    let channel = AlertService::create_channel(
        &db.pool,
        CreateNotificationChannel {
            name: "Alert Channel".to_string(),
            provider_type: ChannelType::Webhook,
            credentials: json!({ "url": "https://example.com/webhook" }),
            is_enabled: true,
        },
    )
    .await
    .expect("Failed to create channel");

    // Create rule
    let create_input = CreateAlertRule {
        name: "New Issue Alert".to_string(),
        alert_type: AlertType::NewIssue,
        channels: vec![AlertRuleChannelInput {
            integration_id: channel.id,
            routing_override: json!({}),
        }],
        conditions: json!({}),
        cooldown_minutes: 5,
    };

    let rule = AlertService::create_rule(&db.pool, project_id, create_input)
        .await
        .expect("Failed to create rule");

    assert_eq!(rule.name, "New Issue Alert");
    assert_eq!(rule.alert_type, AlertType::NewIssue);
    assert_eq!(rule.cooldown_minutes, 5);
    assert!(rule.is_enabled);

    // Verify channel linkage
    let linked_channels = AlertService::get_rule_channels(&db.pool, rule.id)
        .await
        .expect("Failed to get rule channels");
    assert_eq!(linked_channels, vec![channel.id]);

    // List rules
    let rules = AlertService::list_rules(&db.pool, project_id)
        .await
        .expect("Failed to list rules");
    assert_eq!(rules.len(), 1);

    // Get rule
    let fetched = AlertService::get_rule(&db.pool, rule.id)
        .await
        .expect("Failed to get rule");
    assert_eq!(fetched.name, "New Issue Alert");

    // Update rule
    let update_input = UpdateAlertRule {
        name: Some("Updated Alert".to_string()),
        is_enabled: Some(false),
        conditions: None,
        cooldown_minutes: Some(10),
        channels: None,
    };

    let updated = AlertService::update_rule(&db.pool, rule.id, update_input)
        .await
        .expect("Failed to update rule");
    assert_eq!(updated.name, "Updated Alert");
    assert!(!updated.is_enabled);
    assert_eq!(updated.cooldown_minutes, 10);

    // Delete rule
    AlertService::delete_rule(&db.pool, rule.id)
        .await
        .expect("Failed to delete rule");

    // Verify deleted
    let result = AlertService::get_rule(&db.pool, rule.id).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_rule_duplicate_alert_type_fails() {
    let db = TestDb::new().await;

    let project_id = create_test_project(&db.pool).await;

    let channel = AlertService::create_channel(
        &db.pool,
        CreateNotificationChannel {
            name: "Channel for Rules".to_string(),
            provider_type: ChannelType::Webhook,
            credentials: json!({ "url": "https://example.com/webhook" }),
            is_enabled: true,
        },
    )
    .await
    .expect("Failed to create channel");

    // Create first rule
    let create_input = CreateAlertRule {
        name: "First Rule".to_string(),
        alert_type: AlertType::NewIssue,
        channels: vec![AlertRuleChannelInput {
            integration_id: channel.id,
            routing_override: json!({}),
        }],
        conditions: json!({}),
        cooldown_minutes: 0,
    };

    AlertService::create_rule(&db.pool, project_id, create_input)
        .await
        .expect("First rule should succeed");

    // Try to create another with same alert type
    let duplicate_input = CreateAlertRule {
        name: "Duplicate Rule".to_string(),
        alert_type: AlertType::NewIssue, // Same type
        channels: vec![AlertRuleChannelInput {
            integration_id: channel.id,
            routing_override: json!({}),
        }],
        conditions: json!({}),
        cooldown_minutes: 0,
    };

    let result = AlertService::create_rule(&db.pool, project_id, duplicate_input).await;
    assert!(result.is_err());
    let err_msg = format!("{:?}", result.unwrap_err());
    assert!(err_msg.contains("already exists"));
}

#[tokio::test]
async fn test_rule_with_invalid_channel_fails() {
    let db = TestDb::new().await;

    let project_id = create_test_project(&db.pool).await;

    // Create rule with non-existent integration ID
    let create_input = CreateAlertRule {
        name: "Invalid Channel Rule".to_string(),
        alert_type: AlertType::NewIssue,
        channels: vec![AlertRuleChannelInput {
            integration_id: 99999, // Non-existent
            routing_override: json!({}),
        }],
        conditions: json!({}),
        cooldown_minutes: 0,
    };

    let result = AlertService::create_rule(&db.pool, project_id, create_input).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_update_rule_channels() {
    let db = TestDb::new().await;

    let project_id = create_test_project(&db.pool).await;

    // Create two channels
    let channel1 = AlertService::create_channel(
        &db.pool,
        CreateNotificationChannel {
            name: "Channel 1".to_string(),
            provider_type: ChannelType::Webhook,
            credentials: json!({ "url": "https://example.com/webhook1" }),
            is_enabled: true,
        },
    )
    .await
    .unwrap();

    let channel2 = AlertService::create_channel(
        &db.pool,
        CreateNotificationChannel {
            name: "Channel 2".to_string(),
            provider_type: ChannelType::Webhook,
            credentials: json!({ "url": "https://example.com/webhook2" }),
            is_enabled: true,
        },
    )
    .await
    .unwrap();

    // Create rule with channel1
    let rule = AlertService::create_rule(
        &db.pool,
        project_id,
        CreateAlertRule {
            name: "Multi Channel Rule".to_string(),
            alert_type: AlertType::NewIssue,
            channels: vec![AlertRuleChannelInput {
                integration_id: channel1.id,
                routing_override: json!({}),
            }],
            conditions: json!({}),
            cooldown_minutes: 0,
        },
    )
    .await
    .unwrap();

    // Verify initial channels
    let channels = AlertService::get_rule_channels(&db.pool, rule.id)
        .await
        .unwrap();
    assert_eq!(channels, vec![channel1.id]);

    // Update to use both channels
    AlertService::update_rule(
        &db.pool,
        rule.id,
        UpdateAlertRule {
            name: None,
            is_enabled: None,
            conditions: None,
            cooldown_minutes: None,
            channels: Some(vec![
                AlertRuleChannelInput {
                    integration_id: channel1.id,
                    routing_override: json!({}),
                },
                AlertRuleChannelInput {
                    integration_id: channel2.id,
                    routing_override: json!({}),
                },
            ]),
        },
    )
    .await
    .unwrap();

    // Verify updated channels
    let channels = AlertService::get_rule_channels(&db.pool, rule.id)
        .await
        .unwrap();
    assert_eq!(channels.len(), 2);
    assert!(channels.contains(&channel1.id));
    assert!(channels.contains(&channel2.id));

    // Update to remove channel1
    AlertService::update_rule(
        &db.pool,
        rule.id,
        UpdateAlertRule {
            name: None,
            is_enabled: None,
            conditions: None,
            cooldown_minutes: None,
            channels: Some(vec![AlertRuleChannelInput {
                integration_id: channel2.id,
                routing_override: json!({}),
            }]),
        },
    )
    .await
    .unwrap();

    // Verify only channel2 remains
    let channels = AlertService::get_rule_channels(&db.pool, rule.id)
        .await
        .unwrap();
    assert_eq!(channels, vec![channel2.id]);
}

#[tokio::test]
async fn test_deleting_channel_removes_from_rules() {
    let db = TestDb::new().await;

    let project_id = create_test_project(&db.pool).await;

    let channel = AlertService::create_channel(
        &db.pool,
        CreateNotificationChannel {
            name: "Deletable Channel".to_string(),
            provider_type: ChannelType::Webhook,
            credentials: json!({ "url": "https://example.com/webhook" }),
            is_enabled: true,
        },
    )
    .await
    .unwrap();

    // Create rule with this channel
    let rule = AlertService::create_rule(
        &db.pool,
        project_id,
        CreateAlertRule {
            name: "Rule with deletable channel".to_string(),
            alert_type: AlertType::NewIssue,
            channels: vec![AlertRuleChannelInput {
                integration_id: channel.id,
                routing_override: json!({}),
            }],
            conditions: json!({}),
            cooldown_minutes: 0,
        },
    )
    .await
    .unwrap();

    // Delete channel
    AlertService::delete_channel(&db.pool, channel.id)
        .await
        .unwrap();

    // Rule should still exist but have no channels
    let channels = AlertService::get_rule_channels(&db.pool, rule.id)
        .await
        .unwrap();
    assert!(channels.is_empty());
}

#[tokio::test]
async fn test_alert_history_empty() {
    let db = TestDb::new().await;

    let project_id = create_test_project(&db.pool).await;

    let history = AlertService::list_history(&db.pool, project_id, 50)
        .await
        .expect("Failed to list history");

    assert!(history.is_empty());
}

// =============================================================================
// HTTP Route Tests
// =============================================================================

// Note: These tests are marked as ignored because actix-web's test framework
// doesn't properly preserve session cookies. Full testing should be done via
// E2E tests with a real HTTP client.

#[actix_web::test]
async fn test_list_channels_unauthorized() {
    let db = TestDb::new().await;
    let config = create_test_config();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config))
            .wrap(
                SessionMiddleware::builder(CookieSessionStore::default(), test_session_key())
                    .cookie_secure(false)
                    .build(),
            )
            .configure(routes::alerts::configure),
    )
    .await;

    // No session cookie
    let req = test::TestRequest::get()
        .uri("/api/integrations")
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn test_list_rules_unauthorized() {
    let db = TestDb::new().await;
    let config = create_test_config();

    // Create a test project first
    let project_id = create_test_project(&db.pool).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config))
            .wrap(
                SessionMiddleware::builder(CookieSessionStore::default(), test_session_key())
                    .cookie_secure(false)
                    .build(),
            )
            .configure(routes::alerts::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/alert-rules", project_id))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn test_list_history_unauthorized() {
    let db = TestDb::new().await;
    let config = create_test_config();

    let project_id = create_test_project(&db.pool).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config))
            .wrap(
                SessionMiddleware::builder(CookieSessionStore::default(), test_session_key())
                    .cookie_secure(false)
                    .build(),
            )
            .configure(routes::alerts::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/alert-history", project_id))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
#[ignore = "Session cookies not preserved in actix test framework - use E2E tests"]
async fn test_create_channel_success() {
    // This test requires proper session cookie handling
}

#[actix_web::test]
#[ignore = "Session cookies not preserved in actix test framework - use E2E tests"]
async fn test_create_rule_success() {
    // This test requires proper session cookie handling
}

#[actix_web::test]
#[ignore = "Session cookies not preserved in actix test framework - use E2E tests"]
async fn test_test_channel_endpoint() {
    // This test requires proper session cookie handling
}

#[tokio::test]
async fn test_claim_due_retries_leases_rows_exclusively() {
    // A pending history row must be dispatchable by exactly one worker:
    // claiming it takes a lease, so a concurrent (or immediately following)
    // claim cannot pick up the same row and double-deliver the alert.
    let db = TestDb::new().await;
    let project_id = create_test_project(&db.pool).await;

    sqlx::query(
        "INSERT INTO alert_history (project_id, alert_type, channel_type, channel_name, status, idempotency_key) VALUES ($1, 'new_issue', 'webhook', 'unconfigured', 'pending', $2)",
    )
    .bind(project_id)
    .bind(format!("claim-lease-{project_id}"))
    .execute(&db.pool)
    .await
    .expect("pending alert history insert must succeed");

    let first = AlertService::claim_due_retries(&db.pool, 5, 100)
        .await
        .expect("first claim must succeed");
    assert_eq!(first.len(), 1, "first claim must lease the pending row");

    let second = AlertService::claim_due_retries(&db.pool, 5, 100)
        .await
        .expect("second claim must succeed");
    assert!(
        second.is_empty(),
        "a leased row must not be claimable again while its lease holds"
    );
}

#[tokio::test]
async fn test_trigger_alert_does_not_block_on_slow_webhook_delivery() {
    // Relay never couples ingestion latency to notification I/O. Rustrak's
    // digest awaits trigger_*_alert for durability, so the durable part
    // (history rows) must commit synchronously while the network dispatch
    // happens off the digest path: a hung webhook endpoint must not stall
    // the caller for the 30s HTTP timeout.
    let db = TestDb::new().await;
    let project_id = create_test_project(&db.pool).await;
    let project = ProjectService::get_by_id(&db.pool, project_id)
        .await
        .expect("project lookup must succeed");

    // A webhook endpoint that accepts connections and never responds.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind hanging webhook listener");
    let addr = listener.local_addr().expect("listener addr");
    tokio::spawn(async move {
        loop {
            let Ok((socket, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let _socket = socket;
                tokio::time::sleep(Duration::from_secs(3600)).await;
            });
        }
    });

    let channel = AlertService::create_channel(
        &db.pool,
        CreateNotificationChannel {
            name: "Hanging Webhook".to_string(),
            provider_type: ChannelType::Webhook,
            credentials: json!({ "url": format!("http://{addr}/hook") }),
            is_enabled: true,
        },
    )
    .await
    .expect("channel creation must succeed");
    AlertService::create_rule(
        &db.pool,
        project_id,
        CreateAlertRule {
            name: "Slow Webhook Rule".to_string(),
            alert_type: AlertType::NewIssue,
            channels: vec![AlertRuleChannelInput {
                integration_id: channel.id,
                routing_override: json!({}),
            }],
            conditions: json!({}),
            cooldown_minutes: 0,
        },
    )
    .await
    .expect("rule creation must succeed");

    let issue = IssueService::create(
        &db.pool,
        project_id,
        Utc::now(),
        &DenormalizedFields {
            calculated_type: "Error".to_string(),
            calculated_value: "slow webhook".to_string(),
            transaction: "/test".to_string(),
            last_frame_filename: "test.rs".to_string(),
            last_frame_module: "test".to_string(),
            last_frame_function: "test".to_string(),
            culprit: "test".to_string(),
            logger: String::new(),
            release: String::new(),
        },
        Some("error"),
        Some("rust"),
    )
    .await
    .expect("issue creation must succeed");

    let trigger =
        AlertService::trigger_new_issue_alert(&db.pool, &project, &issue, "http://localhost:3000");
    tokio::time::timeout(Duration::from_secs(2), trigger)
        .await
        .expect("trigger must return without waiting for webhook I/O")
        .expect("trigger must succeed");

    // The durable delivery record is committed before the caller returns,
    // leased to the in-flight dispatcher so the retry worker cannot
    // double-send it while the dispatch is still running.
    let (status, leased): (String, bool) = sqlx::query_as(
        "SELECT status, next_retry_at IS NOT NULL AND datetime(next_retry_at) > datetime('now') FROM alert_history WHERE project_id = $1",
    )
    .bind(project_id)
    .fetch_one(&db.pool)
    .await
    .expect("history row must exist before trigger returns");
    assert_eq!(status, "pending");
    assert!(
        leased,
        "in-flight delivery must hold a future next_retry_at lease"
    );
}
