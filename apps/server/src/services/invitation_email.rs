//! Invitation email delivery.
//!
//! Turns a pending invitation into a mail to the invitee's inbox: the invite
//! link arriving *in their own mailbox* is what proves they control that
//! address, so no separate verification-code flow is needed. The link itself
//! is the same single-use, expiring token the manual-share path already uses.
//!
//! Which mailer sends it: the first enabled email integration (lowest id),
//! per-field over the global SMTP_* environment; the global environment alone
//! when there is no integration. Both absent means "email delivery is not
//! configured", which the caller must surface rather than swallow.

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::{AlertIntegration, EmailConfig, Invitation};
use crate::services::mail::{self, GlobalSmtp};

/// Which mailer an email-status answer is pointing at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmailSource {
    Integration,
    Global,
}

/// The wire form of the email-status `source` field: the only values it
/// takes besides `null`, typed as an enum so the generated client names the
/// set instead of accepting any string.
#[cfg(feature = "openapi")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum EmailSourceKind {
    Integration,
    Global,
}

/// The resolved mailer plus where it came from. Integration means a real
/// enabled email row supplied the credentials object; individual fields may
/// still fall back to the global environment.
#[derive(Debug, Clone)]
pub struct ResolvedMailer {
    pub source: EmailSource,
    pub credentials: EmailConfig,
}

/// GET /api/invitations/email-status answer.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct EmailStatusResponse {
    pub available: bool,
    /// `source: null` is a real answer (no SMTP anywhere), never an omitted
    /// field, so the schema keeps it required even though serde models it
    /// as an Option (see openapi_contract_test).
    #[cfg_attr(feature = "openapi", schema(required, value_type = Option<EmailSourceKind>))]
    pub source: Option<&'static str>,
}

/// Finds the mailer an invitation email would use, or None when no host is
/// reachable at all. Reads the DB once and SMTP_* once; no side effects.
pub async fn resolve_mailer(pool: &DbPool) -> AppResult<Option<ResolvedMailer>> {
    let global = GlobalSmtp::from_env();

    if let Some(integration) = first_enabled_email_integration(pool).await? {
        let credentials: EmailConfig =
            serde_json::from_value(integration.credentials).map_err(|e| {
                AppError::Internal(format!("Invalid email integration credentials: {}", e))
            })?;
        // A blank host in the credentials object is not a configuration:
        // counting it as one would suppress the global fallback below and
        // point the send at an empty host name.
        let credentials = EmailConfig {
            smtp_host: credentials.smtp_host.filter(|host| !host.trim().is_empty()),
            ..credentials
        };
        if credentials.smtp_host.is_some() || global.host.is_some() {
            return Ok(Some(ResolvedMailer {
                source: EmailSource::Integration,
                credentials,
            }));
        }
    }

    if global.host.is_some() {
        return Ok(Some(ResolvedMailer {
            source: EmailSource::Global,
            credentials: EmailConfig {
                smtp_host: None,
                smtp_port: None,
                smtp_username: None,
                smtp_password: None,
                from_address: None,
            },
        }));
    }

    Ok(None)
}

/// The admin-facing view of resolve_mailer, without leaking any credential.
pub async fn email_status(pool: &DbPool) -> AppResult<EmailStatusResponse> {
    match resolve_mailer(pool).await? {
        Some(mailer) => Ok(EmailStatusResponse {
            available: true,
            source: Some(match mailer.source {
                EmailSource::Integration => "integration",
                EmailSource::Global => "global",
            }),
        }),
        None => Ok(EmailStatusResponse {
            available: false,
            source: None,
        }),
    }
}

/// The lowest-id enabled email integration, if any.
async fn first_enabled_email_integration(pool: &DbPool) -> AppResult<Option<AlertIntegration>> {
    let row = sqlx::query_as::<_, AlertIntegration>(
        r#"
SELECT id, name, provider_type, credentials, is_enabled, failure_count,
       last_failure_at, last_failure_message, last_success_at,
       created_at, updated_at
FROM alert_integrations
WHERE provider_type = 'email' AND is_enabled = TRUE
ORDER BY id ASC
LIMIT 1
"#,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// The subject line for an invitation email.
pub fn subject() -> &'static str {
    "You have been invited to Rustrak"
}

/// Role shown to the reader: the invitation's role, defaulting to member for
/// anything unexpected (fail toward the least-privileged word).
fn role_display(role: &str) -> &'static str {
    if role == "admin" {
        "admin"
    } else {
        "member"
    }
}

/// Renders the invitation email's plain-text body.
pub fn render_text(
    invite_url: &str,
    inviter_email: &str,
    role: &str,
    expires_at: DateTime<Utc>,
) -> String {
    let expires = expires_at.format("%Y-%m-%d %H:%M UTC");
    format!(
        "{inviter} invited you to join their Rustrak instance as a {role}.\n\n\
Accept the invitation and set a password to confirm this email address and\n\
create your account:\n\n\
  {invite_url}\n\n\
This invitation expires {expires} and can only be used once.\n\n\
--\n\
Did not expect this invitation? You can safely ignore this email.\n",
        inviter = inviter_email,
        role = role_display(role),
    )
}

