use actix_web::http::StatusCode;
use actix_web::{web, HttpResponse};

use crate::auth::ApiActor;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::rbac::{Action, Resource};
use crate::models::{
    is_valid_version, CreateOrgRelease, CreateRelease, UpdateOrgRelease, UpdateRelease,
};
use crate::services::release::SummaryFilter;
use crate::services::{IssueService, ProjectService, ReleaseService};

#[cfg(feature = "openapi")]
use crate::models::{IssueResponse, ReleaseResponse, ReleaseSummary};
#[cfg(feature = "openapi")]
use utoipa::OpenApi;

/// Query params for the new-issues endpoint.
#[derive(serde::Deserialize)]
pub struct NewIssuesQuery {
    /// Max issues to return (default: 10, clamped to 1-50).
    pub limit: Option<i64>,
}

impl NewIssuesQuery {
    fn limit(&self) -> i64 {
        self.limit.unwrap_or(10).clamp(1, 50)
    }
}

/// Query of the release list and of one release.
#[derive(serde::Deserialize)]
pub struct ReleasesQuery {
    pub environment: Option<String>,
    pub period: Option<String>,
    pub query: Option<String>,
    #[serde(default = "crate::pagination::default_page")]
    pub page: i64,
    #[serde(default = "default_per_page")]
    pub per_page: i64,
}

fn default_per_page() -> i64 {
    25
}

impl ReleasesQuery {
    fn filter(&self) -> SummaryFilter {
        SummaryFilter {
            environment: self.environment.clone().filter(|e| !e.is_empty()),
            period_hours: crate::routes::period::parse_period_hours(self.period.as_deref()),
            query: self.query.clone().filter(|q| !q.trim().is_empty()),
        }
    }
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/releases",
    operation_id = "releases.list",
    tag = "Releases",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("environment" = Option<String>, Query, description = "Only the releases seen in this environment, and count only it"),
        ("period" = Option<String>, Query, description = "Count only this window (24h, 7d, 14d...); all time if absent"),
        ("query" = Option<String>, Query, description = "Only the versions containing this text, ignoring case"),
        ("page" = Option<i64>, Query, description = "Page number (1-indexed, default: 1)"),
        ("per_page" = Option<i64>, Query, description = "Items per page (default: 25, max: 100)"),
    ),
    responses(
        (status = 200, description = "Releases, newest first", body = inline(crate::pagination::OffsetPaginatedResponse<ReleaseSummary>)),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Project not found"),
    ),
    security(("bearer_auth" = [])),
))]
/// Lists a project's releases, newest first, with the events, new issues,
/// environments and session health of each.
pub async fn list_releases(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<ReleasesQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();
    actor.require(Resource::Releases, Action::Read, Some(project_id))?;
    ProjectService::get_by_id(pool.get_ref(), project_id).await?;

    let per_page = crate::pagination::per_page(query.per_page)?;
    let page = query.page.max(1);
    let filter = query.filter();
    let mut all = ReleaseService::summaries(pool.get_ref(), project_id, &filter).await?;
    // As Sentry's list: an environment keeps the releases seen in it.
    if let Some(environment) = &filter.environment {
        all.retain(|release| release.environments.contains(environment));
    }
    let total = all.len() as i64;
    let items: Vec<_> = all
        .into_iter()
        .skip(crate::pagination::offset(page, per_page) as usize)
        .take(per_page as usize)
        .collect();

    Ok(
        HttpResponse::Ok().json(crate::pagination::OffsetPaginatedResponse::new(
            items, total, page, per_page,
        )),
    )
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/releases/{release}",
    operation_id = "releases.get",
    tag = "Releases",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("release" = String, Path, description = "Release version"),
        ("environment" = Option<String>, Query, description = "Count only this environment"),
        ("period" = Option<String>, Query, description = "Count only this window; all time if absent"),
    ),
    responses(
        (status = 200, description = "The release", body = ReleaseSummary),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "No such release in this project"),
    ),
    security(("bearer_auth" = [])),
))]
/// Gets one release with its events, new issues, environments and session
/// health.
pub async fn get_release(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, String)>,
    query: web::Query<ReleasesQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, version) = path.into_inner();
    actor.require(Resource::Releases, Action::Read, Some(project_id))?;

    let release = ReleaseService::summaries(pool.get_ref(), project_id, &query.filter())
        .await?
        .into_iter()
        .find(|r| r.version == version)
        .ok_or_else(|| AppError::NotFound(format!("Release {version} not found")))?;
    Ok(HttpResponse::Ok().json(release))
}

