use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::models::stats::ProjectListStats;

/// A list of all valid Sentry `platform` identifiers, mirrored verbatim from
/// Relay: <https://github.com/getsentry/relay/blob/f42b1c8a15bba8cb96d1dfd3bd2b3158c7817c5f/relay-event-schema/src/protocol/constants.rs#L2-L22>
///
/// These constrain an *event*'s `platform` field, and by extension what
/// [`super::super::services::ProjectService::infer_platform_from_event`] may
/// copy onto a project. Real Sentry's auto-inference
/// (`_set_project_platform_if_needed` in `event_manager.py`) filters on this
/// same list. For what a user may pick manually, see [`SELECTABLE_PLATFORMS`]
/// — a deliberately different, much longer list.
pub const VALID_PLATFORMS: &[&str] = &[
    "as3",
    "c",
    "cfml",
    "cocoa",
    "csharp",
    "elixir",
    "go",
    "groovy",
    "haskell",
    "java",
    "javascript",
    "native",
    "node",
    "objc",
    "other",
    "perl",
    "php",
    "python",
    "ruby",
];

/// Every platform id a **user** may pick for a project in settings.
///
/// This is a different domain from [`VALID_PLATFORMS`], which constrains the
/// `platform` field of an *event*. A project's platform is product
/// configuration chosen by a human, so it may name a specific framework
/// (`javascript-nextjs`, `python-django`) that could never appear on an
/// event: Relay deletes any event platform outside `VALID_PLATFORMS` and
/// replaces it with `"other"`
/// (<https://github.com/getsentry/relay/blob/c455da18abe020311b7b4abf41a86b8503d72be9/relay-event-normalization/src/event.rs#L1188-L1199>).
///
/// Real Sentry validates this field against `GETTING_STARTED_DOCS_PLATFORMS`
/// (<https://github.com/getsentry/sentry/blob/a32a33a5106ce9350ccb33b406695f53067c4a9f/src/sentry/models/project.py#L927-L928>),
/// a list it keeps in sync by hand with its frontend's `platforms.tsx`. This
/// list is that one plus the 10 `VALID_PLATFORMS` entries Sentry omits from
/// it (`perl`, `cfml`, `as3`, ...). Sentry's omission means auto-detection
/// can write a platform its own settings picker then refuses to re-select;
/// including them here keeps the invariant that anything Rustrak can
/// auto-detect stays selectable.
pub const SELECTABLE_PLATFORMS: &[&str] = &[
    "android",
    "apple",
    "apple-ios",
    "apple-macos",
    "as3",
    "bun",
    "c",
    "capacitor",
    "cfml",
    "cocoa",
    "cordova",
    "csharp",
    "dart",
    "deno",
    "dotnet",
    "dotnet-aspnet",
    "dotnet-aspnetcore",
    "dotnet-awslambda",
    "dotnet-gcpfunctions",
    "dotnet-maui",
    "dotnet-uwp",
    "dotnet-winforms",
    "dotnet-wpf",
    "dotnet-xamarin",
    "electron",
    "elixir",
    "flutter",
    "go",
    "go-echo",
    "go-fasthttp",
    "go-fiber",
    "go-gin",
    "go-http",
    "go-iris",
    "go-martini",
    "go-negroni",
    "godot",
    "groovy",
    "haskell",
    "ionic",
    "java",
    "java-log4j2",
    "java-logback",
    "java-spring",
    "java-spring-boot",
    "javascript",
    "javascript-angular",
    "javascript-astro",
    "javascript-ember",
    "javascript-gatsby",
    "javascript-nextjs",
    "javascript-nuxt",
    "javascript-react",
    "javascript-react-router",
    "javascript-remix",
    "javascript-solid",
    "javascript-solidstart",
    "javascript-svelte",
    "javascript-sveltekit",
    "javascript-tanstackstart-react",
    "javascript-vue",
    "kotlin",
    "minidump",
    "native",
    "native-qt",
    "nintendo-switch",
    "node",
    "node-awslambda",
    "node-azurefunctions",
    "node-cloudflare-pages",
    "node-cloudflare-workers",
    "node-connect",
    "node-express",
    "node-fastify",
    "node-gcpfunctions",
    "node-hapi",
    "node-hono",
    "node-koa",
    "node-nestjs",
    "objc",
    "other",
    "perl",
    "php",
    "php-laravel",
    "php-symfony",
    "playstation",
    "powershell",
    "python",
    "python-aiohttp",
    "python-asgi",
    "python-awslambda",
    "python-bottle",
    "python-celery",
    "python-chalice",
    "python-django",
    "python-falcon",
    "python-fastapi",
    "python-flask",
    "python-gcpfunctions",
    "python-litestar",
    "python-pylons",
    "python-pymongo",
    "python-pyramid",
    "python-quart",
    "python-rq",
    "python-sanic",
    "python-serverless",
    "python-starlette",
    "python-tornado",
    "python-tryton",
    "python-wsgi",
    "react-native",
    "ruby",
    "ruby-rack",
    "ruby-rails",
    "rust",
    "unity",
    "unreal",
    "xbox",
];

