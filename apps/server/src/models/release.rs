use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Sentry's own release version cap (`Release.version` is `VARCHAR(250)` upstream,
/// but the documented/enforced limit sentry-cli and the web UI honor is 200).
pub const MAX_VERSION_LEN: usize = 200;

/// Release model — one row per `(project_id, version)`.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Release {
    pub id: i32,
    pub project_id: i32,
    pub version: String,
    #[sqlx(rename = "ref")]
    pub reference: Option<String>,
    pub url: Option<String>,
    pub date_created: DateTime<Utc>,
    pub date_released: Option<DateTime<Utc>>,
}

/// DTO for `POST .../releases/`.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateRelease {
    pub version: String,
    #[serde(default, rename = "ref")]
    pub reference: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
}

/// DTO for `PUT .../releases/{version}/` — generic partial update.
/// Setting `date_released` IS "finalize": there is no separate status flag.
/// Absent fields (including an explicit `null`) leave the stored value
/// unchanged, matching Sentry's own partial-update semantics for this endpoint.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdateRelease {
    #[serde(default, rename = "ref")]
    pub reference: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default, rename = "dateReleased")]
    pub date_released: Option<DateTime<Utc>>,
}

/// A project named the way sentry-cli sends it: an all-digit value as a JSON
/// number, anything else as a string (`api/serialization.rs`).
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(untagged)]
pub enum ProjectRef {
    Id(i64),
    Slug(String),
}

impl ProjectRef {
    pub fn as_key(&self) -> String {
        match self {
            ProjectRef::Id(id) => id.to_string(),
            ProjectRef::Slug(slug) => slug.clone(),
        }
    }
}

/// DTO for `POST /api/0/organizations/{org}/releases/`, Sentry's
/// `ReleaseWithVersionSerializer` plus `projects`. Fields Sentry ignores,
/// such as sentry-cli's `dateStarted`, are ignored here too.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateOrgRelease {
    pub version: String,
    /// Required: the release is created in each of these projects. Read as
    /// optional so a missing list gets Sentry's field error, not a parse one.
    #[serde(default)]
    #[cfg_attr(feature = "openapi", schema(required, nullable = false))]
    pub projects: Option<Vec<ProjectRef>>,
    #[serde(default, rename = "ref")]
    pub reference: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    /// Applied only when the release is created, as Sentry does.
    #[serde(default, rename = "dateReleased")]
    pub date_released: Option<DateTime<Utc>>,
}

/// DTO for `PUT /api/0/organizations/{org}/releases/{version}/`. Sentry
/// applies `ref`, `url` and `dateReleased`; the `projects` sentry-cli sends
/// are ignored, as Sentry's serializer has no such field.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdateOrgRelease {
    #[serde(default, rename = "ref")]
    pub reference: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default, rename = "dateReleased")]
    pub date_released: Option<DateTime<Utc>>,
    /// Replaces the release's commits (`sentry-cli releases set-commits`
    /// reading git), newest first.
    #[serde(default)]
    pub commits: Option<Vec<CommitInput>>,
    /// Commits to fetch from a repository integration. Rustrak has none,
    /// so any is refused as Sentry refuses an unknown repository; an empty
    /// list clears the release's commits.
    #[serde(default)]
    pub refs: Option<Vec<RefInput>>,
}

/// One commit, Sentry's `CommitSerializer`. `patch_set` is accepted and not
/// kept.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CommitInput {
    pub id: String,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub author_name: Option<String>,
    #[serde(default)]
    pub author_email: Option<String>,
    #[serde(default)]
    pub timestamp: Option<DateTime<Utc>>,
}

impl CommitInput {
    /// The limits of Sentry's `CommitSerializer`.
    pub fn is_valid(&self) -> bool {
        let fits = |value: &Option<String>, max: usize| {
            value.as_deref().is_none_or(|v| v.chars().count() <= max)
        };
        !self.id.is_empty()
            && self.id.chars().count() <= 64
            && fits(&self.repository, 64)
            && fits(&self.author_name, 128)
            && fits(&self.author_email, 200)
    }
}

/// A ref to a repository's commit range, Sentry's `ReleaseHeadCommitSerializer`.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RefInput {
    pub repository: String,
    #[serde(default)]
    pub commit: Option<String>,
    #[serde(default, rename = "previousCommit")]
    pub previous_commit: Option<String>,
}

