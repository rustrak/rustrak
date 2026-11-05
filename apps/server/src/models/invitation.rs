use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Lifecycle status of an invitation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InvitationStatus {
    Pending,
    Accepted,
    Revoked,
    /// Any value not recognized above. Treated as non-acceptable (fail closed)
    /// so a corrupted status can never make an invitation redeemable.
    Unknown,
}

impl InvitationStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            InvitationStatus::Pending => "pending",
            InvitationStatus::Accepted => "accepted",
            InvitationStatus::Revoked => "revoked",
            InvitationStatus::Unknown => "unknown",
        }
    }

    pub fn from_db(s: &str) -> Self {
        match s {
            "pending" => InvitationStatus::Pending,
            "accepted" => InvitationStatus::Accepted,
            "revoked" => InvitationStatus::Revoked,
            _ => InvitationStatus::Unknown,
        }
    }
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Invitation {
    pub token: String,
    pub email: String,
    pub role: String,
    pub status: String,
    pub expires_at: DateTime<Utc>,
    pub invited_by: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
    /// When the invitation email was last handed to SMTP; NULL when none was.
    pub email_sent_at: Option<DateTime<Utc>>,
    /// A role the new user holds once they accept.
    pub role_id: Option<i32>,
    /// Where `role_id` applies; NULL is the whole instance.
    pub project_id: Option<i32>,
}

impl Invitation {
    pub fn status(&self) -> InvitationStatus {
        InvitationStatus::from_db(&self.status)
    }

    /// An invitation is usable only while pending and unexpired.
    pub fn is_acceptable(&self, now: DateTime<Utc>) -> bool {
        self.status() == InvitationStatus::Pending && self.expires_at > now
    }
}

#[derive(Debug, Default, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateInvitation {
    pub email: String,
    /// Global role the invited user will receive (`admin` | `member`).
    #[cfg_attr(feature = "openapi", schema(value_type = crate::models::UserRole))]
    pub role: String,
    /// Also email the invite link to the recipient (Sentry's `sendInvite`).
    /// Needs an enabled email integration or `SMTP_HOST`, and `DASHBOARD_URL`
    /// or `PUBLIC_URL`. The response
    /// carries `token` for manual sharing either way.
    #[serde(default)]
    pub send_email: bool,
    /// A role the new user holds once they accept, within the inviter's own
    /// permissions.
    #[serde(default)]
    pub role_id: Option<i32>,
    /// Where `role_id` applies; omitted or `null` is the whole instance.
    #[serde(default)]
    pub project_id: Option<i32>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AcceptInvitation {
    pub token: String,
    pub password: String,
}

/// Public-facing invitation (no internal columns beyond what the UI needs).
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct InvitationResponse {
    pub token: String,
    pub email: String,
    #[cfg_attr(feature = "openapi", schema(value_type = crate::models::UserRole))]
    pub role: String,
    pub status: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    /// When the invitation email was last sent; null when none was.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub email_sent_at: Option<DateTime<Utc>>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub role_id: Option<i32>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub project_id: Option<i32>,
}

impl From<Invitation> for InvitationResponse {
    fn from(i: Invitation) -> Self {
        Self {
            token: i.token,
            email: i.email,
            role: i.role,
            status: i.status,
            expires_at: i.expires_at,
            created_at: i.created_at,
            email_sent_at: i.email_sent_at,
            role_id: i.role_id,
            project_id: i.project_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn invite(status: &str, expires_at: DateTime<Utc>) -> Invitation {
        Invitation {
            token: "t".repeat(40),
            email: "a@b.com".into(),
            role: "member".into(),
            status: status.into(),
            expires_at,
            invited_by: Some(1),
            created_at: Utc::now(),
            accepted_at: None,
            email_sent_at: None,
            role_id: None,
            project_id: None,
        }
    }

    #[test]
    fn pending_unexpired_is_acceptable() {
        let now = Utc::now();
        assert!(invite("pending", now + Duration::hours(1)).is_acceptable(now));
    }

    #[test]
    fn expired_is_not_acceptable() {
        let now = Utc::now();
        assert!(!invite("pending", now - Duration::hours(1)).is_acceptable(now));
    }

    #[test]
    fn non_pending_is_not_acceptable() {
        let now = Utc::now();
        assert!(!invite("accepted", now + Duration::hours(1)).is_acceptable(now));
        assert!(!invite("revoked", now + Duration::hours(1)).is_acceptable(now));
    }
}
