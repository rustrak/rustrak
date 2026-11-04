//! Email notification dispatcher.
//!
//! Sends alerts via SMTP using the lettre crate.
//! Supports both plain text and HTML email formats.
//!
//! Routing override (flat struct, no provider_type discriminator — SCL-1):
//! `{"recipients": ["a@b.com", "c@d.com"]}`

use async_trait::async_trait;
use lettre::message::header::ContentType;
use lettre::{AsyncTransport, Message};

use super::{NotificationDispatcher, NotificationResult};
use crate::error::{AppError, AppResult};
use crate::models::{AlertIntegration, AlertPayload, EmailConfig, EmailRoutingOverride};
use crate::services::mail::{self, html_escape, SmtpSettings};

/// Email notification dispatcher
pub struct EmailNotifier {
    /// Global SMTP configuration (fallback if integration doesn't specify)
    global: SmtpSettings,
}

impl EmailNotifier {
    /// Creates a new email notifier with global SMTP settings from environment
    pub fn new() -> Self {
        Self {
            global: SmtpSettings::from_env(),
        }
    }

    /// Formats an alert as HTML email body
    fn format_html(payload: &AlertPayload) -> String {
        let level_color = match payload.issue.level.as_deref() {
            Some("fatal") => "#dc2626",
            Some("error") => "#ef4444",
            Some("warning") => "#f59e0b",
            Some("info") => "#3b82f6",
            _ => "#6b7280",
        };

        let alert_type_display = payload
            .alert_type
            .replace('_', " ")
            .split_whitespace()
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().chain(chars).collect(),
                    None => String::new(),
                }
            })
            .collect::<Vec<String>>()
            .join(" ");

        format!(
            r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
</head>
<body style="font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; margin: 0; padding: 20px; background-color: #f3f4f6;">
    <div style="max-width: 600px; margin: 0 auto; background-color: #ffffff; border-radius: 8px; overflow: hidden; box-shadow: 0 1px 3px rgba(0,0,0,0.1);">
        <div style="background-color: {level_color}; padding: 16px 24px;">
            <h1 style="color: #ffffff; margin: 0; font-size: 18px; font-weight: 600;">
                {alert_type_display} in {project_name}
            </h1>
        </div>
        <div style="padding: 24px;">
            <h2 style="margin: 0 0 8px 0; font-size: 16px; color: #111827;">
                <a href="{issue_url}" style="color: #2563eb; text-decoration: none;">
                    {short_id}
                </a>
            </h2>
            <p style="margin: 0 0 24px 0; font-size: 14px; color: #374151; line-height: 1.5;">
                {title}
            </p>
            <table style="width: 100%; border-collapse: collapse; font-size: 13px;">
                <tr>
                    <td style="padding: 8px 0; color: #6b7280; border-top: 1px solid #e5e7eb;">Events</td>
                    <td style="padding: 8px 0; color: #111827; border-top: 1px solid #e5e7eb; text-align: right;">{event_count}</td>
                </tr>
                <tr>
                    <td style="padding: 8px 0; color: #6b7280; border-top: 1px solid #e5e7eb;">First seen</td>
                    <td style="padding: 8px 0; color: #111827; border-top: 1px solid #e5e7eb; text-align: right;">{first_seen}</td>
                </tr>
                <tr>
                    <td style="padding: 8px 0; color: #6b7280; border-top: 1px solid #e5e7eb;">Last seen</td>
                    <td style="padding: 8px 0; color: #111827; border-top: 1px solid #e5e7eb; text-align: right;">{last_seen}</td>
                </tr>
            </table>
            <div style="margin-top: 24px;">
                <a href="{issue_url}" style="display: inline-block; background-color: #2563eb; color: #ffffff; padding: 10px 20px; border-radius: 6px; text-decoration: none; font-size: 14px; font-weight: 500;">
                    View Issue
                </a>
            </div>
        </div>
        <div style="padding: 16px 24px; background-color: #f9fafb; border-top: 1px solid #e5e7eb;">
            <p style="margin: 0; font-size: 12px; color: #6b7280;">
                This alert was sent by Rustrak for project {project_name}.
            </p>
        </div>
    </div>
</body>
</html>"#,
            level_color = level_color,
            alert_type_display = alert_type_display,
            project_name = html_escape(&payload.project.name),
            issue_url = payload.issue_url,
            short_id = html_escape(&payload.issue.short_id),
            title = html_escape(&payload.issue.title),
            event_count = payload.issue.event_count,
            first_seen = payload.issue.first_seen.format("%Y-%m-%d %H:%M UTC"),
            last_seen = payload.issue.last_seen.format("%Y-%m-%d %H:%M UTC"),
        )
    }

    /// Formats an alert as plain text email body
    fn format_text(payload: &AlertPayload) -> String {
        let alert_type_display = payload
            .alert_type
            .replace('_', " ")
            .split_whitespace()
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().chain(chars).collect(),
                    None => String::new(),
                }
            })
            .collect::<Vec<String>>()
            .join(" ");

        format!(
            r#"{alert_type_display} in {project_name}

{short_id}: {title}

Events: {event_count}
First seen: {first_seen}
Last seen: {last_seen}

View issue: {issue_url}

--
This alert was sent by Rustrak for project {project_name}."#,
            alert_type_display = alert_type_display,
            project_name = payload.project.name,
            short_id = payload.issue.short_id,
            title = payload.issue.title,
            event_count = payload.issue.event_count,
            first_seen = payload.issue.first_seen.format("%Y-%m-%d %H:%M UTC"),
            last_seen = payload.issue.last_seen.format("%Y-%m-%d %H:%M UTC"),
            issue_url = payload.issue_url,
        )
    }

    /// Core send logic, separated for testability.
    /// `credentials` contains SMTP config; `recipients` come from routing_override.
    async fn send_to_recipients(
        &self,
        credentials: &EmailConfig,
        recipients: &[String],
        payload: &AlertPayload,
    ) -> NotificationResult {
        // Integration credentials override the global settings field by field.
        let smtp = self.global.clone().overridden_by(credentials);
        let Some(smtp_host) = smtp.host.as_ref() else {
            return NotificationResult::failure("SMTP host not configured".to_string(), None);
        };

        // Build email subject
        let alert_type_display = payload
            .alert_type
            .replace('_', " ")
            .split_whitespace()
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().chain(chars).collect(),
                    None => String::new(),
                }
            })
            .collect::<Vec<String>>()
            .join(" ");

        let subject = format!(
            "[{}] {} - {}",
            payload.project.name, alert_type_display, payload.issue.short_id
        );

        let html_body = Self::format_html(payload);
        let text_body = Self::format_text(payload);

        let mut sent_any = false;
        for recipient in recipients {
            let email = match Message::builder()
                .from(
                    smtp.from_address
                        .parse()
                        .unwrap_or_else(|_| mail::DEFAULT_FROM.parse().unwrap()),
                )
                .to(match recipient.parse() {
                    Ok(addr) => addr,
                    Err(_) => {
                        log::warn!("Invalid email recipient: {}", recipient);
                        continue;
                    }
                })
                .subject(&subject)
                .multipart(
                    lettre::message::MultiPart::alternative()
                        .singlepart(
                            lettre::message::SinglePart::builder()
                                .header(ContentType::TEXT_PLAIN)
                                .body(text_body.clone()),
                        )
                        .singlepart(
                            lettre::message::SinglePart::builder()
                                .header(ContentType::TEXT_HTML)
                                .body(html_body.clone()),
                        ),
                ) {
                Ok(email) => email,
                Err(e) => {
                    return NotificationResult::failure(
                        format!("Failed to build email: {}", e),
                        None,
                    )
                }
            };

            let mailer = match mail::build_transport(
                smtp_host,
                smtp.port,
                smtp.tls,
                smtp.username.as_ref(),
                smtp.password.as_ref(),
            ) {
                Ok(mailer) => mailer,
                Err(e) => return NotificationResult::failure(e, None),
            };

            match mailer.send(email).await {
                Ok(_) => {
                    sent_any = true;
                    log::debug!("Email sent successfully to {}", recipient);
                }
                Err(e) => {
                    return NotificationResult::failure(
                        format!("Failed to send email to {}: {}", recipient, e),
                        None,
                    )
                }
            }
        }

        if !sent_any {
            return NotificationResult::failure("No valid email recipients".to_string(), None);
        }

        NotificationResult::success(None)
    }
}

