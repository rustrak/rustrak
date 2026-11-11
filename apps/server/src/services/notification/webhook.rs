//! Webhook notification dispatcher.
//!
//! Sends alerts as HTTP POST requests with JSON payloads.
//! Supports HMAC-SHA256 signature verification for security.
//!
//! Routing override (flat struct, no provider_type discriminator):
//! `{"url": "https://override.example.com/hook", "extra_headers": {...}}`
//!
//! Effective URL (K5): routing.url ?? credentials.url

use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use chrono::Utc;
use hmac::{Hmac, KeyInit, Mac};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use sha2::Sha256;

use super::{deliver, http_result, NotificationDispatcher, NotificationResult};
use crate::error::{AppError, AppResult};
use crate::models::{AlertIntegration, AlertPayload, WebhookConfig, WebhookRoutingOverride};

type HmacSha256 = Hmac<Sha256>;

/// Same cap Sentry puts on a Sentry App's `webhook_headers`.
const MAX_CUSTOM_HEADERS: usize = 20;

/// Headers a user may not set: the ones that frame the request, plus
/// everything under `x-rustrak-`, which carries the signature.
const RESERVED_HEADERS: [&str; 4] = [
    "content-type",
    "content-length",
    "host",
    "transfer-encoding",
];
const RESERVED_HEADER_PREFIX: &str = "x-rustrak-";

/// Validates user-supplied headers (credential `headers` or routing
/// `extra_headers`) at save time, so a bad one is a form error rather than
/// a failed delivery when an incident fires.
///
/// Unlike Sentry, which allows only `Authorization` and `X-*`, any name is
/// accepted outside the reserved ones: APIs such as Brevo authenticate with
/// a bare `api-key` header.
pub(crate) fn validate_headers(headers: Option<&HashMap<String, String>>) -> AppResult<()> {
    let Some(headers) = headers else {
        return Ok(());
    };
    if headers.len() > MAX_CUSTOM_HEADERS {
        return Err(AppError::Validation(format!(
            "Cannot configure more than {MAX_CUSTOM_HEADERS} custom headers"
        )));
    }
    let mut seen = HashSet::new();
    for (name, value) in headers {
        let parsed = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| AppError::Validation(format!("Invalid header name '{name}'")))?;
        // HeaderValue refuses CR/LF and other control characters.
        HeaderValue::from_str(value)
            .map_err(|_| AppError::Validation(format!("Invalid value for header '{name}'")))?;
        let lower = parsed.as_str();
        if RESERVED_HEADERS.contains(&lower) || lower.starts_with(RESERVED_HEADER_PREFIX) {
            return Err(AppError::Validation(format!(
                "'{name}' is a reserved header and cannot be set"
            )));
        }
        if !seen.insert(parsed) {
            return Err(AppError::Validation(format!(
                "Duplicate header '{name}': each header may only be set once"
            )));
        }
    }
    Ok(())
}

/// Builds the request headers: each user layer replaces the one before it
/// (credentials, then routing extras), and Rustrak's own headers go last so
/// no custom header can override the signature.
pub(crate) fn request_headers<'a>(
    user_layers: impl IntoIterator<Item = Option<&'a HashMap<String, String>>>,
    own: HeaderMap,
) -> Result<HeaderMap, String> {
    let mut map = HeaderMap::new();
    for (name, value) in user_layers.into_iter().flatten().flatten() {
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| format!("Invalid header name '{name}'"))?;
        let value = HeaderValue::from_str(value)
            .map_err(|_| format!("Invalid value for header '{name}'"))?;
        map.insert(name, value);
    }
    // `extend` replaces existing keys rather than appending to them.
    map.extend(own);
    Ok(map)
}

/// The headers every Rustrak webhook request carries, signature included
/// when a secret is configured.
pub(crate) fn own_headers(
    timestamp: &str,
    request_id: &str,
    signature: Option<String>,
) -> Result<HeaderMap, String> {
    let value =
        |v: &str| HeaderValue::from_str(v).map_err(|_| format!("Invalid header value '{v}'"));
    let mut map = HeaderMap::new();
    map.insert("content-type", HeaderValue::from_static("application/json"));
    map.insert("x-rustrak-timestamp", value(timestamp)?);
    map.insert("x-rustrak-request-id", value(request_id)?);
    if let Some(signature) = signature {
        map.insert(
            "x-rustrak-signature",
            value(&format!("sha256={signature}"))?,
        );
    }
    Ok(map)
}

pub struct WebhookNotifier {
    client: reqwest::Client,
}

impl WebhookNotifier {
    pub fn new() -> Self {
        Self {
            client: super::shared_http_client().clone(),
        }
    }

    /// Generates HMAC-SHA256 signature for webhook payload
    ///
    /// Shared with the custom webhook dispatcher; same `timestamp.payload`
    /// canonicalisation and `sha256=<hex>` scheme.
    pub(crate) fn generate_signature(secret: &str, timestamp: &str, payload: &[u8]) -> String {
        let signature_payload = format!("{}.{}", timestamp, String::from_utf8_lossy(payload));
        let mut mac =
            HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC can take key of any size");
        mac.update(signature_payload.as_bytes());
        let result = mac.finalize();
        hex::encode(result.into_bytes())
    }

