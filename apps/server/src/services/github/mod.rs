//! GitHub integration: storage, the pure rules (code mappings, CODEOWNERS,
//! "Fixes" references), and the two-way sync between linked issues.

pub mod client;
pub mod sync;
pub mod webhook;

use chrono::{DateTime, Duration, Utc};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use uuid::Uuid;

use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::github::{
    CodeMapping, GitHubApp, GitHubInstallation, IssueLink, IssueTemplate, ProjectRepository,
    ProjectRepositoryRow, RepoCommit,
};

pub use client::GitHubClient;

/// Where CODEOWNERS may live, in the order GitHub itself looks.
pub const CODEOWNERS_PATHS: [&str; 3] = [".github/CODEOWNERS", "CODEOWNERS", "docs/CODEOWNERS"];

pub struct GitHubService;

impl GitHubService {
    // ---------------------------------------------------------------- the app

    pub async fn app(pool: &DbPool) -> AppResult<Option<GitHubApp>> {
        Ok(sqlx::query_as::<_, GitHubApp>(
            r#"SELECT app_id, slug, client_id, client_secret, webhook_secret, private_key,
                      html_url, owner_login, web_url, api_url, last_webhook_at, created_at
               FROM github_app WHERE id = 1"#,
        )
        .fetch_optional(pool)
        .await?)
    }

    /// The app, or the error every GitHub action answers without one.
    pub async fn require_app(pool: &DbPool) -> AppResult<GitHubApp> {
        Self::app(pool).await?.ok_or_else(|| {
            AppError::Validation("GitHub is not connected to this Rustrak instance".into())
        })
    }

