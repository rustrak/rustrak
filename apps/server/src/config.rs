//! Every environment variable the server reads is read here, once, at startup.

use actix_web::cookie::Key;
use chrono_tz::Tz;
use std::env;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub database: DatabaseConfig,
    pub rate_limit: RateLimitConfig,
    pub security: SecurityConfig,
    /// Where events wait for the digest.
    pub ingest_dir: PathBuf,
    /// `INGEST_MAX_PENDING_EVENTS`: ingest answers 503 while this many events
    /// wait in the spool for the digest. 0 turns the check off.
    pub ingest_max_pending_events: i64,
    /// `INGEST_MIN_FREE_DISK_MB`: ingest answers 503 while the spool's
    /// volume has less free space than this. 0 turns the check off.
    pub ingest_min_free_disk_bytes: u64,
    /// The most events one digest transaction writes. Not read from the
    /// environment; at most 1000 (a batch's insert binds 26 values per event).
    pub digest_batch_max_events: usize,
    /// Events read and parsed at once for a batch. Not read from the
    /// environment.
    pub digest_prep_workers: usize,
    /// The spool bytes the digest holds at once, being read, waiting for
    /// their commit or inside one. Not read from the environment.
    pub digest_memory_bytes: u64,
    /// Public-facing URL used to build DSN strings shown to users. Includes
    /// the scheme; trailing slashes are stripped at load time.
    pub public_url: Option<String>,
    /// Directory where assembled source map files are stored on disk (CAS layout).
    pub sourcemap_storage_path: String,
    /// Budget for parsed source maps kept in memory across events, in bytes.
    pub sourcemap_cache_bytes: usize,
    pub max_chunk_size_bytes: usize,
    /// How often the session aggregator flushes in-memory buckets to the DB.
    pub session_flush_interval_secs: u64,
    /// Max distinct (release, environment) pairs tracked per project before folding into <overflow>.
    pub session_cardinality_cap: usize,
    pub dashboard: DashboardConfig,
    pub telemetry: TelemetryConfig,
    /// The Prometheus scrape at `/metrics`. Off unless switched on: anyone who
    /// can reach the port can read it.
    pub metrics_enabled: bool,
    /// `RUSTRAK_ALERTS_ALLOW_PRIVATE_NETWORKS`: alerts may reach loopback and
    /// private addresses. Off, since whoever configures an integration picks
    /// the URL the server calls (#504).
    pub alerts_allow_private_networks: bool,
    /// The global SMTP server alert and invitation emails go through.
    pub smtp: SmtpSettings,
    /// `None` when SSO is not configured.
    pub oidc: Option<OidcConfig>,
    /// `RUSTRAK_BOOTSTRAP_TOKEN` is set: mint a first API token on an empty install.
    pub bootstrap_token: bool,
    /// `CREATE_SUPERUSER`, `email:password`.
    pub create_superuser: Option<String>,
    /// GitHub's REST API, or a GitHub Enterprise Server's.
    pub github_api_url: String,
}

/// The anonymous telemetry switches. Whether anything is actually sent is
/// decided in `crate::telemetry`, which also needs to know whether a key was
/// compiled into the binary.
#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    pub enabled: bool,
    /// `DO_NOT_TRACK`, the cross-tool convention from consoledonottrack.com.
    pub do_not_track: bool,
}

#[derive(Debug, Clone)]
pub struct DashboardConfig {
    /// Where the build lives, relative to the working directory or absolute.
    ///
    /// A path, not a switch: the dashboard is mounted when the directory
    /// actually holds an `index.html` and skipped when it does not, so an
    /// image built without one needs no configuration to stay API-only.
    pub dir: String,
    /// Off for the deployment that has a build in the image and runs the
    /// dashboard from another host anyway.
    pub enabled: bool,
    /// Where people open it, when that is not the server's own `PUBLIC_URL`:
    /// a dashboard on its own host. Read through [`Config::dashboard_url`],
    /// which applies the fallbacks.
    pub url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout: Duration,
    pub idle_timeout: Duration,
    pub max_lifetime: Duration,
}

