//! GitHub integration routes: connecting the instance's GitHub App, pointing a
//! project at a repository, linking issues, and GitHub's webhooks.

use actix_session::Session;
use actix_web::{web, HttpRequest, HttpResponse};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::ApiActor;
use crate::config::Config;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::github::{
    CodeMapping, CreateGitHubIssue, GitHubApp, GitHubAppInfo, GitHubStatus, IssueGitHub,
    IssueTemplate, LinkGitHubIssue, ManifestRequest, ManifestResponse, UpdateProjectRepository,
};
use crate::models::{Issue, Project};
use crate::services::access::{self, Action};
use crate::services::github::{
    client, codeowners_for, repo_path, sync, GitHubClient, GitHubService, CODEOWNERS_PATHS,
};
use crate::services::{IssueService, ProjectService};
use uuid::Uuid;

const WEB_URL: &str = "https://github.com";
const STATE_KEY: &str = "github_manifest_state";

fn require_admin(actor: &ApiActor) -> AppResult<()> {
    if actor.is_admin() {
        Ok(())
    } else {
        Err(AppError::Forbidden(
            "Admin privileges required to manage GitHub".to_string(),
        ))
    }
}

/// The address GitHub's webhooks reach this server at: `PUBLIC_URL`, else the
/// bind address. Never the request's `Host`, which the client controls.
fn public_base(config: &Config) -> String {
    config
        .public_url
        .clone()
        .unwrap_or_else(|| format!("http://{}:{}", config.host, config.port))
}

