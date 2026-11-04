//! Invitation emails.
//!
//! They go out through the same SMTP settings as alert emails: the first
//! enabled email integration, field by field over the global `SMTP_*`
//! environment, so an instance configured only from the UI can send them
//! too. They link to the dashboard's configured public address, with the
//! same single-use, expiring token the manual-share path hands back.

use chrono::{DateTime, Utc};
use lettre::message::{header::ContentType, MultiPart, SinglePart};
use lettre::{AsyncTransport, Message};
use serde::Serialize;

use crate::config::Config;
use crate::db::DbPool;
use crate::error::AppResult;
use crate::models::{EmailConfig, Invitation};
use crate::services::mail::{self, SmtpSettings};

const SUBJECT: &str = "You have been invited to Rustrak";

/// How long a send may take. lettre's own timeout is per SMTP command, and
/// the admin's request waits on this one.
const SEND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// GET /api/invitations/email-status answer.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct EmailStatusResponse {
    /// Whether invitation emails can be sent: `SMTP_HOST` is set, and so is
    /// `DASHBOARD_URL` or `PUBLIC_URL` for the link.
    pub available: bool,
}

/// Everything an invitation email needs: an SMTP server and a link base an
/// invitee can open.
pub struct InvitationMailer {
    smtp: SmtpSettings,
    host: String,
    dashboard_url: String,
}

impl InvitationMailer {
    /// The mailer for this instance, or `None` when invitation emails cannot
    /// be sent.
    pub async fn resolve(pool: &DbPool, config: &Config) -> AppResult<Option<Self>> {
        let mut smtp = SmtpSettings::from_env();
        if let Some(credentials) = first_enabled_email_integration(pool).await? {
            smtp = smtp.overridden_by(&credentials);
        }
        Ok(Self::new(smtp, config.public_dashboard_url()))
    }

    /// `None` unless `smtp` names a host and there is a public dashboard
    /// address: the bind-address fallback is not one an invitee can open.
    pub fn new(smtp: SmtpSettings, dashboard_url: Option<&str>) -> Option<Self> {
        Some(Self {
            host: smtp.host.clone()?,
            dashboard_url: dashboard_url?.to_string(),
            smtp,
        })
    }

    /// The invitation email, from the configured sender to the invitee.
    pub fn message(&self, invitation: &Invitation, inviter_email: &str) -> Result<Message, String> {
        let url = format!("{}/invite/{}", self.dashboard_url, invitation.token);
        let from = self
            .smtp
            .from_address
            .parse()
            .unwrap_or_else(|_| mail::DEFAULT_FROM.parse().expect("valid fallback address"));
        let to = invitation
            .email
            .parse()
            .map_err(|e| format!("Invalid invitation recipient {}: {e}", invitation.email))?;

        Message::builder()
            .from(from)
            .to(to)
            .subject(SUBJECT)
            .multipart(
                MultiPart::alternative()
                    .singlepart(SinglePart::builder().header(ContentType::TEXT_PLAIN).body(
                        render_text(&url, inviter_email, &invitation.role, invitation.expires_at),
                    ))
                    .singlepart(SinglePart::builder().header(ContentType::TEXT_HTML).body(
                        render_html(&url, inviter_email, &invitation.role, invitation.expires_at),
                    )),
            )
            .map_err(|e| e.to_string())
    }