#[derive(Clone)]
pub struct SecurityConfig {
    /// Behind a proxy that terminates TLS: session cookies are `Secure`.
    pub ssl_proxy: bool,
    /// `SESSION_SECRET_KEY` was set, so sessions survive a restart.
    pub session_secret_set: bool,
    /// Signs and encrypts session cookies: built from `SESSION_SECRET_KEY`,
    /// or random for this process when it is unset.
    pub session_key: Key,
}

impl std::fmt::Debug for SecurityConfig {
    /// Hand-written so the key cannot reach a log line through the derived
    /// `Debug` on `Config`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecurityConfig")
            .field("ssl_proxy", &self.ssl_proxy)
            .field("session_secret_set", &self.session_secret_set)
            .finish_non_exhaustive()
    }
}

/// OpenID Connect configuration. All four required values must be supplied
/// together; leaving OIDC_ISSUER_URL unset disables SSO.
#[derive(Clone)]
pub struct OidcConfig {
    pub issuer_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub redirect_url: String,
    pub provider_name: String,
    pub scopes: Vec<String>,
    pub allowed_domains: Vec<String>,
    pub auto_provision: bool,
    pub require_email_verified: bool,
    pub link_existing_accounts: bool,
    /// Claim that carries the identity's groups. Operator-configurable because
    /// providers disagree on its name (`groups`, `roles`, `memberOf`, ...).
    pub groups_claim: String,
    /// Groups that grant the global `admin` role on an SSO login. Empty leaves
    /// roles exactly as they are, which keeps SSO from being a role oracle by
    /// accident.
    pub admin_groups: Vec<String>,
    /// Audiences an ID token may carry besides the client ID. Empty trusts
    /// none, as OpenID Connect Core 3.1.3.7 requires: a token naming an
    /// audience the client does not trust is rejected.
    pub additional_trusted_audiences: Vec<String>,
    /// Refuse password sign-in so SSO is the only way in. Startup rejects the
    /// switch when SSO is not configured: it would lock everyone out.
    pub disable_password_login: bool,
}

