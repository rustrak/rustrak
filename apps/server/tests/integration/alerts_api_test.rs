//! Integration tests for the Alerts API
//!
//! Tests the notification channels, alert rules, and alert triggering
//! with a real PostgreSQL database.

use crate::common::TestDb;
use chrono::Utc;
use rustrak::models::{
    AlertRuleChannelInput, AlertType, CreateAlertIntegration, CreateAlertRule, ProviderType,
    UpdateAlertIntegration, UpdateAlertRule,
};
use rustrak::services::grouping::DenormalizedFields;
use rustrak::services::{AlertService, IssueService, ProjectService};
use serde_json::json;
use std::time::Duration;
use uuid::Uuid;

/// Session key for tests
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
        AlertService::process_retry_queue(&db.pool, &Default::default(), 5)
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

/// An integration disabled after a delivery was queued sends nothing, and the
/// history says skipped rather than sent.
#[tokio::test]
async fn test_queued_delivery_to_a_disabled_integration_is_skipped() {
    let db = TestDb::new().await;
    let project_id = create_test_project(&db.pool).await;
    let smtp = rustrak::config::SmtpSettings::default();
    let channel = AlertService::create_channel(
        &db.pool,
        &smtp,
        CreateAlertIntegration {
            name: "Disabled later".to_string(),
            provider_type: ProviderType::Webhook,
            credentials: json!({ "url": "http://127.0.0.1:9/unreachable" }),
            is_enabled: true,
        },
    )
    .await
    .expect("channel creation must succeed");
    let rule = AlertService::create_rule(
        &db.pool,
        project_id,
        CreateAlertRule {
            name: "Rule".to_string(),
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
    AlertService::update_channel(
        &db.pool,
        &smtp,
        channel.id,
        UpdateAlertIntegration {
            name: None,
            credentials: None,
            is_enabled: Some(false),
        },
    )
    .await
    .expect("disabling must succeed");

    let payload = json!({
        "alert_id": "queued", "alert_type": "new_issue", "triggered_at": Utc::now(),
        "project": { "id": project_id, "name": "p", "slug": "p" },
        "issue": { "id": Uuid::nil().to_string(), "short_id": "P-1", "title": "t",
                   "level": null, "first_seen": Utc::now(), "last_seen": Utc::now(),
                   "event_count": 1 },
        "issue_url": "http://localhost/issue", "actor": "Rustrak"
    });
    sqlx::query(
        "INSERT INTO alert_history (alert_rule_id, integration_id, project_id, alert_type, channel_type, channel_name, payload, status, idempotency_key) VALUES ($1, $2, $3, 'new_issue', 'webhook', 'Disabled later', $4, 'pending', 'queued-disabled')",
    )
    .bind(rule.id)
    .bind(channel.id)
    .bind(project_id)
    .bind(payload)
    .execute(&db.pool)
    .await
    .expect("pending alert history insert must succeed");

    AlertService::process_retry_queue(&db.pool, &smtp, 5)
        .await
        .expect("retry queue processing must succeed");

    let status: String = sqlx::query_scalar(
        "SELECT status FROM alert_history WHERE idempotency_key = 'queued-disabled'",
    )
    .fetch_one(&db.pool)
    .await
    .expect("status lookup must succeed");
    assert_eq!(status, "skipped");
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
        &Default::default(),
        CreateAlertIntegration {
            name: "Cooldown Webhook".to_string(),
            provider_type: ProviderType::Webhook,
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
        Default::default(),
        "https://dashboard.example.com",
        &Default::default(),
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

    // Replayed once the cooldown is over, the rule finds its row and stops
    // before reserving a cooldown nothing will use.
    sqlx::query("UPDATE alert_rules SET last_triggered_at = NULL WHERE id = $1")
        .bind(rule.id)
        .execute(&db.pool)
        .await
        .expect("cooldown reset must succeed");
    AlertService::trigger_event_alert(
        &db.pool,
        &project,
        &issue,
        AlertType::NewIssue,
        event_id,
        Default::default(),
        "https://dashboard.example.com",
        &Default::default(),
    )
    .await
    .expect("replay must succeed");
    let last: Option<chrono::DateTime<Utc>> =
        sqlx::query_scalar("SELECT last_triggered_at FROM alert_rules WHERE id = $1")
            .bind(rule.id)
            .fetch_one(&db.pool)
            .await
            .expect("rule lookup must succeed");
    assert_eq!(last, None);
}

/// Retention lowers an issue's count, so a later event can bring it back to a
/// rule's `min_events`; the rule has already fired for that issue and must not
/// fire again.
#[tokio::test]
async fn test_new_issue_threshold_fires_once_per_issue() {
    let db = TestDb::new().await;
    let project_id = create_test_project(&db.pool).await;
    let project = ProjectService::get_by_id(&db.pool, project_id)
        .await
        .expect("project lookup must succeed");
    let channel = AlertService::create_channel(
        &db.pool,
        &Default::default(),
        CreateAlertIntegration {
            name: "Threshold Webhook".to_string(),
            provider_type: ProviderType::Webhook,
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
            name: "Threshold Rule".to_string(),
            alert_type: AlertType::NewIssue,
            channels: vec![AlertRuleChannelInput {
                integration_id: channel.id,
                routing_override: json!({}),
            }],
            conditions: json!({ "min_events": 3 }),
            cooldown_minutes: 60,
        },
    )
    .await
    .expect("rule creation must succeed");
    // In cooldown, so nothing is sent; the history row is written all the same.
    sqlx::query("UPDATE alert_rules SET last_triggered_at = $1 WHERE id = $2")
        .bind(Utc::now())
        .bind(rule.id)
        .execute(&db.pool)
        .await
        .expect("cooldown setup must succeed");
    let mut issue = IssueService::create(
        &db.pool,
        project_id,
        Utc::now(),
        &DenormalizedFields {
            calculated_type: "Error".to_string(),
            calculated_value: "threshold".to_string(),
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
    issue.digested_event_count = 3;

    for _ in 0..2 {
        AlertService::trigger_event_alert(
            &db.pool,
            &project,
            &issue,
            AlertType::NewIssue,
            Uuid::new_v4(),
            Default::default(),
            "https://dashboard.example.com",
            &Default::default(),
        )
        .await
        .expect("trigger must succeed");
    }

    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM alert_history WHERE issue_id = $1")
        .bind(issue.id)
        .fetch_one(&db.pool)
        .await
        .expect("history count must succeed");
    assert_eq!(rows, 1);
}

// These tests bypass HTTP and test the AlertService directly

#[tokio::test]
async fn test_channel_crud_service_level() {
    let db = TestDb::new().await;

    let create_input = CreateAlertIntegration {
        name: "Test Webhook".to_string(),
        provider_type: ProviderType::Webhook,
        credentials: json!({
            "url": "https://example.com/webhook"
        }),
        is_enabled: true,
    };

    let channel = AlertService::create_channel(&db.pool, &Default::default(), create_input)
        .await
        .expect("Failed to create channel");

    assert_eq!(channel.name, "Test Webhook");
    assert_eq!(channel.provider_type, ProviderType::Webhook);
    assert!(channel.is_enabled);

    // List channels
    let channels = AlertService::list_channels(&db.pool)
        .await
        .expect("Failed to list channels");
    assert_eq!(channels.len(), 1);
    assert_eq!(channels[0].id, channel.id);

    let fetched = AlertService::get_channel(&db.pool, channel.id)
        .await
        .expect("Failed to get channel");
    assert_eq!(fetched.name, "Test Webhook");

    // Update channel
    let update_input = UpdateAlertIntegration {
        name: Some("Updated Webhook".to_string()),
        credentials: None,
        is_enabled: Some(false),
    };

    let updated =
        AlertService::update_channel(&db.pool, &Default::default(), channel.id, update_input)
            .await
            .expect("Failed to update channel");
    assert_eq!(updated.name, "Updated Webhook");
    assert!(!updated.is_enabled);

    // Delete channel
    AlertService::delete_channel(&db.pool, channel.id)
        .await
        .expect("Failed to delete channel");

    let result = AlertService::get_channel(&db.pool, channel.id).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_channel_duplicate_name_fails() {
    let db = TestDb::new().await;

    let create_input1 = CreateAlertIntegration {
        name: "Unique Name".to_string(),
        provider_type: ProviderType::Webhook,
        credentials: json!({ "url": "https://example.com/webhook1" }),
        is_enabled: true,
    };

    AlertService::create_channel(&db.pool, &Default::default(), create_input1)
        .await
        .expect("First channel should succeed");

    // Try to create another with the same name
    let create_input2 = CreateAlertIntegration {
        name: "Unique Name".to_string(),
        provider_type: ProviderType::Webhook,
        credentials: json!({ "url": "https://example.com/webhook2" }),
        is_enabled: true,
    };

    let result = AlertService::create_channel(&db.pool, &Default::default(), create_input2).await;
    assert!(result.is_err());
    let err_msg = format!("{:?}", result.unwrap_err());
    assert!(err_msg.contains("already exists"));
}

#[tokio::test]
async fn test_channel_invalid_config_fails() {
    let db = TestDb::new().await;

    // Webhook with invalid URL format should fail
    let create_input = CreateAlertIntegration {
        name: "Invalid Webhook".to_string(),
        provider_type: ProviderType::Webhook,
        credentials: json!({ "url": "not-a-valid-url" }),
        is_enabled: true,
    };

    let result = AlertService::create_channel(&db.pool, &Default::default(), create_input).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_slack_channel_config_validation() {
    let db = TestDb::new().await;

    // Invalid Slack webhook URL (wrong host, but method field present)
    let create_input = CreateAlertIntegration {
        name: "Invalid Slack".to_string(),
        provider_type: ProviderType::Slack,
        credentials: json!({
            "method": "webhook",
            "webhook_url": "https://example.com/not-slack"
        }),
        is_enabled: true,
    };

    let result = AlertService::create_channel(&db.pool, &Default::default(), create_input).await;
    assert!(result.is_err());

    // Valid Slack webhook URL
    let valid_input = CreateAlertIntegration {
        name: "Valid Slack".to_string(),
        provider_type: ProviderType::Slack,
        credentials: json!({
            "method": "webhook",
            "webhook_url": "https://hooks.slack.com/services/T00000000/B00000000/XXXXXXXX"
        }),
        is_enabled: true,
    };

    let channel = AlertService::create_channel(&db.pool, &Default::default(), valid_input)
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
        &Default::default(),
        CreateAlertIntegration {
            name: "Custom No Template".to_string(),
            provider_type: ProviderType::CustomWebhook,
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
        &Default::default(),
        CreateAlertIntegration {
            name: "Custom Trailing Comma".to_string(),
            provider_type: ProviderType::CustomWebhook,
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
        &Default::default(),
        CreateAlertIntegration {
            name: "Custom Typo".to_string(),
            provider_type: ProviderType::CustomWebhook,
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
        &Default::default(),
        CreateAlertIntegration {
            name: "Custom Bad Syntax".to_string(),
            provider_type: ProviderType::CustomWebhook,
            credentials: json!({ "url": "https://example.com/hook", "template": "{% if %}" }),
            is_enabled: true,
        },
    )
    .await;
    assert!(result.is_err());

    // Valid: also proves the widened provider_type CHECK took effect in both
    // dialects, since the INSERT below would fail the old three-value CHECK.
    let channel = AlertService::create_channel(
        &db.pool, &Default::default(),
        CreateAlertIntegration {
            name: "Ops chat bridge".to_string(),
            provider_type: ProviderType::CustomWebhook,
            credentials: json!({ "url": "https://example.com/hooks/incoming", "template": template }),
            is_enabled: true,
        },
    )
    .await
    .expect("Valid custom webhook channel should succeed");
    assert_eq!(channel.provider_type, ProviderType::CustomWebhook);

    // Update with a broken template is rejected; the row keeps the old credentials.
    let result = AlertService::update_channel(
        &db.pool,
        &Default::default(),
        channel.id,
        UpdateAlertIntegration {
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
        &Default::default(),
        channel.id,
        UpdateAlertIntegration {
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

    let project_id = create_test_project(&db.pool).await;

    let channel = AlertService::create_channel(
        &db.pool,
        &Default::default(),
        CreateAlertIntegration {
            name: "Alert Channel".to_string(),
            provider_type: ProviderType::Webhook,
            credentials: json!({ "url": "https://example.com/webhook" }),
            is_enabled: true,
        },
    )
    .await
    .expect("Failed to create channel");

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

    let linked_channels = AlertService::get_rule_channels(&db.pool, rule.id)
        .await
        .expect("Failed to get rule channels");
    assert_eq!(linked_channels, vec![channel.id]);

    // List rules
    let rules = AlertService::list_rules(&db.pool, project_id)
        .await
        .expect("Failed to list rules");
    assert_eq!(rules.len(), 1);

    let fetched = AlertService::get_rule(&db.pool, rule.id)
        .await
        .expect("Failed to get rule");
    assert_eq!(fetched.name, "New Issue Alert");

    // Update rule
    let update_input = UpdateAlertRule {
        alert_type: None,
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

    let result = AlertService::get_rule(&db.pool, rule.id).await;
    assert!(result.is_err());
}

/// Rules are independent: a project can hold several for one trigger, each
/// with its own filters and channels -- new issues in production by email,
/// every new issue to Slack.
#[tokio::test]
async fn test_rules_of_one_type_fire_independently() {
    let db = TestDb::new().await;
    let project_id = create_test_project(&db.pool).await;
    let project = ProjectService::get_by_id(&db.pool, project_id)
        .await
        .unwrap();
    let channel = AlertService::create_channel(
        &db.pool,
        &Default::default(),
        CreateAlertIntegration {
            name: "Shared channel".to_string(),
            provider_type: ProviderType::Webhook,
            // Nothing listens there: the deliveries are recorded, then fail.
            credentials: json!({ "url": "http://127.0.0.1:9/hook" }),
            is_enabled: true,
        },
    )
    .await
    .unwrap();
    let rule = |name: &str, conditions| CreateAlertRule {
        name: name.to_string(),
        alert_type: AlertType::NewIssue,
        channels: vec![AlertRuleChannelInput {
            integration_id: channel.id,
            routing_override: json!({}),
        }],
        conditions,
        cooldown_minutes: 0,
    };
    let everywhere =
        AlertService::create_rule(&db.pool, project_id, rule("Every new issue", json!({})))
            .await
            .expect("a first new-issue rule");
    let also = AlertService::create_rule(
        &db.pool,
        project_id,
        rule("Also every new issue", json!({})),
    )
    .await
    .expect("a second rule for the same trigger");
    let staging = AlertService::create_rule(
        &db.pool,
        project_id,
        rule("Staging only", json!({"environments": ["staging"]})),
    )
    .await
    .expect("a third, filtered rule");

    let issue = IssueService::create(
        &db.pool,
        project_id,
        Utc::now(),
        &DenormalizedFields {
            calculated_type: "Error".to_string(),
            calculated_value: "independent rules".to_string(),
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
    .unwrap();
    AlertService::trigger_event_alert(
        &db.pool,
        &project,
        &issue,
        AlertType::NewIssue,
        uuid::Uuid::new_v4(),
        Default::default(),
        "http://localhost:3000",
        &Default::default(),
    )
    .await
    .unwrap();

    let fired: Vec<(Option<i32>,)> = sqlx::query_as(
        "SELECT alert_rule_id FROM alert_history WHERE project_id = $1 ORDER BY alert_rule_id",
    )
    .bind(project_id)
    .fetch_all(&db.pool)
    .await
    .unwrap();
    // Both unfiltered rules deliver through the channel they share; the
    // staging rule's filter keeps it quiet for an event with no environment.
    assert_eq!(fired, vec![(Some(everywhere.id),), (Some(also.id),)]);
    assert!(!fired.contains(&(Some(staging.id),)));
}

#[tokio::test]
async fn test_rule_with_invalid_channel_fails() {
    let db = TestDb::new().await;

    let project_id = create_test_project(&db.pool).await;

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

    let channel1 = AlertService::create_channel(
        &db.pool,
        &Default::default(),
        CreateAlertIntegration {
            name: "Channel 1".to_string(),
            provider_type: ProviderType::Webhook,
            credentials: json!({ "url": "https://example.com/webhook1" }),
            is_enabled: true,
        },
    )
    .await
    .unwrap();

    let channel2 = AlertService::create_channel(
        &db.pool,
        &Default::default(),
        CreateAlertIntegration {
            name: "Channel 2".to_string(),
            provider_type: ProviderType::Webhook,
            credentials: json!({ "url": "https://example.com/webhook2" }),
            is_enabled: true,
        },
    )
    .await
    .unwrap();

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

    let channels = AlertService::get_rule_channels(&db.pool, rule.id)
        .await
        .unwrap();
    assert_eq!(channels, vec![channel1.id]);

    // Update to use both channels
    AlertService::update_rule(
        &db.pool,
        rule.id,
        UpdateAlertRule {
            alert_type: None,
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
            alert_type: None,
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
        &Default::default(),
        CreateAlertIntegration {
            name: "Deletable Channel".to_string(),
            provider_type: ProviderType::Webhook,
            credentials: json!({ "url": "https://example.com/webhook" }),
            is_enabled: true,
        },
    )
    .await
    .unwrap();

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

    let history = AlertService::list_history(&db.pool, project_id, None, 50)
        .await
        .expect("Failed to list history");

    assert!(history.is_empty());
}

// Note: These tests are marked as ignored because actix-web's test framework
// doesn't properly preserve session cookies. Full testing should be done via
// E2E tests with a real HTTP client.

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

    // A webhook endpoint that accepts connections and never responds. It is
    // on loopback, which alerts refuse by default (#504): without the opt-out
    // the delivery would fail at once and nothing here would be slow.
    rustrak::services::notification::allow_private_destinations(true);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind hanging webhook listener");
    let addr = listener.local_addr().expect("listener addr");
    let connected = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = connected.clone();
    tokio::spawn(async move {
        loop {
            let Ok((socket, _)) = listener.accept().await else {
                break;
            };
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
            tokio::spawn(async move {
                let _socket = socket;
                tokio::time::sleep(Duration::from_secs(3600)).await;
            });
        }
    });

    let channel = AlertService::create_channel(
        &db.pool,
        &Default::default(),
        CreateAlertIntegration {
            name: "Hanging Webhook".to_string(),
            provider_type: ProviderType::Webhook,
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

    let smtp = rustrak::config::SmtpSettings::default();
    let trigger = AlertService::trigger_event_alert(
        &db.pool,
        &project,
        &issue,
        AlertType::NewIssue,
        uuid::Uuid::new_v4(),
        Default::default(),
        "http://localhost:3000",
        &smtp,
    );
    tokio::time::timeout(Duration::from_secs(2), trigger)
        .await
        .expect("trigger must return without waiting for webhook I/O")
        .expect("trigger must succeed");

    // The durable delivery record is committed before the caller returns,
    // leased to the in-flight dispatcher so the retry worker cannot
    // double-send it while the dispatch is still running.
    let (status, lease): (String, Option<chrono::DateTime<Utc>>) =
        sqlx::query_as("SELECT status, next_retry_at FROM alert_history WHERE project_id = $1")
            .bind(project_id)
            .fetch_one(&db.pool)
            .await
            .expect("history row must exist before trigger returns");
    let leased = lease.is_some_and(|until| until > Utc::now());
    assert_eq!(status, "pending");
    assert!(
        leased,
        "in-flight delivery must hold a future next_retry_at lease"
    );

    // The delivery really is in flight against the slow endpoint.
    crate::common::eventually(|| async {
        connected
            .load(std::sync::atomic::Ordering::SeqCst)
            .then_some(())
    })
    .await;
}

#[tokio::test]
async fn test_rule_conditions_are_validated() {
    let db = TestDb::new().await;
    let project_id = create_test_project(&db.pool).await;
    let input = |conditions: serde_json::Value| CreateAlertRule {
        name: "Filtered".to_string(),
        alert_type: AlertType::NewIssue,
        channels: vec![],
        conditions,
        cooldown_minutes: 0,
    };

    let refused =
        AlertService::create_rule(&db.pool, project_id, input(json!({ "min_level": "loud" })))
            .await;
    assert!(refused.is_err(), "an unknown level is refused");

    let rule = AlertService::create_rule(&db.pool, project_id, input(json!({ "min_events": 5 })))
        .await
        .expect("valid conditions are stored");
    assert_eq!(rule.conditions["min_events"], 5);
}

/// A rule's page lists only its own deliveries.
#[tokio::test]
async fn test_alert_history_narrows_to_one_rule() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool).await;
    let rule = |name: &str, alert_type| rustrak::models::CreateAlertRule {
        name: name.to_string(),
        alert_type,
        conditions: json!({}),
        cooldown_minutes: 0,
        channels: vec![],
    };
    let new_issue = AlertService::create_rule(
        &db.pool,
        project,
        rule("New issue", rustrak::models::AlertType::NewIssue),
    )
    .await
    .unwrap();
    let regression = AlertService::create_rule(
        &db.pool,
        project,
        rule("Regression", rustrak::models::AlertType::Regression),
    )
    .await
    .unwrap();
    for (rule_id, key) in [
        (new_issue.id, "a"),
        (new_issue.id, "b"),
        (regression.id, "c"),
    ] {
        sqlx::query(
            "INSERT INTO alert_history (project_id, alert_rule_id, alert_type, channel_type, channel_name, status, idempotency_key) VALUES ($1, $2, 'new_issue', 'webhook', 'test', 'sent', $3)",
        )
        .bind(project)
        .bind(rule_id)
        .bind(key)
        .execute(&db.pool)
        .await
        .unwrap();
    }

    let all = AlertService::list_history(&db.pool, project, None, 50)
        .await
        .unwrap();
    assert_eq!(all.len(), 3);
    let own = AlertService::list_history(&db.pool, project, Some(new_issue.id), 50)
        .await
        .unwrap();
    assert_eq!(own.len(), 2);
    assert!(own.iter().all(|h| h.alert_rule_id == Some(new_issue.id)));
}

/// One rule failing to record its alert does not cost the next rule its own,
/// and the failure still reaches the caller so the event is retried.
#[cfg(not(feature = "postgres"))]
#[tokio::test]
async fn test_a_failing_rule_does_not_stop_the_next() {
    let db = TestDb::new().await;
    let project_id = create_test_project(&db.pool).await;
    let project = ProjectService::get_by_id(&db.pool, project_id)
        .await
        .expect("project lookup must succeed");
    let channel = AlertService::create_channel(
        &db.pool,
        &Default::default(),
        CreateAlertIntegration {
            name: "Isolation Webhook".to_string(),
            provider_type: ProviderType::Webhook,
            credentials: json!({ "url": "https://example.com/webhook" }),
            is_enabled: true,
        },
    )
    .await
    .expect("channel creation must succeed");
    let mut rules = Vec::new();
    for name in ["Broken", "Healthy"] {
        let rule = AlertService::create_rule(
            &db.pool,
            project_id,
            CreateAlertRule {
                name: name.to_string(),
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
        // In cooldown: recorded, never delivered.
        sqlx::query("UPDATE alert_rules SET last_triggered_at = $1 WHERE id = $2")
            .bind(Utc::now())
            .bind(rule.id)
            .execute(&db.pool)
            .await
            .expect("cooldown setup must succeed");
        rules.push(rule.id);
    }
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE TRIGGER broken_rule BEFORE INSERT ON alert_history \
         WHEN NEW.alert_rule_id = {} BEGIN SELECT RAISE(ABORT, 'broken'); END",
        rules[0]
    )))
    .execute(&db.pool)
    .await
    .expect("trigger creation must succeed");
    let issue = IssueService::create(
        &db.pool,
        project_id,
        Utc::now(),
        &DenormalizedFields {
            calculated_type: "Error".to_string(),
            calculated_value: "isolation".to_string(),
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

    let result = AlertService::trigger_event_alert(
        &db.pool,
        &project,
        &issue,
        AlertType::NewIssue,
        Uuid::new_v4(),
        Default::default(),
        "https://dashboard.example.com",
        &Default::default(),
    )
    .await;

    assert!(
        result.is_err(),
        "the broken rule's error must reach the caller"
    );
    let recorded: Vec<i32> =
        sqlx::query_scalar("SELECT alert_rule_id FROM alert_history WHERE project_id = $1")
            .bind(project_id)
            .fetch_all(&db.pool)
            .await
            .expect("history lookup must succeed");
    assert_eq!(recorded, vec![rules[1]]);
}

/// A rule's deliveries per UTC day, counted over the whole window rather than
/// over the newest page of rows: a busy rule's older days still add up.
#[tokio::test]
async fn test_history_by_day_counts_every_delivery_in_the_window() {
    let db = TestDb::new().await;
    let project_id = create_test_project(&db.pool).await;
    let rule = AlertService::create_rule(
        &db.pool,
        project_id,
        CreateAlertRule {
            name: "Busy".to_string(),
            alert_type: AlertType::NewIssue,
            channels: vec![],
            conditions: json!({}),
            cooldown_minutes: 0,
        },
    )
    .await
    .expect("rule creation must succeed");
    let now = Utc::now();
    let today = now.date_naive().and_hms_opt(12, 0, 0).unwrap().and_utc();
    // 150 sent today, 2 failed and 1 skipped two days ago, one sent outside
    // the window, and one for no rule at all.
    let mut rows = vec![("sent", today, Some(rule.id)); 150];
    rows.extend([
        ("failed", today - chrono::Duration::days(2), Some(rule.id)),
        ("failed", today - chrono::Duration::days(2), Some(rule.id)),
        ("skipped", today - chrono::Duration::days(2), Some(rule.id)),
        ("sent", today - chrono::Duration::days(20), Some(rule.id)),
        ("sent", today, None),
    ]);
    for (i, (status, at, rule_id)) in rows.into_iter().enumerate() {
        sqlx::query(
            "INSERT INTO alert_history (alert_rule_id, project_id, alert_type, channel_type, channel_name, status, idempotency_key, created_at) VALUES ($1, $2, 'new_issue', 'webhook', 'test', $3, $4, $5)",
        )
        .bind(rule_id)
        .bind(project_id)
        .bind(status)
        .bind(format!("by-day-{project_id}-{i}"))
        .bind(at)
        .execute(&db.pool)
        .await
        .expect("history insert must succeed");
    }

    let days = AlertService::history_by_day(&db.pool, project_id, Some(rule.id), 14, now)
        .await
        .expect("daily history must load");

    assert_eq!(days.len(), 14);
    let last = days.last().unwrap();
    assert_eq!(
        last.day,
        today.date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc()
    );
    assert_eq!((last.sent, last.failed, last.skipped), (150, 0, 0));
    let two_ago = &days[11];
    assert_eq!((two_ago.sent, two_ago.failed, two_ago.skipped), (0, 2, 1));
    assert_eq!(days.iter().map(|d| d.sent).sum::<i64>(), 150);
}

/// An event delivered before alert ids carried the project left a key of the
/// form `event-{event_id}-{type}-…`. Replayed now, it must still read as done.
#[tokio::test]
async fn test_legacy_event_keys_still_block_a_replay() {
    let db = TestDb::new().await;
    let project_id = create_test_project(&db.pool).await;
    let project = ProjectService::get_by_id(&db.pool, project_id)
        .await
        .expect("project lookup must succeed");
    let channel = AlertService::create_channel(
        &db.pool,
        &Default::default(),
        CreateAlertIntegration {
            name: "Legacy Webhook".to_string(),
            provider_type: ProviderType::Webhook,
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
            name: "Regressions".to_string(),
            alert_type: AlertType::Regression,
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
    let event_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO alert_history (alert_rule_id, project_id, alert_type, channel_type, channel_name, status, idempotency_key) VALUES ($1, $2, 'regression', 'webhook', 'legacy', 'sent', $3)",
    )
    .bind(rule.id)
    .bind(project_id)
    .bind(format!("event-{event_id}-regression-{}", channel.id))
    .execute(&db.pool)
    .await
    .expect("legacy history insert must succeed");
    let issue = IssueService::create(
        &db.pool,
        project_id,
        Utc::now(),
        &DenormalizedFields {
            calculated_type: "Error".to_string(),
            calculated_value: "legacy replay".to_string(),
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

    AlertService::trigger_event_alert(
        &db.pool,
        &project,
        &issue,
        AlertType::Regression,
        event_id,
        Default::default(),
        "https://dashboard.example.com",
        &Default::default(),
    )
    .await
    .expect("replay must succeed");

    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM alert_history WHERE project_id = $1")
        .bind(project_id)
        .fetch_one(&db.pool)
        .await
        .expect("history count must succeed");
    assert_eq!(rows, 1);
}