/// Renders the invitation email's HTML body.
///
/// Everything user-influenced (the inviter address, the role word, the link)
/// is escaped; the link is shown as text as well as a button, so a cautious
/// reader can see where it goes before clicking. Placeholders are substituted
/// with replace, never format!, so the inline CSS braces cannot drift into a
/// format spec.
pub fn render_html(
    invite_url: &str,
    inviter_email: &str,
    role: &str,
    expires_at: DateTime<Utc>,
) -> String {
    let expires = expires_at.format("%Y-%m-%d %H:%M UTC").to_string();
    r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
</head>
<body style="font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; margin: 0; padding: 20px; background-color: #f3f4f6;">
    <div style="max-width: 600px; margin: 0 auto; background-color: #ffffff; border-radius: 8px; overflow: hidden; box-shadow: 0 1px 3px rgba(0,0,0,0.1);">
        <div style="background-color: #2563eb; padding: 16px 24px;">
            <h1 style="color: #ffffff; margin: 0; font-size: 18px; font-weight: 600;">
                You have been invited to Rustrak
            </h1>
        </div>
        <div style="padding: 24px;">
            <p style="margin: 0 0 16px 0; font-size: 14px; color: #374151; line-height: 1.5;">
                {inviter} invited you to join their Rustrak instance as a <strong>{role}</strong>.
            </p>
            <p style="margin: 0 0 24px 0; font-size: 14px; color: #374151; line-height: 1.5;">
                Accepting the invitation and setting a password confirms this
                email address and creates your account.
            </p>
            <div style="margin-bottom: 24px;">
                <a href="{invite_url}" style="display: inline-block; background-color: #2563eb; color: #ffffff; padding: 10px 20px; border-radius: 6px; text-decoration: none; font-size: 14px; font-weight: 500;">
                    Accept invitation
                </a>
            </div>
            <p style="margin: 0 0 8px 0; font-size: 12px; color: #6b7280; word-break: break-all;">
                Or open this link in your browser: {invite_url}
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
</html>"#
        .replace("{inviter}", &mail::html_escape(inviter_email))
        .replace("{role}", role_display(role))
        .replace("{invite_url}", &mail::html_escape(invite_url))
        .replace("{expires}", &expires)
}

/// Renders and hands the invitation email to `transport`, returning the
/// timestamp it accepted the message. The only production caller is
/// [`send_invitation_mail`]; a `test-seams` wrapper exposes this to tests
/// with a [`mail::CapturedMail`], which keeps the subject, the link and the
/// sender assertable without an SMTP server.
async fn send_with_transport(
    transport: &dyn mail::MailTransport,
    mailer: &ResolvedMailer,
    invitation: &Invitation,
    invite_url: &str,
    inviter_email: &str,
) -> AppResult<DateTime<Utc>> {
    let global = GlobalSmtp::from_env();
    let from = mailer
        .credentials
        .from_address
        .as_ref()
        .unwrap_or(&global.from_address);

    let message = mail::build_message(
        from,
        &invitation.email,
        subject(),
        &render_text(
            invite_url,
            inviter_email,
            &invitation.role,
            invitation.expires_at,
        ),
        &render_html(
            invite_url,
            inviter_email,
            &invitation.role,
            invitation.expires_at,
        ),
    )
    .map_err(|e| AppError::Internal(e.to_string()))?;

    transport
        .send_mail(message)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to send invitation email: {}", e)))?;

    Ok(Utc::now())
}

/// **Test seam** for [`send_with_transport`]: the delivery path with a
/// caller-supplied transport. Mirrors the `test-seams` gating of
/// [`mail::CapturedMail`]; not in the shipped binary.
#[cfg(feature = "test-seams")]
pub async fn send_invitation_mail_with_transport(
    transport: &dyn mail::MailTransport,
    mailer: &ResolvedMailer,
    invitation: &Invitation,
    invite_url: &str,
    inviter_email: &str,
) -> AppResult<DateTime<Utc>> {
    send_with_transport(transport, mailer, invitation, invite_url, inviter_email).await
}

/// Sends the invitation mail and returns the timestamp SMTP accepted it at.
///
/// Fails (Err) when no mailer is configured or the send itself failed; the
/// caller decides how loud that is. The invitation row is untouched here —
/// delivery is a side effect of the resource, not its precondition.
pub async fn send_invitation_mail(
    mailer: &ResolvedMailer,
    invitation: &Invitation,
    invite_url: &str,
    inviter_email: &str,
) -> AppResult<DateTime<Utc>> {
    let global = GlobalSmtp::from_env();

    let host = mailer
        .credentials
        .smtp_host
        .as_ref()
        .or(global.host.as_ref())
        .ok_or_else(|| AppError::Internal("SMTP host not configured".to_string()))?;
    let port = mailer.credentials.smtp_port.unwrap_or(global.port);

    let transport = mail::build_transport(
        host,
        port,
        mailer
            .credentials
            .smtp_username
            .as_ref()
            .or(global.username.as_ref()),
        mailer
            .credentials
            .smtp_password
            .as_ref()
            .or(global.password.as_ref()),
    )
    .map_err(AppError::Internal)?;

    send_with_transport(
        transport.as_ref(),
        mailer,
        invitation,
        invite_url,
        inviter_email,
    )
    .await
}

