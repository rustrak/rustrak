use actix_session::Session;
use actix_web::{web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};

use chrono::{DateTime, Utc};

use crate::auth::{self, AuthenticatedUser};
use crate::db::DbPool;
use crate::error::{AppError, AppResult, FieldErrorCode};
use crate::models::{AcceptInvitation, CreateUserRequest, LoginRequest, User};
use crate::services::{InvitationService, UsersService};

#[cfg(feature = "openapi")]
use utoipa::OpenApi;

#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
struct AuthResponse {
    user: UserResponse,
}

#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
struct UserResponse {
    id: i32,
    email: String,
    role: String,
    /// Convenience flag derived from `role` (kept for backward compatibility).
    is_admin: bool,
    /// Chosen dashboard language, or `null` when the user never chose one.
    language: Option<String>,
    /// Chosen IANA timezone, or `null` when the user never chose one.
    timezone: Option<String>,
}

impl From<User> for UserResponse {
    fn from(user: User) -> Self {
        let is_admin = user.is_admin();
        Self {
            id: user.id,
            email: user.email,
            role: user.role,
            is_admin,
            language: user.language,
            timezone: user.timezone,
        }
    }
}

/// Email validation - checks basic format requirements
pub(crate) fn is_valid_email(email: &str) -> bool {
    // Must have exactly one @
    let parts: Vec<&str> = email.split('@').collect();
    if parts.len() != 2 {
        return false;
    }
    let (local, domain) = (parts[0], parts[1]);

    // Local part: non-empty, reasonable chars
    if local.is_empty() || local.len() > 64 {
        return false;
    }

    // Domain: non-empty, has at least one dot, not starting/ending with dot
    if domain.is_empty() || domain.len() > 255 {
        return false;
    }
    if !domain.contains('.') {
        return false;
    }
    if domain.starts_with('.') || domain.ends_with('.') {
        return false;
    }

    // Domain parts must not be empty (catches "user@.com" and "user@domain.")
    let domain_parts: Vec<&str> = domain.split('.').collect();
    if domain_parts.iter().any(|p| p.is_empty()) {
        return false;
    }

    // TLD must be at least 2 chars
    if let Some(tld) = domain_parts.last() {
        if tld.len() < 2 {
            return false;
        }
    }

    true
}

/// Public-facing invitation info for the accept page.
#[derive(serde::Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
struct InvitationInfoResponse {
    email: String,
    role: String,
    status: String,
    expires_at: DateTime<Utc>,
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/auth/register",
    tag = "Auth",
    request_body = CreateUserRequest,
    responses(
        (status = 403, description = "Registration is invite-only", body = crate::error::ErrorResponse),
    ),
    security(()),
))]
/// POST /auth/register
/// Registration is invite-only — always rejected. Use `/auth/accept-invitation`.
pub async fn register(
    _pool: web::Data<crate::db::DbPool>,
    _session: Session,
    _req: web::Json<CreateUserRequest>,
) -> AppResult<impl Responder> {
    Err::<HttpResponse, _>(AppError::Forbidden(
        "Registration is invite-only".to_string(),
    ))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/auth/accept-invitation",
    tag = "Auth",
    request_body = AcceptInvitation,
    responses(
        (status = 201, description = "Invitation accepted, user created and logged in", body = AuthResponse),
        (status = 400, description = "Invalid or expired invitation", body = crate::error::ErrorResponse),
    ),
    security(()),
))]
/// POST /auth/accept-invitation
/// Accept a pending invitation: creates the user (with the invite's email + role) and logs in.
pub async fn accept_invitation(
    pool: web::Data<crate::db::DbPool>,
    session: Session,
    req: web::Json<AcceptInvitation>,
) -> AppResult<impl Responder> {
    let user = InvitationService::accept(pool.get_ref(), &req.token, &req.password).await?;

    auth::set_user_session(&session, user.id)?;

    Ok(HttpResponse::Created().json(AuthResponse { user: user.into() }))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/auth/invitation/{token}",
    tag = "Auth",
    params(("token" = String, Path, description = "Invitation token")),
    responses(
        (status = 200, description = "Invitation info", body = InvitationInfoResponse),
        (status = 400, description = "Invitation expired or used", body = crate::error::ErrorResponse),
        (status = 404, description = "Invitation not found", body = crate::error::ErrorResponse),
    ),
    security(()),
))]
/// GET /auth/invitation/{token}
/// Returns the invitation's details if it is still acceptable (for the accept page).
pub async fn get_invitation(
    pool: web::Data<crate::db::DbPool>,
    path: web::Path<String>,
) -> AppResult<impl Responder> {
    let invitation = InvitationService::get(pool.get_ref(), &path.into_inner())
        .await?
        .ok_or_else(|| AppError::NotFound("Invitation not found".to_string()))?;

    if !invitation.is_acceptable(Utc::now()) {
        return Err(AppError::Validation(
            "Invitation is expired or already used".to_string(),
        ));
    }

    Ok(HttpResponse::Ok().json(InvitationInfoResponse {
        email: invitation.email,
        role: invitation.role,
        status: invitation.status,
        expires_at: invitation.expires_at,
    }))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/auth/login",
    tag = "Auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Logged in", body = AuthResponse),
        (status = 401, description = "Invalid credentials", body = crate::error::ErrorResponse),
    ),
    security(()),
))]
/// POST /auth/login
/// Authenticate user and create session
pub async fn login(
    pool: web::Data<crate::db::DbPool>,
    session: Session,
    req: web::Json<LoginRequest>,
) -> AppResult<impl Responder> {
    // Not normalized: the exact casing picks between legacy case-variant accounts
    let user = UsersService::get_by_email(pool.get_ref(), req.email.trim())
        .await?
        .ok_or_else(|| AppError::Unauthorized("Invalid credentials".to_string()))?;

    // Check if user is active
    if !user.is_active {
        return Err(AppError::Unauthorized("Account is disabled".to_string()));
    }

    // Verify password
    if !user.verify_password(&req.password)? {
        return Err(AppError::Unauthorized("Invalid credentials".to_string()));
    }

    // Update last login
    UsersService::update_last_login(pool.get_ref(), user.id).await?;

    // Set session
    auth::set_user_session(&session, user.id)?;

    Ok(HttpResponse::Ok().json(AuthResponse { user: user.into() }))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/auth/logout",
    tag = "Auth",
    responses(
        (status = 204, description = "Logged out"),
    ),
    security(("session_cookie" = [])),
))]
/// POST /auth/logout
/// Clear session
pub async fn logout(session: Session) -> impl Responder {
    auth::clear_session(&session);
    HttpResponse::NoContent().finish()
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/auth/me",
    tag = "Auth",
    responses(
        (status = 200, description = "Current user", body = UserResponse),
        (status = 401, description = "Not authenticated", body = crate::error::ErrorResponse),
    ),
    security(("session_cookie" = [])),
))]
/// GET /auth/me
/// Get current authenticated user
pub async fn get_current_user(user: AuthenticatedUser) -> impl Responder {
    HttpResponse::Ok().json(UserResponse::from(user.0))
}