impl std::fmt::Debug for OidcConfig {
    /// Hand-written so the client secret cannot reach a log line through the
    /// derived `Debug` on `Config`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OidcConfig")
            .field("issuer_url", &self.issuer_url)
            .field("client_id", &self.client_id)
            .field("client_secret", &"[redacted]")
            .field("redirect_url", &self.redirect_url)
            .field("provider_name", &self.provider_name)
            .field("scopes", &self.scopes)
            .field("allowed_domains", &self.allowed_domains)
            .field("auto_provision", &self.auto_provision)
            .field("require_email_verified", &self.require_email_verified)
            .field("link_existing_accounts", &self.link_existing_accounts)
            .field("groups_claim", &self.groups_claim)
            .field("admin_groups", &self.admin_groups)
            .field(
                "additional_trusted_audiences",
                &self.additional_trusted_audiences,
            )
            .field("disable_password_login", &self.disable_password_login)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateLimitConfig {
    /// Installation-wide.
    pub max_events_per_minute: i64,
    /// Installation-wide.
    pub max_events_per_hour: i64,
    pub max_events_per_project_per_minute: i64,
    pub max_events_per_project_per_hour: i64,
}

/// What the server runs with when the environment sets nothing.
impl Default for Config {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            database: DatabaseConfig::default(),
            rate_limit: RateLimitConfig::default(),
            security: SecurityConfig::default(),
            ingest_dir: PathBuf::from("/tmp/rustrak/ingest"),
            ingest_max_pending_events: 100_000,
            ingest_min_free_disk_bytes: 64 * 1024 * 1024,
            digest_batch_max_events: 256,
            digest_prep_workers: std::thread::available_parallelism()
                .map_or(1, |n| n.get() / 2)
                .clamp(1, 4),
            digest_memory_bytes: 16 * 1024 * 1024,
            public_url: None,
            sourcemap_storage_path: "/data/sourcemaps".to_string(),
            sourcemap_cache_bytes: DEFAULT_SOURCEMAP_CACHE_BYTES,
            max_chunk_size_bytes: 10 * 1024 * 1024,
            session_flush_interval_secs: 30,
            session_cardinality_cap: 10_000,
            dashboard: DashboardConfig {
                dir: "./static".to_string(),
                enabled: true,
                url: None,
            },
            telemetry: TelemetryConfig {
                enabled: true,
                do_not_track: false,
            },
            metrics_enabled: false,
            alerts_allow_private_networks: false,
            smtp: SmtpSettings::default(),
            oidc: None,
            bootstrap_token: false,
            create_superuser: None,
            github_api_url: "https://api.github.com".to_string(),
        }
    }
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let defaults = Self::default();
        Ok(Self {
            host: env::var("HOST").unwrap_or(defaults.host),
            port: match non_empty("PORT") {
                Some(port) => port.trim().parse().map_err(|_| ConfigError::InvalidPort)?,
                None => defaults.port,
            },
            database: DatabaseConfig::from_env()?,
            rate_limit: RateLimitConfig::from_env()?,
            security: SecurityConfig::from_env()?,
            ingest_dir: non_empty("INGEST_DIR")
                .map(PathBuf::from)
                .unwrap_or(defaults.ingest_dir),
            ingest_max_pending_events: env_number(
                "INGEST_MAX_PENDING_EVENTS",
                defaults.ingest_max_pending_events,
            )?,
            ingest_min_free_disk_bytes: env_number(
                "INGEST_MIN_FREE_DISK_MB",
                defaults.ingest_min_free_disk_bytes / (1024 * 1024),
            )?
            .saturating_mul(1024 * 1024),
            digest_batch_max_events: defaults.digest_batch_max_events,
            digest_prep_workers: defaults.digest_prep_workers,
            digest_memory_bytes: defaults.digest_memory_bytes,
            sourcemap_storage_path: env::var("SOURCEMAP_STORAGE_PATH")
                .unwrap_or(defaults.sourcemap_storage_path),
            sourcemap_cache_bytes: env_number(
                "SOURCEMAP_CACHE_MB",
                defaults.sourcemap_cache_bytes / (1024 * 1024),
            )?
            .saturating_mul(1024 * 1024),
            max_chunk_size_bytes: env_number(
                "MAX_CHUNK_SIZE_BYTES",
                defaults.max_chunk_size_bytes,
            )?,
            public_url: url_from_env("PUBLIC_URL"),
            session_flush_interval_secs: env_number(
                "SESSION_FLUSH_INTERVAL_SECS",
                defaults.session_flush_interval_secs,
            )?,
            session_cardinality_cap: env_number(
                "SESSION_CARDINALITY_CAP",
                defaults.session_cardinality_cap,
            )?,
            dashboard: DashboardConfig::from_env()?,
            telemetry: TelemetryConfig::from_env()?,
            metrics_enabled: env_bool("RUSTRAK_METRICS", false)?,
            alerts_allow_private_networks: env_bool(
                "RUSTRAK_ALERTS_ALLOW_PRIVATE_NETWORKS",
                false,
            )?,
            smtp: SmtpSettings::from_env()?,
            oidc: OidcConfig::from_env()?,
            bootstrap_token: env::var("RUSTRAK_BOOTSTRAP_TOKEN").is_ok(),
            create_superuser: env::var("CREATE_SUPERUSER").ok(),
            github_api_url: non_empty("GITHUB_API_URL").unwrap_or(defaults.github_api_url),
        })
    }

    /// The address this server is reached at: `PUBLIC_URL`, else the bind
    /// address. Never a request's `Host`, which the client controls.
    pub fn base_url(&self) -> String {
        self.public_url
            .clone()
            .unwrap_or_else(|| format!("http://{}:{}", self.host, self.port))
    }

    /// Where people open the dashboard, for the links in alert notifications:
    /// `DASHBOARD_URL`, else [`Config::base_url`] (the server serves the
    /// dashboard, so the address SDKs reach is also the one people open).
    pub fn dashboard_url(&self) -> String {
        self.dashboard
            .url
            .clone()
            .unwrap_or_else(|| self.base_url())
    }

    /// [`Config::dashboard_url`] without the bind-address fallback: a link
    /// sent to someone else, like an invitation, needs an address they can
    /// open.
    pub fn public_dashboard_url(&self) -> Option<&str> {
        self.dashboard.url.as_deref().or(self.public_url.as_deref())
    }

    /// The largest artifact bundle sentry-cli may assemble: it uploads at
    /// most 64 chunks per request.
    pub fn max_bundle_size_bytes(&self) -> usize {
        self.max_chunk_size_bytes.saturating_mul(64)
    }
}