/// Project model for reading from the database
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Project {
    pub id: i32,
    pub name: String,
    pub slug: String,
    pub sentry_key: Uuid,
    pub stored_event_count: i32,
    pub digested_event_count: i32,
    /// Events the quota dropped after ingest had accepted them.
    pub rate_limited_event_count: i64,
    /// The project's own limits; they can only tighten the operator's.
    pub rate_limit_per_minute: Option<i64>,
    pub rate_limit_per_hour: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Auto-detected from the first ingested event whose `platform` field is
    /// a valid event platform (see [`VALID_PLATFORMS`]), then never
    /// overwritten by later events, matching real Sentry. A user may still
    /// change it manually in settings, which accepts the wider
    /// [`SELECTABLE_PLATFORMS`].
    pub platform: Option<String>,
    // Rate limiting fields
    #[serde(skip_serializing)]
    pub quota_exceeded_until: Option<DateTime<Utc>>,
    #[serde(skip_serializing)]
    #[allow(dead_code)] // Useful for debugging rate limit issues
    pub quota_exceeded_reason: Option<String>,
    #[serde(skip_serializing)]
    pub next_quota_check: i64,
    #[serde(skip_serializing)]
    #[sqlx(flatten)]
    pub quota: super::QuotaWindows,
}

/// DTO for creating a new project
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateProject {
    pub name: String,
    #[serde(default)]
    pub slug: Option<String>,
    /// Platform identifier for the project, such as `python-django` or
    /// `javascript-nextjs`. Rejected with a 400 if it is not one of the
    /// supported values.
    ///
    /// Optional. Omitting it is not the same as having no platform forever:
    /// the project is then auto-assigned a platform from the first event it
    /// ingests, drawn from the narrower set of platforms an event may declare.
    #[serde(default)]
    pub platform: Option<String>,
}

/// DTO for updating a project
#[derive(Debug, Default, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdateProject {
    pub name: Option<String>,
    /// New slug, slugified before storing.
    ///
    /// A slug already taken by another project returns a 409. It is not
    /// silently de-duplicated the way a slug derived at creation time is,
    /// since storing something other than what was requested would be wrong.
    ///
    /// Sending `null` leaves the current value untouched.
    pub slug: Option<String>,
    /// Manual override of the project's platform, such as `python-django` or
    /// `javascript-nextjs`. Rejected with a 400 if it is not one of the
    /// supported values.
    ///
    /// The supported set is wider than the platforms an event may declare, so
    /// framework-specific identifiers are accepted here. An update overwrites
    /// any auto-assigned value, which is how a wrong detection is corrected.
    /// Sending `null` leaves the current value untouched rather than clearing
    /// it.
    pub platform: Option<String>,
    /// The project's own events-per-minute limit. It can only tighten the
    /// operator's `MAX_EVENTS_PER_PROJECT_PER_MINUTE`, never raise it.
    /// Absent leaves it as it is; `null` removes it.
    #[serde(default, deserialize_with = "deserialize_optional_limit")]
    pub rate_limit_per_minute: Option<Option<i64>>,
    /// The same for the hour, against `MAX_EVENTS_PER_PROJECT_PER_HOUR`.
    #[serde(default, deserialize_with = "deserialize_optional_limit")]
    pub rate_limit_per_hour: Option<Option<i64>>,
}

