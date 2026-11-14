//! Saved views (#531): a project page's under `/api/projects/{project_id}/views`,
//! and the workspace's (pages that span every project) under `/api/views`.
//!
//! Anyone who sees the project (or is signed in, for the workspace) lists and
//! saves views. A shared view is changed by its creator, or by someone who may
//! update the project; on the workspace, by an owner. The same holds for the
//! shared default.

use actix_web::{web, HttpResponse};
use serde::Deserialize;

use crate::auth::ApiActor;
use crate::db::DbPool;
use crate::error::FieldErrorCode;
use crate::error::{AppError, AppResult};
use crate::models::rbac::{Action, Resource};
use crate::models::saved_view::{
    CreateSavedView, DefaultScope, ListSavedViews, SavedView, SetDefaultView, UpdateSavedView,
};
use crate::services::saved_view::SavedViewService;

/// Where views live: a project, or the workspace (`None`).
type Scope = Option<i32>;

/// Fails unless the actor sees `scope`. The workspace's views belong to the
/// page of every project's issues, so they need what that page needs:
/// reading issues somewhere. A token narrowed to nothing gets nothing.
fn require_sees(actor: &ApiActor, scope: Scope) -> AppResult<()> {
    match scope {
        Some(id) => actor.require(Resource::Project, Action::Read, Some(id)),
        None if actor.access.can_somewhere(Resource::Issues, Action::Read) => Ok(()),
        None => Err(AppError::Forbidden(
            "Missing permission issues:read".to_string(),
        )),
    }
}

/// May change anyone's shared views and the shared default of `scope`.
fn manages(actor: &ApiActor, scope: Scope) -> bool {
    match scope {
        Some(id) => actor
            .access
            .can(Resource::Project, Action::Update, Some(id)),
        None => actor.is_owner(),
    }
}

async fn list(
    pool: &DbPool,
    actor: &ApiActor,
    scope: Scope,
    page: &str,
) -> AppResult<HttpResponse> {
    require_sees(actor, scope)?;
    let views = SavedViewService::list(pool, scope, page, actor.user_id()).await?;
    Ok(HttpResponse::Ok().json(views))
}

async fn create(
    pool: &DbPool,
    actor: &ApiActor,
    scope: Scope,
    body: CreateSavedView,
) -> AppResult<HttpResponse> {
    require_sees(actor, scope)?;
    let user = actor.require_user()?;
    let view = SavedViewService::create(pool, scope, user.id, body.validate()?).await?;
    Ok(HttpResponse::Created().json(view))
}

/// The view if the actor may change it: their own, or a shared one they
/// manage.
async fn editable(
    pool: &DbPool,
    actor: &ApiActor,
    scope: Scope,
    view_id: i32,
) -> AppResult<SavedView> {
    require_sees(actor, scope)?;
    let view = SavedViewService::get_visible(pool, scope, view_id, actor.user_id()).await?;
    let own = view.created_by.is_some() && view.created_by == actor.user_id();
    if own || manages(actor, scope) {
        Ok(view)
    } else {
        Err(AppError::Forbidden(
            "Only its creator or an admin changes a shared view".to_string(),
        ))
    }
}

async fn update(
    pool: &DbPool,
    actor: &ApiActor,
    scope: Scope,
    view_id: i32,
    body: UpdateSavedView,
) -> AppResult<HttpResponse> {
    let view = editable(pool, actor, scope, view_id).await?;
    // Validated as the whole view, written as only the fields sent: a
    // concurrent change to another field is kept, not overwritten.
    let merged = view.merge(body.clone())?;
    let changes = UpdateSavedView {
        name: body.name.map(|_| merged.name),
        query: body.query.map(|_| merged.query),
        visibility: body.visibility,
    };
    let view = SavedViewService::update(pool, view.id, changes).await?;
    Ok(HttpResponse::Ok().json(view))
}

async fn delete(
    pool: &DbPool,
    actor: &ApiActor,
    scope: Scope,
    view_id: i32,
) -> AppResult<HttpResponse> {
    let view = editable(pool, actor, scope, view_id).await?;
    SavedViewService::delete(pool, view.id).await?;
    Ok(HttpResponse::NoContent().finish())
}

