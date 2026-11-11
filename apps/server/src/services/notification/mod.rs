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
    let (client, request) = request.build_split();
    let request =
        request.map_err(|e| NotificationResult::failure(format!("Invalid request: {e}"), None))?;
    if !private_destinations_allowed() && !destination_allowed(request.url()) {
        return Err(NotificationResult::failure(NotPublic.to_string(), None));
    }
    let mut response = client.execute(request).await.map_err(|e| {
        let message = if refused_as_not_public(&e) {
            NotPublic.to_string()
        } else if e.is_timeout() {
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

static PRIVATE_DESTINATIONS_ALLOWED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Lets alerts reach private and loopback addresses, for an install whose
/// chat server or automation lives on its own network. Set once at startup
/// from `RUSTRAK_ALERTS_ALLOW_PRIVATE_NETWORKS`, which is also when the shared
/// client is built, so a client that cannot be built stops startup.
pub fn allow_private_destinations(allowed: bool) {
    PRIVATE_DESTINATIONS_ALLOWED.store(allowed, std::sync::atomic::Ordering::Relaxed);
    shared_http_client();
}

fn private_destinations_allowed() -> bool {
    PRIVATE_DESTINATIONS_ALLOWED.load(std::sync::atomic::Ordering::Relaxed)
}

/// An alert destination that is not on the public internet (#504). Whoever
/// configures an integration picks the URL, so without this the server would
/// reach, and echo back, anything on its own network.
#[derive(Debug)]
struct NotPublic;

impl std::fmt::Display for NotPublic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Destination not allowed: it is not on the public internet")
    }
}

impl std::error::Error for NotPublic {}

fn refused_as_not_public(e: &reqwest::Error) -> bool {
    let mut source = std::error::Error::source(e);
    while let Some(err) = source {
        if err.is::<NotPublic>() {
            return true;
        }
        source = err.source();
    }
    false
}

/// Whether a URL whose host is an IP literal points at the public internet.
/// Names are checked by [`PublicOnly`] instead: an IP literal never reaches the
/// resolver.
fn destination_allowed(url: &url::Url) -> bool {
    match url.host() {
        Some(url::Host::Ipv4(ip)) => crate::services::oauth::is_public(ip.into()),
        Some(url::Host::Ipv6(ip)) => crate::services::oauth::is_public(ip.into()),
        _ => true,
    }
}

/// Resolves a name and hands the client only its public addresses. The
/// addresses checked are the ones connected to, so a rebinding DNS answer
/// cannot slip a private one in between a check and the connection.
struct PublicOnly;

impl reqwest::dns::Resolve for PublicOnly {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        Box::pin(async move {
            let addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host((name.as_str(), 0))
                .await?
                .filter(|a| {
                    private_destinations_allowed() || crate::services::oauth::is_public(a.ip())
                })
                .collect();
            if addrs.is_empty() {
                return Err(Box::new(NotPublic) as Box<dyn std::error::Error + Send + Sync>);
            }
            Ok(Box::new(addrs.into_iter()) as reqwest::dns::Addrs)
        })
    }
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
/// Construction only fails on untenable TLS config. It fails closed: a client
/// without [`PublicOnly`] would reach private networks (#504), and
/// `reqwest::Client::new()` panics on the same TLS failure anyway. Startup
/// builds it through [`allow_private_destinations`], so a failure stops the
/// server at boot rather than on an alert.
pub(crate) fn shared_http_client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .dns_resolver(std::sync::Arc::new(PublicOnly))
            // A redirect is a second destination nobody checked.
            .redirect(reqwest::redirect::Policy::none())
            // A proxy resolves the host itself, out of PublicOnly's reach, and
            // reqwest reads HTTP(S)_PROXY / ALL_PROXY even without default
            // features. Alerts connect directly.
            .no_proxy()
            .build()
            .expect("the alert HTTP client could not be built")
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

    /// Answers every request with `response`; the flag turns true once
    /// anything connects.
    async fn local_server(
        response: &'static [u8],
    ) -> (
        std::net::SocketAddr,
        std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let reached = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = reached.clone();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                flag.store(true, std::sync::atomic::Ordering::SeqCst);
                let mut buf = [0u8; 4096];
                let _ = socket.read(&mut buf).await;
                let _ = socket.write_all(response).await;
            }
        });
        (addr, reached)
    }

    #[tokio::test]
    async fn a_delivery_does_not_follow_redirects() {
        // Sentry's safe_urlopen does not follow them either: a redirect is a
        // second destination nobody checked.
        allow_private_destinations(true);
        let (target, reached) =
            local_server(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
                .await;
        let location: &'static str = Box::leak(
            format!(
                "HTTP/1.1 302 Found\r\nlocation: http://{target}/\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
            )
            .into_boxed_str(),
        );
        let (redirector, _) = local_server(location.as_bytes()).await;

        let result = deliver(shared_http_client().post(format!("http://{redirector}/hook"))).await;

        assert_eq!(result.map(|(status, _)| status).ok(), Some(302));
        assert!(!reached.load(std::sync::atomic::Ordering::SeqCst));
    }
}
