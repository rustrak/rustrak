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