/// `RUSTRAK_LOG_TIMEZONE`, the IANA zone log timestamps are shown in.
/// Display-only: event and issue timestamps stay UTC. Read apart from
/// [`Config::from_env`] because the logger starts before the configuration
/// loads. `Err` holds a name that is not an IANA zone.
pub fn log_timezone() -> Result<Option<Tz>, String> {
    match env::var("RUSTRAK_LOG_TIMEZONE") {
        Ok(name) => Tz::from_str(&name).map(Some).map_err(|_| name),
        Err(_) => Ok(None),
    }
}

impl TelemetryConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            enabled: env_bool("RUSTRAK_TELEMETRY", true)?,
            // The convention is "set means opt out": a spelling we do not know
            // still opts out rather than stopping the server.
            do_not_track: env_bool("DO_NOT_TRACK", false).unwrap_or(true),
        })
    }
}

/// A brake for a runaway loop that would fill the disk, not a cap on the
/// spikes an ordinary production app sends.
impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_events_per_minute: 60_000,
            max_events_per_hour: 1_000_000,
            max_events_per_project_per_minute: 20_000,
            max_events_per_project_per_hour: 300_000,
        }
    }
}

impl RateLimitConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        let defaults = Self::default();
        Ok(Self {
            max_events_per_minute: env_number(
                "MAX_EVENTS_PER_MINUTE",
                defaults.max_events_per_minute,
            )?,
            max_events_per_hour: env_number("MAX_EVENTS_PER_HOUR", defaults.max_events_per_hour)?,
            max_events_per_project_per_minute: env_number(
                "MAX_EVENTS_PER_PROJECT_PER_MINUTE",
                defaults.max_events_per_project_per_minute,
            )?,
            max_events_per_project_per_hour: env_number(
                "MAX_EVENTS_PER_PROJECT_PER_HOUR",
                defaults.max_events_per_project_per_hour,
            )?,
        })
    }

    pub fn is_customized(&self) -> bool {
        *self != Self::default()
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "sqlite:///data/rustrak.db".to_string(),
            max_connections: 10,
            min_connections: 1,
            acquire_timeout: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(600),
            max_lifetime: Duration::from_secs(1800),
        }
    }
}

impl DatabaseConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        let defaults = Self::default();
        #[cfg(feature = "sqlite")]
        let url = env::var("DATABASE_URL").unwrap_or(defaults.url);
        #[cfg(not(feature = "sqlite"))]
        let url = env::var("DATABASE_URL").map_err(|_| ConfigError::MissingDatabaseUrl)?;
        let secs = |name: &str, default: Duration| {
            env_number(name, default.as_secs()).map(Duration::from_secs)
        };

        Ok(Self {
            url,
            max_connections: env_number("DATABASE_MAX_CONNECTIONS", defaults.max_connections)?,
            min_connections: env_number("DATABASE_MIN_CONNECTIONS", defaults.min_connections)?,
            acquire_timeout: secs("DATABASE_ACQUIRE_TIMEOUT_SECS", defaults.acquire_timeout)?,
            idle_timeout: secs("DATABASE_IDLE_TIMEOUT_SECS", defaults.idle_timeout)?,
            max_lifetime: secs("DATABASE_MAX_LIFETIME_SECS", defaults.max_lifetime)?,
        })
    }
}

