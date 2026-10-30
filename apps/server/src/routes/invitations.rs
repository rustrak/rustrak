//! Invitation routes (global admin only).
//!
//! - POST   /api/invitations                — create an invitation, optionally
//!   emailing the invite link to the recipient
//! - GET    /api/invitations                — list invitations
//! - GET    /api/invitations/email-status   — is email delivery configured?
//! - POST   /api/invitations/{token}/resend — re-send the invitation email
//! - DELETE /api/invitations/{token}        — revoke a pending invitation
//!
//! The emailed link is the same single-use, expiring token the manual-share
//! path hands back — the mailbox it lands in is what proves the recipient
//! controls that address, so no separate verification-code flow exists.

use actix_web::{web, HttpResponse};
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::auth::ApiActor;
use crate::config::Config;
use crate::db::DbPool;
use crate::error::{AppError, AppResult, FieldErrorCode};
use crate::models::{CreateInvitation, Invitation, InvitationResponse};
use crate::services::invitation_email::{self, ResolvedMailer};
use crate::services::{InvitationService, UsersService};

#[cfg(feature = "openapi")]
use utoipa::OpenApi;

/// Ensures the actor is a global admin, else 403.
fn require_admin(actor: &ApiActor) -> AppResult<()> {
    if actor.is_admin() {
        Ok(())
    } else {
        Err(AppError::Forbidden("Admin privileges required".to_string()))
    }
}

/// Request body for re-sending an invitation email.
#[derive(Debug, Default, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ResendInvitation {
    /// Re-send even when an email was already delivered (e.g. it landed in
    /// spam and the recipient asks for a fresh copy).
    #[serde(default)]
    pub force: bool,
}

