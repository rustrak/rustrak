use actix_web::http::StatusCode;
use actix_web::{web, HttpResponse};

use crate::auth::ApiActor;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::{CreateRelease, UpdateRelease};
use crate::services::access::{self, Action};
use crate::services::{IssueService, ProjectService, ReleaseService};

#[cfg(feature = "openapi")]
use crate::models::{IssueResponse, ReleaseResponse};
#[cfg(feature = "openapi")]
use utoipa::OpenApi;

/// Query params for the new-issues endpoint.
#[derive(serde::Deserialize)]
pub struct NewIssuesQuery {
    /// Max issues to return (default: 10, clamped to 1-50).
    pub limit: Option<i64>,
    pub environment: Option<String>,
}

impl NewIssuesQuery {
    fn limit(&self) -> i64 {
        self.limit.unwrap_or(10).clamp(1, 50)
    }
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/releases/{release}/new-issues",
    tag = "Releases",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("release" = String, Path, description = "Release version"),
        ("limit" = Option<i64>, Query, description = "Max issues to return (default: 10, max: 50)"),
    ),
    responses(
        (status = 200, description = "Issues first seen in this release, most recent first", body = Vec<IssueResponse>),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/projects/{project_id}/releases/{release}/new-issues
pub async fn new_issues_for_release(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, String)>,
    query: web::Query<NewIssuesQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, release) = path.into_inner();

    access::require(
        pool.get_ref(),
        actor.is_admin(),
        actor.user_id(),
        project_id,
        Action::ViewProject,
    )
    .await?;

    let project = ProjectService::get_by_id(pool.get_ref(), project_id).await?;

    let issues = IssueService::top_issues_for_release(
        pool.get_ref(),
        project_id,
        &release,
        query.limit(),
        query.environment.as_deref().filter(|s| !s.is_empty()),
    )
    .await?;

    let environment = query.environment.as_deref().filter(|s| !s.is_empty());
    let issue_ids: Vec<_> = issues.iter().map(|issue| issue.id).collect();
    let stats = if environment.is_some() {
        IssueService::list_stats(pool.get_ref(), &issue_ids, environment).await?
    } else {
        Default::default()
    };
    let responses: Vec<_> = issues
        .iter()
        .map(|issue| {
            let mut response = issue.to_response(&project.slug);
            if let Some(stats) = stats.get(&issue.id) {
                response.user_count = Some(stats.user_count);
                response.trend = Some(stats.trend.clone());
                if let Some(count) = stats.event_count {
                    response.event_count = count as i32;
                }
            }
            response
        })
        .collect();

    Ok(HttpResponse::Ok().json(responses))
}

// ---------------------------------------------------------------------------
// Sentry-compatible CI endpoints (sentry-cli, JS bundler plugins)
// ---------------------------------------------------------------------------
//
// Mounted under `/api/0/projects/{org_slug}/{project_slug}/releases/...`.
// `org_slug` is synthetic — Rustrak has no org concept — and is accepted and
// echoed/ignored exactly like `sourcemaps::org_details`. Auth is Bearer via
// `ApiActor` (not `SentryAuth`), matching the other CI-facing endpoints in
// `sourcemaps.rs`.