impl DashboardConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            dir: non_empty("RUSTRAK_DASHBOARD_DIR")
                .unwrap_or_else(|| Config::default().dashboard.dir),
            // A typo is refused rather than defaulted: one that silently left
            // the dashboard on would expose a UI the operator meant to keep
            // off the box.
            enabled: env_bool("RUSTRAK_DASHBOARD", true)?,
            url: url_from_env("DASHBOARD_URL"),
        })
    }
}

/// Default budget for parsed source maps kept across events (`SOURCEMAP_CACHE_MB`).
pub const DEFAULT_SOURCEMAP_CACHE_BYTES: usize = 64 * 1024 * 1024;

/// The sender used when none is configured or the configured one does not
/// parse.
pub const DEFAULT_FROM: &str = "alerts@rustrak.local";

/// How the connection to the SMTP server is encrypted (`SMTP_TLS`), like
/// Sentry's `mail.use-tls` and `mail.use-ssl`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmtpTls {
    /// Upgrade with STARTTLS, and refuse a server that cannot.
    StartTls,
    /// TLS from the first byte (SMTPS).
    Tls,
    /// No encryption: a relay on a trusted network, or a local test server.
    None,
}

impl SmtpTls {
    /// `starttls`, `tls` or `none`, in any case.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "starttls" => Some(Self::StartTls),
            "tls" => Some(Self::Tls),
            "none" => Some(Self::None),
            _ => None,
        }
    }
}

/// The global SMTP configuration, read from `SMTP_HOST`, `SMTP_PORT`,
/// `SMTP_TLS`, `SMTP_USERNAME`, `SMTP_PASSWORD` and `SMTP_FROM`.
#[derive(Debug, Clone)]
pub struct SmtpSettings {
    pub host: Option<String>,
    pub port: u16,
    /// `None` picks by port: TLS on 465, STARTTLS elsewhere.
    pub tls: Option<SmtpTls>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub from_address: String,
}

impl Default for SmtpSettings {
    fn default() -> Self {
        Self {
            host: None,
            port: 587,
            tls: None,
            username: None,
            password: None,
            from_address: DEFAULT_FROM.to_string(),
        }
    }
}

impl SmtpSettings {
    pub fn from_env() -> Result<Self, ConfigError> {
        let defaults = Self::default();
        Ok(Self {
            host: non_empty("SMTP_HOST"),
            port: env_number("SMTP_PORT", defaults.port)?,
            tls: env::var("SMTP_TLS").ok().and_then(|value| {
                let tls = SmtpTls::parse(&value);
                if tls.is_none() {
                    log::warn!("Ignoring SMTP_TLS={value}: expected starttls, tls or none");
                }
                tls
            }),
            username: env::var("SMTP_USERNAME").ok(),
            password: env::var("SMTP_PASSWORD").ok(),
            from_address: env::var("SMTP_FROM").unwrap_or(defaults.from_address),
        })
    }
}

/// A variable's value, unless it is unset or blank.
fn non_empty(name: &str) -> Option<String> {
    env::var(name).ok().filter(|s| !s.trim().is_empty())
}

/// A number from the environment; unset or blank reads as `default`. A value
/// that does not parse stops startup, like [`env_bool`]: falling back would
/// run a server the operator did not configure.
fn env_number<T: FromStr>(name: &str, default: T) -> Result<T, ConfigError> {
    match non_empty(name) {
        None => Ok(default),
        Some(value) => value
            .trim()
            .parse()
            .map_err(|_| ConfigError::InvalidNumber {
                name: name.to_string(),
                value: value.trim().to_string(),
            }),
    }
}

