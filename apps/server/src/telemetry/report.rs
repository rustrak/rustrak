//! The document that goes on the wire, and the sink that carries it.
//!
//! The report is Rustrak's own shape, versioned by `schema`. A sink adapts it
//! to one backend; `posthog` is the one shipped, and swapping it means one
//! new `Sink` and one line in `main`.

use async_trait::async_trait;
use serde::Serialize;

use super::{Health, Rss};

/// The payload version. Bump when a field changes meaning, not when one is added.
pub const SCHEMA: u32 = 1;

/// Everything one heartbeat carries. The docs page lists these same fields.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Report {
    pub schema: u32,
    /// Random UUIDv4, generated once and stored in `installation`.
    #[serde(skip)]
    pub instance_id: String,
    pub version: String,
    pub os: String,
    pub arch: String,
    pub container: bool,
    pub db_backend: String,
    pub db_version: Option<String>,
    pub dashboard_served: bool,
    pub uptime_secs: u64,
    pub first_since_boot: bool,
    pub cpu_count: u64,
    pub mem_total_mb: Option<u64>,
    pub rss_mb: Rss,
    pub sqlite_db_mb: Option<u64>,
    pub ingest_dir_pending: u64,
    pub volume: Volume,
    pub health: Health,
    pub config: ConfigFacts,
}

/// Counts, already blurred to two significant digits.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct Volume {
    pub projects: u64,
    pub users: u64,
    pub issues_open: u64,
    pub events_24h: u64,
    pub transactions_24h: u64,
    pub sessions_24h: u64,
    pub logs_24h: u64,
}

/// Which knobs are set. Booleans and kinds only, never values.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct ConfigFacts {
    pub ssl_proxy: bool,
    pub public_url_set: bool,
    pub smtp_configured: bool,
    pub session_secret_set: bool,
    pub alert_providers: Vec<String>,
    /// Whether any quota limit differs from its default. Never the limits.
    pub quota_customized: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum SinkError {
    #[error("{0}")]
    Transport(String),
    #[error("the sink answered {0}")]
    Status(u16),
}

/// Where a report goes. One implementation per backend.
#[async_trait]
pub trait Sink: Send + Sync {
    /// For the startup log line.
    fn name(&self) -> &'static str;
    async fn send(&self, report: &Report) -> Result<(), SinkError>;
}