/// Resolves a project id from its slug, matching the pattern used by the
/// source-maps CI endpoints (`sourcemaps.rs`).
async fn resolve_project_id_by_slug(pool: &DbPool, project_slug: &str) -> AppResult<i32> {
    let project: Option<(i32,)> = sqlx::query_as("SELECT id FROM projects WHERE slug = $1")
        .bind(project_slug)
        .fetch_optional(pool)
        .await?;
    project
        .map(|(id,)| id)
        .ok_or_else(|| AppError::NotFound(format!("project not found: {}", project_slug)))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/0/projects/{org_slug}/{project_slug}/releases/",
    tag = "Releases",
    params(
        ("org_slug" = String, Path, description = "Organization slug (synthetic — accepted and ignored)"),
        ("project_slug" = String, Path, description = "Project slug"),
    ),
    request_body = CreateRelease,
    responses(
        (status = 201, description = "Release created", body = ReleaseResponse),
        (status = 208, description = "Release already exists — no duplicate created", body = ReleaseResponse),
        (status = 400, description = "Invalid version", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Project not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// POST /api/0/projects/{org_slug}/{project_slug}/releases/
///
/// Sentry-compatible release creation. Idempotent: repeating the same
/// `(project, version)` returns 208 rather than erroring — sentry-cli and JS
/// bundler plugins call this on every build. Triggers date-based regression
/// clearing (see [`crate::services::issue::IssueService::finalize_release`])
/// on every call, not just the new-row branch: `finalize_release`'s own
/// `UPDATE ... WHERE` is idempotent (a repeat run affects 0 rows once nothing
/// is left to clear), so this also self-heals a prior call that created the
/// row but failed before clearing ran — a plain `if created` gate would
/// otherwise skip clearing forever once a retry lands on the 208 branch.
pub async fn create_release(
    path: web::Path<(String, String)>,
    actor: ApiActor,
    pool: web::Data<DbPool>,
    body: web::Json<CreateRelease>,
) -> AppResult<HttpResponse> {
    let (_org_slug, project_slug) = path.into_inner();
    let project_id = resolve_project_id_by_slug(pool.get_ref(), &project_slug).await?;

    access::require(
        pool.get_ref(),
        actor.is_admin(),
        actor.user_id(),
        project_id,
        Action::UpdateProject,
    )
    .await?;

    let (release, created) =
        ReleaseService::create(pool.get_ref(), project_id, body.into_inner()).await?;

    IssueService::finalize_release(pool.get_ref(), project_id, release.date_created).await?;

    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::from_u16(208).expect("208 is a valid HTTP status code")
    };
    Ok(HttpResponse::build(status).json(release.to_response()))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    put,
    path = "/api/0/projects/{org_slug}/{project_slug}/releases/{version}/",
    tag = "Releases",
    params(
        ("org_slug" = String, Path, description = "Organization slug (synthetic — accepted and ignored)"),
        ("project_slug" = String, Path, description = "Project slug"),
        ("version" = String, Path, description = "Release version"),
    ),
    request_body = UpdateRelease,
    responses(
        (status = 200, description = "Release updated", body = ReleaseResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Project or release not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// PUT /api/0/projects/{org_slug}/{project_slug}/releases/{version}/
///
/// Generic partial update (`ref`, `url`, `dateReleased`). Setting
/// `dateReleased` IS "finalize" — there is no separate status flag.
pub async fn finalize_release(
    path: web::Path<(String, String, String)>,
    actor: ApiActor,
    pool: web::Data<DbPool>,
    body: web::Json<UpdateRelease>,
) -> AppResult<HttpResponse> {
    let (_org_slug, project_slug, version) = path.into_inner();
    let project_id = resolve_project_id_by_slug(pool.get_ref(), &project_slug).await?;

    access::require(
        pool.get_ref(),
        actor.is_admin(),
        actor.user_id(),
        project_id,
        Action::UpdateProject,
    )
    .await?;

    let release =
        ReleaseService::finalize(pool.get_ref(), project_id, &version, body.into_inner()).await?;

    Ok(HttpResponse::Ok().json(release.to_response()))
}

#[cfg(feature = "openapi")]
#[derive(OpenApi)]
#[openapi(
    paths(new_issues_for_release, create_release, finalize_release),
    components(schemas(IssueResponse, CreateRelease, UpdateRelease, ReleaseResponse))
)]
pub struct ReleasesApi;

/// Configure release routes.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(web::scope("/api/projects/{project_id}/releases").route(
        "/{release}/new-issues",
        web::get().to(new_issues_for_release),
    ))
    .route(
        "/api/0/projects/{org_slug}/{project_slug}/releases/",
        web::post().to(create_release),
    )
    .route(
        "/api/0/projects/{org_slug}/{project_slug}/releases/{version}/",
        web::put().to(finalize_release),
    );
}
