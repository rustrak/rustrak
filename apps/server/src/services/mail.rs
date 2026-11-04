//! SMTP plumbing shared by alert emails and invitation emails: the global
//! `SMTP_*` settings, the transport and HTML escaping.

use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, Tokio1Executor};

use crate::models::EmailConfig;

/// The sender used when none is configured or the configured one does not
/// parse.
pub const DEFAULT_FROM: &str = "alerts@rustrak.local";

/// How the connection to the SMTP server is encrypted (`SMTP_TLS`), like
/// Sentry's `mail.use-tls` and `mail.use-ssl`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmtpTls {
    /// Upgrade with STARTTLS, and refuse a server that cannot.
    StartTls,
    /// TLS from the first byte (SMTPS).
    Tls,
    /// No encryption: a relay on a trusted network, or a local test server.
    None,
}

impl SmtpTls {
    /// `starttls`, `tls` or `none`, in any case.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "starttls" => Some(Self::StartTls),
            "tls" => Some(Self::Tls),
            "none" => Some(Self::None),
            _ => None,
        }
    }
}

/// The global SMTP configuration, read from `SMTP_HOST`, `SMTP_PORT`,
/// `SMTP_TLS`, `SMTP_USERNAME`, `SMTP_PASSWORD` and `SMTP_FROM`.
#[derive(Debug, Clone)]
pub struct SmtpSettings {
    pub host: Option<String>,
    pub port: u16,
    /// `None` picks by port: TLS on 465, STARTTLS elsewhere.
    pub tls: Option<SmtpTls>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub from_address: String,
}

impl SmtpSettings {
    pub fn from_env() -> Self {
        Self {
            host: std::env::var("SMTP_HOST")
                .ok()
                .filter(|host| !host.trim().is_empty()),
            port: std::env::var("SMTP_PORT")
                .ok()
                .and_then(|port| port.parse().ok())
                .unwrap_or(587),
            tls: std::env::var("SMTP_TLS").ok().and_then(|value| {
                let tls = SmtpTls::parse(&value);
                if tls.is_none() {
                    log::warn!("Ignoring SMTP_TLS={value}: expected starttls, tls or none");
                }
                tls
            }),
            username: std::env::var("SMTP_USERNAME").ok(),
            password: std::env::var("SMTP_PASSWORD").ok(),
            from_address: std::env::var("SMTP_FROM").unwrap_or_else(|_| DEFAULT_FROM.to_string()),
        }
    }

    /// These settings with every field an email integration sets taking
    /// precedence, the way an integration overrides the environment.
    pub fn overridden_by(self, config: &EmailConfig) -> Self {
        Self {
            host: config
                .smtp_host
                .clone()
                .filter(|host| !host.trim().is_empty())
                .or(self.host),
            port: config.smtp_port.unwrap_or(self.port),
            tls: self.tls,
            username: config.smtp_username.clone().or(self.username),
            password: config.smtp_password.clone().or(self.password),
            from_address: config.from_address.clone().unwrap_or(self.from_address),
        }
    }
}

/// Builds a transport for `host:port`, encrypted as `tls` says (by port when
/// `None`), and authenticated when both a username and a password are given.
pub fn build_transport(
    host: &str,
    port: u16,
    tls: Option<SmtpTls>,
    username: Option<&String>,
    password: Option<&String>,
) -> Result<AsyncSmtpTransport<Tokio1Executor>, String> {
    let tls = tls.unwrap_or(if port == 465 {
        SmtpTls::Tls
    } else {
        SmtpTls::StartTls
    });
    let builder = match tls {
        SmtpTls::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(host),
        SmtpTls::StartTls => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host),
        SmtpTls::None => Ok(AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(
            host,
        )),
    }
    .map_err(|e| format!("Invalid SMTP host: {}", e))?
    .port(port);

    Ok(match (username, password) {
        (Some(u), Some(p)) => builder.credentials(Credentials::new(u.clone(), p.clone())),
        _ => builder,
    }
    .build())
}

/// Simple HTML escaping for email content.
pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smtp_tls_parses_the_three_modes_and_nothing_else() {
        assert_eq!(SmtpTls::parse("starttls"), Some(SmtpTls::StartTls));
        assert_eq!(SmtpTls::parse(" TLS "), Some(SmtpTls::Tls));
        assert_eq!(SmtpTls::parse("none"), Some(SmtpTls::None));
        assert_eq!(SmtpTls::parse("ssl"), None);
    }

    #[test]
    fn an_integration_overrides_the_environment_field_by_field() {
        let env = SmtpSettings {
            host: Some("env.smtp".to_string()),
            port: 587,
            tls: Some(SmtpTls::None),
            username: Some("env-user".to_string()),
            password: Some("env-pass".to_string()),
            from_address: "env@example.com".to_string(),
        };
        let merged = env.overridden_by(&EmailConfig {
            smtp_host: Some("ui.smtp".to_string()),
            smtp_port: Some(465),
            smtp_username: None,
            smtp_password: None,
            from_address: Some("ui@example.com".to_string()),
        });
        assert_eq!(merged.host.as_deref(), Some("ui.smtp"));
        assert_eq!(merged.port, 465);
        assert_eq!(merged.tls, Some(SmtpTls::None));
        assert_eq!(merged.username.as_deref(), Some("env-user"));
        assert_eq!(merged.from_address, "ui@example.com");
    }

    #[test]
    fn a_blank_integration_host_keeps_the_environment_one() {
        let env = SmtpSettings {
            host: Some("env.smtp".to_string()),
            port: 587,
            tls: None,
            username: None,
            password: None,
            from_address: DEFAULT_FROM.to_string(),
        };
        let merged = env.overridden_by(&EmailConfig {
            smtp_host: Some("  ".to_string()),
            smtp_port: None,
            smtp_username: None,
            smtp_password: None,
            from_address: None,
        });
        assert_eq!(merged.host.as_deref(), Some("env.smtp"));
    }

    #[test]
    fn test_html_escape() {
        assert_eq!(html_escape("<script>"), "&lt;script&gt;");
        assert_eq!(html_escape("a & b"), "a &amp; b");
        assert_eq!(html_escape("\"quote\""), "&quot;quote&quot;");
    }
}