fn redirect(location: &str) -> HttpResponse {
    HttpResponse::Found()
        .insert_header(("Location", location))
        .finish()
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/github",
    operation_id = "github.status",
    tag = "GitHub",
    responses(
        (status = 200, description = "GitHub App and installations", body = GitHubStatus),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/github
pub async fn status(
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    require_admin(&actor)?;
    let app = GitHubService::app(pool.get_ref()).await?;
    Ok(HttpResponse::Ok().json(GitHubStatus {
        app: app.as_ref().map(GitHubAppInfo::from),
        installations: GitHubService::installations(pool.get_ref()).await?,
        webhook_url: format!("{}/api/github/webhook", public_base(&config)),
        issue_template: GitHubService::issue_template(pool.get_ref()).await?,
    }))
}

/// Trims what a person typed, and drops empty and repeated labels.
fn clean_template(template: IssueTemplate) -> IssueTemplate {
    let mut labels: Vec<String> = Vec::new();
    for label in template.labels {
        let label = label.trim().to_string();
        if !label.is_empty() && !labels.contains(&label) {
            labels.push(label);
        }
    }
    IssueTemplate {
        labels,
        title_prefix: template.title_prefix,
        body_footer: template.body_footer.trim_end().to_string(),
    }
}

/// The longest body footer a template keeps, well inside GitHub's 65536
/// characters so the event still fits beside it.
const MAX_FOOTER_CHARS: usize = 10_000;

/// A template to save: cleaned, and refused with a footer too long for GitHub.
fn savable_template(template: IssueTemplate) -> AppResult<IssueTemplate> {
    let template = clean_template(template);
    if template.body_footer.chars().count() > MAX_FOOTER_CHARS {
        return Err(AppError::Validation(format!(
            "The body footer is longer than {MAX_FOOTER_CHARS} characters"
        )));
    }
    Ok(template)
}

#[cfg_attr(feature = "openapi", utoipa::path(
    put,
    path = "/api/github/issue-template",
    operation_id = "github.putIssueTemplate",
    tag = "GitHub",
    request_body = IssueTemplate,
    responses(
        (status = 200, description = "Template saved", body = IssueTemplate),
        (status = 400, description = "Invalid request", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// PUT /api/github/issue-template: the labels, title prefix and body footer
/// every GitHub issue starts with, before a project's own.
pub async fn put_issue_template(
    pool: web::Data<DbPool>,
    body: web::Json<IssueTemplate>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    require_admin(&actor)?;
    GitHubService::require_app(pool.get_ref()).await?;
    let template = savable_template(body.into_inner())?;
    GitHubService::set_issue_template(pool.get_ref(), &template).await?;
    Ok(HttpResponse::Ok().json(template))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/api/github",
    operation_id = "github.disconnect",
    tag = "GitHub",
    responses(
        (status = 204, description = "GitHub App removed"),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// DELETE /api/github
pub async fn disconnect(pool: web::Data<DbPool>, actor: ApiActor) -> AppResult<HttpResponse> {
    require_admin(&actor)?;
    GitHubService::delete_app(pool.get_ref()).await?;
    Ok(HttpResponse::NoContent().finish())
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/github/manifest",
    operation_id = "github.manifest",
    tag = "GitHub",
    request_body = ManifestRequest,
    responses(
        (status = 200, description = "Form to post to GitHub", body = ManifestResponse),
        (status = 400, description = "Invalid request", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// POST /api/github/manifest: what the dashboard posts to GitHub so GitHub
/// creates an app already configured for this server.
pub async fn manifest(
    config: web::Data<Config>,
    session: Session,
    body: web::Json<ManifestRequest>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    require_admin(&actor)?;
    let base = public_base(&config);
    // The browser comes back to the dashboard's host, where the session
    // cookie lives; only the webhook goes to PUBLIC_URL.
    let browser = config.dashboard_url();
    let state = crate::auth::token::generate_token();
    session
        .insert(STATE_KEY, &state)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let action_url = match body.organization.as_deref().filter(|o| !o.is_empty()) {
        Some(org) if org.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') => {
            format!("{WEB_URL}/organizations/{org}/settings/apps/new?state={state}")
        }
        Some(_) => return Err(AppError::Validation("Invalid organization name".into())),
        None => format!("{WEB_URL}/settings/apps/new?state={state}"),
    };
    let host = browser.split("://").nth(1).unwrap_or(&browser);
    let manifest = json!({
        // GitHub caps app names at 34 characters; people can rename it on the form.
        "name": format!("Rustrak {host}").chars().take(34).collect::<String>(),
        "url": browser,
        "hook_attributes": { "url": format!("{base}/api/github/webhook") },
        "redirect_url": format!("{browser}/api/github/callback"),
        "setup_url": format!("{browser}/api/github/installed"),
        "setup_on_update": true,
        "public": false,
        "default_permissions": { "issues": "write", "contents": "read", "metadata": "read" },
        "default_events": ["issues", "issue_comment", "push"],
    });
    Ok(HttpResponse::Ok().json(ManifestResponse {
        action_url,
        manifest: manifest.to_string(),
    }))
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    code: String,
    state: String,
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/github/callback",
    operation_id = "github.callback",
    tag = "GitHub",
    params(
        ("code" = String, Query, description = "Manifest code from GitHub"),
        ("state" = String, Query, description = "State issued by the manifest step"),
    ),
    responses(
        (status = 302, description = "Browser redirect to the integrations page"),
        (status = 400, description = "Invalid request", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/github/callback: GitHub sends the browser here after creating the
/// app; the code trades for its credentials.
pub async fn callback(
    pool: web::Data<DbPool>,
    session: Session,
    query: web::Query<CallbackQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    require_admin(&actor)?;
    let expected: Option<String> = session.remove_as(STATE_KEY).and_then(Result::ok);
    if expected.as_deref() != Some(query.state.as_str()) {
        return Err(AppError::Validation(
            "This GitHub callback does not match a setup started here".into(),
        ));
    }
    let api_url =
        std::env::var("GITHUB_API_URL").unwrap_or_else(|_| "https://api.github.com".into());
    let created = client::convert_manifest(&api_url, &query.code).await?;
    let text = |key: &str| {
        created
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    GitHubService::save_app(
        pool.get_ref(),
        &GitHubApp {
            app_id: created
                .get("id")
                .and_then(Value::as_i64)
                .unwrap_or_default(),
            slug: text("slug"),
            client_id: text("client_id"),
            client_secret: text("client_secret"),
            webhook_secret: text("webhook_secret"),
            private_key: text("pem"),
            html_url: text("html_url"),
            owner_login: created
                .pointer("/owner/login")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            web_url: WEB_URL.into(),
            api_url,
            last_webhook_at: None,
            created_at: Utc::now(),
        },
    )
    .await?;
    Ok(redirect("/settings/integrations?github=connected"))
}

#[derive(Deserialize)]
pub struct InstalledQuery {
    installation_id: i64,
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/github/installed",
    operation_id = "github.installed",
    tag = "GitHub",
    params(
        ("installation_id" = i64, Query, description = "GitHub installation ID"),
    ),
    responses(
        (status = 302, description = "Browser redirect to the integrations page"),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/github/installed: GitHub sends the browser here after the app is
/// installed on an account.
pub async fn installed(
    pool: web::Data<DbPool>,
    query: web::Query<InstalledQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    require_admin(&actor)?;
    let app = GitHubService::require_app(pool.get_ref()).await?;
    let (login, account_type) = GitHubClient::new(&app)
        .installation_account(query.installation_id)
        .await?;
    GitHubService::upsert_installation(
        pool.get_ref(),
        query.installation_id,
        &login,
        &account_type,
    )
    .await?;
    Ok(redirect("/settings/integrations?github=installed"))
}

#[derive(Deserialize)]
pub struct RepositoriesQuery {
    project_id: i32,
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/github/repositories",
    operation_id = "github.repositories",
    tag = "GitHub",
    params(
        ("project_id" = i32, Query, description = "Project ID"),
    ),
    responses(
        (status = 200, description = "Repositories the installations reach", body = Vec<crate::models::github::GitHubRepository>),
        (status = 400, description = "GitHub is not connected", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/github/repositories?project_id=: the repositories a project can
/// point at, for whoever may change that project's settings.
pub async fn repositories(
    pool: web::Data<DbPool>,
    query: web::Query<RepositoriesQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    access::require(
        pool.get_ref(),
        actor.is_admin(),
        actor.user_id(),
        query.project_id,
        Action::UpdateProject,
    )
    .await?;
    let app = GitHubService::require_app(pool.get_ref()).await?;
    let client = GitHubClient::new(&app);
    let mut repositories = Vec::new();
    for installation in GitHubService::installations(pool.get_ref()).await? {
        if installation.suspended {
            continue;
        }
        // One installation GitHub no longer has (its `deleted` webhook lost)
        // must not hide every other installation's repositories.
        match client.repositories(installation.id).await {
            Ok(found) => repositories.extend(found),
            Err(e) => log::warn!("Skipping GitHub installation {}: {e}", installation.id),
        }
    }
    Ok(HttpResponse::Ok().json(repositories))
}

async fn require_project(
    pool: &DbPool,
    actor: &ApiActor,
    project_id: i32,
    action: Action,
) -> AppResult<()> {
    access::require(pool, actor.is_admin(), actor.user_id(), project_id, action).await?;
    crate::services::ProjectService::get_by_id(pool, project_id).await?;
    Ok(())
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/github",
    operation_id = "github.getProjectRepository",
    tag = "GitHub",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
    ),
    responses(
        (status = 200, description = "The project's repository, if any", body = Option<crate::models::github::ProjectRepository>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/projects/{project_id}/github
pub async fn get_project_repository(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();
    require_project(pool.get_ref(), &actor, project_id, Action::ViewProject).await?;
    Ok(HttpResponse::Ok()
        .json(GitHubService::project_repository(pool.get_ref(), project_id).await?))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    put,
    path = "/api/projects/{project_id}/github",
    operation_id = "github.putProjectRepository",
    tag = "GitHub",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
    ),
    request_body = UpdateProjectRepository,
    responses(
        (status = 200, description = "Repository saved", body = crate::models::github::ProjectRepository),
        (status = 400, description = "Invalid request", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// PUT /api/projects/{project_id}/github
pub async fn put_project_repository(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    body: web::Json<UpdateProjectRepository>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();
    require_project(pool.get_ref(), &actor, project_id, Action::UpdateProject).await?;
    if !crate::services::github::is_repo_full_name(&body.repo_full_name) {
        return Err(AppError::Validation("Invalid repository name".into()));
    }
    let conditions = body
        .auto_create_conditions
        .clone()
        .unwrap_or_else(|| serde_json::json!({}));
    crate::models::alert::RuleConditions::parse(&conditions)?;
    // Checked before anything is saved, so a refused template saves nothing.
    let template = body
        .issue_template
        .clone()
        .map(savable_template)
        .transpose()?;
    let app = GitHubService::require_app(pool.get_ref()).await?;
    // Asking GitHub proves the installation reaches the repository.
    let repo = GitHubClient::new(&app)
        .repository(body.installation_id, &body.repo_full_name)
        .await?;
    GitHubService::save_project_repository(
        pool.get_ref(),
        project_id,
        body.installation_id,
        &repo.full_name,
        &repo.default_branch,
        body.code_mappings.as_deref().unwrap_or_default(),
        body.auto_create_issues.unwrap_or(false),
        &conditions,
    )
    .await?;
    if let Some(template) = template {
        GitHubService::set_project_issue_template(pool.get_ref(), project_id, &template).await?;
    }
    Ok(HttpResponse::Ok()
        .json(GitHubService::project_repository(pool.get_ref(), project_id).await?))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/api/projects/{project_id}/github",
    operation_id = "github.deleteProjectRepository",
    tag = "GitHub",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
    ),
    responses(
        (status = 204, description = "Repository removed"),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// DELETE /api/projects/{project_id}/github
pub async fn delete_project_repository(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();
    require_project(pool.get_ref(), &actor, project_id, Action::UpdateProject).await?;
    GitHubService::delete_project_repository(pool.get_ref(), project_id).await?;
    Ok(HttpResponse::NoContent().finish())
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/projects/{project_id}/github/codeowners",
    operation_id = "github.syncCodeowners",
    tag = "GitHub",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
    ),
    responses(
        (status = 200, description = "CODEOWNERS synced", body = crate::models::github::ProjectRepository),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// POST /api/projects/{project_id}/github/codeowners: copies the repository's
/// CODEOWNERS so ownership can be answered without asking GitHub each time.
pub async fn sync_codeowners(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();
    require_project(pool.get_ref(), &actor, project_id, Action::UpdateProject).await?;
    let app = GitHubService::require_app(pool.get_ref()).await?;
    let repo = GitHubService::project_repository_row(pool.get_ref(), project_id)
        .await?
        .ok_or_else(|| AppError::NotFound("This project has no GitHub repository".into()))?;
    let client = GitHubClient::new(&app);
    let mut content = None;
    for path in CODEOWNERS_PATHS {
        content = client
            .file(repo.installation_id, &repo.repo_full_name, path)
            .await?;
        if content.is_some() {
            break;
        }
    }
    GitHubService::save_codeowners(
        pool.get_ref(),
        project_id,
        &repo.repo_full_name,
        content.as_deref(),
    )
    .await?;
    Ok(HttpResponse::Ok()
        .json(GitHubService::project_repository(pool.get_ref(), project_id).await?))
}

/// Loads an issue after checking the actor may do `action` in its project.
async fn require_issue(
    pool: &DbPool,
    actor: &ApiActor,
    project_id: i32,
    issue_id: Uuid,
    action: Action,
) -> AppResult<(Project, Issue)> {
    access::require(pool, actor.is_admin(), actor.user_id(), project_id, action).await?;
    let project = ProjectService::get_by_id(pool, project_id).await?;
    let issue = IssueService::get_by_id(pool, issue_id).await?;
    if issue.project_id != project_id {
        return Err(AppError::NotFound(format!("Issue {issue_id} not found")));
    }
    Ok((project, issue))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/issues/{issue_id}/github",
    operation_id = "github.issue",
    tag = "GitHub",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("issue_id" = uuid::Uuid, Path, description = "Issue ID"),
    ),
    responses(
        (status = 200, description = "Links, suspect commits and owners", body = IssueGitHub),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/projects/{project_id}/issues/{issue_id}/github: the issue's links,
/// the commits that may have caused it, and who owns its culprit file.
pub async fn issue_github(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid)>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, issue_id) = path.into_inner();
    let (_, issue) = require_issue(
        pool.get_ref(),
        &actor,
        project_id,
        issue_id,
        Action::ViewProject,
    )
    .await?;
    let mut answer = IssueGitHub {
        links: GitHubService::links(pool.get_ref(), issue_id).await?,
        suspect_commits: Vec::new(),
        owners: Vec::new(),
    };
    if let Some(repo) = GitHubService::project_repository_row(pool.get_ref(), project_id).await? {
        let mappings: Vec<CodeMapping> =
            serde_json::from_str(&repo.code_mappings).unwrap_or_default();
        if let Some(file) = repo_path(&issue.last_frame_filename, &mappings) {
            // A mapping says exactly where the file is; without one, any path
            // that ends the same way may be it.
            let mapped = mappings
                .iter()
                .any(|m| issue.last_frame_filename.starts_with(m.stack_root.as_str()));
            answer.suspect_commits = GitHubService::suspect_commits(
                pool.get_ref(),
                &repo.repo_full_name,
                &file,
                mapped,
                issue.first_seen,
            )
            .await?;
            answer.owners = repo
                .codeowners
                .as_deref()
                .map(|c| codeowners_for(c, &file))
                .unwrap_or_default();
        }
    }
    Ok(HttpResponse::Ok().json(answer))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/issues/{issue_id}/github/draft",
    operation_id = "github.issueDraft",
    tag = "GitHub",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("issue_id" = uuid::Uuid, Path, description = "Issue ID"),
    ),
    responses(
        (status = 200, description = "Title, body and labels to start from", body = crate::models::github::GitHubIssueDraft),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET .../github/draft: what creating a GitHub issue sends by default, with
/// the templates applied, for the person to edit first.
pub async fn github_issue_draft(
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    path: web::Path<(i32, Uuid)>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, issue_id) = path.into_inner();
    let (project, issue) = require_issue(
        pool.get_ref(),
        &actor,
        project_id,
        issue_id,
        Action::MutateIssue,
    )
    .await?;
    let mut draft = sync::draft(pool.get_ref(), &project, &issue, &config.dashboard_url()).await?;
    // Choices only: a GitHub that does not answer leaves a free text field.
    if let (Some(app), Some(repo)) = (
        GitHubService::app(pool.get_ref()).await?,
        GitHubService::project_repository_row(pool.get_ref(), project_id).await?,
    ) {
        match GitHubClient::new(&app)
            .assignees(repo.installation_id, &repo.repo_full_name)
            .await
        {
            Ok(assignees) => draft.assignees = assignees,
            Err(e) => log::warn!("Could not list GitHub assignees: {e}"),
        }
    }
    Ok(HttpResponse::Ok().json(draft))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/projects/{project_id}/issues/{issue_id}/github/issues",
    operation_id = "github.createIssue",
    tag = "GitHub",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("issue_id" = uuid::Uuid, Path, description = "Issue ID"),
    ),
    request_body = CreateGitHubIssue,
    responses(
        (status = 201, description = "GitHub issue created and linked", body = crate::models::github::IssueLink),
        (status = 400, description = "Invalid request", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// POST .../github/issues: opens a GitHub issue and links it.
pub async fn create_github_issue(
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    path: web::Path<(i32, Uuid)>,
    body: web::Json<CreateGitHubIssue>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, issue_id) = path.into_inner();
    let (project, issue) = require_issue(
        pool.get_ref(),
        &actor,
        project_id,
        issue_id,
        Action::MutateIssue,
    )
    .await?;
    let mut input = body.into_inner();
    input.labels = input.labels.map(|labels| {
        clean_template(IssueTemplate {
            labels,
            ..Default::default()
        })
        .labels
    });
    let link = sync::create_issue(
        pool.get_ref(),
        &project,
        &issue,
        input,
        actor.user_id(),
        &config.dashboard_url(),
    )
    .await?;
    Ok(HttpResponse::Created().json(link))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/projects/{project_id}/issues/{issue_id}/github/links",
    operation_id = "github.linkIssue",
    tag = "GitHub",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("issue_id" = uuid::Uuid, Path, description = "Issue ID"),
    ),
    request_body = LinkGitHubIssue,
    responses(
        (status = 201, description = "GitHub issue linked", body = crate::models::github::IssueLink),
        (status = 400, description = "Invalid request", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// POST .../github/links: links an existing GitHub issue or pull request.
pub async fn link_github_issue(
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    path: web::Path<(i32, Uuid)>,
    body: web::Json<LinkGitHubIssue>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, issue_id) = path.into_inner();
    let (project, issue) = require_issue(
        pool.get_ref(),
        &actor,
        project_id,
        issue_id,
        Action::MutateIssue,
    )
    .await?;
    let link = sync::link_issue(
        pool.get_ref(),
        &project,
        &issue,
        &body.reference,
        actor.user_id(),
        &config.dashboard_url(),
    )
    .await?;
    Ok(HttpResponse::Created().json(link))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/api/projects/{project_id}/issues/{issue_id}/github/links/{link_id}",
    operation_id = "github.unlinkIssue",
    tag = "GitHub",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("issue_id" = uuid::Uuid, Path, description = "Issue ID"),
        ("link_id" = i32, Path, description = "Link ID"),
    ),
    responses(
        (status = 204, description = "Link removed"),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// DELETE .../github/links/{link_id}
pub async fn unlink_github_issue(
    pool: web::Data<DbPool>,
    path: web::Path<(i32, Uuid, i32)>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (project_id, issue_id, link_id) = path.into_inner();
    require_issue(
        pool.get_ref(),
        &actor,
        project_id,
        issue_id,
        Action::MutateIssue,
    )
    .await?;
    sync::unlink_issue(pool.get_ref(), issue_id, link_id, actor.user_id()).await?;
    Ok(HttpResponse::NoContent().finish())
}

#[derive(Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GitHubLogin {
    /// The person's GitHub username; `null` clears it.
    #[cfg_attr(feature = "openapi", schema(required))]
    login: Option<String>,
}

async fn my_login(pool: &DbPool, user_id: i32) -> AppResult<HttpResponse> {
    let (login,): (Option<String>,) =
        sqlx::query_as("SELECT github_login FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_one(pool)
            .await?;
    Ok(HttpResponse::Ok().json(json!({ "login": login })))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/github/me",
    operation_id = "github.getMyLogin",
    tag = "GitHub",
    responses(
        (status = 200, description = "Your GitHub username", body = GitHubLogin),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/github/me: the GitHub username assignees sync through.
pub async fn get_my_login(pool: web::Data<DbPool>, actor: ApiActor) -> AppResult<HttpResponse> {
    my_login(pool.get_ref(), actor.require_user()?.id).await
}

#[cfg_attr(feature = "openapi", utoipa::path(
    put,
    path = "/api/github/me",
    operation_id = "github.putMyLogin",
    tag = "GitHub",
    request_body = GitHubLogin,
    responses(
        (status = 200, description = "GitHub username saved", body = GitHubLogin),
        (status = 400, description = "Invalid request", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// PUT /api/github/me
pub async fn put_my_login(
    pool: web::Data<DbPool>,
    body: web::Json<GitHubLogin>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let user_id = actor.require_user()?.id;
    let login = body
        .login
        .as_deref()
        .map(|l| l.trim().trim_start_matches('@'))
        .filter(|l| !l.is_empty());
    if login.is_some_and(|l| !l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')) {
        return Err(AppError::Validation("Invalid GitHub username".into()));
    }
    // Inbound assignments find people by login, so the unique index on it
    // answers whether someone else has it, race included.
    sqlx::query("UPDATE users SET github_login = $2 WHERE id = $1")
        .bind(user_id)
        .bind(login)
        .execute(pool.get_ref())
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(db) if db.is_unique_violation() => AppError::Conflict(format!(
                "GitHub user {} is already someone else's",
                login.unwrap_or_default()
            )),
            e => e.into(),
        })?;
    my_login(pool.get_ref(), user_id).await
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/github/webhook",
    operation_id = "github.webhook",
    tag = "GitHub",
    request_body(content = Object, content_type = "application/json", description = "GitHub event payload, signed in X-Hub-Signature-256"),
    responses(
        (status = 204, description = "Event processed"),
        (status = 400, description = "Invalid request", body = crate::error::ErrorResponse),
        (status = 401, description = "Invalid signature", body = crate::error::ErrorResponse),
        (status = 404, description = "No GitHub App connected", body = crate::error::ErrorResponse),
    ),
    security(()),
))]
/// POST /api/github/webhook: public, and trusted only through the signature
/// GitHub computes with the app's webhook secret.
pub async fn webhook(
    pool: web::Data<DbPool>,
    req: HttpRequest,
    body: web::Bytes,
) -> AppResult<HttpResponse> {
    let app = GitHubService::require_app(pool.get_ref()).await?;
    let header = |name: &str| {
        req.headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string()
    };
    if !crate::services::github::verify_signature(
        &app.webhook_secret,
        &body,
        &header("X-Hub-Signature-256"),
    ) {
        return Err(AppError::Unauthorized("Invalid webhook signature".into()));
    }
    let payload: Value = serde_json::from_slice(&body)
        .map_err(|e| AppError::Validation(format!("Invalid webhook payload: {e}")))?;
    crate::services::github::webhook::handle(
        pool.get_ref(),
        &app,
        &header("X-GitHub-Event"),
        &payload,
    )
    .await?;
    Ok(HttpResponse::NoContent().finish())
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    // Before `issues::configure`: its `/issues` scope would claim these paths.
    cfg.service(
        web::scope("/api/projects/{project_id}/issues/{issue_id}/github")
            .route("", web::get().to(issue_github))
            .route("/draft", web::get().to(github_issue_draft))
            .route("/issues", web::post().to(create_github_issue))
            .route("/links", web::post().to(link_github_issue))
            .route("/links/{link_id}", web::delete().to(unlink_github_issue)),
    );
    cfg.service(
        web::scope("/api/github")
            .route("", web::get().to(status))
            .route("", web::delete().to(disconnect))
            .route("/manifest", web::post().to(manifest))
            .route("/callback", web::get().to(callback))
            .route("/installed", web::get().to(installed))
            .route("/repositories", web::get().to(repositories))
            .route("/issue-template", web::put().to(put_issue_template))
            .service(
                // GitHub sends up to 25 MB; `installation` events list every
                // repository and outgrow actix's 256 KiB default.
                web::resource("/webhook")
                    .app_data(web::PayloadConfig::new(25 * 1024 * 1024))
                    .route(web::post().to(webhook)),
            )
            .route("/me", web::get().to(get_my_login))
            .route("/me", web::put().to(put_my_login)),
    )
    .service(
        web::scope("/api/projects/{project_id}/github")
            .route("", web::get().to(get_project_repository))
            .route("", web::put().to(put_project_repository))
            .route("", web::delete().to(delete_project_repository))
            .route("/codeowners", web::post().to(sync_codeowners)),
    );
}
