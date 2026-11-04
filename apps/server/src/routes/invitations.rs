//! Invitation routes (global admin only).
//!
//! - POST   /api/invitations                — create an invitation, optionally
//!   emailing the invite link
//! - GET    /api/invitations                — list invitations
//! - GET    /api/invitations/email-status   — can invitation emails be sent?
//! - POST   /api/invitations/{token}/resend — email the invite link again
//! - DELETE /api/invitations/{token}        — revoke a pending invitation

use actix_web::{web, HttpResponse};
use chrono::Utc;

use crate::auth::ApiActor;
use crate::config::Config;
use crate::db::DbPool;
use crate::error::{AppError, AppResult, FieldErrorCode};
use crate::models::{CreateInvitation, Invitation, InvitationResponse};
use crate::services::invitation_email::{EmailStatusResponse, InvitationMailer};
use crate::services::InvitationService;

#[cfg(feature = "openapi")]
use utoipa::OpenApi;

const EMAIL_NOT_CONFIGURED: &str = "Invitation emails need an enabled email integration or SMTP_HOST, and DASHBOARD_URL or PUBLIC_URL for the link";

/// Ensures the actor is a global admin, else 403.
fn require_admin(actor: &ApiActor) -> AppResult<()> {
    if actor.is_admin() {
        Ok(())
    } else {
        Err(AppError::Forbidden("Admin privileges required".to_string()))
    }
}

/// Emails the invite link and stamps `email_sent_at`.
async fn deliver(
    pool: &DbPool,
    mailer: &InvitationMailer,
    invitation: &mut Invitation,
    inviter_email: &str,
) -> AppResult<()> {
    mailer
        .send(invitation, inviter_email)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to send invitation email: {e}")))?;
    invitation.email_sent_at =
        Some(InvitationService::mark_email_sent(pool, &invitation.token).await?);
    Ok(())
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/invitations",
    operation_id = "invitations.create",
    tag = "Invitations",
    request_body = CreateInvitation,
    responses(
        (status = 201, description = "Invitation created", body = InvitationResponse),
        (status = 400, description = "Invalid role, or send_email while invitation emails are not configured", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
        (status = 409, description = "Email already a user or pending invite", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// POST /api/invitations — create an invitation (admin only)
///
/// With `send_email: true` the invite link is also emailed. A failed send
/// still creates the invitation, with `email_sent_at: null`, so its link can
/// be shared by hand or the email resent.
pub async fn create_invitation(
    pool: web::Data<DbPool>,
    actor: ApiActor,
    config: web::Data<Config>,
    body: web::Json<CreateInvitation>,
) -> AppResult<HttpResponse> {
    require_admin(&actor)?;

    let inviter = actor.require_user()?;
    let input = body.into_inner();
    let mailer = if input.send_email {
        Some(
            InvitationMailer::resolve(pool.get_ref(), &config)
                .await?
                .ok_or_else(|| {
                    AppError::Validation(EMAIL_NOT_CONFIGURED.to_string())
                        .with_field("send_email", FieldErrorCode::Invalid)
                })?,
        )
    } else {
        None
    };

    let mut invitation = InvitationService::create(pool.get_ref(), input, inviter.id).await?;
    if let Some(mailer) = mailer {
        if let Err(e) = deliver(pool.get_ref(), &mailer, &mut invitation, &inviter.email).await {
            log::error!("Invitation email to {} failed: {e}", invitation.email);
        }
    }

    Ok(HttpResponse::Created().json(InvitationResponse::from(invitation)))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/invitations/email-status",
    operation_id = "invitations.emailStatus",
    tag = "Invitations",
    responses(
        (status = 200, description = "Whether invitation emails can be sent", body = EmailStatusResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
        (status = 500, description = "Reading the email integrations failed", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/invitations/email-status — whether invitation emails can be sent (admin only)
pub async fn email_status(
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    require_admin(&actor)?;

    let available = InvitationMailer::resolve(pool.get_ref(), &config)
        .await?
        .is_some();
    Ok(HttpResponse::Ok().json(EmailStatusResponse { available }))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/invitations/{token}/resend",
    operation_id = "invitations.resend",
    tag = "Invitations",
    params(("token" = String, Path, description = "Invitation token")),
    responses(
        (status = 200, description = "Invitation email sent", body = InvitationResponse),
        (status = 400, description = "Invitation expired or already used, or invitation emails are not configured", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
        (status = 404, description = "Invitation not found", body = crate::error::ErrorResponse),
        (status = 500, description = "The email could not be sent", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// POST /api/invitations/{token}/resend — email the invite link again (admin only)
pub async fn resend_invitation(
    pool: web::Data<DbPool>,
    actor: ApiActor,
    config: web::Data<Config>,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    require_admin(&actor)?;

    let inviter = actor.require_user()?;
    let mut invitation = InvitationService::get(pool.get_ref(), &path.into_inner())
        .await?
        .ok_or_else(|| AppError::NotFound("Invitation not found".to_string()))?;
    if !invitation.is_acceptable(Utc::now()) {
        return Err(AppError::Validation(
            "Invitation is expired or already used".to_string(),
        ));
    }
    let mailer = InvitationMailer::resolve(pool.get_ref(), &config)
        .await?
        .ok_or_else(|| AppError::Validation(EMAIL_NOT_CONFIGURED.to_string()))?;

    deliver(pool.get_ref(), &mailer, &mut invitation, &inviter.email).await?;

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
        EmailStatusResponse,
    ))
)]
pub struct InvitationsApi;

/// Configure invitation routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/invitations")
            .route("", web::post().to(create_invitation))
            .route("", web::get().to(list_invitations))
            .route("/email-status", web::get().to(email_status))
            .route("/{token}/resend", web::post().to(resend_invitation))
            .route("/{token}", web::delete().to(revoke_invitation)),
    );
}
