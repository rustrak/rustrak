//! Alert delivery, one dispatcher per provider (Webhook, Custom Webhook,
//! Email, Slack).

pub mod custom_webhook;
pub mod email;
pub mod json_template;
pub mod slack;
pub mod webhook;

use async_trait::async_trait;

use crate::config::SmtpSettings;
use crate::error::AppResult;
use crate::models::{AlertIntegration, AlertPayload, ProviderType};

pub use custom_webhook::CustomWebhookNotifier;
pub use email::EmailNotifier;
pub use slack::SlackNotifier;
pub use webhook::WebhookNotifier;

#[derive(Debug)]
pub struct NotificationResult {
    pub success: bool,
    pub http_status: Option<u16>,
    pub error_message: Option<String>,
    /// What the endpoint answered, when it answered anything.
    ///
    /// Rustrak judges a delivery by the HTTP status and does not interpret
    /// the body, so the body has to be visible instead: a group bot that
    /// refuses a message still answers 200 and explains itself in here, and
    /// a reader testing an integration has no other way to find that out.
    pub response_body: Option<String>,
}

impl NotificationResult {
    pub fn success(http_status: Option<u16>) -> Self {
        Self {
            success: true,
            http_status,
            error_message: None,
            response_body: None,
        }
    }

    /// Attaches the endpoint's answer, dropping an empty one: "answered
    /// nothing" and "answered the empty string" are the same fact, and the
    /// second one renders as a blank line nobody can read.
    pub fn with_response_body(mut self, body: impl Into<String>) -> Self {
        let body = body.into();
        self.response_body = (!body.trim().is_empty()).then_some(body);
        self
    }

    pub fn failure(error_message: String, http_status: Option<u16>) -> Self {
        Self {
            success: false,
            http_status,
            error_message: Some(error_message),
            response_body: None,
        }
    }
}

/// Upper bound for a response body a dispatcher buffers. Endpoints answer an
/// alert with a small reply; the limit exists so one that answers with a giant
/// (or endless) body cannot pin memory in the notification worker.
const MAX_RESPONSE_BODY_BYTES: usize = 64 * 1024;

/// Sends `request` and returns the status and the body. The body is read up to
/// [`MAX_RESPONSE_BODY_BYTES`]: `Response::text()` buffers whatever the
/// endpoint sends, and the 30s timeout does not limit size. A body that cannot
/// be read (truncated, oversized, non-UTF-8) fails the delivery on every
/// status: on a 2xx we would otherwise record it as sent while unable to see a
/// bot rejection hidden inside it.
pub(crate) async fn deliver(
    request: reqwest::RequestBuilder,
) -> Result<(u16, String), NotificationResult> {
    let mut response = request.send().await.map_err(|e| {
        let message = if e.is_timeout() {
            "Request timed out".to_string()
        } else if e.is_connect() {
            "Connection failed".to_string()
        } else {
            format!("Request failed: {e}")
        };
        NotificationResult::failure(message, None)
    })?;
    let status = response.status().as_u16();
    let read = async {
        let mut bytes: Vec<u8> = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| format!("stream interrupted: {e}"))?
        {
            if bytes.len() + chunk.len() > MAX_RESPONSE_BODY_BYTES {
                return Err(format!(
                    "response body exceeds the {MAX_RESPONSE_BODY_BYTES}-byte limit"
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        String::from_utf8(bytes).map_err(|e| format!("body is not valid UTF-8: {e}"))
    };
    read.await.map(|body| (status, body)).map_err(|e| {
        NotificationResult::failure(
            format!("HTTP {status}: failed to read response body: {e}"),
            Some(status),
        )
    })
}

/// A plain HTTP answer: 2xx is a delivery whatever the body says (the body is
/// kept for the reader), anything else fails with the body as the reason.
pub(crate) fn http_result(status: u16, body: String) -> NotificationResult {
    if !(200..=299).contains(&status) {
        let message = if body.is_empty() {
            format!("HTTP {status}")
        } else {
            format!("HTTP {status}: {body}")
        };
        return NotificationResult::failure(message, Some(status));
    }
    NotificationResult::success(Some(status)).with_response_body(body)
}

/// Shared HTTP client for every dispatcher. Built once process-wide so each
/// delivery reuses the connection pool and TLS session instead of building a
/// fresh client per notification — `create_dispatcher` runs once per alert, so
/// a per-notifier client meant a new pool and a new TLS handshake every time.
/// Construction only fails on untenable TLS config, so a build error falls back
/// to a default client rather than panicking on the request path.
pub(crate) fn shared_http_client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    })
}

#[async_trait]
pub trait NotificationDispatcher: Send + Sync {
    /// Sends one alert. `routing` is the flat JSON routing_override from
    /// alert_rule_channels; each dispatcher deserializes it into its own
    /// routing struct (the provider type is already known). Whether the
    /// integration is enabled is the caller's decision, not the dispatcher's.
    async fn send(
        &self,
        integration: &AlertIntegration,
        routing: &serde_json::Value,
        payload: &AlertPayload,
    ) -> NotificationResult;

    /// Validates credentials before an integration is created or updated.
    fn validate_config(&self, config: &serde_json::Value) -> AppResult<()>;
}

/// The dispatcher for a provider type. `smtp` is the global SMTP configuration
/// (`Config::smtp`) an email integration falls back on.
pub fn create_dispatcher(
    provider_type: ProviderType,
    smtp: &SmtpSettings,
) -> Box<dyn NotificationDispatcher> {
    match provider_type {
        ProviderType::Webhook => Box::new(WebhookNotifier::new()),
        ProviderType::CustomWebhook => Box::new(CustomWebhookNotifier::new()),
        ProviderType::Email => Box::new(EmailNotifier::new(smtp.clone())),
        ProviderType::Slack => Box::new(SlackNotifier::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_dispatcher_shares_one_http_client() {
        // create_dispatcher runs once per alert delivery; a per-notifier client
        // would mean a fresh connection pool and TLS handshake each time.
        assert!(std::ptr::eq(shared_http_client(), shared_http_client()));
    }
}