async fn set_default(
    pool: &DbPool,
    actor: &ApiActor,
    scope: Scope,
    body: SetDefaultView,
) -> AppResult<HttpResponse> {
    let SetDefaultView {
        page,
        view_id,
        scope: whose,
    } = body;
    require_sees(actor, scope)?;
    if whose == DefaultScope::Project && !manages(actor, scope) {
        return Err(AppError::Forbidden(
            "Only an admin sets the shared default".to_string(),
        ));
    }
    if let Some(id) = view_id {
        let view = SavedViewService::get_visible(pool, scope, id, actor.user_id()).await?;
        if view.page != page || (whose == DefaultScope::Project && !view.is_shared()) {
            return Err(
                AppError::Validation("Not a shared view of that page".to_string())
                    .with_field("view_id", FieldErrorCode::Invalid),
            );
        }
    }
    match whose {
        DefaultScope::User => {
            let user = actor.require_user()?;
            SavedViewService::set_user_default(pool, user.id, scope, &page, view_id).await?
        }
        DefaultScope::Project => {
            SavedViewService::set_project_default(pool, scope, &page, view_id).await?
        }
    }
    Ok(HttpResponse::NoContent().finish())
}

#[derive(Deserialize)]
pub struct ViewPath {
    pub project_id: i32,
    pub view_id: i32,
}