/// Lists the issues first seen in a release.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/releases/{release}/new-issues",
    operation_id = "releases.newIssues",
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
pub async fn new_issues_for_release(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, String)>,
    query: web::Query<NewIssuesQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, release) = path.into_inner();

    actor.require(Resource::Releases, Action::Read, Some(project_id))?;
    actor.require(Resource::Issues, Action::Read, Some(project_id))?;

    let project = ProjectService::get_by_id(pool.get_ref(), project_id).await?;

    let issues =
        IssueService::top_issues_for_release(pool.get_ref(), project_id, &release, query.limit())
            .await?;

    let responses: Vec<_> = issues
        .iter()
        .map(|i| i.to_response(&project.slug))
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

async fn resolve_project_id_by_slug(pool: &DbPool, project_slug: &str) -> AppResult<i32> {
    ProjectService::id_by_slug_or_id(pool, project_slug)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("project not found: {}", project_slug)))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/0/projects/{org_slug}/{project_slug}/releases/",
    operation_id = "releases.create",
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
/// Sentry-compatible release creation. Idempotent: repeating the same
/// `(project, version)` returns 208 rather than erroring — sentry-cli and JS
/// bundler plugins call this on every build. A new release resolves the
/// issues waiting on "resolved in next release" (see
/// [`crate::services::resolution::ResolutionService::clear_expired`]).
pub async fn create_release(
    path: web::Path<(String, String)>,
    actor: ApiActor,
    pool: web::Data<DbPool>,
    body: web::Json<CreateRelease>,
) -> AppResult<HttpResponse> {
    let (_org_slug, project_slug) = path.into_inner();
    let project_id = resolve_project_id_by_slug(pool.get_ref(), &project_slug).await?;

    actor.require(Resource::Releases, Action::Create, Some(project_id))?;

    let (release, created) =
        ReleaseService::create(pool.get_ref(), project_id, body.into_inner()).await?;

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
    operation_id = "releases.finalize",
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

    actor.require(Resource::Releases, Action::Update, Some(project_id))?;

    let release =
        ReleaseService::finalize(pool.get_ref(), project_id, &version, body.into_inner()).await?;

    Ok(HttpResponse::Ok().json(release.to_response()))
}

/// Sentry's answer to a project the caller cannot create releases in, or
/// that does not exist (`organization_releases.py`).
fn invalid_projects() -> HttpResponse {
    field_error("projects", "Invalid project ids or slugs")
}

