use actix_web::cookie::Key;
use std::env;
use std::time::Duration;

/// Application configuration loaded from environment variables
#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub database: DatabaseConfig,
    pub rate_limit: RateLimitConfig,
    pub security: SecurityConfig,
    pub ingest_dir: Option<String>,
    /// Optional public-facing URL used to build DSN strings shown to users.
    /// Must include scheme (http:// or https://). Trailing slash is stripped at load time.
    /// Falls back to `http://{HOST}:{PORT}` when unset.
    pub public_url: Option<String>,
    /// Directory where assembled source map files are stored on disk (CAS layout).
    /// Default: /data/sourcemaps. Override with SOURCEMAP_STORAGE_PATH env var.
    pub sourcemap_storage_path: String,
    /// Budget for parsed source maps kept in memory across events, in bytes.
    /// Default: 64 MB. Override with SOURCEMAP_CACHE_MB env var.
    pub sourcemap_cache_bytes: usize,
    /// Maximum allowed size of a single uploaded chunk in bytes.
    /// Default: 10 MB. Override with MAX_CHUNK_SIZE_BYTES env var.
    pub max_chunk_size_bytes: usize,
    /// How often the session aggregator flushes in-memory buckets to the DB (seconds).
    /// Default: 30. Override with SESSION_FLUSH_INTERVAL_SECS env var.
    pub session_flush_interval_secs: u64,
    /// Max distinct (release, environment) pairs tracked per project before folding into <overflow>.
    /// Default: 10000. Override with SESSION_CARDINALITY_CAP env var.
    pub session_cardinality_cap: usize,
    /// The compiled dashboard: where it is and whether to serve it.
    pub dashboard: DashboardConfig,
    /// Anonymous usage telemetry, as the environment describes it.
    pub telemetry: TelemetryConfig,
}

/// The anonymous telemetry switches. Whether anything is actually sent is
/// decided in `crate::telemetry`, which also needs to know whether a key was
/// compiled into the binary.
#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    /// `RUSTRAK_TELEMETRY`: on unless it says otherwise.
    pub enabled: bool,
    /// `DO_NOT_TRACK=1`, the cross-tool convention from consoledonottrack.com.
    pub do_not_track: bool,
}

/// The compiled dashboard, as the environment describes it.
#[derive(Debug, Clone)]
pub struct DashboardConfig {
    /// Where the build lives, relative to the working directory or absolute.
    /// Default: `./static`. Override with RUSTRAK_DASHBOARD_DIR.
    ///
    /// A path, not a switch: the dashboard is mounted when the directory
    /// actually holds an `index.html` and skipped when it does not, so an
    /// image built without one needs no configuration to stay API-only.
    pub dir: String,
    /// Whether to serve it at all. Default: on. Override with
    /// RUSTRAK_DASHBOARD=off, for the deployment that has a build in the image
    /// and runs the dashboard from another host anyway.
    pub enabled: bool,
    /// Where people open it, when that is not the server's own `PUBLIC_URL`:
    /// a dashboard on its own host. Set with DASHBOARD_URL. Read through
    /// [`Config::dashboard_url`], which applies the fallbacks.
    pub url: Option<String>,
}

/// Database connection pool configuration
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout: Duration,
    pub idle_timeout: Duration,
    pub max_lifetime: Duration,
}

/// Security configuration for production deployments
#[derive(Clone)]
pub struct SecurityConfig {
    /// True if server is behind a proxy that terminates SSL (nginx, Cloudflare, etc.)
    /// When true: cookie_secure=true is enabled
    pub ssl_proxy: bool,
    /// Session encryption key (64 hex chars). Required when ssl_proxy=true
    pub session_secret_key: Option<String>,
}

impl std::fmt::Debug for SecurityConfig {
    /// Hand-written so the secret cannot reach a log line through the derived
    /// `Debug` on `Config`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecurityConfig")
            .field("ssl_proxy", &self.ssl_proxy)
            .field(
                "session_secret_key",
                &self.session_secret_key.as_ref().map(|_| "[redacted]"),
            )
            .finish()
    }
}

/// Rate limiting configuration
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateLimitConfig {
    /// Global (installation-wide) max events per minute
    pub max_events_per_minute: i64,
    /// Global (installation-wide) max events per hour
    pub max_events_per_hour: i64,
    /// Per-project max events per minute
    pub max_events_per_project_per_minute: i64,
    /// Per-project max events per hour
    pub max_events_per_project_per_hour: i64,
}