/// Keeps a missing key (`None`) apart from an explicit `null` (`Some(None)`),
/// which plain `Option<Option<T>>` collapses.
fn deserialize_optional_limit<'de, D>(deserializer: D) -> Result<Option<Option<i64>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<i64>::deserialize(deserializer).map(Some)
}

/// Response with DSN included
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ProjectResponse {
    pub id: i32,
    pub name: String,
    pub slug: String,
    pub sentry_key: Uuid,
    pub dsn: String,
    pub stored_event_count: i32,
    pub digested_event_count: i32,
    /// Events the quota dropped after ingest had accepted them.
    pub rate_limited_event_count: i64,
    /// The project's own limits, or `null` when it follows the operator's.
    pub rate_limit_per_minute: Option<i64>,
    pub rate_limit_per_hour: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub platform: Option<String>,
    /// Present only when the request asked for `?stats_period=`.
    ///
    /// Absent rather than zeroed when unrequested: a caller that did not ask
    /// must be able to tell "no stats were computed" from "this project is
    /// quiet", and only one of those should render an empty sparkline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<ProjectListStats>,
}

impl Project {
    /// Builds the DSN for this project
    pub fn dsn(&self, base_url: &str) -> String {
        let key = self.sentry_key.simple().to_string();
        let host = base_url
            .trim_start_matches("http://")
            .trim_start_matches("https://");
        let scheme = if base_url.starts_with("https") {
            "https"
        } else {
            "http"
        };
        format!("{scheme}://{key}@{host}/{}", self.id)
    }

    /// Converts to ProjectResponse with DSN
    pub fn to_response(&self, base_url: &str) -> ProjectResponse {
        ProjectResponse {
            id: self.id,
            name: self.name.clone(),
            slug: self.slug.clone(),
            sentry_key: self.sentry_key,
            dsn: self.dsn(base_url),
            stored_event_count: self.stored_event_count,
            digested_event_count: self.digested_event_count,
            rate_limited_event_count: self.rate_limited_event_count,
            rate_limit_per_minute: self.rate_limit_per_minute,
            rate_limit_per_hour: self.rate_limit_per_hour,
            created_at: self.created_at,
            updated_at: self.updated_at,
            platform: self.platform.clone(),
            stats: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn make_project(id: i32, key: Uuid) -> Project {
        Project {
            id,
            name: "Test Project".to_string(),
            slug: "test-project".to_string(),
            sentry_key: key,
            stored_event_count: 0,
            digested_event_count: 0,
            rate_limited_event_count: 0,
            rate_limit_per_minute: None,
            rate_limit_per_hour: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            platform: None,
            quota_exceeded_until: None,
            quota_exceeded_reason: None,
            next_quota_check: 0,
            quota: Default::default(),
        }
    }

    #[test]
    fn test_dsn_with_https_base_url() {
        let key = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap();
        let project = make_project(1, key);
        let dsn = project.dsn("https://api.example.com");
        let key_str = key.simple().to_string();
        assert_eq!(dsn, format!("https://{}@api.example.com/1", key_str));
    }

    #[test]
    fn test_dsn_with_http_base_url() {
        let key = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let project = make_project(1, key);
        let dsn = project.dsn("http://192.168.1.10:9090");
        let key_str = key.simple().to_string();
        assert_eq!(dsn, format!("http://{}@192.168.1.10:9090/1", key_str));
    }

    #[test]
    fn test_dsn_fallback_no_scheme() {
        let key = Uuid::new_v4();
        let project = make_project(1, key);
        let dsn = project.dsn("0.0.0.0:8080");
        assert!(dsn.starts_with("http://"), "DSN should start with http://");
        assert!(
            dsn.contains("0.0.0.0:8080"),
            "DSN should contain 0.0.0.0:8080"
        );
    }
}