impl Default for EmailNotifier {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl NotificationDispatcher for EmailNotifier {
    async fn send(
        &self,
        integration: &AlertIntegration,
        routing: &serde_json::Value,
        payload: &AlertPayload,
    ) -> NotificationResult {
        // is_enabled check FIRST (SCL-1)
        if !integration.is_enabled {
            log::debug!(
                "Skipping dispatch to disabled Email integration {} ({})",
                integration.id,
                integration.name
            );
            return NotificationResult::success(None);
        }

        // Parse credentials (SMTP config, no recipients)
        let credentials: EmailConfig = match serde_json::from_value(integration.credentials.clone())
        {
            Ok(c) => c,
            Err(e) => {
                return NotificationResult::failure(
                    format!("Invalid email credentials: {}", e),
                    None,
                )
            }
        };

        // Parse flat routing override — recipients come from here (SCL-1, no tag)
        let routing_override: EmailRoutingOverride = match serde_json::from_value(routing.clone()) {
            Ok(r) => r,
            Err(e) => {
                return NotificationResult::failure(format!("Invalid email routing: {}", e), None)
            }
        };

        self.send_to_recipients(&credentials, &routing_override.recipients, payload)
            .await
    }

    fn validate_config(&self, config: &serde_json::Value) -> AppResult<()> {
        // Validate SMTP credentials — not recipients (those live in routing_override)
        let email_config: EmailConfig = serde_json::from_value(config.clone())
            .map_err(|e| AppError::Validation(format!("Invalid email config: {}", e)))?;

        // If no global SMTP and no integration SMTP, warn
        if email_config.smtp_host.is_none() && self.global.host.is_none() {
            return Err(AppError::Validation(
                "SMTP host must be configured either globally or per-integration".to_string(),
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn create_test_payload() -> AlertPayload {
        AlertPayload {
            alert_id: "test-123".to_string(),
            alert_type: "new_issue".to_string(),
            triggered_at: Utc::now(),
            project: crate::models::ProjectInfo {
                id: 1,
                name: "Test Project".to_string(),
                slug: "test-project".to_string(),
            },
            issue: crate::models::IssueInfo {
                id: "abc-123".to_string(),
                short_id: "TEST-1".to_string(),
                title: "TypeError: Cannot read property 'x' of undefined".to_string(),
                level: Some("error".to_string()),
                first_seen: Utc::now(),
                last_seen: Utc::now(),
                event_count: 5,
            },
            issue_url: "https://example.com/issues/abc-123".to_string(),
            actor: "Rustrak".to_string(),
        }
    }

    fn make_email_integration(is_enabled: bool) -> AlertIntegration {
        AlertIntegration {
            id: 2,
            name: "Test Email".to_string(),
            provider_type: crate::models::ProviderType::Email,
            credentials: serde_json::json!({
                "smtp_host": "smtp.example.com",
                "smtp_port": 587,
                "smtp_username": "user",
                "smtp_password": "pass",
                "from_address": "alerts@example.com"
            }),
            is_enabled,
            failure_count: 0,
            last_failure_at: None,
            last_failure_message: None,
            last_success_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    // -------------------------------------------------------------------------
    // Task 4 RED→GREEN: Email dispatcher reads recipients from flat routing
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_disabled_email_integration_skipped() {
        let notifier = EmailNotifier::new();
        let integration = make_email_integration(false);
        let routing = serde_json::json!({"recipients": ["a@b.com"]});
        let payload = create_test_payload();

        let result = notifier.send(&integration, &routing, &payload).await;

        assert!(
            result.success,
            "disabled integration must return success (skip)"
        );
    }

    #[test]
    fn test_format_html_contains_key_elements() {
        let payload = create_test_payload();
        let html = EmailNotifier::format_html(&payload);

        assert!(html.contains("Test Project"));
        assert!(html.contains("TEST-1"));
        assert!(html.contains("TypeError"));
        assert!(html.contains("View Issue"));
    }

    #[test]
    fn test_format_text_contains_key_elements() {
        let payload = create_test_payload();
        let text = EmailNotifier::format_text(&payload);

        assert!(text.contains("Test Project"));
        assert!(text.contains("TEST-1"));
        assert!(text.contains("TypeError"));
        assert!(text.contains("https://example.com/issues/abc-123"));
    }
}
