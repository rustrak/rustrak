//! Roles, the permission catalog, and who holds which role where.
//!
//! - GET    /api/access/catalog              — every resource and its actions
//! - GET    /api/access/me                   — what the caller may do
//! - GET    /api/roles                       — list roles
//! - POST   /api/roles                       — create a role
//! - PUT    /api/roles/{id}                  — replace a role
//! - DELETE /api/roles/{id}                  — delete an unused role
//! - GET    /api/users/{id}/assignments      — a user's roles
//! - PUT    /api/users/{id}/assignments      — replace a user's roles
//!
//! Nobody can write a permission into a role, or hand a role out, beyond what
//! they hold themselves; and nobody edits their own assignments.

use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};

use crate::auth::rbac::{Action, Resource, Scope};
use crate::auth::ApiActor;
use crate::db::DbPool;
use crate::error::{AppError, AppResult, FieldErrorCode};
#[cfg(feature = "openapi")]
use crate::models::Role;
use crate::models::{Permission, PersonalAccess, RoleAssignment};
use crate::services::{RoleService, UsersService};

#[cfg(feature = "openapi")]
use utoipa::OpenApi;

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CatalogResource {
    pub key: &'static str,
    pub module: &'static str,
    pub scope: Scope,
    pub actions: Vec<Action>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Catalog {
    pub resources: Vec<CatalogResource>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ProjectPermissions {
    pub project_id: i32,
    pub permissions: Vec<Permission>,
}

/// What the caller may do. An owner may do everything and gets empty lists.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct MyAccess {
    pub is_owner: bool,
    /// Workspace permissions.
    pub instance: Vec<Permission>,
    /// Project permissions held on every project.
    pub all_projects: Vec<Permission>,
    /// Project permissions held on one project.
    pub projects: Vec<ProjectPermissions>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RoleInput {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub permissions: Vec<Permission>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AssignmentInput {
    pub role_id: i32,
    /// `null` reaches every project.
    pub project_id: Option<i32>,
}

/// Everything a user holds.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SetAccess {
    pub assignments: Vec<AssignmentInput>,
    /// Omitted or `null`: no personal permissions.
    #[serde(default)]
    pub personal: Option<PersonalAccess>,
}

/// Everything a user holds.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UserAccess {
    pub assignments: Vec<RoleAssignment>,
    /// What their personal role holds; its places are in `assignments`.
    pub personal_permissions: Vec<Permission>,
}

fn permissions(pairs: Vec<(Resource, Action)>) -> Vec<Permission> {
    pairs
        .into_iter()
        .map(|(resource, action)| Permission { resource, action })
        .collect()
}

fn validate(name: &str) -> AppResult<()> {
    if name.trim().is_empty() {
        return Err(AppError::Validation("Role name is required".to_string())
            .with_field("name", FieldErrorCode::Required));
    }
    if name.chars().count() > 100 {
        return Err(
            AppError::Validation("Role name is at most 100 characters".to_string())
                .with_field("name", FieldErrorCode::TooLong),
        );
    }
    Ok(())
}

async fn user_access(pool: &DbPool, user_id: i32) -> AppResult<UserAccess> {
    Ok(UserAccess {
        assignments: RoleService::assignments_for(pool, user_id).await?,
        personal_permissions: RoleService::personal_permissions(pool, user_id).await?,
    })
}

fn beyond_your_own() -> AppError {
    AppError::Forbidden("You cannot grant permissions you do not hold".to_string())
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/access/catalog",
    operation_id = "access.catalog",
    tag = "Access",
    responses(
        (status = 200, description = "Every resource a permission can name", body = Catalog),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Lists every resource a permission can name, with its module, scope and actions.
pub async fn catalog(_actor: ApiActor) -> AppResult<HttpResponse> {
    let resources = Resource::ALL
        .iter()
        .map(|r| CatalogResource {
            key: r.key(),
            module: r.module(),
            scope: r.scope(),
            actions: r.actions().to_vec(),
        })
        .collect();
    Ok(HttpResponse::Ok().json(Catalog { resources }))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/access/me",
    operation_id = "access.me",
    tag = "Access",
    responses(
        (status = 200, description = "What the caller may do", body = MyAccess),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Returns what the caller may do, on the instance and on each project.
pub async fn me(actor: ApiActor) -> AppResult<HttpResponse> {
    let a = &actor.access;
    if a.is_owner() {
        return Ok(HttpResponse::Ok().json(MyAccess {
            is_owner: true,
            instance: vec![],
            all_projects: vec![],
            projects: vec![],
        }));
    }
    let (instance, all_projects): (Vec<_>, Vec<_>) = a
        .instance_pairs()
        .into_iter()
        .partition(|(r, _)| r.scope() == Scope::Instance);
    Ok(HttpResponse::Ok().json(MyAccess {
        is_owner: false,
        instance: permissions(instance),
        all_projects: permissions(all_projects),
        projects: a
            .project_pairs()
            .into_iter()
            .map(|(project_id, pairs)| ProjectPermissions {
                project_id,
                permissions: permissions(pairs),
            })
            .collect(),
    }))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/roles",
    operation_id = "roles.list",
    tag = "Access",
    responses(
        (status = 200, description = "Every role", body = Vec<Role>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Missing roles:read", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Lists every role with its permissions and how often it is assigned.
pub async fn list_roles(pool: web::Data<DbPool>, actor: ApiActor) -> AppResult<HttpResponse> {
    actor.require(Resource::Roles, Action::Read, None)?;
    let roles = RoleService::list(pool.get_ref()).await?;
    Ok(HttpResponse::Ok().json(roles))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/roles",
    operation_id = "roles.create",
    tag = "Access",
    request_body = RoleInput,
    responses(
        (status = 201, description = "Created", body = Role),
        (status = 400, description = "Invalid role", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Missing roles:create, or a permission the caller lacks", body = crate::error::ErrorResponse),
        (status = 409, description = "Name taken", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Creates a role from permissions the caller holds themselves.
pub async fn create_role(
    pool: web::Data<DbPool>,
    actor: ApiActor,
    body: web::Json<RoleInput>,
) -> AppResult<HttpResponse> {
    actor.require(Resource::Roles, Action::Create, None)?;
    validate(&body.name)?;
    if !actor
        .access
        .covers(&Permission::grants(&body.permissions), None)
    {
        return Err(beyond_your_own());
    }
    let b = body.into_inner();
    let role = RoleService::create(pool.get_ref(), &b.name, &b.description, b.permissions).await?;
    Ok(HttpResponse::Created().json(role))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    put,
    path = "/api/roles/{id}",
    operation_id = "roles.update",
    tag = "Access",
    params(("id" = i32, Path, description = "Role ID")),
    request_body = RoleInput,
    responses(
        (status = 200, description = "Updated", body = Role),
        (status = 400, description = "Invalid role", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Missing roles:update, or a permission the caller lacks", body = crate::error::ErrorResponse),
        (status = 404, description = "No such role", body = crate::error::ErrorResponse),
        (status = 409, description = "Name taken", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Replaces a role's name, description and permissions.
///
/// The caller must hold both the permissions being added and those being
/// removed, so nobody strips a role of something they could not give back.
pub async fn update_role(
    pool: web::Data<DbPool>,
    actor: ApiActor,
    path: web::Path<i32>,
    body: web::Json<RoleInput>,
) -> AppResult<HttpResponse> {
    actor.require(Resource::Roles, Action::Update, None)?;
    validate(&body.name)?;
    let id = path.into_inner();
    let current = RoleService::get(pool.get_ref(), id).await?;
    if !actor
        .access
        .covers(&Permission::grants(&body.permissions), None)
        || !actor.access.covers(&current.grants(), None)
    {
        return Err(beyond_your_own());
    }
    let b = body.into_inner();
    let role =
        RoleService::update(pool.get_ref(), id, &b.name, &b.description, b.permissions).await?;
    Ok(HttpResponse::Ok().json(role))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/api/roles/{id}",
    operation_id = "roles.delete",
    tag = "Access",
    params(("id" = i32, Path, description = "Role ID")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Missing roles:delete", body = crate::error::ErrorResponse),
        (status = 404, description = "No such role", body = crate::error::ErrorResponse),
        (status = 409, description = "Role still assigned", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Deletes a role nobody holds.
pub async fn delete_role(
    pool: web::Data<DbPool>,
    actor: ApiActor,
    path: web::Path<i32>,
) -> AppResult<HttpResponse> {
    actor.require(Resource::Roles, Action::Delete, None)?;
    RoleService::delete(pool.get_ref(), path.into_inner()).await?;
    Ok(HttpResponse::NoContent().finish())
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/users/{id}/assignments",
    operation_id = "users.assignments",
    tag = "Access",
    params(("id" = i32, Path, description = "User ID")),
    responses(
        (status = 200, description = "What the user holds", body = UserAccess),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Missing users:read", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Lists the roles a user holds, on the instance and on each project.
pub async fn list_assignments(
    pool: web::Data<DbPool>,
    actor: ApiActor,
    path: web::Path<i32>,
) -> AppResult<HttpResponse> {
    let user_id = path.into_inner();
    if actor.user_id() != Some(user_id) {
        actor.require(Resource::Users, Action::Read, None)?;
    }
    Ok(HttpResponse::Ok().json(user_access(pool.get_ref(), user_id).await?))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    put,
    path = "/api/users/{id}/assignments",
    operation_id = "users.setAssignments",
    tag = "Access",
    params(("id" = i32, Path, description = "User ID")),
    request_body = SetAccess,
    responses(
        (status = 200, description = "What the user holds now", body = UserAccess),
        (status = 400, description = "Unknown role, or personal permissions with no project", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Missing users:update, own assignments, or a role beyond the caller's", body = crate::error::ErrorResponse),
        (status = 404, description = "No such user or project", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Replaces every role and personal permission a user holds.
///
/// The caller must hold every role being given or taken away, on the scope
/// it applies to.
pub async fn set_assignments(
    pool: web::Data<DbPool>,
    actor: ApiActor,
    path: web::Path<i32>,
    body: web::Json<SetAccess>,
) -> AppResult<HttpResponse> {
    actor.require(Resource::Users, Action::Update, None)?;
    let user_id = path.into_inner();
    if actor.user_id() == Some(user_id) {
        return Err(AppError::Forbidden(
            "You cannot change your own roles".to_string(),
        ));
    }
    UsersService::get_by_id(pool.get_ref(), user_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("User {user_id} not found")))?;
    // Only roles of the list can be given; what is taken away may be anyone's.
    let shared = RoleService::list(pool.get_ref()).await?;
    let unknown = |id: i32| {
        AppError::Validation(format!("Unknown role {id}"))
            .with_field("role_id", FieldErrorCode::Invalid)
    };
    for a in &body.assignments {
        let role = shared
            .iter()
            .find(|r| r.id == a.role_id)
            .ok_or_else(|| unknown(a.role_id))?;
        if !actor.access.covers(&role.grants(), a.project_id) {
            return Err(beyond_your_own());
        }
    }
    let all = RoleService::list_all(pool.get_ref()).await?;
    for a in RoleService::assignments_for(pool.get_ref(), user_id).await? {
        let role = all
            .iter()
            .find(|r| r.id == a.role_id)
            .ok_or_else(|| unknown(a.role_id))?;
        if !actor.access.covers(&role.grants(), a.project_id) {
            return Err(beyond_your_own());
        }
    }
    if let Some(personal) = &body.personal {
        let grants = Permission::grants(&personal.permissions);
        let covered = match &personal.project_ids {
            None => actor.access.covers(&grants, None),
            Some(ids) if ids.is_empty() => {
                return Err(
                    AppError::Validation("Choose at least one project".to_string())
                        .with_field("project_ids", FieldErrorCode::Invalid),
                );
            }
            Some(ids) => ids.iter().all(|id| actor.access.covers(&grants, Some(*id))),
        };
        if !covered {
            return Err(beyond_your_own());
        }
    }
    let pairs: Vec<_> = body
        .assignments
        .iter()
        .map(|a| (a.role_id, a.project_id))
        .collect();
    RoleService::set_assignments(pool.get_ref(), user_id, &pairs, body.personal.as_ref()).await?;
    Ok(HttpResponse::Ok().json(user_access(pool.get_ref(), user_id).await?))
}

#[cfg(feature = "openapi")]
#[derive(OpenApi)]
#[openapi(
    paths(
        catalog,
        me,
        list_roles,
        create_role,
        update_role,
        delete_role,
        list_assignments,
        set_assignments
    ),
    components(schemas(
        Catalog,
        CatalogResource,
        MyAccess,
        ProjectPermissions,
        RoleInput,
        AssignmentInput,
        SetAccess,
        UserAccess,
        PersonalAccess,
        Role,
        RoleAssignment,
        Permission,
        Resource,
        Action,
        Scope
    ))
)]
pub struct RolesApi;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/api/access/catalog", web::get().to(catalog))
        .route("/api/access/me", web::get().to(me))
        .route("/api/roles", web::get().to(list_roles))
        .route("/api/roles", web::post().to(create_role))
        .route("/api/roles/{id}", web::put().to(update_role))
        .route("/api/roles/{id}", web::delete().to(delete_role))
        .route(
            "/api/users/{id}/assignments",
            web::get().to(list_assignments),
        )
        .route(
            "/api/users/{id}/assignments",
            web::put().to(set_assignments),
        );
}
