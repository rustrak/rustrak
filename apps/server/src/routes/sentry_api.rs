//! Sentry management API subset (`/api/0/...`): what GitLab's error tracking
//! (`lib/error_tracking/sentry_client` in GitLab) reads, so a GitLab project
//! can list, inspect and resolve Rustrak issues.
//!
//! Every response is shaped after the Sentry serializer named on it (Sentry
//! monolith, `src/sentry/...`). A field is only sent when Rustrak can back it
//! with real data; the rest of Sentry's payload is left out rather than faked.
//!
//! Rustrak has no organizations: one synthetic org stands in for the whole
//! instance, and an `{org_slug}` in a path is accepted and ignored, as in
//! `sourcemaps.rs` and `releases.rs`.

use std::collections::HashMap;

use actix_web::{web, HttpRequest, HttpResponse};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::rbac::{Action, ProjectScope, Resource};
use crate::auth::ApiActor;
use crate::config::Config;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::{Event, Issue, Project, UpdateIssueState};
use crate::pagination::{IssueCategory, IssueFilter, SortOrder};
use crate::services::grouping::exception_values;
use crate::services::scope::Scope;

use crate::services::{EventService, IssueService, IssueSocialService, ProjectService};

/// The synthetic organization's id, slug and name.
const ORG_ID: &str = "1";
const ORG_SLUG: &str = "rustrak";
const ORG_NAME: &str = "Rustrak";

/// Sentry's page size cap (`api/base.py`, `per_page` max 100).
const MAX_PER_PAGE: i64 = 100;

/// `{"id", "name"}` of a status, as Sentry nests it in an organization.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SentryStatus {
    pub id: &'static str,
    pub name: &'static str,
}

/// Subset of `OrganizationSummarySerializer`
/// (`api/serializers/models/organization.py`).
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct SentryOrganization {
    pub id: &'static str,
    pub slug: &'static str,
    pub name: &'static str,
    pub status: SentryStatus,
}

fn organization() -> SentryOrganization {
    SentryOrganization {
        id: ORG_ID,
        slug: ORG_SLUG,
        name: ORG_NAME,
        status: SentryStatus {
            id: "active",
            name: "active",
        },
    }
}

/// Subset of `ProjectWithOrganizationSerializer`
/// (`api/serializers/models/project.py`). Sentry's ids are strings.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct SentryProject {
    pub id: String,
    pub slug: String,
    pub name: String,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub platform: Option<String>,
    pub date_created: DateTime<Utc>,
    /// `STATUS_LABELS[ObjectStatus.ACTIVE]`: Rustrak only lists live projects.
    pub status: &'static str,
    pub organization: SentryOrganization,
}

impl From<&Project> for SentryProject {
    fn from(p: &Project) -> Self {
        Self {
            id: p.id.to_string(),
            slug: p.slug.clone(),
            name: p.name.clone(),
            platform: p.platform.clone(),
            date_created: p.created_at,
            status: "active",
            organization: organization(),
        }
    }
}