/// A commit's author, as Sentry serializes it.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CommitAuthor {
    #[cfg_attr(feature = "openapi", schema(required))]
    pub name: Option<String>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub email: Option<String>,
}

/// A release's commit, as Sentry's `CommitSerializer` answers it.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CommitResponse {
    pub id: String,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub message: Option<String>,
    #[serde(rename = "dateCreated")]
    #[cfg_attr(feature = "openapi", schema(required))]
    pub date_created: Option<DateTime<Utc>>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub repository: Option<String>,
    pub author: CommitAuthor,
}

/// DTO for `POST .../releases/{version}/deploys/`, Sentry's
/// `DeploySerializer`. `environment` is required.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateDeploy {
    /// Required. Read as optional so a missing one gets Sentry's field error.
    #[serde(default)]
    #[cfg_attr(feature = "openapi", schema(required, nullable = false))]
    pub environment: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default, rename = "dateStarted")]
    pub date_started: Option<DateTime<Utc>>,
    /// Now when absent, as Sentry's `Deploy.date_finished` default.
    #[serde(default, rename = "dateFinished")]
    pub date_finished: Option<DateTime<Utc>>,
    /// Only these of the release's projects; all of them when absent.
    #[serde(default)]
    pub projects: Option<Vec<ProjectRef>>,
}

/// A deploy as Sentry serializes it (`api/serializers/models/deploy.py`).
#[derive(Debug, Serialize, FromRow)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DeployResponse {
    pub id: String,
    pub environment: String,
    #[serde(rename = "dateStarted")]
    #[cfg_attr(feature = "openapi", schema(required))]
    pub date_started: Option<DateTime<Utc>>,
    #[serde(rename = "dateFinished")]
    pub date_finished: DateTime<Utc>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub name: Option<String>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub url: Option<String>,
}

/// Sentry's `Environment.is_valid_name`: at most 64 characters, none of
/// them a line break or `/`. A deploy's environment cannot be blank.
pub fn is_valid_environment(name: &str) -> bool {
    !name.is_empty()
        && name.chars().count() <= 64
        && !name
            .chars()
            .any(|c| matches!(c, '\n' | '\r' | '\x0c' | '/'))
}

/// Sentry's 400 for invalid fields: each field with its messages, such as
/// `{"projects": ["Invalid project ids or slugs"]}`.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct FieldErrors(pub std::collections::BTreeMap<String, Vec<String>>);

impl FieldErrors {
    pub fn one(field: &str, message: impl Into<String>) -> Self {
        Self(std::collections::BTreeMap::from([(
            field.to_string(),
            vec![message.into()],
        )]))
    }
}

/// Sentry's 400 that explains a refusal in one sentence.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Detail {
    pub detail: String,
}

/// One of a release's projects, as sentry-cli reads it.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ReleaseProject {
    pub id: i32,
    pub slug: String,
    pub name: String,
}

/// A release across the projects that have it: Sentry's organization-level
/// release, which sentry-cli's `ReleaseInfo` reads (`version` and
/// `dateCreated` are required).
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct OrgReleaseResponse {
    pub version: String,
    #[serde(rename = "ref")]
    #[cfg_attr(feature = "openapi", schema(required))]
    pub reference: Option<String>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub url: Option<String>,
    #[serde(rename = "dateCreated")]
    pub date_created: DateTime<Utc>,
    #[serde(rename = "dateReleased")]
    #[cfg_attr(feature = "openapi", schema(required))]
    pub date_released: Option<DateTime<Utc>>,
    pub projects: Vec<ReleaseProject>,
    #[serde(rename = "commitCount")]
    pub commit_count: i64,
    /// The newest commit, which `set-commits` starts the next release from.
    #[serde(rename = "lastCommit")]
    #[cfg_attr(feature = "openapi", schema(required))]
    pub last_commit: Option<CommitResponse>,
}

/// API response for a release.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ReleaseResponse {
    pub version: String,
    #[serde(rename = "ref")]
    #[cfg_attr(feature = "openapi", schema(required))]
    pub reference: Option<String>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub url: Option<String>,
    #[serde(rename = "dateCreated")]
    pub date_created: DateTime<Utc>,
    #[serde(rename = "dateReleased")]
    #[cfg_attr(feature = "openapi", schema(required))]
    pub date_released: Option<DateTime<Utc>>,
}