/// What a reader may change about how the dashboard is presented to them.
///
/// Every field is doubly optional, and both levels are used: absent means
/// "leave it", `null` means "clear it". `#[serde(default)]` with
/// `deserialize_with` is what keeps those distinguishable, because plain
/// `Option<Option<T>>` collapses a missing key and an explicit null.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdatePreferencesRequest {
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    pub language: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    pub timezone: Option<Option<String>>,
}

fn deserialize_optional_field<'de, D>(deserializer: D) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize as _;
    Ok(Some(Option::<String>::deserialize(deserializer)?))
}

/// Whether `value` is shaped like a BCP-47 language tag.
///
/// Shape, not membership: `pt-BR` passes even though this server has no idea
/// whether any dashboard renders Portuguese. Rustrak's API is usable without
/// the dashboard, so hard-coding that dashboard's locale list here would mean
/// redeploying the server to add a language to a frontend.
fn is_language_tag(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 35
        && value
            .split('-')
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_alphanumeric()))
}

/// Whether `value` is shaped like an IANA timezone name (`Area/Location`).
///
/// Same reasoning, plus one of its own: the tz database changes without this
/// server being rebuilt, so a fixed list would go stale on its own.
fn is_time_zone_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.split('/').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '+')
        })
}

#[cfg_attr(feature = "openapi", utoipa::path(
    patch,
    path = "/auth/me",
    tag = "Auth",
    request_body = UpdatePreferencesRequest,
    responses(
        (status = 200, description = "Updated user", body = UserResponse),
        (status = 400, description = "Malformed language tag or timezone", body = crate::error::ErrorResponse),
        (status = 401, description = "Not authenticated", body = crate::error::ErrorResponse),
    ),
    security(("session_cookie" = [])),
))]
/// PATCH /auth/me
/// Update the current user's presentation preferences
pub async fn update_current_user(
    user: AuthenticatedUser,
    pool: web::Data<DbPool>,
    body: web::Json<UpdatePreferencesRequest>,
) -> AppResult<HttpResponse> {
    let body = body.into_inner();

    // Only a present, non-null value is checked. Absent leaves the column
    // alone and `null` clears it, and neither is a value to validate.
    if let Some(Some(language)) = body.language.as_ref() {
        if !is_language_tag(language) {
            return Err(AppError::Validation("Invalid language tag".to_string())
                .with_field("language", FieldErrorCode::Invalid));
        }
    }
    if let Some(Some(timezone)) = body.timezone.as_ref() {
        if !is_time_zone_name(timezone) {
            return Err(AppError::Validation("Invalid timezone name".to_string())
                .with_field("timezone", FieldErrorCode::Invalid));
        }
    }

    UsersService::update_preferences(pool.get_ref(), user.0.id, body.language, body.timezone)
        .await?;

    let updated = UsersService::get_by_id(pool.get_ref(), user.0.id)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    Ok(HttpResponse::Ok().json(UserResponse::from(updated)))
}

#[cfg(feature = "openapi")]
#[derive(OpenApi)]
#[openapi(
    paths(
        register,
        accept_invitation,
        get_invitation,
        login,
        logout,
        get_current_user,
        update_current_user
    ),
    components(schemas(
        crate::models::CreateUserRequest,
        crate::models::LoginRequest,
        crate::models::AcceptInvitation,
        AuthResponse,
        UserResponse,
        UpdatePreferencesRequest,
        InvitationInfoResponse,
    ))
)]
pub struct AuthApi;

/// Configure auth routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/auth")
            .route("/register", web::post().to(register))
            .route("/accept-invitation", web::post().to(accept_invitation))
            .route("/invitation/{token}", web::get().to(get_invitation))
            .route("/login", web::post().to(login))
            .route("/logout", web::post().to(logout))
            .route("/me", web::get().to(get_current_user))
            .route("/me", web::patch().to(update_current_user)),
    );
}
