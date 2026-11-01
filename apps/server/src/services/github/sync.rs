//! What Rustrak does on GitHub: open and link issues, and mirror status,
//! comments and assignees onto the GitHub issues linked to a Rustrak issue.
//!
//! The mirroring runs off the request path: a slow or unreachable GitHub must
//! never fail or delay the change a person made in Rustrak.

use std::collections::HashSet;
use std::future::Future;
use std::sync::{LazyLock, Mutex};

use chrono::Utc;

use serde_json::json;
use uuid::Uuid;

use super::client::GitHubClient;
use super::{event_text, issue_body, parse_issue_reference, GitHubService};
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::alert::RuleConditions;
use crate::models::github::{CreateGitHubIssue, GitHubApp, GitHubIssueDraft, IssueLink};
use crate::models::{AlertType, Issue, Project};
use crate::services::{AlertService, EventService, IssueSocialService};

/// Runs a sync in the background and logs, rather than surfaces, a failure.
// ponytail: fire-and-forget, no retry; a durable queue if lost syncs matter.
pub fn spawn<F>(what: &'static str, future: F)
where
    F: Future<Output = AppResult<()>> + Send + 'static,
{
    tokio::spawn(async move {
        if let Err(error) = future.await {
            log::warn!("GitHub {what} failed: {error}");
        }
    });
}

/// The app and the installation that act for a project's repository.
async fn project_installation(
    pool: &DbPool,
    project_id: i32,
) -> AppResult<Option<(GitHubApp, i64, String)>> {
    let Some(app) = GitHubService::app(pool).await? else {
        return Ok(None);
    };
    Ok(GitHubService::project_repository_row(pool, project_id)
        .await?
        .map(|repo| (app, repo.installation_id, repo.repo_full_name)))
}

/// What creating a GitHub issue sends unless told otherwise, as Sentry
/// writes it: a link back, then the latest event's exceptions in a code
/// block, with the project's and the instance's templates applied.
pub async fn draft(
    pool: &DbPool,
    project: &Project,
    issue: &Issue,
    dashboard_url: &str,
) -> AppResult<GitHubIssueDraft> {
    let template = GitHubService::effective_issue_template(pool, project.id).await?;
    let url = AlertService::build_issue_url(dashboard_url, project.id, issue.id);
    let header = format!("Rustrak Issue: [{}]({url})", issue.short_id(&project.slug));
    let mut text = EventService::latest_for_issue(pool, issue.id)
        .await?
        .map(|event| event_text(&event.data))
        .unwrap_or_default();
    if text.is_empty() && !issue.calculated_value.is_empty() {
        text = format!("{}: {}", issue.calculated_type, issue.calculated_value);
    }
    let body = issue_body(&header, &text, &template.body_footer);
    let assignee = match (issue.assigned_to, issue.assignee_type.as_deref()) {
        (Some(user_id), None | Some("user")) => github_login(pool, user_id).await?,
        _ => None,
    };
    Ok(GitHubIssueDraft {
        title: format!("{}{}", template.title_prefix, issue.title()),
        body,
        labels: template.labels,
        assignee,
        assignees: Vec::new(),
    })
}

async fn record_link(
    pool: &DbPool,
    issue_id: Uuid,
    user_id: Option<i32>,
    link: &IssueLink,
    action: &str,
) {
    let data = json!({
        "action": action,
        "repo": link.repo_full_name,
        "number": link.number,
        "title": link.title,
        "url": link.url,
    })
    .to_string();
    if let Err(e) =
        IssueSocialService::add_activity(pool, issue_id, user_id, "github_link", &data).await
    {
        log::warn!("Could not record GitHub link activity: {e}");
    }
}

/// Opens a GitHub issue in the project's repository and links it.
pub async fn create_issue(
    pool: &DbPool,
    project: &Project,
    issue: &Issue,
    input: CreateGitHubIssue,
    user_id: Option<i32>,
    dashboard_url: &str,
) -> AppResult<IssueLink> {
    let (app, installation_id, repo) = project_installation(pool, project.id)
        .await?
        .ok_or_else(|| AppError::Validation("This project has no GitHub repository".into()))?;
    let draft = draft(pool, project, issue, dashboard_url).await?;
    let title = input
        .title
        .filter(|t| !t.trim().is_empty())
        .unwrap_or(draft.title);
    // An empty body is one the person cleared, not one left out.
    let body = input.body.unwrap_or(draft.body);
    let labels = input.labels.unwrap_or(draft.labels);
    let assignee = input.assignee.or(draft.assignee);
    let assignee = assignee.as_deref().map(str::trim).filter(|a| !a.is_empty());
    let remote = GitHubClient::new(&app)
        .create_issue(installation_id, &repo, &title, &body, &labels, assignee)
        .await?;
    let link = GitHubService::add_link(pool, issue.id, &repo, &remote).await?;
    record_link(pool, issue.id, user_id, &link, "created").await;
    // A status change made while GitHub was answering found no link to
    // mirror onto. Only that one: an issue opened for an already resolved
    // Rustrak issue stays open, as it was asked for.
    let now = crate::services::IssueService::get_by_id(pool, issue.id).await?;
    if now.status != issue.status {
        spawn("status sync", mirror_status(pool.clone(), issue.id));
    }
    Ok(link)
}

/// Links an existing GitHub issue, and leaves a comment on it pointing back,
/// as Sentry does. Not a pull request: the issues API answers for one too, but
/// its status would not sync back, and closing it from here would close it
/// unmerged.
pub async fn link_issue(
    pool: &DbPool,
    project: &Project,
    issue: &Issue,
    reference: &str,
    user_id: Option<i32>,
    dashboard_url: &str,
) -> AppResult<IssueLink> {
    let (app, installation_id, project_repo) = project_installation(pool, project.id)
        .await?
        .ok_or_else(|| AppError::Validation("This project has no GitHub repository".into()))?;
    let (repo, number) =
        parse_issue_reference(reference, Some(&project_repo)).ok_or_else(|| {
            AppError::Validation(
                "Use a GitHub issue URL, owner/repo#123, or #123 for this project's repository"
                    .into(),
            )
        })?;
    let client = GitHubClient::new(&app);
    let remote = client.issue(installation_id, &repo, number).await?;
    if remote.url.contains("/pull/") {
        return Err(AppError::Validation(format!(
            "{repo}#{number} is a pull request; link a GitHub issue instead"
        )));
    }
    let link = GitHubService::add_link(pool, issue.id, &repo, &remote).await?;
    record_link(pool, issue.id, user_id, &link, "linked").await;
    let url = AlertService::build_issue_url(dashboard_url, project.id, issue.id);
    let note = format!(
        "Linked to Rustrak issue [{}]({url}).",
        issue.short_id(&project.slug)
    );
    if let Err(e) = client
        .add_comment(installation_id, &repo, number, &note)
        .await
    {
        log::warn!("Could not comment on {repo}#{number}: {e}");
    }
    Ok(link)
}

pub async fn unlink_issue(
    pool: &DbPool,
    issue_id: Uuid,
    link_id: i32,
    user_id: Option<i32>,
) -> AppResult<()> {
    let link = GitHubService::links(pool, issue_id)
        .await?
        .into_iter()
        .find(|l| l.id == link_id)
        .ok_or_else(|| AppError::NotFound(format!("Link {link_id} not found")))?;
    GitHubService::delete_link(pool, issue_id, link_id).await?;
    record_link(pool, issue_id, user_id, &link, "unlinked").await;
    Ok(())
}

/// The links to mirror onto, with the client and installation to use.
async fn mirror_targets(
    pool: &DbPool,
    issue_id: Uuid,
) -> AppResult<Option<(GitHubApp, i64, Vec<IssueLink>)>> {
    let links = GitHubService::links(pool, issue_id).await?;
    if links.is_empty() {
        return Ok(None);
    }
    let issue = crate::services::IssueService::get_by_id(pool, issue_id).await?;
    Ok(project_installation(pool, issue.project_id)
        .await?
        .map(|(app, installation_id, _)| (app, installation_id, links)))
}

/// Closes (or reopens) the linked GitHub issues to match the Rustrak issue's
/// status as it is now. Ignored issues leave GitHub alone.
///
/// A change made while GitHub was answering may have been skipped by its own
/// sync, which saw this one's link state from before; so after each pass the
/// status is read again, and another pass runs until it stops moving.
pub async fn mirror_status(pool: DbPool, issue_id: Uuid) -> AppResult<()> {
    loop {
        let issue = crate::services::IssueService::get_by_id(&pool, issue_id).await?;
        let state = match issue.status.as_str() {
            crate::models::STATUS_RESOLVED => "closed",
            crate::models::STATUS_UNRESOLVED => "open",
            _ => return Ok(()),
        };
        let Some((app, installation_id, links)) = mirror_targets(&pool, issue_id).await? else {
            return Ok(());
        };
        let client = GitHubClient::new(&app);
        for link in links.iter().filter(|l| l.state != state) {
            let result = async {
                client
                    .set_issue_state(installation_id, &link.repo_full_name, link.number, state)
                    .await?;
                GitHubService::set_remote_state(&pool, &link.repo_full_name, link.number, state)
                    .await
            }
            .await;
            skip_failed(link, result);
        }
        let now = crate::services::IssueService::get_by_id(&pool, issue_id).await?;
        if now.status == issue.status {
            return Ok(());
        }
    }
}

/// One link GitHub refuses (a repository the installation lost, say) must not
/// keep the others from syncing.
fn skip_failed(link: &IssueLink, result: AppResult<()>) {
    if let Err(e) = result {
        log::warn!(
            "GitHub sync skipped {}#{}: {e}",
            link.repo_full_name,
            link.number
        );
    }
}

/// The user's GitHub login, if they set one.
async fn github_login(pool: &DbPool, user_id: i32) -> AppResult<Option<String>> {
    let login: Option<(Option<String>,)> =
        sqlx::query_as("SELECT github_login FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(pool)
            .await?;
    Ok(login.and_then(|(l,)| l).filter(|l| !l.is_empty()))
}

/// Copies a Rustrak comment onto the linked GitHub issues. The author shows
/// as their GitHub login, never their email: linked repositories can be public.
pub async fn mirror_comment(
    pool: DbPool,
    issue_id: Uuid,
    user_id: Option<i32>,
    text: String,
) -> AppResult<()> {
    let Some((app, installation_id, links)) = mirror_targets(&pool, issue_id).await? else {
        return Ok(());
    };
    let author = match user_id {
        Some(id) => github_login(&pool, id).await?.map(|l| format!("@{l}")),
        None => None,
    }
    .unwrap_or_else(|| "A Rustrak user".to_string());
    let client = GitHubClient::new(&app);
    let body = format!("**{author}** commented in Rustrak:\n\n{text}");
    for link in &links {
        skip_failed(
            link,
            client
                .add_comment(installation_id, &link.repo_full_name, link.number, &body)
                .await,
        );
    }
    Ok(())
}

/// Assigns the linked GitHub issues to the Rustrak assignee's GitHub login,
/// or clears them when the issue is unassigned. An assignee without a GitHub
/// login set leaves GitHub alone, and so does one that is not a user, and so
/// does `except`, the GitHub issue the change came from.
///
/// Reads the assignee as it is now, not as it was when the change was made,
/// and again after each pass, as `mirror_status` does: a slow request for an
/// earlier assignee must not land last.
pub async fn mirror_assignee(
    pool: DbPool,
    issue_id: Uuid,
    except: Option<(String, i64)>,
) -> AppResult<()> {
    let assignee = |issue: &Issue| (issue.assigned_to, issue.assignee_type.clone());
    loop {
        let issue = crate::services::IssueService::get_by_id(&pool, issue_id).await?;
        let user_id = match (issue.assigned_to, issue.assignee_type.as_deref()) {
            // Cleared, whatever type came with it.
            (None, _) => None,
            // No type is how the dashboard assigns a person.
            (Some(user_id), None | Some("user")) => Some(user_id),
            (Some(_), Some(_)) => return Ok(()),
        };
        let logins = match user_id {
            Some(user_id) => match github_login(&pool, user_id).await? {
                Some(login) => vec![login],
                None => return Ok(()),
            },
            None => Vec::new(),
        };
        let Some((app, installation_id, links)) = mirror_targets(&pool, issue_id).await? else {
            return Ok(());
        };
        let client = GitHubClient::new(&app);
        let skipped = |link: &IssueLink| {
            except.as_ref().is_some_and(|(repo, number)| {
                link.repo_full_name.eq_ignore_ascii_case(repo) && link.number == *number
            })
        };
        for link in links.iter().filter(|l| !skipped(l)) {
            skip_failed(
                link,
                client
                    .set_assignees(installation_id, &link.repo_full_name, link.number, &logins)
                    .await,
            );
        }
        let now = crate::services::IssueService::get_by_id(&pool, issue_id).await?;
        if assignee(&now) == assignee(&issue) {
            return Ok(());
        }
    }
}

/// The Rustrak issues an `auto_create` is opening a GitHub issue for right now.
// ponytail: in memory, fine for one process per database (the only deployment).
static OPENING: LazyLock<Mutex<HashSet<Uuid>>> = LazyLock::new(Default::default);

/// Holds an issue in `OPENING` until the run that claimed it ends.
struct Opening(Uuid);

impl Opening {
    fn claim(issue_id: Uuid) -> Option<Self> {
        let claimed = OPENING
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(issue_id);
        // Built only once the lock is released: a dropped `Opening` takes it.
        claimed.then(|| Opening(issue_id))
    }
}

impl Drop for Opening {
    fn drop(&mut self) {
        OPENING
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.0);
    }
}

/// Opens a GitHub issue for a new Rustrak issue when the project asked for it
/// and the event passes the project's filters. An issue that already has one
/// keeps it: a replayed event must not open a second, nor one that arrives
/// while the first run is still waiting for GitHub.
pub async fn auto_create(
    pool: DbPool,
    project: Project,
    issue: Issue,
    level: Option<String>,
    environment: Option<String>,
    dashboard_url: String,
) -> AppResult<()> {
    let Some(repo) = GitHubService::project_repository_row(&pool, project.id).await? else {
        return Ok(());
    };
    let conditions =
        serde_json::from_str::<RuleConditions>(&repo.auto_create_conditions).unwrap_or_default();
    if !repo.auto_create_issues
        || !conditions.matches(
            AlertType::NewIssue,
            level.as_deref(),
            environment.as_deref(),
            issue.digested_event_count,
        )
    {
        return Ok(());
    }
    let Some(_opening) = Opening::claim(issue.id) else {
        return Ok(());
    };
    // The row whose settings passed above, not a second read that could name
    // a repository chosen since.
    let Some(app) = GitHubService::app(&pool).await? else {
        return Ok(());
    };
    let (installation_id, repo) = (repo.installation_id, repo.repo_full_name);
    let client = GitHubClient::new(&app);
    let started = Utc::now() - chrono::Duration::minutes(1);
    // Hidden in the body and new to this run, so only the issue this run
    // opened carries it; a pasted link to the Rustrak issue does not.
    let marker = format!("<!-- rustrak:{} -->", Uuid::new_v4());
    // Only this event reaches the count, so a failure is retried here rather
    // than left for a later event. GitHub has no idempotency key and a failed
    // attempt may have opened the issue anyway (an answer lost), so a retry
    // looks for the marker before opening another; one GitHub did return is
    // kept, and only its link is saved again.
    // ponytail: retried in memory; a restart mid-retry still loses it.
    let mut remote: Option<super::client::RemoteIssue> = None;
    for (attempt, wait) in [1, 2, 0].into_iter().enumerate() {
        if !GitHubService::links(&pool, issue.id).await?.is_empty() {
            return Ok(());
        }
        let opened = async {
            let found = match remote.take() {
                Some(known) => Some(known),
                None if attempt > 0 => client
                    .issues_since(installation_id, &repo, started)
                    .await?
                    .into_iter()
                    .find(|(text, _)| text.contains(&marker))
                    .map(|(_, found)| found),
                None => None,
            };
            let opened = match found {
                Some(opened) => opened,
                None => {
                    // Read only to open one, and again in each attempt, so a
                    // failed read is retried and never blocks linking one
                    // GitHub already opened.
                    let draft = draft(&pool, &project, &issue, &dashboard_url).await?;
                    let body = format!("{}\n\n{marker}", draft.body);
                    client
                        .create_issue(
                            installation_id,
                            &repo,
                            &draft.title,
                            &body,
                            &draft.labels,
                            draft.assignee.as_deref(),
                        )
                        .await?
                }
            };
            remote = Some(opened.clone());
            let link = GitHubService::add_link(&pool, issue.id, &repo, &opened).await?;
            record_link(&pool, issue.id, None, &link, "created").await;
            AppResult::Ok(())
        }
        .await;
        match opened {
            // A status change made while GitHub was answering found no link to
            // mirror onto; the new link catches up with it here.
            Ok(()) => return mirror_status(pool, issue.id).await,
            Err(error) if wait > 0 => {
                log::warn!("GitHub issue auto-create failed, retrying in {wait}s: {error}");
                tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}