/// Sentry's 400 for one invalid field.
fn field_error(field: &str, message: impl Into<String>) -> HttpResponse {
    HttpResponse::BadRequest().json(crate::models::FieldErrors::one(field, message))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/0/organizations/{org_slug}/releases/",
    operation_id = "releases.createInOrganization",
    tag = "Releases",
    params(("org_slug" = String, Path, description = "Organization slug (synthetic: accepted and ignored)")),
    request_body = CreateOrgRelease,
    responses(
        (status = 201, description = "Release created in at least one project", body = crate::models::OrgReleaseResponse),
        (status = 208, description = "Release already existed in every project", body = crate::models::OrgReleaseResponse),
        (status = 400, description = "Missing or invalid projects, or an invalid version", body = crate::models::FieldErrors),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Creates a release in each listed project, as `sentry-cli releases new`
/// does. `url` and `dateReleased` apply only where the release is created.
pub async fn create_org_release(
    _org: web::Path<String>,
    actor: ApiActor,
    pool: web::Data<DbPool>,
    body: web::Json<CreateOrgRelease>,
) -> AppResult<HttpResponse> {
    let input = body.into_inner();
    let Some(refs) = input.projects.filter(|refs| !refs.is_empty()) else {
        return Ok(field_error("projects", "This field is required."));
    };
    let version = input.version.trim().to_string();
    if !is_valid_version(&version) {
        return Err(AppError::Validation(format!(
            "Invalid release version: {:?}",
            input.version
        )));
    }
    let mut project_ids = Vec::new();
    for project in &refs {
        match ProjectService::id_by_slug_or_id(pool.get_ref(), &project.as_key()).await? {
            Some(id)
                if actor
                    .require(Resource::Releases, Action::Create, Some(id))
                    .is_ok() =>
            {
                project_ids.push(id)
            }
            _ => return Ok(invalid_projects()),
        }
    }
    project_ids.sort_unstable();
    project_ids.dedup();

    let mut created_any = false;
    for &project_id in &project_ids {
        let create = CreateRelease {
            version: version.clone(),
            reference: input.reference.clone(),
            url: input.url.clone(),
        };
        let (_, created) = ReleaseService::create(pool.get_ref(), project_id, create).await?;
        if created {
            created_any = true;
            if let Some(date_released) = input.date_released {
                let update = UpdateRelease {
                    reference: None,
                    url: None,
                    date_released: Some(date_released),
                };
                ReleaseService::finalize(pool.get_ref(), project_id, &version, update).await?;
            }
        }
    }
    let release = ReleaseService::org_release(pool.get_ref(), &version, &project_ids)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Release {version} not found")))?;
    let status = if created_any {
        StatusCode::CREATED
    } else {
        StatusCode::from_u16(208).expect("208 is a valid HTTP status code")
    };
    Ok(HttpResponse::build(status).json(release))
}

/// The projects that have `version` and where the actor may do `action` on
/// releases. Projects it cannot reach are left out, never an error.
async fn release_projects(
    pool: &DbPool,
    actor: &ApiActor,
    version: &str,
    action: Action,
) -> AppResult<Vec<i32>> {
    let mut allowed = Vec::new();
    let mut denied = None;
    for id in ReleaseService::projects_with(pool, version).await? {
        match actor.require(Resource::Releases, action, Some(id)) {
            Ok(()) => allowed.push(id),
            // Sees the project but lacks the permission: a 403 if that is
            // all there is, not a 404 that hides the release.
            Err(error) if matches!(error.kind(), AppError::Forbidden(_)) => denied = Some(error),
            Err(_) => {}
        }
    }
    match denied {
        Some(error) if allowed.is_empty() => Err(error),
        _ => Ok(allowed),
    }
}

fn release_not_found(version: &str) -> AppError {
    AppError::NotFound(format!("Release {version} not found"))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    put,
    path = "/api/0/organizations/{org_slug}/releases/{version}/",
    operation_id = "releases.updateInOrganization",
    tag = "Releases",
    params(
        ("org_slug" = String, Path, description = "Organization slug (synthetic: accepted and ignored)"),
        ("version" = String, Path, description = "Release version"),
    ),
    request_body = UpdateOrgRelease,
    responses(
        (status = 200, description = "Release updated", body = crate::models::OrgReleaseResponse),
        (status = 400, description = "Invalid commits, or refs to a repository Rustrak does not have", body = crate::models::FieldErrors),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Can see the project but lacks the releases permission", body = crate::error::ErrorResponse),
        (status = 404, description = "Release not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Updates a release in every project that has it: `ref`, `url` and
/// `dateReleased`, which is what `sentry-cli releases finalize` sends.
pub async fn update_org_release(
    path: web::Path<(String, String)>,
    actor: ApiActor,
    pool: web::Data<DbPool>,
    body: web::Json<UpdateOrgRelease>,
) -> AppResult<HttpResponse> {
    let (_org, version) = path.into_inner();
    let input = body.into_inner();
    let project_ids = release_projects(pool.get_ref(), &actor, &version, Action::Update).await?;
    if project_ids.is_empty() {
        return Err(release_not_found(&version));
    }
    if let Some(commits) = &input.commits {
        if !commits.iter().all(|c| c.is_valid()) {
            return Ok(field_error(
                "commits",
                "Invalid commit: id (at most 64 characters) is required",
            ));
        }
    }
    for &project_id in &project_ids {
        let update = UpdateRelease {
            reference: input.reference.clone(),
            url: input.url.clone(),
            date_released: input.date_released,
        };
        ReleaseService::finalize(pool.get_ref(), project_id, &version, update).await?;
    }
    // Sentry sets the commits, then the refs.
    if let Some(commits) = input.commits.as_deref().filter(|c| !c.is_empty()) {
        ReleaseService::set_commits(pool.get_ref(), &version, &project_ids, commits).await?;
    }
    match input.refs.as_deref() {
        Some([]) => {
            ReleaseService::set_commits(pool.get_ref(), &version, &project_ids, &[]).await?
        }
        // Refs need a repository integration, and Rustrak lists none.
        Some(refs) => {
            let names: Vec<&str> = refs.iter().map(|r| r.repository.as_str()).collect();
            return Ok(field_error(
                "refs",
                format!("Invalid repository names: {}", names.join(",")),
            ));
        }
        None => {}
    }
    let release = ReleaseService::org_release(pool.get_ref(), &version, &project_ids)
        .await?
        .ok_or_else(|| release_not_found(&version))?;
    Ok(HttpResponse::Ok().json(release))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/api/0/organizations/{org_slug}/releases/{version}/",
    operation_id = "releases.deleteInOrganization",
    tag = "Releases",
    params(
        ("org_slug" = String, Path, description = "Organization slug (synthetic: accepted and ignored)"),
        ("version" = String, Path, description = "Release version"),
    ),
    responses(
        (status = 204, description = "Release deleted"),
        (status = 400, description = "An issue started in the release, or it has health data", body = crate::models::Detail),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Can see the project but lacks the releases permission", body = crate::error::ErrorResponse),
        (status = 404, description = "Release not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Deletes a release from every project that has it, unless an issue
/// started in it or it has health data, as Sentry refuses to.
pub async fn delete_org_release(
    path: web::Path<(String, String)>,
    actor: ApiActor,
    pool: web::Data<DbPool>,
) -> AppResult<HttpResponse> {
    let (_org, version) = path.into_inner();
    let project_ids = release_projects(pool.get_ref(), &actor, &version, Action::Delete).await?;
    if project_ids.is_empty() {
        return Err(release_not_found(&version));
    }
    match ReleaseService::delete(pool.get_ref(), &version, &project_ids).await? {
        Ok(()) => Ok(HttpResponse::NoContent().finish()),
        Err(detail) => Ok(HttpResponse::BadRequest().json(crate::models::Detail {
            detail: detail.to_string(),
        })),
    }
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/0/organizations/{org_slug}/releases/{version}/deploys/",
    operation_id = "releases.createDeploy",
    tag = "Releases",
    params(
        ("org_slug" = String, Path, description = "Organization slug (synthetic: accepted and ignored)"),
        ("version" = String, Path, description = "Release version"),
    ),
    request_body = crate::models::CreateDeploy,
    responses(
        (status = 201, description = "Deploy recorded", body = crate::models::DeployResponse),
        (status = 400, description = "Missing or invalid environment or name, or a project the release is not in", body = crate::models::FieldErrors),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Can see the project but lacks the releases permission", body = crate::error::ErrorResponse),
        (status = 404, description = "Release not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Records that a release went out to an environment, in each of its
/// projects or the listed ones: `sentry-cli releases deploys new`.
pub async fn create_deploy(
    path: web::Path<(String, String)>,
    actor: ApiActor,
    pool: web::Data<DbPool>,
    body: web::Json<crate::models::CreateDeploy>,
) -> AppResult<HttpResponse> {
    let (_org, version) = path.into_inner();
    let input = body.into_inner();
    let mut project_ids =
        release_projects(pool.get_ref(), &actor, &version, Action::Update).await?;
    if project_ids.is_empty() {
        return Err(release_not_found(&version));
    }
    let environment = input.environment.clone().unwrap_or_default();
    if !crate::models::is_valid_environment(&environment) {
        return Ok(field_error(
            "environment",
            "Enter a valid environment name.",
        ));
    }
    // Sentry's `DeploySerializer`: `name` at most 64 characters.
    if input
        .name
        .as_deref()
        .is_some_and(|n| n.chars().count() > 64)
    {
        return Ok(field_error(
            "name",
            "Ensure this field has no more than 64 characters.",
        ));
    }
    if let Some(refs) = &input.projects {
        // Sentry requires a non-empty list when one is given.
        if refs.is_empty() {
            return Ok(invalid_projects());
        }
        let mut listed = Vec::new();
        for project in refs {
            match ProjectService::id_by_slug_or_id(pool.get_ref(), &project.as_key()).await? {
                Some(id) if project_ids.contains(&id) => listed.push(id),
                _ => return Ok(invalid_projects()),
            }
        }
        listed.sort_unstable();
        listed.dedup();
        project_ids = listed;
    }
    let deploy =
        ReleaseService::create_deploy(pool.get_ref(), &version, &project_ids, &environment, &input)
            .await?;
    Ok(HttpResponse::Created().json(deploy))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/0/organizations/{org_slug}/releases/{version}/deploys/",
    operation_id = "releases.listDeploys",
    tag = "Releases",
    params(
        ("org_slug" = String, Path, description = "Organization slug (synthetic: accepted and ignored)"),
        ("version" = String, Path, description = "Release version"),
    ),
    responses(
        (status = 200, description = "The latest deploy per project and environment", body = Vec<crate::models::DeployResponse>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Can see the project but lacks the releases permission", body = crate::error::ErrorResponse),
        (status = 404, description = "Release not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// A release's deploys, the latest per project and environment:
/// `sentry-cli releases deploys list`.
pub async fn list_deploys(
    path: web::Path<(String, String)>,
    actor: ApiActor,
    pool: web::Data<DbPool>,
) -> AppResult<HttpResponse> {
    let (_org, version) = path.into_inner();
    let project_ids = release_projects(pool.get_ref(), &actor, &version, Action::Read).await?;
    if project_ids.is_empty() {
        return Err(release_not_found(&version));
    }
    Ok(HttpResponse::Ok()
        .json(ReleaseService::deploys(pool.get_ref(), &version, &project_ids).await?))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/0/organizations/{org_slug}/releases/{version}/commits/",
    operation_id = "releases.listCommits",
    tag = "Releases",
    params(
        ("org_slug" = String, Path, description = "Organization slug (synthetic: accepted and ignored)"),
        ("version" = String, Path, description = "Release version"),
    ),
    responses(
        (status = 200, description = "The release's commits, newest first", body = Vec<crate::models::CommitResponse>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Can see the project but lacks the releases permission", body = crate::error::ErrorResponse),
        (status = 404, description = "Release not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// A release's commits in the order they were set: `sentry-cli releases
/// info --show-commits`.
pub async fn list_release_commits(
    path: web::Path<(String, String)>,
    actor: ApiActor,
    pool: web::Data<DbPool>,
) -> AppResult<HttpResponse> {
    let (_org, version) = path.into_inner();
    let project_ids = release_projects(pool.get_ref(), &actor, &version, Action::Read).await?;
    if project_ids.is_empty() {
        return Err(release_not_found(&version));
    }
    let mut release_ids = Vec::with_capacity(project_ids.len());
    for &project_id in &project_ids {
        release_ids.push(
            ReleaseService::get_by_version(pool.get_ref(), project_id, &version)
                .await?
                .id,
        );
    }
    Ok(HttpResponse::Ok().json(ReleaseService::commits(pool.get_ref(), &release_ids).await?))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/0/organizations/{org_slug}/releases/{version}/previous-with-commits/",
    operation_id = "releases.previousWithCommits",
    tag = "Releases",
    params(
        ("org_slug" = String, Path, description = "Organization slug (synthetic: accepted and ignored)"),
        ("version" = String, Path, description = "Release version"),
    ),
    responses(
        (status = 200, description = "The previous release with commits, or `{}`", body = Object),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Can see the project but lacks the releases permission", body = crate::error::ErrorResponse),
        (status = 404, description = "Release not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// The release before this one that has commits, or `{}`: where `sentry-cli
/// releases set-commits` starts reading git.
pub async fn previous_with_commits(
    path: web::Path<(String, String)>,
    actor: ApiActor,
    pool: web::Data<DbPool>,
) -> AppResult<HttpResponse> {
    let (_org, version) = path.into_inner();
    let project_ids = release_projects(pool.get_ref(), &actor, &version, Action::Read).await?;
    if project_ids.is_empty() {
        return Err(release_not_found(&version));
    }
    Ok(
        match ReleaseService::previous_with_commits(pool.get_ref(), &version, &project_ids).await? {
            Some(release) => HttpResponse::Ok().json(release),
            None => HttpResponse::Ok().json(serde_json::Map::new()),
        },
    )
}

/// sentry-cli reads only the first page of a release list.
const RELEASE_LIST_LIMIT: i64 = 100;

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/0/organizations/{org_slug}/releases/{version}/",
    operation_id = "releases.getInOrganization",
    tag = "Releases",
    params(
        ("org_slug" = String, Path, description = "Organization slug (synthetic: accepted and ignored)"),
        ("version" = String, Path, description = "Release version"),
    ),
    responses(
        (status = 200, description = "The release", body = crate::models::OrgReleaseResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Can see the project but lacks the releases permission", body = crate::error::ErrorResponse),
        (status = 404, description = "Release not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// A release across the projects the caller can read, as `sentry-cli
/// releases info` reads it.
pub async fn get_org_release(
    path: web::Path<(String, String)>,
    actor: ApiActor,
    pool: web::Data<DbPool>,
) -> AppResult<HttpResponse> {
    let (_org, version) = path.into_inner();
    let project_ids = release_projects(pool.get_ref(), &actor, &version, Action::Read).await?;
    let release = ReleaseService::org_release(pool.get_ref(), &version, &project_ids)
        .await?
        .ok_or_else(|| release_not_found(&version))?;
    Ok(HttpResponse::Ok().json(release))
}

/// Each version as an organization-level release, over the projects the
/// caller can read.
// ponytail: one query per listed release, at most RELEASE_LIST_LIMIT of
// them; join them into one query if the list grows.
async fn org_releases(
    pool: &DbPool,
    actor: &ApiActor,
    versions: Vec<String>,
) -> AppResult<Vec<crate::models::OrgReleaseResponse>> {
    let mut releases = Vec::with_capacity(versions.len());
    for version in versions {
        let project_ids = release_projects(pool, actor, &version, Action::Read).await?;
        if let Some(release) = ReleaseService::org_release(pool, &version, &project_ids).await? {
            releases.push(release);
        }
    }
    Ok(releases)
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/0/organizations/{org_slug}/releases/",
    operation_id = "releases.listInOrganization",
    tag = "Releases",
    params(("org_slug" = String, Path, description = "Organization slug (synthetic: accepted and ignored)")),
    responses(
        (status = 200, description = "Releases, newest first", body = Vec<crate::models::OrgReleaseResponse>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// The releases of every project the caller can read, newest first:
/// `sentry-cli releases list` without a project.
pub async fn list_org_releases(
    _org: web::Path<String>,
    actor: ApiActor,
    pool: web::Data<DbPool>,
) -> AppResult<HttpResponse> {
    let readable = match actor.projects_where(Resource::Releases, Action::Read) {
        crate::models::rbac::ProjectScope::All => None,
        crate::models::rbac::ProjectScope::Only(ids) => Some(ids),
    };
    let versions =
        ReleaseService::latest_versions(pool.get_ref(), readable.as_deref(), RELEASE_LIST_LIMIT)
            .await?;
    Ok(HttpResponse::Ok().json(org_releases(pool.get_ref(), &actor, versions).await?))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/0/projects/{org_slug}/{project_slug}/releases/",
    operation_id = "releases.listInProject",
    tag = "Releases",
    params(
        ("org_slug" = String, Path, description = "Organization slug (synthetic: accepted and ignored)"),
        ("project_slug" = String, Path, description = "Project slug"),
    ),
    responses(
        (status = 200, description = "The project's releases, newest first", body = Vec<crate::models::OrgReleaseResponse>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Project not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// One project's releases, newest first: `sentry-cli releases list` with
/// a project.
pub async fn list_project_releases(
    path: web::Path<(String, String)>,
    actor: ApiActor,
    pool: web::Data<DbPool>,
) -> AppResult<HttpResponse> {
    let (_org, project_slug) = path.into_inner();
    let project_id = resolve_project_id_by_slug(pool.get_ref(), &project_slug).await?;
    actor.require(Resource::Releases, Action::Read, Some(project_id))?;
    let versions =
        ReleaseService::latest_versions(pool.get_ref(), Some(&[project_id]), RELEASE_LIST_LIMIT)
            .await?;
    Ok(HttpResponse::Ok().json(org_releases(pool.get_ref(), &actor, versions).await?))
}

#[cfg(feature = "openapi")]
#[derive(OpenApi)]
#[openapi(
    paths(new_issues_for_release, create_release, finalize_release),
    components(schemas(
        IssueResponse,
        CreateRelease,
        UpdateRelease,
        ReleaseResponse,
        ReleaseSummary
    ))
)]
pub struct ReleasesApi;

pub fn configure_project(cfg: &mut web::ServiceConfig) {
    cfg.route("/releases", web::get().to(list_releases))
        .route("/releases/{release}", web::get().to(get_release))
        .route(
            "/releases/{release}/new-issues",
            web::get().to(new_issues_for_release),
        );
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route(
        "/api/0/projects/{org_slug}/{project_slug}/releases/",
        web::post().to(create_release),
    )
    .route(
        "/api/0/projects/{org_slug}/{project_slug}/releases/",
        web::get().to(list_project_releases),
    )
    .route(
        "/api/0/organizations/{org_slug}/releases/",
        web::get().to(list_org_releases),
    )
    .route(
        "/api/0/organizations/{org_slug}/releases/{version}/",
        web::get().to(get_org_release),
    )
    .route(
        "/api/0/organizations/{org_slug}/releases/{version}/",
        web::delete().to(delete_org_release),
    )
    .route(
        "/api/0/organizations/{org_slug}/releases/{version}/deploys/",
        web::post().to(create_deploy),
    )
    .route(
        "/api/0/organizations/{org_slug}/releases/{version}/deploys/",
        web::get().to(list_deploys),
    )
    .route(
        "/api/0/organizations/{org_slug}/releases/{version}/commits/",
        web::get().to(list_release_commits),
    )
    .route(
        "/api/0/organizations/{org_slug}/releases/{version}/previous-with-commits/",
        web::get().to(previous_with_commits),
    )
    .route(
        "/api/0/projects/{org_slug}/{project_slug}/releases/{version}/",
        web::put().to(finalize_release),
    )
    .route(
        "/api/0/organizations/{org_slug}/releases/",
        web::post().to(create_org_release),
    )
    .route(
        "/api/0/organizations/{org_slug}/releases/{version}/",
        web::put().to(update_org_release),
    );
}
