//! Unit tests for notification channel configuration validation
//!
//! Tests the public validate_config API for webhook, slack, and email notifiers.

use rustrak::config::SmtpSettings;
use rustrak::models::ProviderType;
use rustrak::services::create_dispatcher;
use serde_json::json;

/// The global SMTP configuration with a host set.
fn smtp_with_host() -> SmtpSettings {
    SmtpSettings {
        host: Some("smtp.example.com".to_string()),
        ..SmtpSettings::default()
    }
}

#[test]
fn test_webhook_validate_config_valid() {
    let dispatcher = create_dispatcher(ProviderType::Webhook, &Default::default());
    let config = json!({
        "url": "https://example.com/webhook",
        "secret": "my-secret"
    });

    assert!(dispatcher.validate_config(&config).is_ok());
}

#[test]
fn test_webhook_validate_config_empty_credentials_ok() {
    // In two-tier model, URL is optional in credentials — it can come from routing_override.
    // Empty credentials are valid; URL presence is enforced at rule-create time.
    let dispatcher = create_dispatcher(ProviderType::Webhook, &Default::default());
    let config = json!({});

    assert!(dispatcher.validate_config(&config).is_ok());
}

#[test]
fn test_webhook_validate_config_invalid_url() {
    let dispatcher = create_dispatcher(ProviderType::Webhook, &Default::default());
    let config = json!({
        "url": "not-a-url"
    });

    assert!(dispatcher.validate_config(&config).is_err());
}

#[test]
fn test_webhook_validate_config_invalid_scheme() {
    let dispatcher = create_dispatcher(ProviderType::Webhook, &Default::default());
    let config = json!({
        "url": "ftp://example.com/webhook"
    });

    assert!(dispatcher.validate_config(&config).is_err());
}

/// A template body the dispatcher can render for every test here.
const VALID_TEMPLATE: &str = r#"{"msgtype":"text","text":{"content":"{{ issue.title }}"}}"#;

#[test]
fn test_custom_webhook_factory_returns_dispatcher() {
    // create_dispatcher must know the variant or this panics on None behavior
    let dispatcher = create_dispatcher(ProviderType::CustomWebhook, &Default::default());
    assert!(dispatcher
        .validate_config(&json!({"url": "https://example.com/hook", "template": VALID_TEMPLATE}))
        .is_ok());
}

#[test]
fn test_slack_validate_config_valid() {
    let dispatcher = create_dispatcher(ProviderType::Slack, &Default::default());
    let config = json!({
        "method": "webhook",
        "webhook_url": "https://hooks.slack.com/services/T00000000/B00000000/XXXXXXXX"
    });

    assert!(dispatcher.validate_config(&config).is_ok());
}

#[test]
fn test_slack_validate_config_missing_url() {
    let dispatcher = create_dispatcher(ProviderType::Slack, &Default::default());
    // method field present but webhook_url missing — serde should fail
    let config = json!({ "method": "webhook" });

    assert!(dispatcher.validate_config(&config).is_err());
}

#[test]
fn test_slack_validate_config_missing_method_field_is_err() {
    // Legacy shape without method field — should fail after migration
    // (migration adds the field; configs missing it are invalid at validate time)
    let dispatcher = create_dispatcher(ProviderType::Slack, &Default::default());
    let config = json!({
        "webhook_url": "https://hooks.slack.com/services/T00000000/B00000000/XXXXXXXX"
    });

    assert!(dispatcher.validate_config(&config).is_err());
}

#[test]
fn test_slack_validate_config_invalid_domain() {
    let dispatcher = create_dispatcher(ProviderType::Slack, &Default::default());
    let config = json!({
        "method": "webhook",
        "webhook_url": "https://example.com/webhook"
    });

    assert!(dispatcher.validate_config(&config).is_err());
}

#[test]
fn test_slack_validate_config_rejects_subdomain_bypass() {
    let dispatcher = create_dispatcher(ProviderType::Slack, &Default::default());
    let config = json!({
        "method": "webhook",
        "webhook_url": "https://hooks.slack.com.evil.com/services/T00000000/B00000000/XXXXXXXX"
    });

    assert!(dispatcher.validate_config(&config).is_err());
}

#[test]
fn test_slack_validate_config_rejects_http() {
    let dispatcher = create_dispatcher(ProviderType::Slack, &Default::default());
    let config = json!({
        "method": "webhook",
        "webhook_url": "http://hooks.slack.com/services/T00000000/B00000000/XXXXXXXX"
    });

    assert!(dispatcher.validate_config(&config).is_err());
}

#[test]
fn test_slack_bot_token_validate_config_valid() {
    // In two-tier model, channel is in routing_override (not credentials).
    // validate_config only checks the token prefix.
    let dispatcher = create_dispatcher(ProviderType::Slack, &Default::default());
    let config = json!({
        "method": "bot_token",
        "token": "xoxb-123456789-abcdefghij"
    });

    assert!(dispatcher.validate_config(&config).is_ok());
}