/// Query of the project list (`api/base.py`, `get_per_page`).
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct ProjectListQuery {
    /// Page size, at most 100 (default: 100).
    pub per_page: Option<i64>,
    /// A cursor from the `Link` header.
    pub cursor: Option<String>,
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/0/projects/",
    operation_id = "sentry.listProjects",
    tag = "Sentry API",
    params(
        ProjectListQuery
    ),
    responses(
        (status = 200, description = "Projects, newest first; paginated by the `Link` header", body = Vec<SentryProject>),
        (status = 400, description = "Invalid query parameter", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/0/projects/
///
/// The projects the token can see, newest first (Sentry's
/// `ProjectIndexEndpoint`, `core/endpoints/project_index.py`).
pub async fn list_projects(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    query: web::Query<ProjectListQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let limit = query
        .per_page
        .unwrap_or(MAX_PER_PAGE)
        .clamp(1, MAX_PER_PAGE);
    let page = Page::from_cursor(query.cursor.as_deref(), limit)?;
    let (projects, total) = match actor.visible_projects() {
        ProjectScope::All => {
            ProjectService::list_offset(pool.get_ref(), SortOrder::Desc, page.number + 1, page.size)
                .await?
        }
        ProjectScope::Only(ids) => {
            ProjectService::list_offset_for_ids(
                pool.get_ref(),
                &ids,
                SortOrder::Desc,
                page.number + 1,
                page.size,
            )
            .await?
        }
    };

    let body: Vec<SentryProject> = projects.iter().map(SentryProject::from).collect();
    let has_more = page.start + (projects.len() as i64) < total;
    Ok(HttpResponse::Ok()
        .insert_header(("Link", link_header(&req, &config, &page, has_more)))
        .json(body))
}

/// `GroupProjectResponse` (`api/serializers/models/group.py`).
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SentryGroupProject {
    pub id: String,
    pub name: String,
    pub slug: String,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub platform: Option<String>,
}

/// Subset of `GroupSerializer` plus its seen stats
/// (`api/serializers/models/group.py`), and `stats` when the caller asked
/// for a series (`StreamGroupSerializer`, `group_stream.py`).
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct SentryIssue {
    /// The issue's `numeric_id`: Sentry's group ids are integers.
    pub id: String,
    pub short_id: String,
    pub title: String,
    pub culprit: String,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub logger: Option<String>,
    pub level: String,
    pub status: String,
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub status_details: serde_json::Value,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub substatus: Option<String>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub platform: Option<String>,
    pub project: SentryGroupProject,
    /// `error`, or `default` for an issue made of messages.
    #[serde(rename = "type")]
    pub event_type: &'static str,
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub metadata: serde_json::Value,
    pub issue_type: String,
    pub issue_category: String,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub priority: Option<String>,
    /// `str(times_seen)`: Sentry sends the count as a string.
    pub count: String,
    pub user_count: i64,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    /// Links to external issues; Rustrak keeps none, so always empty.
    #[cfg_attr(feature = "openapi", schema(value_type = Vec<Object>))]
    pub annotations: Vec<serde_json::Value>,
    /// `[unix_ts, count]` per bucket, keyed by stats period.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "openapi", schema(value_type = Option<Object>))]
    pub stats: Option<HashMap<String, Vec<[i64; 2]>>>,
}

/// A release as `serialize_releases` (`api/helpers/group_index/index.py`)
/// falls back to it, with the serializer's `shortVersion`, which is the
/// version itself (`api/serializers/models/release.py`).
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct SentryRelease {
    pub version: String,
    pub short_version: String,
}

fn release(version: &str) -> Option<SentryRelease> {
    (!version.is_empty()).then(|| SentryRelease {
        version: version.to_string(),
        short_version: version.to_string(),
    })
}

/// Subset of `GroupDetailsEndpoint`'s response
/// (`issues/endpoints/group_details.py`).
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct SentryIssueDetails {
    #[serde(flatten)]
    pub issue: SentryIssue,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub first_release: Option<SentryRelease>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub last_release: Option<SentryRelease>,
    pub user_report_count: i64,
}

impl SentryIssue {
    fn new(issue: &Issue, project: &Project, user_count: i64) -> Self {
        // Rustrak titles an issue after its main exception, and falls back to
        // "Log Message" / "Unknown" only when there is none, which is when
        // Sentry's event type is `default` rather than `error`
        // (`eventtypes/error.py`, `eventtypes/base.py`).
        let is_error = !matches!(issue.calculated_type.as_str(), "Log Message" | "Unknown");
        let metadata = if is_error {
            let mut m = serde_json::json!({
                "type": issue.calculated_type,
                "value": issue.calculated_value,
            });
            if !issue.last_frame_filename.is_empty() {
                m["filename"] = issue.last_frame_filename.clone().into();
            }
            if !issue.last_frame_function.is_empty() {
                m["function"] = issue.last_frame_function.clone().into();
            }
            m
        } else {
            serde_json::json!({ "title": issue.title() })
        };

        Self {
            id: issue.numeric_id.to_string(),
            short_id: issue.short_id(&project.slug),
            title: issue.title(),
            culprit: issue.culprit.clone(),
            logger: Some(issue.logger.clone()).filter(|l| !l.is_empty()),
            level: issue.level.clone().unwrap_or_else(|| "error".to_string()),
            status: issue.status.clone(),
            status_details: serde_json::from_str(&issue.status_details)
                .unwrap_or_else(|_| serde_json::json!({})),
            substatus: issue.substatus.clone(),
            platform: issue.platform.clone(),
            project: SentryGroupProject {
                id: project.id.to_string(),
                name: project.name.clone(),
                slug: project.slug.clone(),
                platform: project.platform.clone(),
            },
            event_type: if is_error { "error" } else { "default" },
            metadata,
            issue_type: issue.issue_type.clone(),
            issue_category: issue.issue_category.clone(),
            priority: issue.priority.clone(),
            count: issue.digested_event_count.to_string(),
            user_count,
            first_seen: issue.first_seen,
            last_seen: issue.last_seen,
            annotations: Vec::new(),
            stats: None,
        }
    }
}

async fn project_by_slug(pool: &DbPool, slug: &str) -> AppResult<Project> {
    let id: Option<i32> = sqlx::query_scalar("SELECT id FROM projects WHERE slug = $1")
        .bind(slug)
        .fetch_optional(pool)
        .await?;
    match id {
        Some(id) => ProjectService::get_by_id(pool, id).await,
        None => Err(AppError::NotFound(format!("project not found: {slug}"))),
    }
}

/// Query of the issue list (`api/helpers/group_index/index.py`).
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
#[serde(rename_all = "camelCase")]
pub struct IssueListQuery {
    /// Search: `is:unresolved|resolved|ignored` and free text (default: `is:unresolved`).
    pub query: Option<String>,
    /// `date` (default), `new` or `freq`.
    pub sort: Option<String>,
    /// Page size, at most 100 (default: 100).
    pub limit: Option<String>,
    /// A cursor from the `Link` header.
    pub cursor: Option<String>,
    /// `24h` (default), `14d`, or empty for no stats.
    pub stats_period: Option<String>,
    /// Only issues seen in this environment, counted there.
    pub environment: Option<String>,
}

/// What [`parse_search`] reads from a search query.
struct Search {
    filter: IssueFilter,
    category: IssueCategory,
    environment: Option<String>,
    text: Option<String>,
}

/// Splits a search on whitespace, keeping a double-quoted value whole, as
/// Sentry's search grammar reads `environment:"Acme Corp"`.
///
/// As in that grammar (`quoted_value`, `free_text_unquoted` in
/// `api/event_search.py`), a quote starts a value only where a term or a
/// filter value begins, and only when it is closed; `\"` does not close it.
/// Any other quote is a literal character, so a stray one in error text
/// cannot swallow the filters after it.
fn search_tokens(query: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut start = None;
    let mut chars = query.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c.is_whitespace() {
            if let Some(s) = start.take() {
                tokens.push(&query[s..i]);
            }
            continue;
        }
        let token_start = *start.get_or_insert(i);
        let opens_value = token_start == i || query[..i].ends_with(':');
        if c == '"' && opens_value {
            if let Some(close) = closing_quote(&query[i + 1..]).map(|end| i + 1 + end) {
                while chars.next_if(|&(j, _)| j <= close).is_some() {}
            }
        }
    }
    if let Some(s) = start {
        tokens.push(&query[s..]);
    }
    tokens
}

/// Byte offset of the quote that closes a quoted value, skipping `\"`.
fn closing_quote(rest: &str) -> Option<usize> {
    let mut escaped = false;
    for (i, c) in rest.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '"' => return Some(i),
            _ => {}
        }
    }
    None
}