    /// Hands the invitation email to SMTP.
    pub async fn send(&self, invitation: &Invitation, inviter_email: &str) -> Result<(), String> {
        let message = self.message(invitation, inviter_email)?;
        let transport = mail::build_transport(
            &self.host,
            self.smtp.port,
            self.smtp.tls,
            self.smtp.username.as_ref(),
            self.smtp.password.as_ref(),
        )?;
        tokio::time::timeout(SEND_TIMEOUT, transport.send(message))
            .await
            .map_err(|_| "Timed out sending the invitation email".to_string())?
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

/// The credentials of the lowest-id enabled email integration. One whose
/// credentials do not parse is skipped, as if it were not there.
async fn first_enabled_email_integration(pool: &DbPool) -> AppResult<Option<EmailConfig>> {
    let credentials: Option<serde_json::Value> = sqlx::query_scalar(
        "SELECT credentials FROM alert_integrations
         WHERE provider_type = 'email' AND is_enabled = TRUE
         ORDER BY id LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;
    Ok(credentials.and_then(|value| {
        serde_json::from_value(value)
            .inspect_err(|e| log::warn!("Ignoring an email integration for invitations: {e}"))
            .ok()
    }))
}

/// The role shown to the reader, with its article; anything unexpected
/// reads as the least-privileged one.
fn role_display(role: &str) -> &'static str {
    if role == "admin" {
        "an admin"
    } else {
        "a member"
    }
}

fn render_text(url: &str, inviter_email: &str, role: &str, expires_at: DateTime<Utc>) -> String {
    format!(
        "{inviter_email} invited you to join their Rustrak instance as {role}.\n\n\
Accept the invitation and set a password to create your account:\n\n\
  {url}\n\n\
This invitation expires {expires} and can only be used once.\n\n\
--\n\
Did not expect this invitation? You can safely ignore this email.\n",
        role = role_display(role),
        expires = expires_at.format("%Y-%m-%d %H:%M UTC"),
    )
}

fn render_html(url: &str, inviter_email: &str, role: &str, expires_at: DateTime<Utc>) -> String {
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
</head>
<body style="font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; margin: 0; padding: 20px; background-color: #f3f4f6;">
    <div style="max-width: 600px; margin: 0 auto; background-color: #ffffff; border-radius: 8px; overflow: hidden; box-shadow: 0 1px 3px rgba(0,0,0,0.1);">
        <div style="background-color: #2563eb; padding: 16px 24px;">
            <h1 style="color: #ffffff; margin: 0; font-size: 18px; font-weight: 600;">{SUBJECT}</h1>
        </div>
        <div style="padding: 24px;">
            <p style="margin: 0 0 24px 0; font-size: 14px; color: #374151; line-height: 1.5;">
                {inviter} invited you to join their Rustrak instance as <strong>{role}</strong>.
            </p>
            <div style="margin-bottom: 24px;">
                <a href="{url}" style="display: inline-block; background-color: #2563eb; color: #ffffff; padding: 10px 20px; border-radius: 6px; text-decoration: none; font-size: 14px; font-weight: 500;">
                    Accept invitation
                </a>
            </div>
            <p style="margin: 0 0 8px 0; font-size: 12px; color: #6b7280; word-break: break-all;">
                Or open this link in your browser: {url}
            </p>
            <p style="margin: 0; font-size: 12px; color: #6b7280;">
                This invitation expires {expires} and can only be used once.
            </p>
        </div>
        <div style="padding: 16px 24px; background-color: #f9fafb; border-top: 1px solid #e5e7eb;">
            <p style="margin: 0; font-size: 12px; color: #6b7280;">
                Did not expect this invitation? You can safely ignore this email.
            </p>
        </div>
    </div>
</body>
</html>"#,
        inviter = mail::html_escape(inviter_email),
        role = role_display(role),
        url = mail::html_escape(url),
        expires = expires_at.format("%Y-%m-%d %H:%M UTC"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn smtp(host: Option<&str>, port: u16) -> SmtpSettings {
        SmtpSettings {
            host: host.map(str::to_string),
            port,
            tls: None,
            username: None,
            password: None,
            from_address: "invites@example.com".to_string(),
        }
    }

    fn invitation() -> Invitation {
        Invitation {
            token: "tok123".to_string(),
            email: "invitee@example.com".to_string(),
            role: "member".to_string(),
            status: "pending".to_string(),
            expires_at: Utc::now() + chrono::Duration::hours(48),
            invited_by: Some(1),
            created_at: Utc::now(),
            accepted_at: None,
            email_sent_at: None,
        }
    }

    #[test]
    fn mailer_needs_an_smtp_host_and_a_public_dashboard_url() {
        let url = Some("https://rustrak.example.com");
        assert!(InvitationMailer::new(smtp(Some("smtp.test"), 587), url).is_some());
        assert!(InvitationMailer::new(smtp(None, 587), url).is_none());
        assert!(InvitationMailer::new(smtp(Some("smtp.test"), 587), None).is_none());
    }

    #[test]
    fn message_goes_from_the_sender_to_the_invitee_with_the_link() {
        let mailer = InvitationMailer::new(
            smtp(Some("smtp.test"), 587),
            Some("https://rustrak.example.com"),
        )
        .unwrap();
        let wire = String::from_utf8(
            mailer
                .message(&invitation(), "admin@example.com")
                .unwrap()
                .formatted(),
        )
        .unwrap();
        assert!(wire.contains("To: invitee@example.com"));
        assert!(wire.contains("From: invites@example.com"));
        assert!(wire.contains("https://rustrak.example.com/invite/tok123"));
    }

    #[tokio::test]
    async fn send_fails_when_smtp_refuses() {
        // Port 1 on loopback refuses at once: no DNS, no waiting.
        let mailer = InvitationMailer::new(
            smtp(Some("127.0.0.1"), 1),
            Some("https://rustrak.example.com"),
        )
        .unwrap();
        assert!(mailer
            .send(&invitation(), "admin@example.com")
            .await
            .is_err());
    }

    #[test]
    fn role_display_falls_back_to_member() {
        assert_eq!(role_display("admin"), "an admin");
        assert_eq!(role_display("member"), "a member");
        assert_eq!(role_display("superuser"), "a member");
    }

    #[test]
    fn render_html_escapes_the_inviter_and_links_the_invitation() {
        let html = render_html(
            "http://rustrak.local/invite/abc",
            "evil<b>@example.com",
            "admin",
            Utc::now(),
        );
        assert!(html.contains("href=\"http://rustrak.local/invite/abc\""));
        assert!(!html.contains("<b>"));
        assert!(html.contains("&lt;b&gt;"));
        assert!(html.contains("<strong>an admin</strong>"));
    }

    #[test]
    fn render_text_carries_the_link_the_inviter_and_role() {
        let text = render_text(
            "http://x/invite/y",
            "admin@example.com",
            "member",
            Utc::now(),
        );
        assert!(text.contains("http://x/invite/y"));
        assert!(text.contains("admin@example.com invited you"));
        assert!(text.contains("as a member"));
    }
}