    /// Computes the effective URL: routing.url ?? credentials.url (K5).
    pub fn effective_url<'a>(
        routing: &'a WebhookRoutingOverride,
        credentials: &'a WebhookConfig,
    ) -> Option<&'a str> {
        routing.url.as_deref().or(credentials.url.as_deref())
    }
}

impl Default for WebhookNotifier {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl NotificationDispatcher for WebhookNotifier {
    async fn send(
        &self,
        integration: &AlertIntegration,
        routing: &serde_json::Value,
        payload: &AlertPayload,
    ) -> NotificationResult {
        let credentials: WebhookConfig =
            match serde_json::from_value(integration.credentials.clone()) {
                Ok(c) => c,
                Err(e) => {
                    return NotificationResult::failure(
                        format!("Invalid webhook credentials: {}", e),
                        None,
                    )
                }
            };

        let routing_override: WebhookRoutingOverride = serde_json::from_value(routing.clone())
            .unwrap_or(WebhookRoutingOverride {
                url: None,
                extra_headers: None,
            });

        // Effective URL: routing wins over credentials (K5)
        let url = match Self::effective_url(&routing_override, &credentials) {
            Some(u) => u.to_string(),
            None => {
                return NotificationResult::failure(
                    "Webhook URL not configured in credentials or routing_override".to_string(),
                    None,
                )
            }
        };

        let body = match serde_json::to_vec(payload) {
            Ok(b) => b,
            Err(e) => {
                return NotificationResult::failure(
                    format!("Failed to serialize payload: {}", e),
                    None,
                )
            }
        };

        let timestamp = Utc::now().timestamp().to_string();

        let signature = credentials
            .secret
            .as_deref()
            .map(|secret| Self::generate_signature(secret, &timestamp, &body));
        let own = match own_headers(&timestamp, &payload.alert_id, signature) {
            Ok(h) => h,
            Err(e) => return NotificationResult::failure(e, None),
        };
        // Credential-level headers, routing-level extras on top, ours last.
        let headers = match request_headers(
            [
                credentials.headers.as_ref(),
                routing_override.extra_headers.as_ref(),
            ],
            own,
        ) {
            Ok(h) => h,
            Err(e) => return NotificationResult::failure(e, None),
        };

        match deliver(self.client.post(url).headers(headers).body(body)).await {
            Ok((status, body)) => http_result(status, body),
            Err(failure) => failure,
        }
    }