/// An address from the environment: unset or blank is `None`, surrounding
/// whitespace and trailing slashes go, and the scheme is lowercased (RFC 3986:
/// it is case-insensitive).
fn url_from_env(var: &str) -> Option<String> {
    non_empty(var).map(|s| {
        let trimmed = s.trim().trim_end_matches('/');
        if let Some(pos) = trimmed.find("://") {
            format!("{}{}", trimmed[..pos].to_lowercase(), &trimmed[pos..])
        } else {
            trimmed.to_string()
        }
    })
}

#[derive(Debug)]
pub enum ConfigError {
    InvalidPort,
    MissingDatabaseUrl,
    MissingSessionSecret,
    SessionSecretTooShort { len: usize },
    IncompleteOidcConfig(String),
    InvalidBoolean { name: String, value: String },
    InvalidNumber { name: String, value: String },
    PasswordLoginDisabledWithoutSso,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::InvalidPort => write!(f, "PORT must be a valid number"),
            ConfigError::MissingDatabaseUrl => {
                write!(f, "DATABASE_URL environment variable is required")
            }
            ConfigError::MissingSessionSecret => {
                write!(
                    f,
                    "SESSION_SECRET_KEY is required when SSL_PROXY is enabled"
                )
            }
            ConfigError::SessionSecretTooShort { len } => {
                write!(
                    f,
                    "SESSION_SECRET_KEY is {len} bytes, but at least {min} are required. \
                     Generate one with: openssl rand -hex 32",
                    min = SecurityConfig::MIN_SECRET_LEN
                )
            }
            ConfigError::IncompleteOidcConfig(name) => {
                write!(f, "{name} is required when OIDC_ISSUER_URL is configured")
            }
            ConfigError::InvalidBoolean { name, value } => write!(
                f,
                "{name} must be on or off (also accepted: true/false, 1/0, yes/no), got \"{value}\""
            ),
            ConfigError::InvalidNumber { name, value } => {
                write!(f, "{name} must be a number, got \"{value}\"")
            }
            ConfigError::PasswordLoginDisabledWithoutSso => write!(
                f,
                "OIDC_DISABLE_PASSWORD_LOGIN is on, but SSO is not configured (OIDC_ISSUER_URL is unset). \
                 That would leave no way to sign in"
            ),
        }
    }
}

/// Parse a boolean environment variable.
///
/// Unset or empty yields `default`, the same "empty means unset" rule
/// `OIDC_ISSUER_URL` follows. An unrecognized value stops startup instead of
/// falling back, so a typo in a security switch never picks a value the
/// operator did not write.
fn env_bool(name: &str, default: bool) -> Result<bool, ConfigError> {
    let Ok(value) = env::var(name) else {
        return Ok(default);
    };
    match value.trim().to_ascii_lowercase().as_str() {
        "" => Ok(default),
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => Err(ConfigError::InvalidBoolean {
            name: name.to_string(),
            value: value.trim().to_string(),
        }),
    }
}

impl OidcConfig {
    /// Whether anyone with an account at the provider can create one here.
    pub fn admits_any_provider_account(&self) -> bool {
        self.auto_provision && self.allowed_domains.is_empty()
    }

