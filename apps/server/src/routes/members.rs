//! Project members: who holds which roles on one project.
//!
//! - GET    /api/projects/{id}/members            — list members (`project-members:read`)
//! - PUT    /api/projects/{id}/members            — set a member's roles (`project-members:update`)
//! - DELETE /api/projects/{id}/members/{user_id}  — remove a member (`project-members:update`)
//!
//! Nobody changes their own membership, or hands out a role beyond what they
//! hold on this project.

use actix_web::{web, HttpResponse};
use serde::Deserialize;

use crate::auth::rbac::{Action, Resource};
use crate::auth::ApiActor;
use crate::db::DbPool;
use crate::error::{AppError, AppResult, FieldErrorCode};
#[cfg(feature = "openapi")]
use crate::services::role::ProjectMember;
use crate::services::RoleService;

#[cfg(feature = "openapi")]
use utoipa::OpenApi;

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SetProjectMember {
    pub user_id: i32,
    /// The member's roles on this project; empty removes them.
    pub role_ids: Vec<i32>,
}

fn not_yourself(actor: &ApiActor, user_id: i32) -> AppResult<()> {
    if actor.user_id() == Some(user_id) {
        return Err(AppError::Forbidden(
            "You cannot change your own project roles".to_string(),
        ));
    }
    Ok(())
}

/// Every role being given or taken away must be within the caller's own
/// permissions on this project.
async fn require_covered(
    pool: &DbPool,
    actor: &ApiActor,
    project_id: i32,
    user_id: i32,
    new_role_ids: &[i32],
) -> AppResult<()> {
    // Only roles of the list can be given; what is taken away may be anyone's.
    let shared = RoleService::list(pool).await?;
    let all = RoleService::list_all(pool).await?;
    let current = RoleService::assignments_for(pool, user_id).await?;
    let new = new_role_ids
        .iter()
        .map(|id| shared.iter().find(|r| r.id == *id).ok_or(*id));
    let old = current
        .iter()
        .filter(|a| a.project_id == Some(project_id))
        .map(|a| all.iter().find(|r| r.id == a.role_id).ok_or(a.role_id));
    for role in new.chain(old) {
        let role = role.map_err(|id| {
            AppError::Validation(format!("Unknown role {id}"))
                .with_field("role_ids", FieldErrorCode::Invalid)
        })?;
        if !actor.access.covers(&role.grants(), Some(project_id)) {
            return Err(AppError::Forbidden(
                "You cannot grant permissions you do not hold".to_string(),
            ));
        }
    }
    Ok(())
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/members",
    operation_id = "members.list",
    tag = "Members",
    params(("project_id" = i32, Path, description = "Project ID")),
    responses(
        (status = 200, description = "Project members", body = Vec<ProjectMember>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Missing project-members:read", body = crate::error::ErrorResponse),
        (status = 404, description = "Project not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Lists everyone holding a role on the project, with those roles.
pub async fn list_members(
    pool: web::Data<DbPool>,
    actor: ApiActor,
    path: web::Path<i32>,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();
    actor.require(Resource::ProjectMembers, Action::Read, Some(project_id))?;
    let members = RoleService::project_members(pool.get_ref(), project_id).await?;
    Ok(HttpResponse::Ok().json(members))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    put,
    path = "/api/projects/{project_id}/members",
    operation_id = "members.upsert",
    tag = "Members",
    params(("project_id" = i32, Path, description = "Project ID")),
    request_body = SetProjectMember,
    responses(
        (status = 200, description = "Project members now", body = Vec<ProjectMember>),
        (status = 400, description = "Unknown role", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Missing project-members:update, own membership, or a role beyond the caller's", body = crate::error::ErrorResponse),
        (status = 404, description = "Project or user not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Sets the roles a user holds on the project.
pub async fn upsert_member(
    pool: web::Data<DbPool>,
    actor: ApiActor,
    path: web::Path<i32>,
    body: web::Json<SetProjectMember>,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();
    actor.require(Resource::ProjectMembers, Action::Update, Some(project_id))?;
    not_yourself(&actor, body.user_id)?;
    require_covered(
        pool.get_ref(),
        &actor,
        project_id,
        body.user_id,
        &body.role_ids,
    )
    .await?;
    RoleService::set_project_roles(pool.get_ref(), project_id, body.user_id, &body.role_ids)
        .await?;
    let members = RoleService::project_members(pool.get_ref(), project_id).await?;
    Ok(HttpResponse::Ok().json(members))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/api/projects/{project_id}/members/{user_id}",
    operation_id = "members.remove",
    tag = "Members",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("user_id" = i32, Path, description = "User ID"),
    ),
    responses(
        (status = 204, description = "Removed"),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Missing project-members:update, own membership, or a role beyond the caller's", body = crate::error::ErrorResponse),
        (status = 404, description = "Project not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Removes every role a user holds on the project.
pub async fn remove_member(
    pool: web::Data<DbPool>,
    actor: ApiActor,
    path: web::Path<(i32, i32)>,
) -> AppResult<HttpResponse> {
    let (project_id, user_id) = path.into_inner();
    actor.require(Resource::ProjectMembers, Action::Update, Some(project_id))?;
    not_yourself(&actor, user_id)?;
    require_covered(pool.get_ref(), &actor, project_id, user_id, &[]).await?;
    RoleService::set_project_roles(pool.get_ref(), project_id, user_id, &[]).await?;
    Ok(HttpResponse::NoContent().finish())
}

#[cfg(feature = "openapi")]
#[derive(OpenApi)]
#[openapi(
    paths(list_members, upsert_member, remove_member),
    components(schemas(ProjectMember, crate::services::role::RoleRef, SetProjectMember))
)]
pub struct MembersApi;

/// Configure project member routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/projects/{project_id}/members")
            .route("", web::get().to(list_members))
            .route("", web::put().to(upsert_member))
            .route("/{user_id}", web::delete().to(remove_member)),
    );
}