/// Splits a search query into a status filter, an environment and free
/// text. Only `is:` filters on a status and `environment:x` are understood;
/// every other token is free text, matched against the issue's title,
/// culprit and location.
fn parse_search(query: Option<&str>) -> AppResult<Search> {
    // No `query` means `is:unresolved`; an empty one means every issue
    // (`index.py`, `DEFAULT_QUERY`).
    let Some(query) = query else {
        return Ok(Search {
            filter: IssueFilter::Open,
            category: IssueCategory::Error,
            environment: None,
            text: None,
        });
    };
    let mut environment = None;
    let mut filter = IssueFilter::All;
    // Feedback is out of the default search unless asked for by category.
    let mut category = IssueCategory::Error;
    let mut text = Vec::new();
    for token in search_tokens(query) {
        if let Some(value) = token.strip_prefix("environment:") {
            let value = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .unwrap_or(value);
            // One name; Sentry's negation and lists are not supported.
            if value.is_empty() || value.starts_with('[') {
                return Err(AppError::Validation(format!(
                    "Error parsing search query: unsupported {token}"
                )));
            }
            environment = Some(value.to_string());
            continue;
        }
        if token.starts_with("!environment:") {
            return Err(AppError::Validation(format!(
                "Error parsing search query: unsupported {token}"
            )));
        }
        if let Some(value) = token.strip_prefix("issue.category:") {
            category = match value {
                "error" => IssueCategory::Error,
                "feedback" => IssueCategory::Feedback,
                other => {
                    return Err(AppError::Validation(format!(
                        "Error parsing search query: unsupported issue.category:{other}"
                    )))
                }
            };
            continue;
        }
        match token.strip_prefix("is:") {
            Some("unresolved") => filter = IssueFilter::Open,
            Some("resolved") => filter = IssueFilter::Resolved,
            Some("ignored" | "archived") => filter = IssueFilter::Muted,
            Some(other) => {
                return Err(AppError::Validation(format!(
                    "Error parsing search query: unsupported is:{other}"
                )))
            }
            None => text.push(token),
        }
    }
    Ok(Search {
        filter,
        category,
        environment,
        text: Some(text.join(" ")).filter(|t| !t.is_empty()),
    })
}

/// `ORDER BY` for Sentry's issue sorts (`search/snuba/executors.py`).
fn order_clause(sort: Option<&str>) -> AppResult<&'static str> {
    match sort.unwrap_or("date") {
        "date" => Ok("last_seen DESC, id DESC"),
        "new" => Ok("first_seen DESC, id DESC"),
        "freq" => Ok("digested_event_count DESC, id DESC"),
        // Same message as Sentry's `InvalidSearchQuery` (`search/snuba/backend.py`).
        other => Err(AppError::Validation(format!(
            "Sort key '{other}' not supported."
        ))),
    }
}

/// One page of an offset pagination.
struct Page {
    size: i64,
    /// Zero-based.
    number: i64,
    /// Index of the page's first row.
    start: i64,
}

impl Page {
    /// The page a Sentry cursor names, `"{value}:{offset}:{is_prev}"`
    /// (`utils/cursors.py`), or the first page of `limit` rows without one.
    /// For offset pagination `value` is the page size and `offset` the page
    /// number, and the cursor's size wins over `limit` (`api/paginator.py`,
    /// `OffsetPaginator`); `is_prev` is parsed but unused. A cursor Rustrak
    /// could not have issued is a 400.
    fn from_cursor(raw: Option<&str>, limit: i64) -> AppResult<Self> {
        // An empty `cursor` is none, as Sentry's `if request.GET.get("cursor")`.
        let Some(raw) = raw.filter(|r| !r.is_empty()) else {
            return Ok(Self {
                size: limit,
                number: 0,
                start: 0,
            });
        };
        let invalid = || AppError::Validation("Invalid cursor parameter.".to_string());
        let bits: Vec<&str> = raw.split(':').collect();
        let [value, offset, is_prev] = bits[..] else {
            return Err(invalid());
        };
        is_prev.parse::<i64>().map_err(|_| invalid())?;
        let size: i64 = value.parse().map_err(|_| invalid())?;
        let number: i64 = offset.parse().map_err(|_| invalid())?;
        if !(1..=MAX_PER_PAGE).contains(&size) || number < 0 {
            return Err(invalid());
        }
        let start = number
            .checked_add(1)
            .and_then(|_| number.checked_mul(size))
            .ok_or_else(invalid)?;
        Ok(Self {
            size,
            number,
            start,
        })
    }
}

