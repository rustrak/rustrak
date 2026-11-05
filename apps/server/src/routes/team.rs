//! Team management routes.
//!
//! - GET   /api/team                  — list all users
//! - PATCH /api/team/{user_id}/role   — change a user's global role

use actix_web::{web, HttpResponse};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::auth::rbac::{Action, Resource};
use crate::auth::ApiActor;
use crate::db::DbPool;
use crate::error::{AppError, AppResult, FieldErrorCode};
use crate::models::{User, UserRole};
use crate::services::UsersService;

#[cfg(feature = "openapi")]
use utoipa::OpenApi;

/// A user as exposed on the team roster.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TeamMemberResponse {
    pub id: i32,
    pub email: String,
    #[cfg_attr(feature = "openapi", schema(value_type = crate::models::UserRole))]
    pub role: String,
    pub is_active: bool,
    /// True for the first-registered (primary) account, which cannot be demoted
    /// or deleted.
    pub is_primary: bool,
    pub created_at: DateTime<Utc>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub last_login: Option<DateTime<Utc>>,
}

impl TeamMemberResponse {
    fn from_user(u: User, primary_id: Option<i32>) -> Self {
        Self {
            is_primary: primary_id == Some(u.id),
            id: u.id,
            email: u.email,
            role: u.role,
            is_active: u.is_active,
            created_at: u.created_at,
            last_login: u.last_login,
        }
    }
}

/// Request body for changing a user's global role.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdateUserRole {
    #[cfg_attr(feature = "openapi", schema(value_type = crate::models::UserRole))]
    pub role: String,
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/team",
    operation_id = "team.list",
    tag = "Team",
    responses(
        (status = 200, description = "List of team members", body = Vec<TeamMemberResponse>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/team — list all users (needs `users:read`)
pub async fn list_team(pool: web::Data<DbPool>, actor: ApiActor) -> AppResult<HttpResponse> {
    actor.require(Resource::Users, Action::Read, None)?;

    let primary_id = UsersService::primary_user_id(pool.get_ref()).await?;
    let users = UsersService::list(pool.get_ref()).await?;
    let responses: Vec<TeamMemberResponse> = users
        .into_iter()
        .map(|u| TeamMemberResponse::from_user(u, primary_id))
        .collect();

    Ok(HttpResponse::Ok().json(responses))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    patch,
    path = "/api/team/{user_id}/role",
    operation_id = "team.updateRole",
    tag = "Team",
    params(("user_id" = i32, Path, description = "User ID")),
    request_body = UpdateUserRole,
    responses(
        (status = 204, description = "Role updated"),
        (status = 400, description = "Invalid role", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
        (status = 404, description = "User not found", body = crate::error::ErrorResponse),
        (status = 409, description = "Cannot demote the last admin", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// PATCH /api/team/{user_id}/role — change a user's global role (needs `users:update`)
pub async fn update_team_role(
    pool: web::Data<DbPool>,
    actor: ApiActor,
    path: web::Path<i32>,
    body: web::Json<UpdateUserRole>,
) -> AppResult<HttpResponse> {
    actor.require(Resource::Users, Action::Update, None)?;

    let user_id = path.into_inner();
    // Same `(role, invalid)` pair as the last-admin guard below, and
    // deliberately so: `role` is the only input in the body either can blame.
    // The status is what separates them: a 400 here means "not a role at
    // all", the 409 below means "a real role, but not acceptable right now".
    // See the `FieldErrorCode::Invalid` docs.
    let new_role = UserRole::parse(&body.role).ok_or_else(|| {
        AppError::Validation(format!("Invalid role: {}", body.role))
            .with_field("role", FieldErrorCode::Invalid)
    })?;

    // Owners are made and unmade only by owners: anything else would let
    // `users:update` grant itself everything.
    let target = UsersService::get_by_id(pool.get_ref(), user_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("User {} not found", user_id)))?;
    if (new_role == UserRole::Admin || target.is_admin()) && !actor.is_owner() {
        return Err(AppError::Forbidden(
            "Only an owner can change an owner".to_string(),
        ));
    }

    // Guard: the primary (first-registered) account must remain an admin.
    if new_role != UserRole::Admin
        && UsersService::primary_user_id(pool.get_ref()).await? == Some(user_id)
    {
        return Err(AppError::Forbidden(
            "The primary admin's role cannot be changed".to_string(),
        ));
    }

    // Guard: never demote the last remaining admin.
    if new_role == UserRole::Member
        && target.is_admin()
        && UsersService::admin_count(pool.get_ref()).await? <= 1
    {
        // `invalid`, not `already_exists`: the role the caller picked is
        // well-formed but not acceptable for this user right now. The
        // blamed input is the role select, the only field in the body.
        return Err(
            AppError::Conflict("Cannot demote the last admin".to_string())
                .with_field("role", FieldErrorCode::Invalid),
        );
    }

    UsersService::update_role(pool.get_ref(), user_id, new_role).await?;

    Ok(HttpResponse::NoContent().finish())
}

#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/api/team/{user_id}",
    operation_id = "team.deleteUser",
    tag = "Team",
    params(("user_id" = i32, Path, description = "User ID")),
    responses(
        (status = 204, description = "User deleted"),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
        (status = 404, description = "User not found", body = crate::error::ErrorResponse),
        (status = 409, description = "Cannot delete the last admin", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// DELETE /api/team/{user_id} — remove a user from the instance (needs `users:delete`)
pub async fn delete_user(
    pool: web::Data<DbPool>,
    actor: ApiActor,
    path: web::Path<i32>,
) -> AppResult<HttpResponse> {
    actor.require(Resource::Users, Action::Delete, None)?;

    let user_id = path.into_inner();

    // Cannot delete your own account (avoids accidental self-lockout).
    if actor.user_id() == Some(user_id) {
        return Err(AppError::Forbidden(
            "You cannot delete your own account".to_string(),
        ));
    }

    // The primary (first-registered) account cannot be deleted.
    if UsersService::primary_user_id(pool.get_ref()).await? == Some(user_id) {
        return Err(AppError::Forbidden(
            "The primary admin cannot be deleted".to_string(),
        ));
    }

    // Never delete the last remaining admin.
    let target = UsersService::get_by_id(pool.get_ref(), user_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("User {} not found", user_id)))?;
    if target.is_admin() && !actor.is_owner() {
        return Err(AppError::Forbidden(
            "Only an owner can delete an owner".to_string(),
        ));
    }
    if target.is_admin() && UsersService::admin_count(pool.get_ref()).await? <= 1 {
        return Err(AppError::Conflict(
            "Cannot delete the last admin".to_string(),
        ));
    }

    UsersService::delete(pool.get_ref(), user_id).await?;

    Ok(HttpResponse::NoContent().finish())
}

#[cfg(feature = "openapi")]
#[derive(OpenApi)]
#[openapi(
    paths(list_team, update_team_role, delete_user),
    components(schemas(TeamMemberResponse, UpdateUserRole))
)]
pub struct TeamApi;

/// Configure team routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/team")
            .route("", web::get().to(list_team))
            .route("/{user_id}/role", web::patch().to(update_team_role))
            .route("/{user_id}", web::delete().to(delete_user)),
    );
}