/// Lists the views of one project page the caller sees: their own and the shared ones.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/views",
    operation_id = "savedViews.list",
    tag = "Saved Views",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("page" = String, Query, description = "The dashboard page (`issues`, `logs`, …)"),
    ),
    responses(
        (status = 200, description = "The page's views", body = Vec<SavedView>),
        (status = 400, description = "Invalid query parameter", body = String, content_type = "text/plain"),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Project not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
pub async fn list_views(
    pool: web::Data<DbPool>,
    actor: ApiActor,
    path: web::Path<i32>,
    query: web::Query<ListSavedViews>,
) -> AppResult<HttpResponse> {
    list(&pool, &actor, Some(path.into_inner()), &query.page).await
}

/// Saves a project page's address as a view, personal unless `visibility` is `project`.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/projects/{project_id}/views",
    operation_id = "savedViews.create",
    tag = "Saved Views",
    params(("project_id" = i32, Path, description = "Project ID")),
    request_body = CreateSavedView,
    responses(
        (status = 201, description = "View saved", body = SavedView),
        (status = 400, description = "Invalid view", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Needs a user account", body = crate::error::ErrorResponse),
        (status = 404, description = "Project not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
pub async fn create_view(
    pool: web::Data<DbPool>,
    actor: ApiActor,
    path: web::Path<i32>,
    body: web::Json<CreateSavedView>,
) -> AppResult<HttpResponse> {
    create(&pool, &actor, Some(path.into_inner()), body.into_inner()).await
}

/// Renames a project view, or changes its address or who sees it.
#[cfg_attr(feature = "openapi", utoipa::path(
    patch,
    path = "/api/projects/{project_id}/views/{view_id}",
    operation_id = "savedViews.update",
    tag = "Saved Views",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("view_id" = i32, Path, description = "Saved view ID"),
    ),
    request_body = UpdateSavedView,
    responses(
        (status = 200, description = "View updated", body = SavedView),
        (status = 400, description = "Invalid view", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Someone else's shared view", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
pub async fn update_view(
    pool: web::Data<DbPool>,
    actor: ApiActor,
    path: web::Path<ViewPath>,
    body: web::Json<UpdateSavedView>,
) -> AppResult<HttpResponse> {
    update(
        &pool,
        &actor,
        Some(path.project_id),
        path.view_id,
        body.into_inner(),
    )
    .await
}

/// Deletes a project view.
#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/api/projects/{project_id}/views/{view_id}",
    operation_id = "savedViews.delete",
    tag = "Saved Views",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("view_id" = i32, Path, description = "Saved view ID"),
    ),
    responses(
        (status = 204, description = "View deleted"),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Someone else's shared view", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
pub async fn delete_view(
    pool: web::Data<DbPool>,
    actor: ApiActor,
    path: web::Path<ViewPath>,
) -> AppResult<HttpResponse> {
    delete(&pool, &actor, Some(path.project_id), path.view_id).await
}

/// Sets or clears the view a project page opens on, for the caller or the project.
#[cfg_attr(feature = "openapi", utoipa::path(
    put,
    path = "/api/projects/{project_id}/views/default",
    operation_id = "savedViews.setDefault",
    tag = "Saved Views",
    params(("project_id" = i32, Path, description = "Project ID")),
    request_body = SetDefaultView,
    responses(
        (status = 204, description = "Default set"),
        (status = 400, description = "Not a view of that page, or a personal view as the project's default", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "The project's default needs project:update", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
pub async fn set_default_view(
    pool: web::Data<DbPool>,
    actor: ApiActor,
    path: web::Path<i32>,
    body: web::Json<SetDefaultView>,
) -> AppResult<HttpResponse> {
    set_default(&pool, &actor, Some(path.into_inner()), body.into_inner()).await
}

/// Lists the views of one workspace page (one that spans every project).
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/views",
    operation_id = "workspaceViews.list",
    tag = "Saved Views",
    params(("page" = String, Query, description = "The dashboard page (`issues`, …)")),
    responses(
        (status = 200, description = "The page's views", body = Vec<SavedView>),
        (status = 400, description = "Invalid query parameter", body = String, content_type = "text/plain"),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
pub async fn list_workspace_views(
    pool: web::Data<DbPool>,
    actor: ApiActor,
    query: web::Query<ListSavedViews>,
) -> AppResult<HttpResponse> {
    list(&pool, &actor, None, &query.page).await
}

/// Saves a workspace page's address as a view, personal unless `visibility` is `project`.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/views",
    operation_id = "workspaceViews.create",
    tag = "Saved Views",
    request_body = CreateSavedView,
    responses(
        (status = 201, description = "View saved", body = SavedView),
        (status = 400, description = "Invalid view", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Needs a user account", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
pub async fn create_workspace_view(
    pool: web::Data<DbPool>,
    actor: ApiActor,
    body: web::Json<CreateSavedView>,
) -> AppResult<HttpResponse> {
    create(&pool, &actor, None, body.into_inner()).await
}

/// Renames a workspace view, or changes its address or who sees it.
#[cfg_attr(feature = "openapi", utoipa::path(
    patch,
    path = "/api/views/{view_id}",
    operation_id = "workspaceViews.update",
    tag = "Saved Views",
    params(("view_id" = i32, Path, description = "Saved view ID")),
    request_body = UpdateSavedView,
    responses(
        (status = 200, description = "View updated", body = SavedView),
        (status = 400, description = "Invalid view", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Someone else's shared view", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
pub async fn update_workspace_view(
    pool: web::Data<DbPool>,
    actor: ApiActor,
    path: web::Path<i32>,
    body: web::Json<UpdateSavedView>,
) -> AppResult<HttpResponse> {
    update(&pool, &actor, None, path.into_inner(), body.into_inner()).await
}

/// Deletes a workspace view.
#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/api/views/{view_id}",
    operation_id = "workspaceViews.delete",
    tag = "Saved Views",
    params(("view_id" = i32, Path, description = "Saved view ID")),
    responses(
        (status = 204, description = "View deleted"),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Someone else's shared view", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
pub async fn delete_workspace_view(
    pool: web::Data<DbPool>,
    actor: ApiActor,
    path: web::Path<i32>,
) -> AppResult<HttpResponse> {
    delete(&pool, &actor, None, path.into_inner()).await
}

/// Sets or clears the view a workspace page opens on, for the caller or everyone.
#[cfg_attr(feature = "openapi", utoipa::path(
    put,
    path = "/api/views/default",
    operation_id = "workspaceViews.setDefault",
    tag = "Saved Views",
    request_body = SetDefaultView,
    responses(
        (status = 204, description = "Default set"),
        (status = 400, description = "Not a view of that page, or a personal view as everyone's default", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Everyone's default needs an owner", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
pub async fn set_workspace_default_view(
    pool: web::Data<DbPool>,
    actor: ApiActor,
    body: web::Json<SetDefaultView>,
) -> AppResult<HttpResponse> {
    set_default(&pool, &actor, None, body.into_inner()).await
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/api/views", web::get().to(list_workspace_views))
        .route("/api/views", web::post().to(create_workspace_view))
        .route(
            "/api/views/default",
            web::put().to(set_workspace_default_view),
        )
        .route(
            "/api/views/{view_id}",
            web::patch().to(update_workspace_view),
        )
        .route(
            "/api/views/{view_id}",
            web::delete().to(delete_workspace_view),
        );
}

pub fn configure_project(cfg: &mut web::ServiceConfig) {
    cfg.route("/views", web::get().to(list_views))
        .route("/views", web::post().to(create_view))
        .route("/views/default", web::put().to(set_default_view))
        .route("/views/{view_id}", web::patch().to(update_view))
        .route("/views/{view_id}", web::delete().to(delete_view));
}
