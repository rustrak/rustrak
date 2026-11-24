use actix_web::{web, HttpResponse};

use crate::auth::ApiActor;
use crate::db::DbPool;
use crate::error::AppResult;
use crate::models::sidebar::SidebarProjects;
use crate::services::sidebar::SidebarService;

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/sidebar/projects",
    operation_id = "sidebar.getProjects",
    tag = "Sidebar",
    responses(
        (status = 200, description = "How the reader's sidebar lists the projects", body = SidebarProjects),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// How your sidebar lists the projects.
///
/// The default (manual order, nothing hidden) until you save one, and for a
/// token without a user.
pub async fn get_projects(pool: web::Data<DbPool>, actor: ApiActor) -> AppResult<HttpResponse> {
    let sidebar = match actor.user_id() {
        Some(user_id) => SidebarService::projects(pool.get_ref(), user_id).await?,
        None => SidebarProjects::default(),
    };
    Ok(HttpResponse::Ok().json(sidebar))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    put,
    path = "/api/sidebar/projects",
    operation_id = "sidebar.updateProjects",
    tag = "Sidebar",
    request_body = SidebarProjects,
    responses(
        (status = 200, description = "Saved", body = SidebarProjects),
        (status = 400, description = "Invalid body, such as an unknown sort", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "A token without a user", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Replace how your sidebar lists the projects.
pub async fn update_projects(
    pool: web::Data<DbPool>,
    actor: ApiActor,
    // Read after the caller: an anonymous request is a 401 whatever it sent,
    // and a body that does not parse is an `ErrorResponse`, not plain text.
    body: AppResult<web::Json<SidebarProjects>>,
) -> AppResult<HttpResponse> {
    let user_id = actor.require_user()?.id;
    let sidebar = body?.into_inner();
    SidebarService::save_projects(pool.get_ref(), user_id, &sidebar).await?;
    Ok(HttpResponse::Ok().json(sidebar))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/api/sidebar/projects", web::get().to(get_projects))
        .route("/api/sidebar/projects", web::put().to(update_projects));
}
