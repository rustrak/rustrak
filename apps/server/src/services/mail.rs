//! SMTP plumbing shared by alert emails and invitation emails: an
//! integration's overrides of the global settings, the transport and HTML
//! escaping.

use lettre::transport::smtp::authentication::Credentials;
use lettre::transport::smtp::client::{Tls, TlsParameters};
use lettre::{AsyncSmtpTransport, Tokio1Executor};

use crate::config::{SmtpSettings, SmtpTls};
use crate::models::EmailConfig;
use crate::services::{notification, oauth};

impl SmtpSettings {
    /// These settings with every field an email integration sets taking
    /// precedence, the way an integration overrides the environment.
    ///
    /// An integration naming its own host logs in with its own credentials
    /// only: the environment's are for the environment's server.
    pub fn overridden_by(self, config: &EmailConfig) -> Self {
        let own_host = config
            .smtp_host
            .clone()
            .filter(|host| !host.trim().is_empty());
        let (username, password) = if own_host.is_some() {
            (None, None)
        } else {
            (self.username, self.password)
        };
        Self {
            port: config.smtp_port.unwrap_or(self.port),
            tls: self.tls,
            username: config.smtp_username.clone().or(username),
            password: config.smtp_password.clone().or(password),
            host: own_host.or(self.host),
            from_address: config.from_address.clone().unwrap_or(self.from_address),
        }
    }
}

/// Builds a transport for `host:port`, encrypted as `tls` says (by port when
/// `None`), and authenticated when both a username and a password are given.
///
/// A host an email integration named (`from_integration`) is a destination
/// like a webhook URL (#504): unless private destinations are allowed, every
/// address it resolves to must be public, and the transport connects to the
/// checked address while TLS still verifies the name. The operator's own
/// `SMTP_HOST` is trusted as configured.
pub async fn build_transport(
    host: &str,
    port: u16,
    tls: Option<SmtpTls>,
    username: Option<&String>,
    password: Option<&String>,
    from_integration: bool,
) -> Result<AsyncSmtpTransport<Tokio1Executor>, String> {
    let server = if from_integration && !notification::private_destinations_allowed() {
        let addrs: Vec<_> = tokio::net::lookup_host((host, port))
            .await
            .map_err(|e| format!("Invalid SMTP host: {e}"))?
            .collect();
        match addrs.first() {
            Some(addr) if addrs.iter().all(|a| oauth::is_public(a.ip())) => addr.ip().to_string(),
            _ => return Err(notification::NotPublic.to_string()),
        }
    } else {
        host.to_string()
    };
    let tls_name =
        || TlsParameters::new(host.to_string()).map_err(|e| format!("Invalid SMTP host: {e}"));
    // What `relay` and `starttls_relay` build, with the connection address
    // apart from the TLS name.
    let tls = match tls.unwrap_or(if port == 465 {
        SmtpTls::Tls
    } else {
        SmtpTls::StartTls
    }) {
        SmtpTls::Tls => Tls::Wrapper(tls_name()?),
        SmtpTls::StartTls => Tls::Required(tls_name()?),
        SmtpTls::None => Tls::None,
    };
    let builder = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(server)
        .port(port)
        .tls(tls);

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
    use crate::config::DEFAULT_FROM;

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
        assert_eq!(merged.from_address, "ui@example.com");
        // The environment's login is for the environment's server only.
        assert_eq!(merged.username, None);
        assert_eq!(merged.password, None);
    }

    #[test]
    fn an_integration_without_a_host_keeps_the_environment_login() {
        let env = SmtpSettings {
            host: Some("env.smtp".to_string()),
            port: 587,
            tls: None,
            username: Some("env-user".to_string()),
            password: Some("env-pass".to_string()),
            from_address: DEFAULT_FROM.to_string(),
        };
        let merged = env.overridden_by(&EmailConfig {
            smtp_host: None,
            smtp_port: None,
            smtp_username: None,
            smtp_password: None,
            from_address: Some("ui@example.com".to_string()),
        });
        assert_eq!(merged.username.as_deref(), Some("env-user"));
        assert_eq!(merged.password.as_deref(), Some("env-pass"));
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