impl Config {
    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            host: env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            port: env::var("PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .map_err(|_| ConfigError::InvalidPort)?,
            database: DatabaseConfig::from_env()?,
            rate_limit: RateLimitConfig::from_env(),
            security: SecurityConfig::from_env()?,
            ingest_dir: env::var("INGEST_DIR").ok(),
            sourcemap_storage_path: env::var("SOURCEMAP_STORAGE_PATH")
                .unwrap_or_else(|_| "/data/sourcemaps".to_string()),
            sourcemap_cache_bytes: env::var("SOURCEMAP_CACHE_MB")
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .and_then(|mb| mb.checked_mul(1024 * 1024))
                .unwrap_or(crate::services::sourcemap::DEFAULT_SOURCEMAP_CACHE_BYTES),
            max_chunk_size_bytes: env::var("MAX_CHUNK_SIZE_BYTES")
                .unwrap_or_else(|_| (10 * 1024 * 1024).to_string())
                .parse()
                .unwrap_or(10 * 1024 * 1024),
            public_url: url_from_env("PUBLIC_URL"),
            session_flush_interval_secs: env::var("SESSION_FLUSH_INTERVAL_SECS")
                .unwrap_or_else(|_| "30".to_string())
                .parse()
                .unwrap_or(30),
            session_cardinality_cap: env::var("SESSION_CARDINALITY_CAP")
                .unwrap_or_else(|_| "10000".to_string())
                .parse()
                .unwrap_or(10_000),
            dashboard: DashboardConfig::from_env()?,
            telemetry: TelemetryConfig::from_env()?,
        })
    }
}

impl Config {
    /// Where people open the dashboard, for the links in alert notifications:
    /// `DASHBOARD_URL`, else `PUBLIC_URL` (the server serves the dashboard, so
    /// the address SDKs reach is also the one people open), else the bind
    /// address.
    pub fn dashboard_url(&self) -> String {
        self.dashboard
            .url
            .clone()
            .or_else(|| self.public_url.clone())
            .unwrap_or_else(|| format!("http://{}:{}", self.host, self.port))
    }
}

impl TelemetryConfig {
    /// Load telemetry configuration from environment variables
    pub fn from_env() -> Result<Self, ConfigError> {
        let enabled = match env::var("RUSTRAK_TELEMETRY").ok() {
            None => true,
            Some(value) => match value.trim().to_ascii_lowercase().as_str() {
                "on" | "true" | "1" => true,
                "off" | "false" | "0" => false,
                _ => return Err(ConfigError::InvalidTelemetrySwitch { value }),
            },
        };
        let do_not_track = env::var("DO_NOT_TRACK")
            .map(|v| matches!(v.trim(), "1" | "true"))
            .unwrap_or(false);
        Ok(Self {
            enabled,
            do_not_track,
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
    /// Load rate limit configuration from environment variables
    pub fn from_env() -> Self {
        let defaults = Self::default();
        let limit = |name: &str, default: i64| {
            env::var(name)
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(default)
        };
        Self {
            max_events_per_minute: limit("MAX_EVENTS_PER_MINUTE", defaults.max_events_per_minute),
            max_events_per_hour: limit("MAX_EVENTS_PER_HOUR", defaults.max_events_per_hour),
            max_events_per_project_per_minute: limit(
                "MAX_EVENTS_PER_PROJECT_PER_MINUTE",
                defaults.max_events_per_project_per_minute,
            ),
            max_events_per_project_per_hour: limit(
                "MAX_EVENTS_PER_PROJECT_PER_HOUR",
                defaults.max_events_per_project_per_hour,
            ),
        }
    }

    /// Whether any limit differs from its default.
    pub fn is_customized(&self) -> bool {
        *self != Self::default()
    }
}

impl DatabaseConfig {
    /// Load database configuration from environment variables
    pub fn from_env() -> Result<Self, ConfigError> {
        #[cfg(feature = "sqlite")]
        let url =
            env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:///data/rustrak.db".to_string());
        #[cfg(not(feature = "sqlite"))]
        let url = env::var("DATABASE_URL").map_err(|_| ConfigError::MissingDatabaseUrl)?;

        Ok(Self {
            url,
            max_connections: env::var("DATABASE_MAX_CONNECTIONS")
                .unwrap_or_else(|_| "10".to_string())
                .parse()
                .unwrap_or(10),
            min_connections: env::var("DATABASE_MIN_CONNECTIONS")
                .unwrap_or_else(|_| "1".to_string())
                .parse()
                .unwrap_or(1),
            acquire_timeout: Duration::from_secs(
                env::var("DATABASE_ACQUIRE_TIMEOUT_SECS")
                    .unwrap_or_else(|_| "5".to_string())
                    .parse()
                    .unwrap_or(5),
            ),
            idle_timeout: Duration::from_secs(
                env::var("DATABASE_IDLE_TIMEOUT_SECS")
                    .unwrap_or_else(|_| "600".to_string())
                    .parse()
                    .unwrap_or(600),
            ),
            max_lifetime: Duration::from_secs(
                env::var("DATABASE_MAX_LIFETIME_SECS")
                    .unwrap_or_else(|_| "1800".to_string())
                    .parse()
                    .unwrap_or(1800),
            ),
        })
    }
}

impl DashboardConfig {
    /// Load dashboard configuration from environment variables
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            dir: env::var("RUSTRAK_DASHBOARD_DIR")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "./static".to_string()),
            enabled: Self::switch(env::var("RUSTRAK_DASHBOARD").ok())?,
            url: url_from_env("DASHBOARD_URL"),
        })
    }

    /// `RUSTRAK_DASHBOARD`: on unless it says otherwise.
    ///
    /// Refuses anything it does not recognise rather than defaulting, because
    /// the two ways to misread a switch are not symmetric here: a typo that
    /// silently left the dashboard *on* would expose a UI the operator meant
    /// to keep off the box.
    fn switch(value: Option<String>) -> Result<bool, ConfigError> {
        let Some(value) = value else {
            return Ok(true);
        };
        match value.trim().to_ascii_lowercase().as_str() {
            "on" | "true" | "1" => Ok(true),
            "off" | "false" | "0" => Ok(false),
            _ => Err(ConfigError::InvalidDashboardSwitch { value }),
        }
    }
}