    /// Stores the app GitHub created from the manifest, replacing any other.
    pub async fn save_app(pool: &DbPool, app: &GitHubApp) -> AppResult<()> {
        let mut tx = pool.begin().await?;
        sqlx::query("DELETE FROM github_installations")
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM github_app")
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            r#"INSERT INTO github_app (id, app_id, slug, client_id, client_secret, webhook_secret,
                                       private_key, html_url, owner_login, web_url, api_url, created_at)
               VALUES (1, $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"#,
        )
        .bind(app.app_id)
        .bind(&app.slug)
        .bind(&app.client_id)
        .bind(&app.client_secret)
        .bind(&app.webhook_secret)
        .bind(&app.private_key)
        .bind(&app.html_url)
        .bind(&app.owner_login)
        .bind(&app.web_url)
        .bind(&app.api_url)
        .bind(Utc::now())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        client::clear_token_cache();
        Ok(())
    }

    /// Forgets the app. Installations and the project repositories that hang
    /// off them go with it; links on issues stay, they are history.
    pub async fn delete_app(pool: &DbPool) -> AppResult<()> {
        let mut tx = pool.begin().await?;
        sqlx::query("DELETE FROM project_repositories")
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM github_installations")
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM github_app")
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        client::clear_token_cache();
        Ok(())
    }

    /// The instance's issue template; the empty one without an app.
    pub async fn issue_template(pool: &DbPool) -> AppResult<IssueTemplate> {
        let stored: Option<String> =
            sqlx::query_scalar("SELECT issue_template FROM github_app WHERE id = 1")
                .fetch_optional(pool)
                .await?;
        Ok(stored
            .as_deref()
            .map(IssueTemplate::parse)
            .unwrap_or_default())
    }

    pub async fn set_issue_template(pool: &DbPool, template: &IssueTemplate) -> AppResult<()> {
        sqlx::query("UPDATE github_app SET issue_template = $1 WHERE id = 1")
            .bind(serde_json::to_string(template).map_err(|e| AppError::Internal(e.to_string()))?)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// The project's template on top of the instance's.
    pub async fn effective_issue_template(
        pool: &DbPool,
        project_id: i32,
    ) -> AppResult<IssueTemplate> {
        let project = Self::project_repository_row(pool, project_id)
            .await?
            .map(|row| IssueTemplate::parse(&row.issue_template))
            .unwrap_or_default();
        Ok(Self::issue_template(pool).await?.with(&project))
    }

    pub async fn touch_webhook(pool: &DbPool) -> AppResult<()> {
        sqlx::query("UPDATE github_app SET last_webhook_at = $1 WHERE id = 1")
            .bind(Utc::now())
            .execute(pool)
            .await?;
        Ok(())
    }

    // -------------------------------------------------------- installations

    pub async fn installations(pool: &DbPool) -> AppResult<Vec<GitHubInstallation>> {
        Ok(sqlx::query_as::<_, GitHubInstallation>(
            "SELECT id, account_login, account_type, suspended, created_at
             FROM github_installations ORDER BY account_login",
        )
        .fetch_all(pool)
        .await?)
    }

    pub async fn upsert_installation(
        pool: &DbPool,
        id: i64,
        account_login: &str,
        account_type: &str,
    ) -> AppResult<()> {
        sqlx::query(
            r#"INSERT INTO github_installations (id, account_login, account_type, created_at)
               VALUES ($1, $2, $3, $4)
               ON CONFLICT (id) DO UPDATE SET account_login = EXCLUDED.account_login,
                                              account_type = EXCLUDED.account_type,
                                              suspended = FALSE"#,
        )
        .bind(id)
        .bind(account_login)
        .bind(account_type)
        .bind(Utc::now())
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn set_installation_suspended(
        pool: &DbPool,
        id: i64,
        suspended: bool,
    ) -> AppResult<()> {
        sqlx::query("UPDATE github_installations SET suspended = $2 WHERE id = $1")
            .bind(id)
            .bind(suspended)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn delete_installation(pool: &DbPool, id: i64) -> AppResult<()> {
        sqlx::query("DELETE FROM project_repositories WHERE installation_id = $1")
            .bind(id)
            .execute(pool)
            .await?;
        sqlx::query("DELETE FROM github_installations WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    // ------------------------------------------------- project repositories

    pub async fn project_repository_row(
        pool: &DbPool,
        project_id: i32,
    ) -> AppResult<Option<ProjectRepositoryRow>> {
        Ok(sqlx::query_as::<_, ProjectRepositoryRow>(
            r#"SELECT project_id, installation_id, repo_full_name, default_branch, code_mappings,
                      auto_create_issues, auto_create_conditions, codeowners, codeowners_synced_at,
                      issue_template
               FROM project_repositories WHERE project_id = $1"#,
        )
        .bind(project_id)
        .fetch_optional(pool)
        .await?)
    }

    pub async fn project_repository(
        pool: &DbPool,
        project_id: i32,
    ) -> AppResult<Option<ProjectRepository>> {
        let Some(row) = Self::project_repository_row(pool, project_id).await? else {
            return Ok(None);
        };
        let web_url = Self::app(pool)
            .await?
            .map(|app| app.web_url)
            .unwrap_or_else(|| "https://github.com".into());
        Ok(Some(ProjectRepository {
            project_id: row.project_id,
            installation_id: row.installation_id,
            html_url: format!("{web_url}/{}", row.repo_full_name),
            code_mappings: serde_json::from_str(&row.code_mappings).unwrap_or_default(),
            codeowners_rules: row.codeowners.as_deref().map(|c| parse_codeowners(c).len()),
            repo_full_name: row.repo_full_name,
            default_branch: row.default_branch,
            auto_create_issues: row.auto_create_issues,
            auto_create_conditions: serde_json::from_str(&row.auto_create_conditions)
                .unwrap_or_else(|_| serde_json::json!({})),
            codeowners_synced_at: row.codeowners_synced_at,
            issue_template: IssueTemplate::parse(&row.issue_template),
        }))
    }

    /// Points a project at a repository. Changing the repository clears the
    /// CODEOWNERS copy, which belonged to the old one.
    #[allow(clippy::too_many_arguments)]
    pub async fn save_project_repository(
        pool: &DbPool,
        project_id: i32,
        installation_id: i64,
        repo_full_name: &str,
        default_branch: &str,
        code_mappings: &[CodeMapping],
        auto_create_issues: bool,
        auto_create_conditions: &serde_json::Value,
    ) -> AppResult<()> {
        let mappings =
            serde_json::to_string(code_mappings).map_err(|e| AppError::Internal(e.to_string()))?;
        sqlx::query(
            r#"INSERT INTO project_repositories
                   (project_id, installation_id, repo_full_name, default_branch, code_mappings,
                    auto_create_issues, auto_create_conditions, created_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
               ON CONFLICT (project_id) DO UPDATE SET
                   installation_id = EXCLUDED.installation_id,
                   default_branch = EXCLUDED.default_branch,
                   code_mappings = EXCLUDED.code_mappings,
                   auto_create_issues = EXCLUDED.auto_create_issues,
                   auto_create_conditions = EXCLUDED.auto_create_conditions,
                   codeowners = CASE WHEN LOWER(project_repositories.repo_full_name) = LOWER(EXCLUDED.repo_full_name)
                                     THEN project_repositories.codeowners END,
                   codeowners_synced_at = CASE WHEN LOWER(project_repositories.repo_full_name) = LOWER(EXCLUDED.repo_full_name)
                                     THEN project_repositories.codeowners_synced_at END,
                   repo_full_name = EXCLUDED.repo_full_name"#,
        )
        .bind(project_id)
        .bind(installation_id)
        .bind(repo_full_name)
        .bind(default_branch)
        .bind(mappings)
        .bind(auto_create_issues)
        .bind(auto_create_conditions.to_string())
        .bind(Utc::now())
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn set_project_issue_template(
        pool: &DbPool,
        project_id: i32,
        template: &IssueTemplate,
    ) -> AppResult<()> {
        sqlx::query("UPDATE project_repositories SET issue_template = $1 WHERE project_id = $2")
            .bind(serde_json::to_string(template).map_err(|e| AppError::Internal(e.to_string()))?)
            .bind(project_id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn delete_project_repository(pool: &DbPool, project_id: i32) -> AppResult<()> {
        sqlx::query("DELETE FROM project_repositories WHERE project_id = $1")
            .bind(project_id)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Stores `repo_full_name`'s CODEOWNERS, unless the project moved to
    /// another repository while GitHub was being asked.
    pub async fn save_codeowners(
        pool: &DbPool,
        project_id: i32,
        repo_full_name: &str,
        content: Option<&str>,
    ) -> AppResult<()> {
        sqlx::query(
            "UPDATE project_repositories SET codeowners = $2, codeowners_synced_at = $3
             WHERE project_id = $1 AND LOWER(repo_full_name) = LOWER($4)",
        )
        .bind(project_id)
        .bind(content)
        .bind(Utc::now())
        .bind(repo_full_name)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Every project that points at a repository, as its id and slug.
    pub async fn projects_for_repo(
        pool: &DbPool,
        repo_full_name: &str,
    ) -> AppResult<Vec<(i32, String)>> {
        Ok(sqlx::query_as(
            r#"SELECT p.id, p.slug FROM project_repositories r
               JOIN projects p ON p.id = r.project_id
               WHERE LOWER(r.repo_full_name) = LOWER($1)"#,
        )
        .bind(repo_full_name)
        .fetch_all(pool)
        .await?)
    }

    // ---------------------------------------------------------------- links

    pub async fn links(pool: &DbPool, issue_id: Uuid) -> AppResult<Vec<IssueLink>> {
        Ok(sqlx::query_as::<_, IssueLink>(
            "SELECT id, issue_id, repo_full_name, number, title, url, state, created_at
             FROM issue_external_links WHERE issue_id = $1 ORDER BY created_at",
        )
        .bind(issue_id)
        .fetch_all(pool)
        .await?)
    }

    pub async fn add_link(
        pool: &DbPool,
        issue_id: Uuid,
        repo_full_name: &str,
        remote: &client::RemoteIssue,
    ) -> AppResult<IssueLink> {
        let existing: Option<IssueLink> = sqlx::query_as(
            "SELECT id, issue_id, repo_full_name, number, title, url, state, created_at
             FROM issue_external_links
             WHERE issue_id = $1 AND LOWER(repo_full_name) = LOWER($2) AND number = $3",
        )
        .bind(issue_id)
        .bind(repo_full_name)
        .bind(remote.number)
        .fetch_optional(pool)
        .await?;
        if existing.is_some() {
            return Err(AppError::Conflict(format!(
                "{repo_full_name}#{} is already linked to this issue",
                remote.number
            )));
        }
        Ok(sqlx::query_as::<_, IssueLink>(
            r#"INSERT INTO issue_external_links (issue_id, repo_full_name, number, title, url, state, created_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7)
               RETURNING id, issue_id, repo_full_name, number, title, url, state, created_at"#,
        )
        .bind(issue_id)
        .bind(repo_full_name)
        .bind(remote.number)
        .bind(&remote.title)
        .bind(&remote.url)
        .bind(&remote.state)
        .bind(Utc::now())
        .fetch_one(pool)
        .await?)
    }

    pub async fn delete_link(pool: &DbPool, issue_id: Uuid, link_id: i32) -> AppResult<()> {
        let result =
            sqlx::query("DELETE FROM issue_external_links WHERE id = $1 AND issue_id = $2")
                .bind(link_id)
                .bind(issue_id)
                .execute(pool)
                .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("Link {link_id} not found")));
        }
        Ok(())
    }

    /// The Rustrak issues linked to one GitHub issue, in projects still
    /// connected to GitHub. A project disconnected since keeps its links as a
    /// record, but GitHub no longer changes it, as Rustrak no longer changes
    /// GitHub for it.
    pub async fn links_to_remote(
        pool: &DbPool,
        repo_full_name: &str,
        number: i64,
    ) -> AppResult<Vec<IssueLink>> {
        Ok(sqlx::query_as::<_, IssueLink>(
            "SELECT l.id, l.issue_id, l.repo_full_name, l.number, l.title, l.url, l.state, l.created_at
             FROM issue_external_links l
             JOIN issues i ON i.id = l.issue_id
             JOIN project_repositories r ON r.project_id = i.project_id
             WHERE LOWER(l.repo_full_name) = LOWER($1) AND l.number = $2",
        )
        .bind(repo_full_name)
        .bind(number)
        .fetch_all(pool)
        .await?)
    }

    pub async fn update_remote(
        pool: &DbPool,
        repo_full_name: &str,
        number: i64,
        title: &str,
        state: &str,
    ) -> AppResult<()> {
        sqlx::query(
            "UPDATE issue_external_links SET title = $3, state = $4
             WHERE LOWER(repo_full_name) = LOWER($1) AND number = $2",
        )
        .bind(repo_full_name)
        .bind(number)
        .bind(title)
        .bind(state)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Records a state Rustrak set on GitHub. The title is left alone: a
    /// rename the webhook stored meanwhile is newer than any Rustrak holds.
    pub async fn set_remote_state(
        pool: &DbPool,
        repo_full_name: &str,
        number: i64,
        state: &str,
    ) -> AppResult<()> {
        sqlx::query(
            "UPDATE issue_external_links SET state = $3
             WHERE LOWER(repo_full_name) = LOWER($1) AND number = $2",
        )
        .bind(repo_full_name)
        .bind(number)
        .bind(state)
        .execute(pool)
        .await?;
        Ok(())
    }

    // -------------------------------------------------------------- commits

    /// Keeps a commit; `false` when it was already here (a redelivered push).
    pub async fn save_commit<'e, E>(executor: E, commit: &RepoCommit) -> AppResult<bool>
    where
        E: sqlx::Executor<'e, Database = crate::db::Db>,
    {
        let result = sqlx::query(
            r#"INSERT INTO repo_commits (repo_full_name, sha, message, author_name, author_login, url, files, committed_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
               ON CONFLICT (repo_full_name, sha) DO NOTHING"#,
        )
        .bind(&commit.repo_full_name)
        .bind(&commit.sha)
        .bind(&commit.message)
        .bind(&commit.author_name)
        .bind(&commit.author_login)
        .bind(&commit.url)
        .bind(&commit.files)
        .bind(commit.committed_at)
        .execute(executor)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Up to three commits that touched `path` in the 30 days before
    /// `before`, newest first. `exact` when a code mapping placed `path`;
    /// otherwise a path that ends the same way counts (see [`same_file`]).
    pub async fn suspect_commits(
        pool: &DbPool,
        repo_full_name: &str,
        path: &str,
        exact: bool,
        before: DateTime<Utc>,
    ) -> AppResult<Vec<RepoCommit>> {
        // ponytail: a LIKE on the file name narrows the window in SQL, Rust
        // matches the path; a commit_files table if one name is everywhere.
        // `files` is JSON, so the name is looked for as JSON writes it, and
        // `!` escapes what LIKE would read as a pattern.
        let name = path.rsplit('/').next().unwrap_or(path);
        let encoded = serde_json::to_string(name).unwrap_or_default();
        let pattern = encoded[1..encoded.len() - 1]
            .replace('!', "!!")
            .replace('%', "!%")
            .replace('_', "!_");
        let commits = sqlx::query_as::<_, RepoCommit>(
            r#"SELECT repo_full_name, sha, message, author_name, author_login, url, files, committed_at
               FROM repo_commits
               WHERE LOWER(repo_full_name) = LOWER($1) AND committed_at <= $2 AND committed_at >= $3
                 AND files LIKE $4 ESCAPE '!'
               ORDER BY committed_at DESC
               LIMIT 1000"#,
        )
        .bind(repo_full_name)
        .bind(before)
        .bind(before - Duration::days(30))
        .bind(format!("%{pattern}%"))
        .fetch_all(pool)
        .await?;
        Ok(commits
            .into_iter()
            .filter(|commit| {
                serde_json::from_str::<Vec<String>>(&commit.files)
                    .unwrap_or_default()
                    .iter()
                    .any(|file| {
                        if exact {
                            file == path
                        } else {
                            same_file(file, path)
                        }
                    })
            })
            .take(3)
            .collect())
    }
}

// ------------------------------------------------------------- pure rules

/// Verifies `X-Hub-Signature-256` (`sha256=<hex>`) in constant time.
/// The event as Sentry writes it into an issue it opens on GitHub: each
/// exception as `Type: value` with its last five in-app frames, or else the
/// message.
pub fn event_text(data: &serde_json::Value) -> String {
    let exceptions = data["exception"]["values"]
        .as_array()
        .or_else(|| data["exception"].as_array());
    let Some(exceptions) = exceptions.filter(|e| !e.is_empty()) else {
        let message = data["logentry"]["formatted"]
            .as_str()
            .or_else(|| data["logentry"]["message"].as_str())
            .or_else(|| data["message"].as_str())
            .or_else(|| data["message"]["formatted"].as_str());
        return message.unwrap_or_default().to_string();
    };
    exceptions
        .iter()
        .map(|exc| {
            let mut out = format!(
                "{}: {}",
                exc["type"].as_str().unwrap_or_default(),
                exc["value"].as_str().unwrap_or_default()
            );
            let all = exc["stacktrace"]["frames"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let in_app: Vec<_> = all.iter().filter(|f| f["in_app"] != false).collect();
            let frames = if in_app.is_empty() {
                all.iter().collect()
            } else {
                in_app
            };
            let shown = &frames[frames.len().saturating_sub(5)..];
            if shown.len() < all.len() {
                out.push_str(&format!(
                    "\n({} additional frame(s) were not displayed)\n...",
                    all.len() - shown.len()
                ));
            }
            for frame in shown {
                out.push('\n');
                out.push_str(&frame_text(frame));
            }
            out
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Sentry's `frames/default.txt`.
fn frame_text(frame: &serde_json::Value) -> String {
    let mut out = match (frame["filename"].as_str(), frame["module"].as_str()) {
        (Some(file), _) => format!("  File \"{file}\""),
        (None, Some(module)) => format!("  Module \"{module}\""),
        _ => "  ?".to_string(),
    };
    if let Some(line) = frame["lineno"].as_i64() {
        out.push_str(&format!(", line {line}"));
    }
    if let Some(function) = frame["function"].as_str() {
        out.push_str(&format!(", in {function}"));
    }
    if let Some(code) = frame["context_line"].as_str() {
        out.push_str(&format!("\n    {}", code.trim()));
    }
    out
}

pub fn verify_signature(secret: &str, body: &[u8], header: &str) -> bool {
    let Some(hex_signature) = header.strip_prefix("sha256=") else {
        return false;
    };
    let Ok(expected) = hex::decode(hex_signature) else {
        return false;
    };
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC takes a key of any size");
    mac.update(body);
    mac.verify_slice(&expected).is_ok()
}

/// The repository path of a stack frame's file. The first code mapping whose
/// stack root prefixes the path wins; without one, the URL schemes bundlers
/// put in front (`webpack:///`, `app:///`) and leading `./` or `/` go.
pub fn repo_path(frame_path: &str, mappings: &[CodeMapping]) -> Option<String> {
    if frame_path.is_empty() {
        return None;
    }
    for mapping in mappings {
        if let Some(rest) = frame_path.strip_prefix(mapping.stack_root.as_str()) {
            let root = mapping.source_root.trim_matches('/');
            let rest = rest.trim_start_matches('/');
            return Some(if root.is_empty() {
                rest.to_string()
            } else {
                format!("{root}/{rest}")
            });
        }
    }
    let mut path = frame_path;
    if let Some(index) = path.find(":///") {
        path = &path[index + 4..];
    } else if let Some(index) = path.find("://") {
        // `app://host/path`: drop the scheme and the host.
        path = path[index + 3..].split_once('/').map_or("", |(_, p)| p);
    }
    let path = path.trim_start_matches("./").trim_start_matches('/');
    (!path.is_empty()).then(|| path.to_string())
}

/// Whether a file a commit touched is the file a frame names. Equal, or one is
/// a whole-segment suffix of the other, so a frame from `/app/src/db.ts`
/// still finds a commit to `src/db.ts` when no mapping says how they relate.
pub fn same_file(commit_file: &str, frame_path: &str) -> bool {
    commit_file == frame_path
        || frame_path.ends_with(&format!("/{commit_file}"))
        || commit_file.ends_with(&format!("/{frame_path}"))
}

/// `Fixes ACME-12`, `closes acme-12, ACME-14`: the short ids a commit message
/// resolves, as `(PROJECT-SLUG-UPPERCASE, number)`. The verbs are Sentry's.
pub fn fixes_references(message: &str) -> Vec<(String, i32)> {
    const VERBS: [&str; 9] = [
        "fix", "fixes", "fixed", "close", "closes", "closed", "resolve", "resolves", "resolved",
    ];
    let words: Vec<&str> = message
        .split(|c: char| c.is_whitespace() || c == ',' || c == ':')
        .filter(|w| !w.is_empty())
        .collect();
    let mut found = Vec::new();
    let mut after_verb = false;
    for word in words {
        let lower = word.to_ascii_lowercase();
        if VERBS.contains(&lower.as_str()) {
            after_verb = true;
            continue;
        }
        if after_verb {
            let word = word.trim_end_matches(['.', ')', ';']);
            match word.rsplit_once('-') {
                Some((slug, number))
                    if !slug.is_empty()
                        && slug
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') =>
                {
                    if let Ok(number) = number.parse::<i32>() {
                        found.push((slug.to_ascii_uppercase(), number));
                        continue;
                    }
                }
                _ => {}
            }
            if lower != "and" {
                after_verb = false;
            }
        }
    }
    found
}

/// Whether `name` is a GitHub `owner/repo`. It goes into API paths, so
/// anything else would point the installation token at another endpoint.
pub fn is_repo_full_name(name: &str) -> bool {
    let part = |p: &str| {
        !p.is_empty()
            && p != "."
            && p != ".."
            && p.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    name.split_once('/')
        .is_some_and(|(owner, repo)| part(owner) && part(repo))
}

/// A GitHub issue reference: a full issue or pull request URL, `owner/repo#12`,
/// or `#12` in `default_repo`.
pub fn parse_issue_reference(reference: &str, default_repo: Option<&str>) -> Option<(String, i64)> {
    let number = |n: &str| n.parse::<i64>().ok().filter(|n| *n > 0);
    let reference = reference.trim();
    if let Some(n) = reference.strip_prefix('#') {
        return Some((default_repo?.to_string(), number(n)?));
    }
    if let Some((repo, n)) = reference.split_once('#') {
        if is_repo_full_name(repo) {
            return Some((repo.to_string(), number(n)?));
        }
    }
    let url = url::Url::parse(reference).ok()?;
    if !matches!(url.scheme(), "https" | "http")
        || !matches!(url.host_str(), Some("github.com" | "www.github.com"))
    {
        return None;
    }
    let segments: Vec<&str> = url.path_segments()?.filter(|s| !s.is_empty()).collect();
    match segments.as_slice() {
        [owner, repo, "issues" | "pull", n, ..] => {
            let repo = format!("{owner}/{repo}");
            is_repo_full_name(&repo).then_some(())?;
            Some((repo, number(n)?))
        }
        _ => None,
    }
}

/// CODEOWNERS as `(pattern, owners)` rules in file order.
pub fn parse_codeowners(content: &str) -> Vec<(String, Vec<String>)> {
    // gitignore rules: `#` starts a comment at the start of a line or after
    // whitespace; `\#` is a literal `#` in a pattern.
    content
        .lines()
        .filter_map(|line| {
            let mut parts = line
                .split_whitespace()
                .take_while(|word| !word.starts_with('#'));
            let pattern = parts.next()?.replace("\\#", "#");
            let owners: Vec<String> = parts.map(str::to_string).collect();
            Some((pattern, owners))
        })
        .collect()
}

/// The owners of `path`: the last rule that matches wins, as on GitHub.
pub fn codeowners_for(content: &str, path: &str) -> Vec<String> {
    parse_codeowners(content)
        .into_iter()
        .rev()
        .find(|(pattern, _)| codeowners_matches(pattern, path))
        .map(|(_, owners)| owners)
        .unwrap_or_default()
}

/// gitignore-style matching, the subset CODEOWNERS files use: `*`, `**`,
/// a leading `/` anchoring to the root, a trailing `/` meaning a directory,
/// and a pattern without a `/` matching at any depth. A pattern that matches
/// a directory owns everything below it, except `dir/*`, which GitHub reads
/// as the directory's direct children only (`dir/*/` is still every folder
/// in `dir`, and all of each one).
pub fn codeowners_matches(pattern: &str, path: &str) -> bool {
    let path_parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    let directory = pattern.ends_with('/');
    let trimmed = pattern.trim_matches('/');
    if trimmed.is_empty() || pattern == "*" {
        return true;
    }
    let anchored = pattern.starts_with('/') || trimmed.contains('/');
    let parts: Vec<&str> = trimmed.split('/').collect();
    let owns_below = directory || parts.last() != Some(&"*");
    let starts = if anchored { 0..1 } else { 0..path_parts.len() };
    starts.into_iter().any(|start| {
        let rest = &path_parts[start..];
        (1..=rest.len()).any(|end| {
            let whole = end == rest.len();
            if (whole && directory) || (!whole && !owns_below) {
                return false;
            }
            glob_segments(&parts, &rest[..end])
        })
    })
}

/// Path segments against pattern segments, `**` standing for any number of
/// them. A table rather than backtracking: the rules come from the
/// repository, and backtracking over many `**` takes exponential time.
fn glob_segments(pattern: &[&str], path: &[&str]) -> bool {
    // `rest[j]`: the pattern from the segment after this one matches `path[j..]`.
    let mut rest = vec![false; path.len() + 1];
    rest[path.len()] = true;
    for part in pattern.iter().rev() {
        let mut here = vec![false; path.len() + 1];
        for j in (0..=path.len()).rev() {
            here[j] = if *part == "**" {
                rest[j] || (j < path.len() && here[j + 1])
            } else {
                j < path.len() && glob_segment(part, path[j]) && rest[j + 1]
            };
        }
        rest = here;
    }
    rest[0]
}

/// `*` and `?` within one path segment.
fn glob_segment(pattern: &str, text: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    let (mut pi, mut ti, mut star, mut mark) = (0, 0, None, 0);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mapping(stack_root: &str, source_root: &str) -> CodeMapping {
        CodeMapping {
            stack_root: stack_root.into(),
            source_root: source_root.into(),
        }
    }

    #[test]
    fn repo_path_applies_the_first_matching_mapping() {
        let mappings = [mapping("/app/", "src/"), mapping("/", "")];
        assert_eq!(
            repo_path("/app/lib/db.ts", &mappings).as_deref(),
            Some("src/lib/db.ts")
        );
        assert_eq!(
            repo_path("/other/x.py", &mappings).as_deref(),
            Some("other/x.py")
        );
    }

    #[test]
    fn repo_path_without_mappings_strips_bundler_schemes() {
        assert_eq!(
            repo_path("webpack:///./src/a.ts", &[]).as_deref(),
            Some("src/a.ts")
        );
        assert_eq!(repo_path("app:///main.js", &[]).as_deref(), Some("main.js"));
        assert_eq!(
            repo_path("app://localhost/js/x.js", &[]).as_deref(),
            Some("js/x.js")
        );
        assert_eq!(repo_path("/srv/app.py", &[]).as_deref(), Some("srv/app.py"));
        assert_eq!(repo_path("", &[]), None);
    }

    #[test]
    fn same_file_matches_whole_segment_suffixes_only() {
        assert!(same_file("src/db.ts", "app/src/db.ts"));
        assert!(same_file("app/src/db.ts", "src/db.ts"));
        assert!(!same_file("b.ts", "src/db.ts"));
    }

    #[test]
    fn fixes_references_reads_sentry_verbs() {
        assert_eq!(
            fixes_references("Refactor pool. Fixes ACME-12, closes my-app-3 and API-9."),
            vec![("ACME".into(), 12), ("MY-APP".into(), 3), ("API".into(), 9)]
        );
        assert!(fixes_references("ACME-12 mentioned but not fixed").is_empty());
        assert!(fixes_references("fix typo").is_empty());
    }

    #[test]
    fn issue_references_take_urls_and_shorthand() {
        assert_eq!(
            parse_issue_reference("https://github.com/o/r/issues/12", None),
            Some(("o/r".into(), 12))
        );
        assert_eq!(
            parse_issue_reference("https://github.com/o/r/pull/7/files", None),
            Some(("o/r".into(), 7))
        );
        assert_eq!(
            parse_issue_reference("o/r#3", None),
            Some(("o/r".into(), 3))
        );
        assert_eq!(
            parse_issue_reference("#4", Some("a/b")),
            Some(("a/b".into(), 4))
        );
        assert_eq!(parse_issue_reference("#4", None), None);
        assert_eq!(parse_issue_reference("nonsense", Some("a/b")), None);
    }

    #[test]
    fn codeowners_hash_is_a_comment_only_where_gitignore_says() {
        let file = "/docs/\\#notes.md @team # who\n#comment @nobody\n";
        assert_eq!(
            parse_codeowners(file),
            vec![("/docs/#notes.md".to_string(), vec!["@team".to_string()])]
        );
        assert_eq!(codeowners_for(file, "docs/#notes.md"), vec!["@team"]);
    }

    #[test]
    fn codeowners_last_matching_rule_wins() {
        let file =
            "# owners\n* @everyone\n/src/api/ @backend\n*.ts @frontend # TS\ndocs/** @docs\n";
        assert_eq!(codeowners_for(file, "README.md"), vec!["@everyone"]);
        assert_eq!(codeowners_for(file, "src/api/users.rs"), vec!["@backend"]);
        assert_eq!(codeowners_for(file, "src/api/users.ts"), vec!["@frontend"]);
        assert_eq!(codeowners_for(file, "docs/a/b.md"), vec!["@docs"]);
        assert!(codeowners_for("", "x").is_empty());
    }

    #[test]
    fn codeowners_patterns() {
        assert!(codeowners_matches("apps/", "web/apps/x.js"));
        assert!(codeowners_matches("/apps/", "apps/x/y.js"));
        assert!(!codeowners_matches("/apps/", "web/apps/x.js"));
        assert!(codeowners_matches("docs/*", "docs/a.md"));
        assert!(!codeowners_matches("docs/*", "docs/a/b.md"));
        // `dir/*/` is every folder in `dir`, and all of each one.
        assert!(codeowners_matches("/apps/*/", "apps/web/src/main.ts"));
        assert!(!codeowners_matches("/apps/*/", "apps/README.md"));
        // `/*` is the root's own files; `*` is every file.
        assert!(codeowners_matches("/*", "README.md"));
        assert!(!codeowners_matches("/*", "src/api/users.ts"));
        assert!(codeowners_matches("*", "src/api/users.ts"));
        assert!(codeowners_matches("/a/**/b/**/c.ts", "a/x/b/y/z/c.ts"));
        assert!(!codeowners_matches("/a/**/b/**/c.ts", "a/x/y/c.ts"));
    }

    #[test]
    fn codeowners_matching_stays_fast_on_many_double_stars() {
        // A rule from the repository, so its shape is not ours to choose.
        let pattern = format!("{}never.ts", "**/".repeat(20));
        let path = format!("{}file.ts", "d/".repeat(20));
        let started = std::time::Instant::now();
        assert!(!codeowners_matches(&pattern, &path));
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        assert!(codeowners_matches("**/logs", "a/b/logs"));
        assert!(codeowners_matches("Makefile", "build/Makefile"));
    }

    #[test]
    fn issue_references_refuse_what_github_would_not_name() {
        for bad in [
            "https://gitlab.com/o/r/issues/12",
            "ftp://github.com/o/r/issues/12",
            "../..#1",
            "/x#1",
            "a/#1",
            "o/r#-5",
            "o/r#0",
            "#0",
            "https://github.com/o/r/issues/-3",
        ] {
            assert_eq!(parse_issue_reference(bad, Some("a/b")), None, "{bad}");
        }
        assert_eq!(
            parse_issue_reference("https://www.github.com/o/r/issues/12", None),
            Some(("o/r".into(), 12))
        );
    }

    #[test]
    fn webhook_signature_is_checked() {
        let secret = crate::auth::token::generate_token();
        let other = crate::auth::token::generate_token();
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(b"{}");
        let header = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));
        assert!(verify_signature(&secret, b"{}", &header));
        assert!(!verify_signature(&other, b"{}", &header));
        assert!(!verify_signature(&secret, b"{}", "sha1=abc"));
    }
}