    /// Load OIDC settings from the environment, or `None` when SSO is not
    /// configured. The four connection settings are all-or-nothing: setting
    /// `OIDC_ISSUER_URL` without the rest is a startup error, not a silently
    /// disabled provider.
    pub fn from_env() -> Result<Option<Self>, ConfigError> {
        // Read before the early return: the switch is meaningless without SSO,
        // and silently ignoring it would let an operator believe password
        // login is off while it still works.
        let disable_password_login = env_bool("OIDC_DISABLE_PASSWORD_LOGIN", false)?;
        let Some(issuer_url) = non_empty("OIDC_ISSUER_URL") else {
            if disable_password_login {
                return Err(ConfigError::PasswordLoginDisabledWithoutSso);
            }
            return Ok(None);
        };

        let required = |name: &str| {
            non_empty(name).ok_or_else(|| ConfigError::IncompleteOidcConfig(name.to_string()))
        };
        let list = |name: &str| -> Vec<String> {
            env::var(name)
                .unwrap_or_default()
                .split(',')
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect()
        };

        Ok(Some(Self {
            issuer_url: issuer_url.trim().to_string(),
            client_id: required("OIDC_CLIENT_ID")?,
            client_secret: required("OIDC_CLIENT_SECRET")?,
            redirect_url: required("OIDC_REDIRECT_URL")?,
            provider_name: non_empty("OIDC_PROVIDER_NAME").unwrap_or_else(|| "SSO".to_string()),
            scopes: env::var("OIDC_SCOPES")
                .unwrap_or_else(|_| "openid email profile".to_string())
                .split_whitespace()
                .map(str::to_string)
                .collect(),
            allowed_domains: list("OIDC_ALLOWED_DOMAINS")
                .into_iter()
                .map(|domain| domain.to_ascii_lowercase())
                .collect(),
            auto_provision: env_bool("OIDC_AUTO_PROVISION", false)?,
            require_email_verified: env_bool("OIDC_REQUIRE_EMAIL_VERIFIED", true)?,
            link_existing_accounts: env_bool("OIDC_LINK_EXISTING_ACCOUNTS", false)?,
            groups_claim: non_empty("OIDC_GROUPS_CLAIM")
                .map(|value| value.trim().to_string())
                .unwrap_or_else(|| "groups".to_string()),
            admin_groups: list("OIDC_ADMIN_GROUPS"),
            additional_trusted_audiences: list("OIDC_ADDITIONAL_TRUSTED_AUDIENCES"),
            disable_password_login,
        }))
    }
}

impl std::error::Error for ConfigError {}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            ssl_proxy: false,
            session_secret_set: false,
            session_key: Key::generate(),
        }
    }
}

impl SecurityConfig {
    /// Required length of `SESSION_SECRET_KEY`, in bytes: the cookie master key
    /// is a 256-bit signing key followed by a 256-bit encryption key.
    pub const MIN_SECRET_LEN: usize = 64;

    /// Builds the key here so an unusable secret stops the process before it
    /// opens the database, runs migrations and starts the workers.
    pub fn from_env() -> Result<Self, ConfigError> {
        let secret = env::var("SESSION_SECRET_KEY").ok();
        let ssl_proxy = env_bool("SSL_PROXY", false)?;
        if ssl_proxy && secret.is_none() {
            return Err(ConfigError::MissingSessionSecret);
        }
        let session_key = match &secret {
            Some(secret) if secret.len() < Self::MIN_SECRET_LEN => {
                return Err(ConfigError::SessionSecretTooShort { len: secret.len() })
            }
            Some(secret) => Key::from(secret.as_bytes()),
            None => Key::generate(),
        };
        Ok(Self {
            ssl_proxy,
            session_secret_set: secret.is_some(),
            session_key,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_wait_for_the_digest_under_tmp_by_default() {
        assert_eq!(
            Config::default().ingest_dir,
            PathBuf::from("/tmp/rustrak/ingest")
        );
    }

    #[test]
    fn smtp_tls_parses_the_three_modes_and_nothing_else() {
        assert_eq!(SmtpTls::parse("starttls"), Some(SmtpTls::StartTls));
        assert_eq!(SmtpTls::parse(" TLS "), Some(SmtpTls::Tls));
        assert_eq!(SmtpTls::parse("none"), Some(SmtpTls::None));
        assert_eq!(SmtpTls::parse("ssl"), None);
    }

    #[test]
    fn base_url_is_the_public_url_when_set() {
        let config = Config {
            public_url: Some("https://api.example.com".to_string()),
            ..Config::default()
        };
        assert_eq!(config.base_url(), "https://api.example.com");
    }

    #[test]
    fn base_url_falls_back_to_the_bind_address() {
        let config = Config {
            host: "127.0.0.1".to_string(),
            port: 9090,
            ..Config::default()
        };
        assert_eq!(config.base_url(), "http://127.0.0.1:9090");
    }
}
