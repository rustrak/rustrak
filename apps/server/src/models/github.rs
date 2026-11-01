//! GitHub integration: the instance's GitHub App, its installations, the
//! repository a project points at, and what links issues to GitHub.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// The instance's one GitHub App, as GitHub returned it from the manifest flow.
/// Secrets never leave the server: [`GitHubAppInfo`] is what the API answers.
#[derive(Debug, Clone, FromRow)]
pub struct GitHubApp {
    pub app_id: i64,
    pub slug: String,
    pub client_id: String,
    pub client_secret: String,
    pub webhook_secret: String,
    pub private_key: String,
    pub html_url: String,
    pub owner_login: String,
    /// `https://github.com`, or a GitHub Enterprise Server address.
    pub web_url: String,
    /// `https://api.github.com`, or `<web_url>/api/v3`.
    pub api_url: String,
    pub last_webhook_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GitHubAppInfo {
    pub app_id: i64,
    pub slug: String,
    pub html_url: String,
    pub owner_login: String,
    /// Where people install the app on an account or organization.
    pub install_url: String,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub last_webhook_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl From<&GitHubApp> for GitHubAppInfo {
    fn from(app: &GitHubApp) -> Self {
        Self {
            app_id: app.app_id,
            slug: app.slug.clone(),
            html_url: app.html_url.clone(),
            owner_login: app.owner_login.clone(),
            install_url: format!("{}/apps/{}/installations/new", app.web_url, app.slug),
            last_webhook_at: app.last_webhook_at,
            created_at: app.created_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, FromRow)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GitHubInstallation {
    pub id: i64,
    pub account_login: String,
    pub account_type: String,
    pub suspended: bool,
    pub created_at: DateTime<Utc>,
}

/// `GET /api/github`: everything the integrations page needs in one answer.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GitHubStatus {
    #[cfg_attr(feature = "openapi", schema(required))]
    pub app: Option<GitHubAppInfo>,
    pub installations: Vec<GitHubInstallation>,
    /// The address GitHub delivers webhooks to.
    pub webhook_url: String,
    /// What every GitHub issue starts with. Until an admin sets it: no
    /// labels, an empty title prefix and an empty body footer.
    pub issue_template: IssueTemplate,
}

/// What a GitHub issue Rustrak opens starts with. A project's template goes
/// on top of the instance's: its labels add to them, and its title prefix
/// and body footer replace theirs unless empty.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct IssueTemplate {
    #[serde(default)]
    pub labels: Vec<String>,
    /// Put before the Rustrak issue's title, e.g. `[Rustrak] `.
    #[serde(default)]
    pub title_prefix: String,
    /// Markdown appended to the body, e.g. `cc @acme/oncall`.
    #[serde(default)]
    pub body_footer: String,
}

impl IssueTemplate {
    /// Parses a stored template; anything unreadable is the empty one.
    pub fn parse(stored: &str) -> Self {
        serde_json::from_str(stored).unwrap_or_default()
    }

    /// `self` (the instance's) with `project`'s on top.
    pub fn with(&self, project: &IssueTemplate) -> Self {
        let mut labels = self.labels.clone();
        for label in &project.labels {
            if !labels.contains(label) {
                labels.push(label.clone());
            }
        }
        let pick = |own: &String, base: &String| {
            if own.is_empty() {
                base.clone()
            } else {
                own.clone()
            }
        };
        Self {
            labels,
            title_prefix: pick(&project.title_prefix, &self.title_prefix),
            body_footer: pick(&project.body_footer, &self.body_footer),
        }
    }
}

/// Maps a path as it appears in a stack trace to the path in the repository.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CodeMapping {
    /// Prefix stripped from the frame's path, e.g. `/app/` or `webpack:///./`.
    pub stack_root: String,
    /// Prefix put in its place, e.g. `src/`. Empty for the repository root.
    pub source_root: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct ProjectRepositoryRow {
    pub project_id: i32,
    pub installation_id: i64,
    pub repo_full_name: String,
    pub default_branch: String,
    pub code_mappings: String,
    pub auto_create_issues: bool,
    pub auto_create_conditions: String,
    pub codeowners: Option<String>,
    pub codeowners_synced_at: Option<DateTime<Utc>>,
    pub issue_template: String,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ProjectRepository {
    pub project_id: i32,
    pub installation_id: i64,
    pub repo_full_name: String,
    pub default_branch: String,
    pub html_url: String,
    pub code_mappings: Vec<CodeMapping>,
    /// Open a GitHub issue for new issues that pass `auto_create_conditions`.
    pub auto_create_issues: bool,
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub auto_create_conditions: serde_json::Value,
    /// How many ownership rules the last CODEOWNERS sync found; `None` before
    /// the first sync.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub codeowners_rules: Option<usize>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub codeowners_synced_at: Option<DateTime<Utc>>,
    /// Goes on top of the instance's template.
    pub issue_template: IssueTemplate,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdateProjectRepository {
    pub installation_id: i64,
    pub repo_full_name: String,
    #[serde(default)]
    pub code_mappings: Option<Vec<CodeMapping>>,
    #[serde(default)]
    pub auto_create_issues: Option<bool>,
    /// The same filters an alert rule takes: `min_level`, `environments`,
    /// `min_events`.
    #[serde(default)]
    #[cfg_attr(feature = "openapi", schema(value_type = Option<Object>))]
    pub auto_create_conditions: Option<serde_json::Value>,
    #[serde(default)]
    pub issue_template: Option<IssueTemplate>,
}

/// A repository an installation can reach.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GitHubRepository {
    pub installation_id: i64,
    pub full_name: String,
    pub default_branch: String,
    pub private: bool,
    pub html_url: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct IssueLink {
    pub id: i32,
    pub issue_id: Uuid,
    pub repo_full_name: String,
    pub number: i64,
    pub title: String,
    pub url: String,
    /// `open` or `closed`, as of the last webhook or sync.
    pub state: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RepoCommit {
    pub repo_full_name: String,
    pub sha: String,
    pub message: String,
    pub author_name: String,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub author_login: Option<String>,
    pub url: String,
    #[serde(skip)]
    pub files: String,
    pub committed_at: DateTime<Utc>,
}

/// `GET .../issues/{id}/github`: the issue's "Code" block in one answer.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct IssueGitHub {
    pub links: Vec<IssueLink>,
    /// Commits that touched the issue's culprit file shortly before it first
    /// appeared, newest first.
    pub suspect_commits: Vec<RepoCommit>,
    /// Owners of the culprit file according to CODEOWNERS.
    pub owners: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateGitHubIssue {
    /// Defaults to the Rustrak issue's title.
    #[serde(default)]
    pub title: Option<String>,
    /// Defaults to a link back and the latest event's exceptions, else its
    /// message, else the issue's `type: value`; `""` sends an empty one.
    #[serde(default)]
    pub body: Option<String>,
    /// Defaults to the templates' labels; `[]` sends none.
    #[serde(default)]
    pub labels: Option<Vec<String>>,
    /// A GitHub login. Defaults to the Rustrak assignee's; `""` sends none.
    #[serde(default)]
    pub assignee: Option<String>,
}

/// `GET .../github/draft`: what creating a GitHub issue sends by default, for
/// the person to edit first.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GitHubIssueDraft {
    pub title: String,
    pub body: String,
    pub labels: Vec<String>,
    /// The Rustrak assignee's GitHub login, if they set one.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub assignee: Option<String>,
    /// The logins GitHub lets the repository's issues be assigned to.
    pub assignees: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct LinkGitHubIssue {
    /// A GitHub issue or pull request URL, or `#123` for the project's
    /// repository.
    pub reference: String,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ManifestRequest {
    /// Create the app under this organization instead of the user's account.
    #[serde(default)]
    pub organization: Option<String>,
}

/// What the dashboard posts to GitHub to create the app.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ManifestResponse {
    /// The form's `action`.
    pub action_url: String,
    /// The form's one field, `manifest`.
    pub manifest: String,
}
