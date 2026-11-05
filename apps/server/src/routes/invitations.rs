//! Invitation routes (global admin only).
//!
//! - POST   /api/invitations                — create an invitation, optionally
//!   emailing the invite link
//! - GET    /api/invitations                — list invitations
//! - GET    /api/invitations/email-status   — can invitation emails be sent?
//! - POST   /api/invitations/{id}/resend    — email the invite link again
//! - DELETE /api/invitations/{id}           — revoke a pending invitation

use actix_web::{web, HttpResponse};
use chrono::Utc;

use crate::auth::rbac::{Action, Resource};
use crate::auth::ApiActor;
use crate::config::Config;
use crate::db::DbPool;
use crate::error::{AppError, AppResult, FieldErrorCode};
use crate::models::{CreateInvitation, Invitation, InvitationResponse, Role};
use crate::services::invitation_email::{EmailStatusResponse, InvitationMailer};
use crate::services::{InvitationService, RoleService};

#[cfg(feature = "openapi")]
use utoipa::OpenApi;

const EMAIL_NOT_CONFIGURED: &str = "Invitation emails need an enabled email integration or SMTP_HOST, and DASHBOARD_URL or PUBLIC_URL for the link";

/// Emails the invite link and stamps `email_sent_at`. Once SMTP has accepted
/// the email, a failed stamp is only logged: reporting the send as failed
/// would prompt the admin to send a second copy.
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
    match InvitationService::mark_email_sent(pool, &invitation.token).await {
        Ok(sent_at) => invitation.email_sent_at = Some(sent_at),
        Err(e) => log::error!(
            "Invitation email to {} was sent but its delivery time was not saved: {e}",
            invitation.email
        ),
    }
    Ok(())
}

/// A token claims the invitation's account, so it only reaches who could have
/// created this same invitation: reading the list must not outrank inviting.
fn may_hold_token(actor: &ApiActor, roles: &[Role], invitation: &Invitation) -> bool {
    actor
        .access
        .can(Resource::Invitations, Action::Create, None)
        && (invitation.role != "admin" || actor.is_owner())
        && invitation.role_id.is_none_or(|id| {
            roles
                .iter()
                .find(|r| r.id == id)
                .is_some_and(|r| actor.access.covers(&r.grants(), invitation.project_id))
        })
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
    actor.require(Resource::Invitations, Action::Create, None)?;

    let inviter = actor.require_user()?;
    let input = body.into_inner();
    // An invitation hands out access: never beyond the inviter's own.
    if input.role == "admin" && !actor.is_owner() {
        return Err(AppError::Forbidden(
            "Only an owner can invite an owner".to_string(),
        ));
    }
    if let Some(role_id) = input.role_id {
        let role = RoleService::get(pool.get_ref(), role_id)
            .await
            .map_err(|_| {
                AppError::Validation(format!("Unknown role {role_id}"))
                    .with_field("role_id", FieldErrorCode::Invalid)
            })?;
        if !actor.access.covers(&role.grants(), input.project_id) {
            return Err(AppError::Forbidden(
                "You cannot grant permissions you do not hold".to_string(),
            ));
        }
    }
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
    actor.require(Resource::Invitations, Action::Read, None)?;

    let available = InvitationMailer::resolve(pool.get_ref(), &config)
        .await?
        .is_some();
    Ok(HttpResponse::Ok().json(EmailStatusResponse { available }))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/invitations/{id}/resend",
    operation_id = "invitations.resend",
    tag = "Invitations",
    params(("id" = String, Path, description = "Invitation id")),
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
/// POST /api/invitations/{id}/resend — email the invite link again (admin only)
pub async fn resend_invitation(
    pool: web::Data<DbPool>,
    actor: ApiActor,
    config: web::Data<Config>,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    actor.require(Resource::Invitations, Action::Create, None)?;

    let inviter = actor.require_user()?;
    let mut invitation = InvitationService::get_by_id(pool.get_ref(), &path.into_inner())
        .await?
        .ok_or_else(|| AppError::NotFound("Invitation not found".to_string()))?;
    let roles = RoleService::list_all(pool.get_ref()).await?;
    if !may_hold_token(&actor, &roles, &invitation) {
        return Err(AppError::Forbidden(
            "You cannot resend an invitation you could not create".to_string(),
        ));
    }
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
    actor.require(Resource::Invitations, Action::Read, None)?;

    let invitations = InvitationService::list(pool.get_ref()).await?;
    let roles = RoleService::list_all(pool.get_ref()).await?;
    let responses: Vec<InvitationResponse> = invitations
        .into_iter()
        .map(|invitation| {
            let hold = may_hold_token(&actor, &roles, &invitation);
            let mut response = InvitationResponse::from(invitation);
            if !hold {
                response.token = None;
            }
            response
        })
        .collect();

    Ok(HttpResponse::Ok().json(responses))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/api/invitations/{id}",
    operation_id = "invitations.revoke",
    tag = "Invitations",
    params(("id" = String, Path, description = "Invitation id")),
    responses(
        (status = 204, description = "Invitation revoked"),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
        (status = 404, description = "Pending invitation not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// DELETE /api/invitations/{id} — revoke a pending invitation (admin only)
pub async fn revoke_invitation(
    pool: web::Data<DbPool>,
    actor: ApiActor,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    actor.require(Resource::Invitations, Action::Delete, None)?;

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
            .route("/{id}/resend", web::post().to(resend_invitation))
            .route("/{id}", web::delete().to(revoke_invitation)),
    );
}