/// An address from the environment: unset or blank is `None`, surrounding
/// whitespace and trailing slashes go, and the scheme is lowercased (RFC 3986:
/// it is case-insensitive).
fn url_from_env(var: &str) -> Option<String> {
    env::var(var)
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| {
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
    InvalidDashboardSwitch { value: String },
    InvalidTelemetrySwitch { value: String },
    MissingDatabaseUrl,
    MissingSessionSecret,
    SessionSecretTooShort { len: usize },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::InvalidPort => write!(f, "PORT must be a valid number"),
            ConfigError::InvalidDashboardSwitch { value } => write!(
                f,
                "RUSTRAK_DASHBOARD is {value:?}, but it must be on or off"
            ),
            ConfigError::InvalidTelemetrySwitch { value } => write!(
                f,
                "RUSTRAK_TELEMETRY is {value:?}, but it must be on or off"
            ),
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
        }
    }
}

impl std::error::Error for ConfigError {}

impl SecurityConfig {
    /// Required length of `SESSION_SECRET_KEY`, in bytes: the cookie master key
    /// is a 256-bit signing key followed by a 256-bit encryption key.
    pub const MIN_SECRET_LEN: usize = 64;

    /// Load security configuration from environment variables
    pub fn from_env() -> Result<Self, ConfigError> {
        let session_secret_key = env::var("SESSION_SECRET_KEY").ok();

        let ssl_proxy = env::var("SSL_PROXY")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);

        // When SSL_PROXY is enabled, SESSION_SECRET_KEY is required
        if ssl_proxy && session_secret_key.is_none() {
            return Err(ConfigError::MissingSessionSecret);
        }

        // Build the key here so an unusable secret stops the process before it
        // opens the database, runs migrations and starts the workers.
        if let Some(secret) = &session_secret_key {
            Self::key_from_secret(secret)?;
        }

        Ok(Self {
            ssl_proxy,
            session_secret_key,
        })
    }

    /// Builds the master key used to sign and encrypt session cookies.
    pub fn session_key(&self) -> Result<Key, ConfigError> {
        match &self.session_secret_key {
            Some(secret) => Self::key_from_secret(secret),
            None => Ok(Key::generate()),
        }
    }

    fn key_from_secret(secret: &str) -> Result<Key, ConfigError> {
        let bytes = secret.as_bytes();

        if bytes.len() < Self::MIN_SECRET_LEN {
            return Err(ConfigError::SessionSecretTooShort { len: bytes.len() });
        }

        Ok(Key::from(bytes))
    }
}