/// Sends the invite link for `invitation` through `mailer` and stamps the
/// row with the delivery time.
///
/// The inviter is named by their email address; a deleted or unknown inviter
/// degrades to a neutral phrase instead of blocking the send.
async fn deliver_invitation(
    pool: &DbPool,
    config: &Config,
    mailer: &ResolvedMailer,
    invitation: &Invitation,
) -> AppResult<DateTime<Utc>> {
    let inviter_email = match invitation.invited_by {
        Some(user_id) => match UsersService::get_by_id(pool, user_id).await? {
            Some(user) => user.email,
            None => "a Rustrak administrator".to_string(),
        },
        None => "a Rustrak administrator".to_string(),
    };

    let invite_url = format!("{}/invite/{}", config.dashboard_url(), invitation.token);

    let sent_at =
        invitation_email::send_invitation_mail(mailer, invitation, &invite_url, &inviter_email)
            .await?;

    // Once SMTP has the message, a stamp failure is a bookkeeping problem,
    // not a delivery one: reporting it as undelivered invites a resend that
    // sends the same link twice. The worst case here is the pending list
    // showing no timestamp until the next delivery, so log and keep going.
    if let Err(e) = invitation_email::mark_email_sent(pool, &invitation.token, sent_at).await {
        log::error!(
            "invitation email to {} was accepted but its delivery stamp failed: {e}",
            invitation.email
        );
    }
    Ok(sent_at)
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/invitations",
    operation_id = "invitations.create",
    tag = "Invitations",
    request_body = CreateInvitation,
    responses(
        (status = 201, description = "Invitation created", body = InvitationResponse),
        (status = 400, description = "Invalid role, or send_email requested while no SMTP source is configured or the invite link would be unreachable", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
        (status = 409, description = "Email already a user or pending invite", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// POST /api/invitations — create an invitation (admin only)
///
/// With `send_email: true` the invite link is emailed to the recipient; the
/// mailer used is the first enabled email integration, falling back field by
/// field to the global SMTP_* environment. A send failure still creates the
/// invitation (201, `email_sent_at: null`) so the link remains copyable from
/// the pending list — email delivery is a side effect, not a precondition.
pub async fn create_invitation(
    pool: web::Data<DbPool>,
    actor: ApiActor,
    config: web::Data<Config>,
    body: web::Json<CreateInvitation>,
) -> AppResult<HttpResponse> {
    require_admin(&actor)?;

    let invited_by = actor.require_user()?.id;
    let input = body.into_inner();

    // Resolve before writing: a request asking for email delivery against an
    // unconfigured instance must fail loudly, not create a row that pretends
    // a mail went out.
    let mut mailer: Option<ResolvedMailer> = None;
    if input.send_email {
        // The link we are about to mail must be openable by its recipient.
        // dashboard_url() falls back to the bind address, whose default
        // 0.0.0.0 is a wildcard nobody can visit: reject before writing the
        // row instead of stamping a delivery the invitee cannot redeem.
        if !config.emailed_links_are_reachable() {
            return Err(AppError::Validation(
                "The invite link would use the unreachable bind address; set DASHBOARD_URL or PUBLIC_URL to email invitations".to_string(),
            )
            .with_field("send_email", FieldErrorCode::Invalid));
        }
        mailer = Some(
            invitation_email::resolve_mailer(pool.get_ref()).await?.ok_or_else(
                || {
                    AppError::Validation(
                        "Email delivery is not configured: add an enabled email integration or set the global SMTP_* environment"
                            .to_string(),
                    )
                    .with_field("send_email", FieldErrorCode::Invalid)
                },
            )?,
        );
    }

    let mut invitation = InvitationService::create(pool.get_ref(), input, invited_by).await?;

    let mut email_sent_at: Option<DateTime<Utc>> = None;
    if let Some(mailer) = &mailer {
        // Same claim as resend, for the same reason: the row is already
        // committed and any admin can list its token, so a resend started
        // while this send is in flight must not slip past the null stamp and
        // email the same invitation twice. The claim is also the create
        // path's own double-click guard against two near-simultaneous
        // "invite with email" posts from one admin.
        let claim =
            invitation_email::claim_email_delivery(pool.get_ref(), &invitation.token).await?;
        if let Some(claimed_at) = claim {
            match deliver_invitation(pool.get_ref(), &config, mailer, &invitation).await {
                Ok(sent_at) => email_sent_at = Some(sent_at),
                Err(e) => {
                    // Best effort: a failed send leaves the invitation usable
                    // by hand, and the release unblocks a later resend.
                    log::error!(
                        "invitation email delivery failed for {} (invitation still created): {e}",
                        invitation.email
                    );
                    let _ = invitation_email::release_email_delivery(
                        pool.get_ref(),
                        &invitation.token,
                        claimed_at,
                    )
                    .await;
                }
            }
        } else {
            // Someone claimed it between the INSERT and here. Harmless and
            // vanishingly rare for a row this request just created: leave the
            // invitation undelivered, its link still copyable.
            log::warn!(
                "invitation email for {} was skipped: a delivery claim was already held",
                invitation.email
            );
        }
    }
    invitation.email_sent_at = email_sent_at;

    Ok(HttpResponse::Created().json(InvitationResponse::from(invitation)))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/invitations/email-status",
    operation_id = "invitations.emailStatus",
    tag = "Invitations",
    responses(
        (status = 200, description = "Whether invitation emails can be sent, and from which source", body = invitation_email::EmailStatusResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/invitations/email-status — is email delivery configured (admin)
///
/// Drives the invite form's "send verification email" switch: it is only
/// offered when this reports available. Reports the source without any
/// credential detail.
pub async fn email_status(pool: web::Data<DbPool>, actor: ApiActor) -> AppResult<HttpResponse> {
    require_admin(&actor)?;

    let status = invitation_email::email_status(pool.get_ref()).await?;
    Ok(HttpResponse::Ok().json(status))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/invitations/{token}/resend",
    operation_id = "invitations.resend",
    tag = "Invitations",
    request_body = Option<ResendInvitation>,
    params(("token" = String, Path, description = "Invitation token")),
    responses(
        (status = 200, description = "Email sent (or re-sent)", body = InvitationResponse),
        (status = 400, description = "Invitation expired or used, already emailed without force, a delivery already in progress, or no SMTP source configured", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
        (status = 404, description = "Invitation not found", body = crate::error::ErrorResponse),
        (status = 500, description = "The SMTP send failed", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// POST /api/invitations/{token}/resend — re-send the invitation email (admin)
///
/// Unlike the create path this is an explicit user action on an existing
/// invitation, so a failed send surfaces as a 500 instead of degrading to
/// manual sharing. `force: true` is required when an email was already
/// delivered, so accidental double-sends are a deliberate choice.
pub async fn resend_invitation(
    pool: web::Data<DbPool>,
    actor: ApiActor,
    config: web::Data<Config>,
    path: web::Path<String>,
    body: Option<web::Json<ResendInvitation>>,
) -> AppResult<HttpResponse> {
    require_admin(&actor)?;

    let token = path.into_inner();
    let force = body.map(|b| b.force).unwrap_or(false);

    let invitation = InvitationService::get(pool.get_ref(), &token)
        .await?
        .ok_or_else(|| AppError::NotFound("Invitation not found".to_string()))?;

    if !invitation.is_acceptable(Utc::now()) {
        return Err(AppError::Validation(
            "Invitation is expired or already used".to_string(),
        ));
    }

    if invitation.email_sent_at.is_some() && !force {
        return Err(AppError::Validation(
            "An invitation email was already sent to this address; pass force: true to send it again"
                .to_string(),
        )
        .with_field("force", FieldErrorCode::Invalid));
    }

    // Same reachability rule as create: never mail a link nobody can open.
    if !config.emailed_links_are_reachable() {
        return Err(AppError::Validation(
            "The invite link would use the unreachable bind address; set DASHBOARD_URL or PUBLIC_URL to email invitations".to_string(),
        )
        .with_field("send_email", FieldErrorCode::Invalid));
    }

    let mailer = invitation_email::resolve_mailer(pool.get_ref()).await?.ok_or_else(
        || {
            AppError::Validation(
                "Email delivery is not configured: add an enabled email integration or set the global SMTP_* environment"
                    .to_string(),
            )
            .with_field("send_email", FieldErrorCode::Invalid)
        },
    )?;

    // Claim the right to deliver before sending: the claim lives on its own
    // column and only succeeds while no live claim is held, so two
    // concurrent resends cannot both send and a lost claim means a delivery
    // is already in flight. `force` bypasses the claim by design: it is the
    // admin asking for a deliberate extra copy.
    let claim = if force {
        None
    } else {
        match invitation_email::claim_email_delivery(pool.get_ref(), &token).await? {
            Some(claimed_at) => Some(claimed_at),
            None => {
                return Err(AppError::Validation(
                    "An invitation email delivery is already in progress".to_string(),
                ));
            }
        }
    };

    let sent_at = match deliver_invitation(pool.get_ref(), &config, &mailer, &invitation).await {
        Ok(sent_at) => sent_at,
        // Hand the row back before surfacing the failure, so a retry or the
        // manual-share fallback still sees an undelivered invitation. The
        // release only clears the timestamp while it is still ours, which
        // keeps it from clobbering a concurrent force resend's real delivery.
        Err(error) => {
            if let Some(claimed_at) = claim {
                // Best effort: the send error is the one that matters.
                let _ =
                    invitation_email::release_email_delivery(pool.get_ref(), &token, claimed_at)
                        .await;
            }
            return Err(error);
        }
    };
    let mut invitation = invitation;
    invitation.email_sent_at = Some(sent_at);

    Ok(HttpResponse::Ok().json(InvitationResponse::from(invitation)))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/invitations",
    operation_id = "invitations.list",
    tag = "Invitations",
    responses(
        (status = 200, description = "List of invitations", body = Vec<InvitationResponse>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/invitations — list invitations (admin only)
pub async fn list_invitations(pool: web::Data<DbPool>, actor: ApiActor) -> AppResult<HttpResponse> {
    require_admin(&actor)?;

    let invitations = InvitationService::list(pool.get_ref()).await?;
    let responses: Vec<InvitationResponse> = invitations
        .into_iter()
        .map(InvitationResponse::from)
        .collect();

    Ok(HttpResponse::Ok().json(responses))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/api/invitations/{token}",
    operation_id = "invitations.revoke",
    tag = "Invitations",
    params(("token" = String, Path, description = "Invitation token")),
    responses(
        (status = 204, description = "Invitation revoked"),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
        (status = 404, description = "Pending invitation not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// DELETE /api/invitations/{token} — revoke a pending invitation (admin only)
pub async fn revoke_invitation(
    pool: web::Data<DbPool>,
    actor: ApiActor,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    require_admin(&actor)?;

    InvitationService::revoke(pool.get_ref(), &path.into_inner()).await?;

    Ok(HttpResponse::NoContent().finish())
}

#[cfg(feature = "openapi")]
#[derive(OpenApi)]
#[openapi(
    paths(
        create_invitation,
        list_invitations,
        email_status,
        resend_invitation,
        revoke_invitation,
    ),
    components(schemas(
        crate::models::CreateInvitation,
        crate::models::InvitationResponse,
        ResendInvitation,
        invitation_email::EmailStatusResponse,
        invitation_email::EmailSourceKind,
    ))
)]
pub struct InvitationsApi;

/// Configure invitation routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/invitations")
            .route("", web::post().to(create_invitation))
            .route("", web::get().to(list_invitations))
            // The literal path must be registered before the {token} routes
            // so actix never binds "email-status" as a token.
            .route("/email-status", web::get().to(email_status))
            .route("/{token}/resend", web::post().to(resend_invitation))
            .route("/{token}", web::delete().to(revoke_invitation)),
    );
}
