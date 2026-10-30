//! Shared SMTP plumbing for every outbound mail the server sends.
//!
//! Both the alert dispatcher (`notification::email`) and invitation mail
//! build their transport and messages through here, so the environment
//! fallback, the TLS branch on the port, and the address rules live once.

use async_trait::async_trait;
use lettre::message::{header::ContentType, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

/// The address a message falls back to when the configured sender cannot be
/// parsed. Kept from the original alert path so behaviour does not drift.
pub const DEFAULT_FROM: &str = "alerts@rustrak.local";

/// The global SMTP configuration read from the environment. Per-integration
/// credentials override it field by field.
#[derive(Debug, Clone)]
pub struct GlobalSmtp {
    pub host: Option<String>,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub from_address: String,
}

impl GlobalSmtp {
    /// Reads `SMTP_HOST` / `SMTP_PORT` / `SMTP_USERNAME` / `SMTP_PASSWORD` /
    /// `SMTP_FROM`, with the same defaults the alert notifier has always used.
    pub fn from_env() -> Self {
        Self {
            host: std::env::var("SMTP_HOST")
                .ok()
                .filter(|host| !host.trim().is_empty()),
            port: std::env::var("SMTP_PORT")
                .unwrap_or_else(|_| "587".to_string())
                .parse()
                .unwrap_or(587),
            username: std::env::var("SMTP_USERNAME").ok(),
            password: std::env::var("SMTP_PASSWORD").ok(),
            from_address: std::env::var("SMTP_FROM").unwrap_or_else(|_| DEFAULT_FROM.to_string()),
        }
    }
}

/// The transport seam: real SMTP in production, a capturing stub in tests.
#[async_trait]
pub trait MailTransport: Send + Sync {
    async fn send_mail(&self, message: Message) -> Result<(), String>;
}

#[async_trait]
impl MailTransport for AsyncSmtpTransport<Tokio1Executor> {
    async fn send_mail(&self, message: Message) -> Result<(), String> {
        AsyncTransport::send(self, message)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

/// Builds one transport for a host/port pair. Port 465 gets an implicit-TLS
/// wrapper; anything else uses STARTTLS, matching the behaviour the alert
/// path had before this was factored out.
pub fn build_transport(
    host: &str,
    port: u16,
    username: Option<&String>,
    password: Option<&String>,
) -> Result<Box<dyn MailTransport>, String> {
    let builder = if port == 465 {
        let tls_params = lettre::transport::smtp::client::TlsParameters::new(host.to_string())
            .map_err(|e| format!("Invalid TLS parameters for SMTP host: {}", e))?;
        AsyncSmtpTransport::<Tokio1Executor>::relay(host)
            .map(|b| {
                b.port(port)
                    .tls(lettre::transport::smtp::client::Tls::Wrapper(tls_params))
            })
            .map_err(|e| format!("Invalid SMTP host: {}", e))
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)
            .map(|b| b.port(port))
            .map_err(|e| format!("Invalid SMTP host: {}", e))
    }?;

    let builder = match (username, password) {
        (Some(u), Some(p)) => builder.credentials(Credentials::new(u.clone(), p.clone())),
        _ => builder,
    };
    Ok(Box::new(builder.build()))
}

/// Why building a message failed, kept as two cases because the callers treat
/// them differently: an unparseable *recipient* is one bad row in a list the
/// dispatch should walk past, while every other failure means the message
/// itself could not be built and the whole send is suspect.
#[derive(Debug)]
pub enum BuildError {
    /// The recipient address did not parse. Carries the message for the log.
    BadRecipient(String),
    /// Anything else, as a message.
    Other(String),
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::BadRecipient(m) | BuildError::Other(m) => f.write_str(m),
        }
    }
}

/// Builds one plain+HTML multipart message.
///
/// A sender that does not parse falls back to [`DEFAULT_FROM`], as the alert
/// path has always done. A recipient that does not parse is
/// [`BuildError::BadRecipient`]: the caller decides whether to skip it or fail.
pub fn build_message(
    from: &str,
    to: &str,
    subject: &str,
    text: &str,
    html: &str,
) -> Result<Message, BuildError> {
    let from_mb = from
        .parse()
        .unwrap_or_else(|_| DEFAULT_FROM.parse().expect("valid fallback address"));
    let to_mb = to
        .parse()
        .map_err(|e| BuildError::BadRecipient(format!("Invalid email recipient {to}: {e}")))?;
    Message::builder()
        .from(from_mb)
        .to(to_mb)
        .subject(subject)
        .multipart(
            MultiPart::alternative()
                .singlepart(
                    SinglePart::builder()
                        .header(ContentType::TEXT_PLAIN)
                        .body(text.to_string()),
                )
                .singlepart(
                    SinglePart::builder()
                        .header(ContentType::TEXT_HTML)
                        .body(html.to_string()),
                ),
        )
        .map_err(|e| BuildError::Other(e.to_string()))
}

/// Simple HTML escaping for email content.
pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// A transport that records the messages it is asked to send instead of
/// delivering them.
///
/// **Test seam**, gated on `test-seams` like the send entry point that takes
/// it (`invitation_email::send_invitation_mail_with_transport`): it does not
/// exist in the shipped binary. The capture is the formatted message, so a
/// test can assert on the recipient, the link and the sender without an
/// SMTP server.
#[cfg(feature = "test-seams")]
#[derive(Clone, Default)]
pub struct CapturedMail {
    sent: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

#[cfg(feature = "test-seams")]
impl CapturedMail {
    pub fn new() -> Self {
        Self::default()
    }

    /// Everything handed to `send_mail` so far, in order.
    pub fn sent(&self) -> Vec<String> {
        self.sent.lock().expect("captured mail lock").clone()
    }
}

#[cfg(feature = "test-seams")]
#[async_trait]
impl MailTransport for CapturedMail {
    async fn send_mail(&self, message: Message) -> Result<(), String> {
        // lettre's Message has no Display: capture the wire format,
        // exactly what a real transport would hand the SMTP server.
        self.sent
            .lock()
            .expect("captured mail lock")
            .push(String::from_utf8_lossy(&message.formatted()).into_owned());
        Ok(())
    }
}