#[test]
fn test_slack_bot_token_validate_config_channel_is_routing_concern() {
    // In two-tier model, channel moves to routing_override — not validated in credentials.
    // validate_config only checks the token prefix; channel (empty or absent) is accepted.
    let dispatcher = create_dispatcher(ProviderType::Slack, &Default::default());
    let config = json!({
        "method": "bot_token",
        "token": "xoxb-valid"
    });

    assert!(dispatcher.validate_config(&config).is_ok());
}

// In two-tier model, email credentials are SMTP config only; recipients live in
// routing_override and are validated by validate_routing_override at rule-create time.

#[test]
fn test_email_validate_config_valid() {
    // Valid: SMTP host is set globally, credentials may be empty
    let dispatcher = create_dispatcher(ProviderType::Email, &smtp_with_host());
    let config = json!({});

    let result = dispatcher.validate_config(&config);
    assert!(result.is_ok());
}

#[test]
fn test_email_validate_config_no_smtp_host_fails() {
    // No global SMTP and no integration SMTP should fail validation
    let dispatcher = create_dispatcher(ProviderType::Email, &Default::default());
    let config = json!({});

    let result = dispatcher.validate_config(&config);
    assert!(result.is_err());
}

#[test]
fn test_email_validate_config_recipients_are_routing_concern() {
    // recipients in credentials are silently ignored — they belong in routing_override
    let dispatcher = create_dispatcher(ProviderType::Email, &smtp_with_host());
    let config = json!({ "smtp_host": "smtp.example.com" });

    // Extra fields or missing recipients: validate_config still returns Ok
    assert!(dispatcher.validate_config(&config).is_ok());
}

// =============================================================================
// Destinations on private networks (SSRF, #504)
// =============================================================================

/// An HTTP server on loopback that answers every request with a 500 carrying
/// `internal-secret`, standing in for an internal service. Returns its address
/// and a flag that turns true when anything connects to it.
async fn internal_service() -> (
    std::net::SocketAddr,
    std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let reached = std::sync::Arc::new(AtomicBool::new(false));
    let flag = reached.clone();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            flag.store(true, Ordering::SeqCst);
            let mut buf = [0u8; 4096];
            let _ = socket.read(&mut buf).await;
            let _ = socket
                .write_all(
                    b"HTTP/1.1 500 Internal Server Error\r\ncontent-length: 15\r\nconnection: close\r\n\r\ninternal-secret",
                )
                .await;
        }
    });
    (addr, reached)
}

fn webhook_integration(credentials: serde_json::Value) -> rustrak::models::AlertIntegration {
    rustrak::models::AlertIntegration {
        id: 1,
        name: "webhook".to_string(),
        provider_type: ProviderType::Webhook,
        credentials,
        is_enabled: true,
        failure_count: 0,
        last_failure_at: None,
        last_failure_message: None,
        last_success_at: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

fn test_payload() -> rustrak::models::AlertPayload {
    use rustrak::models::{AlertPayload, IssueInfo, ProjectInfo};
    AlertPayload {
        alert_id: "test".to_string(),
        alert_type: "test".to_string(),
        triggered_at: chrono::Utc::now(),
        project: ProjectInfo {
            id: 0,
            name: "Test Project".to_string(),
            slug: "test-project".to_string(),
        },
        issue: IssueInfo {
            id: "00000000-0000-0000-0000-000000000000".to_string(),
            short_id: "TEST-1".to_string(),
            title: "test".to_string(),
            level: None,
            first_seen: chrono::Utc::now(),
            last_seen: chrono::Utc::now(),
            event_count: 1,
        },
        issue_url: "https://example.com/test".to_string(),
        actor: "test".to_string(),
    }
}

#[tokio::test]
async fn webhook_to_a_loopback_ip_is_refused_without_connecting() {
    let (addr, reached) = internal_service().await;
    let dispatcher = create_dispatcher(ProviderType::Webhook, &Default::default());

    let result = dispatcher
        .send(
            &webhook_integration(json!({})),
            &json!({ "url": format!("http://{addr}/hook") }),
            &test_payload(),
        )
        .await;

    assert!(!result.success);
    let message = result.error_message.unwrap_or_default();
    assert!(message.contains("not allowed"), "got: {message}");
    assert!(!message.contains("internal-secret"));
    assert!(!reached.load(std::sync::atomic::Ordering::SeqCst));
}

#[tokio::test]
async fn webhook_to_a_host_resolving_to_loopback_is_refused_without_connecting() {
    let (addr, reached) = internal_service().await;
    let dispatcher = create_dispatcher(ProviderType::Webhook, &Default::default());

    let result = dispatcher
        .send(
            &webhook_integration(json!({})),
            &json!({ "url": format!("http://localhost:{}/hook", addr.port()) }),
            &test_payload(),
        )
        .await;

    assert!(!result.success);
    let message = result.error_message.unwrap_or_default();
    assert!(message.contains("not allowed"), "got: {message}");
    assert!(!reached.load(std::sync::atomic::Ordering::SeqCst));
}