/// Persists that the invitation email was accepted by SMTP, and retires this
/// invitation's delivery claim in the same write: a completed send leaves
/// nothing behind that could block a later resend.
pub async fn mark_email_sent(pool: &DbPool, token: &str, sent_at: DateTime<Utc>) -> AppResult<()> {
    sqlx::query("UPDATE invitations SET email_sent_at = $2, email_delivery_claimed_at = NULL WHERE token = $1")
        .bind(token)
        .bind(sent_at)
        .execute(pool)
        .await?;
    Ok(())
}

/// How long a delivery claim is honoured before it is treated as abandoned.
///
/// A claim is written seconds before the SMTP call it protects and cleared
/// just after it returns, so any real send is long gone by this horizon; what
/// survives past it is a crashed process, not a delivery. The bound has to sit
/// above the transport timeout, which is why it is generous rather than tight.
const STALE_CLAIM_AFTER: chrono::Duration = chrono::Duration::hours(1);

/// Atomically claims the right to deliver this invitation's email, recorded
/// on its own column so it is never confused with a finished delivery.
///
/// The conditional UPDATE only goes through while no live claim is held, so of
/// two overlapping sends exactly one wins; a claim older than
/// [`STALE_CLAIM_AFTER`] came from a dead process and stops blocking.
/// `Some` hands back the claimed timestamp, which the caller passes to
/// [`release_email_delivery`] if the send fails.
pub async fn claim_email_delivery(pool: &DbPool, token: &str) -> AppResult<Option<DateTime<Utc>>> {
    let claimed_at = Utc::now();
    let stale_before = claimed_at - STALE_CLAIM_AFTER;
    let result = sqlx::query(
        "UPDATE invitations
         SET email_delivery_claimed_at = $1
         WHERE token = $2 AND (email_delivery_claimed_at IS NULL OR email_delivery_claimed_at < $3)",
    )
    .bind(claimed_at)
    .bind(token)
    .bind(stale_before)
    .execute(pool)
    .await?;
    Ok((result.rows_affected() > 0).then_some(claimed_at))
}

/// Clears the claim after a failed send. Guarded by the claimed timestamp so
/// it can never clear a claim someone else took in the meantime. The stamp of
/// a *successful* delivery (`mark_email_sent` nulls the claim in the same
/// statement), which is why a lost claim never hides the delivered state.
pub async fn release_email_delivery(
    pool: &DbPool,
    token: &str,
    claimed_at: DateTime<Utc>,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE invitations SET email_delivery_claimed_at = NULL
         WHERE token = $1 AND email_delivery_claimed_at = $2",
    )
    .bind(token)
    .bind(claimed_at)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn sample_invitation() -> Invitation {
        Invitation {
            token: "t".repeat(40),
            email: "invitee@example.com".to_string(),
            role: "member".to_string(),
            status: "pending".to_string(),
            expires_at: Utc::now() + Duration::hours(48),
            invited_by: Some(1),
            created_at: Utc::now(),
            accepted_at: None,
            email_sent_at: None,
            email_delivery_claimed_at: None,
        }
    }

    #[test]
    fn render_text_carries_the_link_the_inviter_and_expiry() {
        let inv = sample_invitation();
        let text = render_text(
            "http://rustrak.local/invite/abc",
            "admin@example.com",
            &inv.role,
            inv.expires_at,
        );
        assert!(text.contains("http://rustrak.local/invite/abc"));
        assert!(text.contains("admin@example.com invited you"));
        assert!(text.contains("as a member"));
        assert!(text.contains("can only be used once"));
    }

    #[test]
    fn render_html_links_to_the_invitation_and_escapes_the_inviter() {
        let inv = sample_invitation();
        let html = render_html(
            "http://rustrak.local/invite/abc",
            "evil<b>@example.com",
            "admin",
            inv.expires_at,
        );
        assert!(html.contains("href=\"http://rustrak.local/invite/abc\""));
        // The inviter address is model-controlled text in a mail the invitee
        // trusts: markup must not survive into the body.
        assert!(!html.contains("<b>"));
        assert!(html.contains("&lt;b&gt;"));
        assert!(html.contains("<strong>admin</strong>"));
    }

    #[test]
    fn role_display_falls_back_to_member() {
        assert_eq!(role_display("admin"), "admin");
        assert_eq!(role_display("member"), "member");
        // Unknown DB values read as the least-privileged word.
        assert_eq!(role_display("superuser"), "member");
        assert_eq!(role_display(""), "member");
    }

    #[test]
    fn render_html_does_not_leak_unescaped_placeholders() {
        // If a token name ever drifts from the replace list, the reader would
        // see literal {curly} text — catch that in a test, not an inbox.
        let html = render_html("http://x/invite/y", "a@b.c", "member", Utc::now());
        for token in ["{inviter}", "{role}", "{invite_url}", "{expires}"] {
            assert!(!html.contains(token), "unreplaced token {token}");
        }
    }
}