/// One release as the releases page reads it: its record, if CI created
/// one, and what it brought -- events, new issues, environments and session
/// health -- the shape of Sentry's release list with `health=1`.
///
/// Counts and rates cover the asked window and environment; the dates do not,
/// so a release keeps its age however narrow the window.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ReleaseSummary {
    pub version: String,
    /// Whether it has a release record (from CI or from an error event), so
    /// an issue can be resolved in it. One known only from sessions has none.
    pub registered: bool,
    /// The record's, else the first time the release was seen.
    pub date_created: DateTime<Utc>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub date_released: Option<DateTime<Utc>>,
    #[serde(rename = "ref")]
    #[cfg_attr(feature = "openapi", schema(required))]
    pub reference: Option<String>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub url: Option<String>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub first_event: Option<DateTime<Utc>>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub last_event: Option<DateTime<Utc>>,
    pub event_count: i64,
    /// Error issues first seen in it.
    pub new_issues: i64,
    /// Where it ran, from its events and its sessions.
    pub environments: Vec<String>,
    pub sessions: i64,
    pub crashed: i64,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub crash_free_sessions: Option<f64>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub crash_free_users: Option<f64>,
    /// Its share of every release's sessions in the window, 0 to 1.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub adoption: Option<f64>,
}

impl Release {
    /// The columns the release ordering reads.
    pub fn release_ref(&self) -> crate::models::resolution::ReleaseRef {
        crate::models::resolution::ReleaseRef {
            id: self.id,
            version: self.version.clone(),
            date_created: self.date_created,
            date_released: self.date_released,
        }
    }

    pub fn to_response(&self) -> ReleaseResponse {
        ReleaseResponse {
            version: self.version.clone(),
            reference: self.reference.clone(),
            url: self.url.clone(),
            date_created: self.date_created,
            date_released: self.date_released,
        }
    }
}

/// Sentry-compatible release version validation, mirrored from Sentry's
/// `is_valid_version` (`src/sentry/models/release.py`): rejects empty/missing,
/// the literal path segments `.`/`..`, the reserved alias `latest`
/// (case-insensitive), any of the control/separator characters
/// `\r \n \x0c \t / \` (a release version is used as a URL path segment
/// upstream), and anything over 200 chars.
pub fn is_valid_version(version: &str) -> bool {
    if version.is_empty() || version.chars().count() > MAX_VERSION_LEN {
        return false;
    }
    if version == "." || version == ".." {
        return false;
    }
    if version.eq_ignore_ascii_case("latest") {
        return false;
    }
    const FORBIDDEN: &[char] = &['\r', '\n', '\x0c', '\t', '/', '\\'];
    !version.chars().any(|c| FORBIDDEN.contains(&c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_simple_version_accepted() {
        assert!(is_valid_version("1.2.1"));
    }

    #[test]
    fn empty_version_rejected() {
        assert!(!is_valid_version(""));
    }

    #[test]
    fn dot_rejected() {
        assert!(!is_valid_version("."));
    }

    #[test]
    fn dotdot_rejected() {
        assert!(!is_valid_version(".."));
    }

    #[test]
    fn path_traversal_rejected() {
        assert!(!is_valid_version("../etc"));
    }

    #[test]
    fn latest_rejected_case_insensitive() {
        assert!(!is_valid_version("latest"));
        assert!(!is_valid_version("Latest"));
        assert!(!is_valid_version("LATEST"));
    }

    #[test]
    fn slash_rejected() {
        assert!(!is_valid_version("a/b"));
    }

    #[test]
    fn backslash_rejected() {
        assert!(!is_valid_version("a\\b"));
    }

    #[test]
    fn control_chars_rejected() {
        assert!(!is_valid_version("a\rb"));
        assert!(!is_valid_version("a\nb"));
        assert!(!is_valid_version("a\tb"));
        assert!(!is_valid_version("a\x0cb"));
    }

    #[test]
    fn over_max_len_rejected() {
        let version = "a".repeat(MAX_VERSION_LEN + 1);
        assert!(!is_valid_version(&version));
    }

    #[test]
    fn exactly_max_len_accepted() {
        let version = "a".repeat(MAX_VERSION_LEN);
        assert!(is_valid_version(&version));
    }
}