/// Sentry's `Link` header: `previous` and `next`, always both
/// (`api/base.py`, `CURSOR_LINK_HEADER`). Like Sentry's `absolute_uri`, it
/// names the configured public URL, never the request's `Host`; without
/// `PUBLIC_URL` the links are relative paths.
fn link_header(req: &HttpRequest, config: &Config, page: &Page, has_more: bool) -> String {
    let query: Vec<String> = req
        .query_string()
        .split('&')
        .filter(|kv| !kv.is_empty() && !kv.starts_with("cursor="))
        .map(str::to_string)
        .collect();
    let base = format!(
        "{}{}?{}",
        config.public_url.as_deref().unwrap_or_default(),
        req.path(),
        query.join("&")
    );
    let link = |name: &str, cursor: String, results: bool| {
        format!(r#"<{base}&cursor={cursor}>; rel="{name}"; results="{results}"; cursor="{cursor}""#)
    };
    let (size, number) = (page.size, page.number);
    format!(
        "{}, {}",
        link("previous", format!("{size}:{}:1", number - 1), number > 0),
        link("next", format!("{size}:{}:0", number + 1), has_more)
    )
}

/// `(segments, interval)` of a stats period (`group_stream.py`,
/// `STATS_PERIOD_CHOICES`). No `statsPeriod` means "24h", an empty one no
/// stats (`project_group_index.py`).
// ponytail: "auto" and custom ranges are not supported.
fn stats_period(period: Option<&str>) -> AppResult<Option<(&str, i64, i64)>> {
    match period {
        None | Some("24h") => Ok(Some(("24h", 24, 3600))),
        Some("14d") => Ok(Some(("14d", 14, 86_400))),
        Some("") => Ok(None),
        Some(other) => Err(AppError::Validation(format!(
            "Invalid stats_period: {other}"
        ))),
    }
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/0/projects/{org_slug}/{project_slug}/issues/",
    operation_id = "sentry.listIssues",
    tag = "Sentry API",
    params(
        ("org_slug" = String, Path, description = "Organization slug (synthetic — accepted and ignored)"),
        ("project_slug" = String, Path, description = "Project slug"),
        IssueListQuery
    ),
    responses(
        (status = 200, description = "Issues; paginated by the `Link` header", body = Vec<SentryIssue>),
        (status = 400, description = "Invalid query parameter", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/0/projects/{org_slug}/{project_slug}/issues/
///
/// A project's issues, filtered by `query`, sorted by `sort` and paginated
/// by `cursor` (Sentry's `ProjectGroupIndexEndpoint`,
/// `issues/endpoints/project_group_index.py`).
pub async fn list_issues(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    path: web::Path<(String, String)>,
    query: web::Query<IssueListQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (_org_slug, project_slug) = path.into_inner();
    let project = project_by_slug(pool.get_ref(), &project_slug).await?;
    actor.require(Resource::Issues, Action::Read, Some(project.id))?;

    let search = parse_search(query.query.as_deref())?;
    let order = order_clause(query.sort.as_deref())?;
    let limit = match query.limit.as_deref() {
        Some(raw) => raw
            .parse::<i64>()
            .map_err(|_| AppError::Validation("invalid limit".to_string()))?,
        None => MAX_PER_PAGE,
    }
    .clamp(1, MAX_PER_PAGE);
    let page = Page::from_cursor(query.cursor.as_deref(), limit)?;
    let period = stats_period(query.stats_period.as_deref())?;

    // `?environment=` and `environment:` both hold for the issue's events,
    // and an event is in one environment.
    let environment = match (query.environment.as_deref(), search.environment.as_deref()) {
        (Some(param), Some(term)) if param != term => {
            return Ok(HttpResponse::Ok()
                .insert_header(("Link", link_header(&req, &config, &page, false)))
                .json(Vec::<SentryIssue>::new()));
        }
        (param, term) => param.or(term),
    };

    let (issues, total) = IssueService::list_window(
        pool.get_ref(),
        project.id,
        order,
        search.filter,
        search.category,
        page.start,
        page.size,
        search.text.as_deref(),
        Scope::environment(environment),
        &crate::services::issue::IssueFilters::default(),
    )
    .await?;

    let ids: Vec<Uuid> = issues.iter().map(|i| i.id).collect();
    // Users seen in the last 24h, as Rustrak's own issue list counts them;
    // in an environment, its users and series (`group_stream.py:206-229`).
    let list_stats =
        IssueService::list_stats(pool.get_ref(), &ids, Scope::environment(environment)).await?;
    let mut series = match period {
        Some((_, segments, interval)) => {
            IssueService::event_series(pool.get_ref(), &ids, segments, interval, environment)
                .await?
        }
        None => HashMap::new(),
    };

    let body: Vec<SentryIssue> = issues
        .iter()
        .map(|issue| {
            let users = list_stats.get(&issue.id).map_or(0, |s| s.user_count);
            let mut item = SentryIssue::new(issue, &project, users);
            if let (Some((key, ..)), Some(points)) = (period, series.remove(&issue.id)) {
                item.stats = Some(HashMap::from([(key.to_string(), points)]));
            }
            item
        })
        .collect();

    let has_more = page.start + (issues.len() as i64) < total;
    Ok(HttpResponse::Ok()
        .insert_header(("Link", link_header(&req, &config, &page, has_more)))
        .json(body))
}

/// An issue by its Sentry id, once the actor may perform `action` on it.
/// Anything that is not one of Rustrak's numeric ids is a 404, as an unknown
/// group is in Sentry (`issues/endpoints/bases/group.py`).
async fn issue_for(
    pool: &DbPool,
    actor: &ApiActor,
    issue_id: &str,
    resource: Resource,
    action: Action,
) -> AppResult<(Issue, Project)> {
    let numeric_id: i64 = issue_id
        .parse()
        .map_err(|_| AppError::NotFound(format!("Issue {issue_id} not found")))?;
    let issue = IssueService::get_by_numeric_id(pool, numeric_id).await?;
    actor.require(resource, action, Some(issue.project_id))?;
    let project = ProjectService::get_by_id(pool, issue.project_id).await?;
    Ok((issue, project))
}

/// A Sentry issue with its details and both stats series.
async fn issue_details(
    pool: &DbPool,
    issue: &Issue,
    project: &Project,
) -> AppResult<SentryIssueDetails> {
    let ids = [issue.id];
    let users = IssueService::list_stats(pool, &ids, Scope::default())
        .await?
        .remove(&issue.id)
        .map_or(0, |s| s.user_count);
    let mut item = SentryIssue::new(issue, project, users);
    let hourly = IssueService::event_series(pool, &ids, 24, 3600, None).await?;
    let daily = IssueService::event_series(pool, &ids, 30, 86_400, None).await?;
    item.stats = Some(HashMap::from([
        (
            "24h".to_string(),
            hourly.into_values().next().unwrap_or_default(),
        ),
        (
            "30d".to_string(),
            daily.into_values().next().unwrap_or_default(),
        ),
    ]));
    Ok(SentryIssueDetails {
        issue: item,
        first_release: release(&issue.first_release),
        last_release: release(&issue.last_release),
        user_report_count: IssueSocialService::user_report_count(pool, issue.id).await?,
    })
}

/// The issue in a path, named so the organization-scoped routes, whose
/// `{org_slug}` is ignored, share the handler.
#[derive(Debug, Deserialize)]
pub struct IssuePath {
    pub issue_id: String,
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/0/issues/{issue_id}/",
    operation_id = "sentry.getIssue",
    tag = "Sentry API",
    params(
        ("issue_id" = String, Path, description = "Issue id (`numeric_id`)", pattern = "^[0-9]+$")
    ),
    responses(
        (status = 200, description = "Issue details", body = SentryIssueDetails),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/0/issues/{issue_id}/
///
/// One issue with its releases and its 24h and 30d series (Sentry's
/// `GroupDetailsEndpoint`). Also served under `/groups/` and the
/// organization-scoped paths, as Sentry does.
pub async fn get_issue(
    pool: web::Data<DbPool>,
    path: web::Path<IssuePath>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (issue, project) = issue_for(
        pool.get_ref(),
        &actor,
        &path.issue_id,
        Resource::Issues,
        Action::Read,
    )
    .await?;
    Ok(HttpResponse::Ok().json(issue_details(pool.get_ref(), &issue, &project).await?))
}

/// Subset of `EventSerializer` for an error event
/// (`api/serializers/models/event.py`).
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct SentryEvent {
    pub id: String,
    #[serde(rename = "eventID")]
    pub event_id: String,
    #[serde(rename = "groupID")]
    pub group_id: String,
    #[serde(rename = "projectID")]
    pub project_id: String,
    #[cfg_attr(feature = "openapi", schema(value_type = Vec<Object>))]
    pub entries: Vec<serde_json::Value>,
    pub message: String,
    pub title: String,
    pub platform: String,
    #[serde(rename = "type")]
    pub event_type: &'static str,
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub metadata: serde_json::Value,
    pub tags: Vec<SentryTag>,
    pub culprit: String,
    pub date_created: DateTime<Utc>,
    pub date_received: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SentryTag {
    pub key: String,
    pub value: String,
}

/// `Frame.get_api_context`'s `context`: `[lineno, line]` pairs from
/// `pre_context`, `context_line` and `post_context`, or none without a
/// line number or any source (`interfaces/stacktrace.py::get_context`).
fn frame_context(frame: &serde_json::Value) -> serde_json::Value {
    let Some(lineno) = frame.get("lineno").and_then(|l| l.as_i64()) else {
        return serde_json::json!([]);
    };
    let lines = |key: &str| -> Vec<serde_json::Value> {
        frame
            .get(key)
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default()
    };
    let (pre, post) = (lines("pre_context"), lines("post_context"));
    let line = frame
        .get("context_line")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    if line.is_null() && pre.is_empty() && post.is_empty() {
        return serde_json::json!([]);
    }
    let first = lineno - pre.len() as i64;
    (first..)
        .zip(pre.into_iter().chain([line]).chain(post))
        .map(|(at, l)| serde_json::json!([at, l]))
        .collect()
}

/// `Stacktrace.get_api_context` (`interfaces/stacktrace.py`), frames in
/// camelCase and oldest first as stored.
fn stacktrace_entry(stacktrace: &serde_json::Value) -> serde_json::Value {
    let frames: Vec<serde_json::Value> = stacktrace
        .get("frames")
        .and_then(|f| f.as_array())
        .into_iter()
        .flatten()
        .map(|f| {
            let get = |key: &str| f.get(key).cloned().unwrap_or(serde_json::Value::Null);
            serde_json::json!({
                "filename": get("filename"),
                "absPath": get("abs_path"),
                "module": get("module"),
                "package": get("package"),
                "platform": get("platform"),
                "instructionAddr": get("instruction_addr"),
                "symbolAddr": get("symbol_addr"),
                "function": get("function"),
                "rawFunction": get("raw_function"),
                "symbol": get("symbol"),
                "context": frame_context(f),
                "lineNo": get("lineno"),
                "colNo": get("colno"),
                "inApp": get("in_app"),
                "vars": get("vars"),
            })
        })
        .collect();
    let has_system_frames = frames.iter().any(|f| f["inApp"] == true);
    serde_json::json!({
        "frames": frames,
        "framesOmitted": stacktrace.get("frames_omitted"),
        "registers": stacktrace.get("registers"),
        "hasSystemFrames": has_system_frames,
    })
}

/// The event's interfaces as Sentry's `entries`, highest display score
/// first: message (2050), then exception (2000) (`get_entries`).
// ponytail: only the message and exception interfaces; add breadcrumbs,
// request and threads when a client reads them.
fn entries(data: &serde_json::Value) -> Vec<serde_json::Value> {
    let mut entries = Vec::new();

    // Relay stores a plain `message` string as `logentry.formatted`.
    let logentry = match (data.get("logentry"), data.get("message")) {
        (Some(l), _) if l.is_object() => Some(l.clone()),
        (_, Some(m)) if m.is_object() => Some(m.clone()),
        (_, Some(serde_json::Value::String(m))) => Some(serde_json::json!({ "formatted": m })),
        _ => None,
    };
    if let Some(logentry) = logentry {
        let mut entry = serde_json::Map::new();
        for key in ["message", "formatted", "params"] {
            if let Some(v) = logentry.get(key).filter(|v| !v.is_null()) {
                entry.insert(key.to_string(), v.clone());
            }
        }
        entries.push(serde_json::json!({ "type": "message", "data": entry }));
    }

    if let Some(values) = exception_values(data).filter(|v| !v.is_empty()) {
        let values: Vec<serde_json::Value> = values
            .iter()
            .map(|v| {
                let stacktrace = v.get("stacktrace").filter(|s| s.is_object());
                serde_json::json!({
                    "type": v.get("type"),
                    "value": v.get("value"),
                    "mechanism": v.get("mechanism"),
                    "threadId": v.get("thread_id"),
                    "module": v.get("module"),
                    "stacktrace": stacktrace.map(stacktrace_entry),
                    "rawStacktrace": v.get("raw_stacktrace").filter(|s| s.is_object()).map(stacktrace_entry),
                })
            })
            .collect();
        let has_system_frames = values
            .iter()
            .any(|v| v["stacktrace"]["hasSystemFrames"] == true);
        entries.push(serde_json::json!({
            "type": "exception",
            "data": {
                "values": values,
                "hasSystemFrames": has_system_frames,
                "excOmitted": data.get("exception").and_then(|e| e.get("exc_omitted")),
            },
        }));
    }
    entries
}

/// `get_tags_with_meta`: `[{key, value}]` sorted by key, `sentry:` prefixes
/// dropped. SDKs send tags as an object or as pairs.
fn tags(data: &serde_json::Value) -> Vec<SentryTag> {
    let pairs: Vec<(String, String)> = match data.get("tags") {
        Some(serde_json::Value::Object(map)) => map
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    v.as_str().map_or_else(|| v.to_string(), str::to_string),
                )
            })
            .collect(),
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .filter_map(|pair| {
                let k = pair.get(0)?.as_str()?;
                let v = pair.get(1)?;
                Some((
                    k.to_string(),
                    v.as_str().map_or_else(|| v.to_string(), str::to_string),
                ))
            })
            .collect(),
        _ => Vec::new(),
    };
    let mut tags: Vec<SentryTag> = pairs
        .into_iter()
        .map(|(key, value)| SentryTag {
            key: key
                .split_once("sentry:")
                .map_or(key.clone(), |(_, k)| k.to_string()),
            value,
        })
        .collect();
    tags.sort_by(|a, b| a.key.cmp(&b.key));
    tags
}

impl SentryEvent {
    fn new(event: &Event, issue: &SentryIssue) -> Self {
        let data = &event.data;
        // `_get_legacy_message_with_meta`: formatted, then the raw message.
        let message = ["formatted", "message"]
            .iter()
            .find_map(|k| data.get("logentry")?.get(k)?.as_str())
            .or_else(|| data.get("message")?.as_str())
            .unwrap_or_default()
            .to_string();
        let event_id = event.event_id.simple().to_string();
        Self {
            id: event_id.clone(),
            event_id,
            group_id: issue.id.clone(),
            project_id: event.project_id.to_string(),
            entries: entries(data),
            message,
            title: issue.title.clone(),
            platform: event.platform.clone(),
            event_type: issue.event_type,
            metadata: issue.metadata.clone(),
            tags: tags(data),
            culprit: issue.culprit.clone(),
            date_created: event.timestamp,
            date_received: event.ingested_at,
        }
    }
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/0/issues/{issue_id}/events/latest/",
    operation_id = "sentry.getLatestEvent",
    tag = "Sentry API",
    params(
        ("issue_id" = String, Path, description = "Issue id (`numeric_id`)", pattern = "^[0-9]+$")
    ),
    responses(
        (status = 200, description = "The issue's latest event", body = SentryEvent),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/0/issues/{issue_id}/events/latest/
///
/// The issue's most recent event (Sentry's `GroupEventDetailsEndpoint`,
/// `issues/endpoints/group_event_details.py`).
pub async fn latest_event(
    pool: web::Data<DbPool>,
    path: web::Path<String>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (issue, project) = issue_for(
        pool.get_ref(),
        &actor,
        &path,
        Resource::Issues,
        Action::Read,
    )
    .await?;
    let (latest, _) =
        EventService::list_paginated(pool.get_ref(), issue.id, SortOrder::Desc, None, 1, None)
            .await?;
    let summary = latest
        .first()
        .ok_or_else(|| AppError::NotFound("No events found for group".to_string()))?;
    let event = EventService::get_by_id(pool.get_ref(), summary.id).await?;
    let group = SentryIssue::new(&issue, &project, 0);
    Ok(HttpResponse::Ok().json(SentryEvent::new(&event, &group)))
}

/// Body of an issue update: the fields of `GroupValidator`
/// (`api/helpers/group_index/validators/group.py`) Rustrak acts on.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct IssueUpdate {
    /// `resolved`, `unresolved`, `ignored`, `resolvedInNextRelease` or `muted`.
    pub status: Option<String>,
    pub substatus: Option<String>,
}

#[cfg_attr(feature = "openapi", utoipa::path(
    put,
    path = "/api/0/issues/{issue_id}/",
    operation_id = "sentry.updateIssue",
    tag = "Sentry API",
    params(
        ("issue_id" = String, Path, description = "Issue id (`numeric_id`)", pattern = "^[0-9]+$")
    ),
    request_body = IssueUpdate,
    responses(
        (status = 200, description = "Issue updated", body = SentryIssue),
        (status = 400, description = "Invalid query parameter", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// PUT /api/0/issues/{issue_id}/
///
/// Sets an issue's status and returns the issue (Sentry's
/// `GroupDetailsEndpoint.put`). The change goes through Rustrak's own
/// issue update, so it is recorded and mirrored like any other.
pub async fn update_issue(
    pool: web::Data<DbPool>,
    path: web::Path<String>,
    body: web::Json<IssueUpdate>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let (issue, project) = issue_for(
        pool.get_ref(),
        &actor,
        &path,
        Resource::Issues,
        Action::Update,
    )
    .await?;
    let IssueUpdate { status, substatus } = body.into_inner();
    let update = UpdateIssueState {
        // "muted" is Sentry's legacy name for "ignored" (`STATUS_UPDATE_CHOICES`).
        status: status.map(|s| {
            if s == "muted" {
                "ignored".to_string()
            } else {
                s
            }
        }),
        substatus,
        ..Default::default()
    };
    crate::routes::issues::update_issue(
        pool.clone(),
        web::Path::from((issue.project_id, issue.id)),
        web::Json(update),
        actor,
    )
    .await?;

    let issue = IssueService::get_by_id(pool.get_ref(), issue.id).await?;
    let users = IssueService::list_stats(pool.get_ref(), &[issue.id], Scope::default())
        .await?
        .remove(&issue.id)
        .map_or(0, |s| s.user_count);
    Ok(HttpResponse::Ok().json(SentryIssue::new(&issue, &project, users)))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/0/organizations/{org_slug}/repos/",
    operation_id = "sentry.listRepos",
    tag = "Sentry API",
    params(
        ("org_slug" = String, Path, description = "Organization slug (synthetic — accepted and ignored)")
    ),
    responses(
        (status = 200, description = "Connected repositories: always none", body = Vec<Object>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/0/organizations/{org_slug}/repos/
///
/// The code repositories connected to the organization (Sentry's
/// `OrganizationRepositoriesEndpoint`). Rustrak connects none through
/// Sentry's integrations, so the list is always empty.
pub async fn list_repos(_actor: ApiActor) -> AppResult<HttpResponse> {
    Ok(HttpResponse::Ok().json([(); 0]))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/{org_slug}/{project_slug}/issues/{issue_id}",
    operation_id = "sentry.openIssue",
    tag = "Sentry API",
    params(
        ("org_slug" = String, Path, description = "Organization slug (synthetic — accepted and ignored)"),
        ("project_slug" = String, Path, description = "Project slug (ignored: the id names the issue)"),
        ("issue_id" = String, Path, description = "Issue id (`numeric_id`), digits only", pattern = "^[0-9]+$"),
    ),
    responses(
        (status = 302, description = "Browser redirect to the issue in the dashboard, or to the login page"),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security((), ("session_cookie" = []), ("bearer_auth" = [])),
))]
/// GET /{org_slug}/{project_slug}/issues/{issue_id}/
///
/// The issue page GitLab's "View in Sentry" opens: Sentry redirects this
/// legacy path to the issue (`static/app/router/routes.tsx`), Rustrak to its
/// dashboard. A visitor who is not signed in goes to the login page instead,
/// so a stranger learns nothing about which issues exist.
pub async fn issue_link(
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    path: web::Path<(String, String, String)>,
    actor: Option<ApiActor>,
) -> AppResult<HttpResponse> {
    let dashboard = config.dashboard_url();
    let Some(actor) = actor else {
        return Ok(redirect(format!("{dashboard}/login")));
    };
    let (_org_slug, _project_slug, issue_id) = path.into_inner();
    let (issue, project) = issue_for(
        pool.get_ref(),
        &actor,
        &issue_id,
        Resource::Issues,
        Action::Read,
    )
    .await?;
    Ok(redirect(format!(
        "{dashboard}/projects/{}/issues/{}",
        project.id, issue.id
    )))
}

/// Whether `path` is [`issue_link`]'s: `/{org}/{project}/issues/{digits}`,
/// with or without a trailing slash. The session middleware lets it through
/// because the handler authenticates the visitor itself.
pub fn is_issue_link(path: &str) -> bool {
    let parts: Vec<&str> = path.strip_suffix('/').unwrap_or(path).split('/').collect();
    matches!(
        parts[..],
        ["", org, project, "issues", id]
            if !org.is_empty()
                && !project.is_empty()
                && !id.is_empty()
                && id.bytes().all(|b| b.is_ascii_digit())
    )
}

fn redirect(location: String) -> HttpResponse {
    HttpResponse::Found()
        .insert_header(("Location", location))
        .finish()
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/0/organizations/",
    operation_id = "sentry.listOrganizations",
    tag = "Sentry API",
    responses(
        (status = 200, description = "The synthetic organization", body = Vec<SentryOrganization>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/0/organizations/
///
/// The organizations the token belongs to (Sentry's
/// `OrganizationIndexEndpoint`): always the one synthetic org.
pub async fn list_organizations(_actor: ApiActor) -> AppResult<HttpResponse> {
    Ok(HttpResponse::Ok().json([organization()]))
}

/// Body of a user report (`UserReportSerializer` in Sentry's
/// `api/serializers/models/userreport.py`), as the report was received.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SentryUserReport {
    /// The feedback the report became.
    pub id: String,
    /// The error event the report is about.
    #[serde(rename = "eventID")]
    pub event_id: String,
    pub name: String,
    pub email: String,
    pub comments: String,
    #[serde(rename = "dateCreated")]
    pub date_created: DateTime<Utc>,
}

/// Request body of a user report.
#[cfg(feature = "openapi")]
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct SentryUserReportRequest {
    /// The error event the report is about.
    event_id: String,
    name: Option<String>,
    email: Option<String>,
    comments: Option<String>,
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/0/projects/{org_slug}/{project_slug}/user-feedback/",
    operation_id = "sentry.submitUserFeedback",
    tag = "Sentry API",
    params(
        ("org_slug" = String, Path, description = "Organization slug (ignored)"),
        ("project_slug" = String, Path, description = "Project slug"),
    ),
    request_body = SentryUserReportRequest,
    responses(
        (status = 200, description = "The report, or an empty body under DSN auth", body = SentryUserReport),
        (status = 400, description = "Invalid report", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
        (status = 404, description = "Project not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// POST /api/0/projects/{org_slug}/{project_slug}/user-feedback/
///
/// Submit a user report about an error event (Sentry's
/// `ProjectUserReportsEndpoint`, `feedback/endpoints/project_user_reports.py`).
/// Accepts `Authorization: DSN <dsn>` as well as a token; the report becomes a
/// feedback, as the `user_report` envelope item does.
pub async fn submit_user_feedback(
    pool: web::Data<DbPool>,
    processors: web::Data<crate::digest::processors::Processors>,
    path: web::Path<(String, String)>,
    req: HttpRequest,
    body: web::Bytes,
) -> AppResult<HttpResponse> {
    use actix_web::FromRequest as _;
    let (_org_slug, project_slug) = path.into_inner();
    let project = project_by_slug(pool.get_ref(), &project_slug).await?;

    let dsn = req
        .headers()
        .get(actix_web::http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("DSN "));
    if let Some(dsn) = dsn {
        // Sentry's `DSNAuthentication`: the DSN's public key must be this
        // project's.
        if dsn_public_key(dsn) != Some(project.sentry_key) {
            return Err(AppError::Unauthorized("invalid DSN".to_string()));
        }
    } else {
        let actor = ApiActor::extract(&req).await?;
        actor.require(Resource::Issues, Action::Update, Some(project.id))?;
    }

    let shim = crate::routes::ingest::user_report_to_feedback(
        &body,
        None,
        "user_report_sentry_django_endpoint",
    )?;
    crate::routes::ingest::enqueue_digest(
        &processors,
        pool.get_ref(),
        crate::ingest::EventMetadata {
            event_id: shim.feedback_id.simple().to_string(),
            project_id: project.id,
            ingested_at: Utc::now(),
            remote_addr: None,
        },
        &shim.payload,
    )
    .await?;

    // Sentry answers a DSN-authenticated report with an empty body.
    if dsn.is_some() {
        return Ok(HttpResponse::Ok().finish());
    }
    Ok(HttpResponse::Ok().json(SentryUserReport {
        id: shim.feedback_id.simple().to_string(),
        event_id: shim.event_id.simple().to_string(),
        name: shim.report.name,
        email: shim.report.email,
        comments: shim.report.comments,
        date_created: Utc::now(),
    }))
}

/// The public key of a DSN, `<scheme>://<key>[:<secret>]@<host>/<project>`.
fn dsn_public_key(dsn: &str) -> Option<Uuid> {
    let (_, rest) = dsn.trim().split_once("://")?;
    let (userinfo, _) = rest.split_once('@')?;
    let key = userinfo.split(':').next()?;
    Uuid::parse_str(key).ok()
}

/// The aliases Sentry also serves (`/groups/`, organization-scoped issue
/// paths) and the browser redirect are left out of the spec: they add no
/// method a client needs.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/api/0/projects/", web::get().to(list_projects))
        .route("/api/0/organizations/", web::get().to(list_organizations))
        .route(
            "/api/0/organizations/{org_slug}/repos/",
            web::get().to(list_repos),
        )
        .route(
            "/api/0/projects/{org_slug}/{project_slug}/issues/",
            web::get().to(list_issues),
        )
        .route(
            "/api/0/projects/{org_slug}/{project_slug}/groups/",
            web::get().to(list_issues),
        );
    // `user-reports` is Sentry's older name for the same endpoint.
    for name in ["user-feedback", "user-reports"] {
        cfg.route(
            &format!("/api/0/projects/{{org_slug}}/{{project_slug}}/{name}/"),
            web::post().to(submit_user_feedback),
        );
    }
    for prefix in ["/api/0/issues", "/api/0/groups"] {
        cfg.route(&format!("{prefix}/{{issue_id}}/"), web::get().to(get_issue))
            .route(
                &format!("{prefix}/{{issue_id}}/"),
                web::put().to(update_issue),
            )
            .route(
                &format!("{prefix}/{{issue_id}}/events/latest/"),
                web::get().to(latest_event),
            );
    }
    // Digits only, so the dashboard's own `/projects/{id}/issues/{uuid}`
    // pages never match.
    for suffix in ["", "/"] {
        cfg.route(
            &format!("/{{org_slug}}/{{project_slug}}/issues/{{issue_id:\\d+}}{suffix}"),
            web::get().to(issue_link),
        );
    }
    for kind in ["issues", "groups"] {
        cfg.route(
            &format!("/api/0/organizations/{{org_slug}}/{kind}/{{issue_id}}/"),
            web::get().to(get_issue),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::search_tokens;

    /// A stray quote is literal text, as in Sentry's `free_text_unquoted`; it
    /// must not swallow the filters after it.
    #[test]
    fn an_unmatched_quote_does_not_swallow_later_filters() {
        assert_eq!(
            search_tokens(r#"unexpected " is:resolved"#),
            ["unexpected", "\"", "is:resolved"]
        );
    }

    #[test]
    fn a_quoted_filter_value_stays_whole() {
        assert_eq!(
            search_tokens(r#"environment:"Acme Corp" is:resolved"#),
            [r#"environment:"Acme Corp""#, "is:resolved"]
        );
    }

    #[test]
    fn a_quoted_term_stays_whole() {
        assert_eq!(
            search_tokens(r#""bad thing" is:resolved"#),
            [r#""bad thing""#, "is:resolved"]
        );
    }

    /// Only a quote that opens a term or a filter value starts a quoted value.
    #[test]
    fn a_quote_inside_a_word_is_literal() {
        assert_eq!(search_tokens(r#"foo"bar baz""#), [r#"foo"bar"#, r#"baz""#]);
    }

    #[test]
    fn an_escaped_quote_does_not_close_the_value() {
        assert_eq!(
            search_tokens(r#""a \" b" is:resolved"#),
            [r#""a \" b""#, "is:resolved"]
        );
    }
}