    fn validate_config(&self, config: &serde_json::Value) -> AppResult<()> {
        let webhook_config: WebhookConfig = serde_json::from_value(config.clone())
            .map_err(|e| AppError::Validation(format!("Invalid webhook config: {}", e)))?;

        // URL in credentials is optional — webhook can rely entirely on routing_override.url
        // Validate format if present
        if let Some(ref url) = webhook_config.url {
            if url.is_empty() {
                return Err(AppError::Validation(
                    "Webhook URL cannot be empty if provided".to_string(),
                ));
            }

            let parsed_url = url::Url::parse(url)
                .map_err(|_| AppError::Validation("Invalid webhook URL format".to_string()))?;

            if parsed_url.scheme() != "http" && parsed_url.scheme() != "https" {
                return Err(AppError::Validation(
                    "Webhook URL must use HTTP or HTTPS".to_string(),
                ));
            }
        }

        validate_headers(webhook_config.headers.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_effective_url_routing_wins_over_credentials() {
        let routing = WebhookRoutingOverride {
            url: Some("https://routing.example.com/hook".to_string()),
            extra_headers: None,
        };
        let credentials = WebhookConfig {
            url: Some("https://creds.example.com/hook".to_string()),
            secret: None,
            headers: None,
        };
        let url = WebhookNotifier::effective_url(&routing, &credentials);
        assert_eq!(url, Some("https://routing.example.com/hook"));
    }

    #[test]
    fn test_effective_url_falls_back_to_credentials() {
        let routing = WebhookRoutingOverride {
            url: None,
            extra_headers: None,
        };
        let credentials = WebhookConfig {
            url: Some("https://creds.example.com/hook".to_string()),
            secret: None,
            headers: None,
        };
        let url = WebhookNotifier::effective_url(&routing, &credentials);
        assert_eq!(url, Some("https://creds.example.com/hook"));
    }

    #[test]
    fn test_effective_url_none_when_both_absent() {
        let routing = WebhookRoutingOverride {
            url: None,
            extra_headers: None,
        };
        let credentials = WebhookConfig {
            url: None,
            secret: None,
            headers: None,
        };
        let url = WebhookNotifier::effective_url(&routing, &credentials);
        assert!(url.is_none());
    }

    #[test]
    fn test_generate_signature() {
        let secret = "test-secret";
        let timestamp = "1706140800";
        let payload = b"{\"test\":\"data\"}";

        let signature = WebhookNotifier::generate_signature(secret, timestamp, payload);

        assert_eq!(signature.len(), 64);
        assert!(signature.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_signature_consistency() {
        let secret = "my-secret";
        let timestamp = "1234567890";
        let payload = b"hello world";

        let sig1 = WebhookNotifier::generate_signature(secret, timestamp, payload);
        let sig2 = WebhookNotifier::generate_signature(secret, timestamp, payload);

        assert_eq!(sig1, sig2);
    }

    #[test]
    fn test_signature_changes_with_secret() {
        let timestamp = "1234567890";
        let payload = b"hello world";

        let sig1 = WebhookNotifier::generate_signature("secret1", timestamp, payload);
        let sig2 = WebhookNotifier::generate_signature("secret2", timestamp, payload);

        assert_ne!(sig1, sig2);
    }

    // -------------------------------------------------------------------------
    // Custom headers (#419)
    // -------------------------------------------------------------------------

    fn headers(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn test_validate_headers_accepts_auth_headers_of_real_apis() {
        // Bearer token (Resend, SendGrid), bare API key (Brevo), vendor X-*.
        let h = headers(&[
            ("Authorization", "Bearer re_123"),
            ("api-key", "xkeysib-123"),
            ("X-Postmark-Server-Token", "abc"),
        ]);
        validate_headers(Some(&h)).expect("ordinary auth headers must pass");
        validate_headers(None).expect("no headers must pass");
    }

    #[test]
    fn test_validate_headers_rejects_newline_in_value() {
        let h = headers(&[("Authorization", "Bearer x\r\nX-Injected: 1")]);
        let err = validate_headers(Some(&h)).expect_err("CR/LF must be rejected");
        assert!(err.to_string().contains("Authorization"), "got: {err}");
    }

    #[test]
    fn test_validate_headers_rejects_invalid_name() {
        let h = headers(&[("Bad Header", "x")]);
        assert!(validate_headers(Some(&h)).is_err());
        let h = headers(&[("", "x")]);
        assert!(validate_headers(Some(&h)).is_err());
    }

    #[test]
    fn test_validate_headers_rejects_reserved_names() {
        for name in [
            "Content-Type",
            "content-length",
            "Host",
            "Transfer-Encoding",
            "X-Rustrak-Signature",
            "x-rustrak-anything",
        ] {
            let h = headers(&[(name, "x")]);
            let err = validate_headers(Some(&h))
                .expect_err(&format!("{name} is reserved and must be rejected"));
            assert!(err.to_string().contains("reserved"), "got: {err}");
        }
    }

    #[test]
    fn test_validate_headers_rejects_case_insensitive_duplicates() {
        // HashMap keeps both; which one is sent would depend on hash order.
        let h = headers(&[("Authorization", "a"), ("authorization", "b")]);
        let err = validate_headers(Some(&h)).expect_err("duplicate names must be rejected");
        assert!(err.to_string().contains("Duplicate"), "got: {err}");
    }

    #[test]
    fn test_validate_headers_rejects_more_than_twenty() {
        let names: Vec<String> = (0..21).map(|i| format!("X-H{i}")).collect();
        let h: HashMap<String, String> = names.iter().map(|n| (n.clone(), "v".into())).collect();
        assert!(validate_headers(Some(&h)).is_err());
        let h: HashMap<String, String> = names[..20]
            .iter()
            .map(|n| (n.clone(), "v".into()))
            .collect();
        validate_headers(Some(&h)).expect("twenty headers must pass");
    }

    #[test]
    fn test_validate_config_rejects_invalid_headers() {
        let config = serde_json::json!({
            "url": "https://example.com/hook",
            "headers": {"Content-Type": "text/plain"},
        });
        assert!(WebhookNotifier::new().validate_config(&config).is_err());
    }

    #[test]
    fn test_request_headers_routing_extra_replaces_credential_header() {
        let creds = headers(&[("Authorization", "cred")]);
        let extra = headers(&[("authorization", "route")]);
        let map =
            request_headers([Some(&creds), Some(&extra)], HeaderMap::new()).expect("valid headers");
        let values: Vec<_> = map.get_all("authorization").iter().collect();
        assert_eq!(values, vec!["route"], "one value, the routing one");
    }

    #[test]
    fn test_request_headers_rustrak_headers_win() {
        // Stored before validation existed, or sneaked in: still cannot
        // override the signature.
        let creds = headers(&[("X-Rustrak-Signature", "forged"), ("X-Custom", "kept")]);
        let mut own = HeaderMap::new();
        own.insert(
            "x-rustrak-signature",
            HeaderValue::from_static("sha256=real"),
        );
        let map = request_headers([Some(&creds), None], own).expect("valid headers");
        let sig: Vec<_> = map.get_all("x-rustrak-signature").iter().collect();
        assert_eq!(sig, vec!["sha256=real"]);
        assert_eq!(map.get("x-custom").unwrap(), "kept");
    }

    #[test]
    fn test_request_headers_invalid_header_is_an_error() {
        let creds = headers(&[("Bad Header", "x")]);
        assert!(request_headers([Some(&creds), None], HeaderMap::new()).is_err());
    }
}
