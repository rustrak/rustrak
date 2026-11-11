//! Configuration parsing for benchmark scenarios.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Configuration errors
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Failed to read {}: {source}", path.display())]
    IoError {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("Failed to parse TOML: {0}")]
    TomlError(#[from] toml::de::Error),
}

/// Scenario type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ScenarioType {
    /// Single request baseline measurement
    Baseline,
    /// Burst traffic pattern
    Burst,
    /// Sustained constant load
    #[default]
    Sustained,
    /// Stress test to find limits
    Stress,
    /// Measure the asynchronous digest pipeline, not just HTTP ingest
    Drain,
    /// Measure the read path (dashboard queries) against a populated database
    Read,
}

impl std::fmt::Display for ScenarioType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Baseline => write!(f, "baseline"),
            Self::Burst => write!(f, "burst"),
            Self::Sustained => write!(f, "sustained"),
            Self::Stress => write!(f, "stress"),
            Self::Drain => write!(f, "drain"),
            Self::Read => write!(f, "read"),
        }
    }
}

/// Event configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventConfig {
    /// Number of breadcrumbs per event
    #[serde(default = "default_breadcrumb_count")]
    pub breadcrumb_count: usize,
    /// Stack trace depth
    #[serde(default = "default_stack_depth")]
    pub stack_depth: usize,
    /// Include user context
    #[serde(default = "default_true")]
    pub include_user: bool,
    /// Include tags
    #[serde(default = "default_true")]
    pub include_tags: bool,
    /// Include extra data
    #[serde(default)]
    pub include_extra: bool,
    /// How many distinct issues events should group into (see
    /// [`crate::envelope::EventConfig::distinct_groups`]). Omit for one issue
    /// per event.
    #[serde(default)]
    pub distinct_groups: Option<u32>,
    /// Fraction of payloads sent as transactions instead of error events
    /// (0.0-1.0). Defaults to 0.0: errors only, matching the original
    /// behaviour.
    #[serde(default)]
    pub transaction_ratio: f64,
    /// Child spans per transaction.
    #[serde(default = "default_spans_per_transaction")]
    pub spans_per_transaction: usize,
}

fn default_spans_per_transaction() -> usize {
    10
}

fn default_breadcrumb_count() -> usize {
    5
}

fn default_stack_depth() -> usize {
    10
}

fn default_true() -> bool {
    true
}

impl Default for EventConfig {
    fn default() -> Self {
        Self {
            breadcrumb_count: default_breadcrumb_count(),
            stack_depth: default_stack_depth(),
            include_user: true,
            include_tags: true,
            include_extra: false,
            distinct_groups: None,
            transaction_ratio: 0.0,
            spans_per_transaction: default_spans_per_transaction(),
        }
    }
}

/// Burst pattern configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BurstConfig {
    /// Number of events per burst
    #[serde(default = "default_burst_size")]
    pub burst_size: u64,
    /// Pause between bursts in seconds
    #[serde(default = "default_pause_secs")]
    pub pause_secs: u64,
    /// Number of burst cycles
    #[serde(default = "default_cycles")]
    pub cycles: u32,
}

fn default_burst_size() -> u64 {
    1000
}

fn default_pause_secs() -> u64 {
    5
}

fn default_cycles() -> u32 {
    5
}

impl Default for BurstConfig {
    fn default() -> Self {
        Self {
            burst_size: default_burst_size(),
            pause_secs: default_pause_secs(),
            cycles: default_cycles(),
        }
    }
}

/// Stress test configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StressConfig {
    /// Initial requests per second
    #[serde(default = "default_initial_rps")]
    pub initial_rps: u64,
    /// RPS increment per step
    #[serde(default = "default_rps_increment")]
    pub rps_increment: u64,
    /// Duration of each step in seconds
    #[serde(default = "default_step_duration")]
    pub step_duration_secs: u64,
    /// Maximum RPS to attempt
    #[serde(default = "default_max_rps")]
    pub max_rps: u64,
    /// Error rate threshold to stop (0.0-1.0)
    #[serde(default = "default_error_threshold")]
    pub error_threshold: f64,
}

fn default_initial_rps() -> u64 {
    100
}

fn default_rps_increment() -> u64 {
    100
}

fn default_step_duration() -> u64 {
    30
}

fn default_max_rps() -> u64 {
    10000
}

fn default_error_threshold() -> f64 {
    0.05
}

impl Default for StressConfig {
    fn default() -> Self {
        Self {
            initial_rps: default_initial_rps(),
            rps_increment: default_rps_increment(),
            step_duration_secs: default_step_duration(),
            max_rps: default_max_rps(),
            error_threshold: default_error_threshold(),
        }
    }
}

/// Digest-drain configuration.
///
/// The HTTP ingest endpoint returns as soon as the event is written to the
/// filesystem; the database work (grouping, issue upsert, event insert) happens
/// in a spawned task afterwards. Measuring only the HTTP response therefore says
/// almost nothing about the database — which is precisely what a PostgreSQL
/// version comparison needs to see. This scenario sends a fixed number of events
/// and then waits for the backlog to reach zero, timing the digest itself.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrainConfig {
    /// Total events to send before waiting for the digest to complete
    #[serde(default = "default_drain_event_count")]
    pub event_count: u64,
    /// In-flight requests while sending
    #[serde(default = "default_drain_send_concurrency")]
    pub send_concurrency: u32,
    /// How often to poll the digested-row count
    #[serde(default = "default_drain_poll_interval_ms")]
    pub poll_interval_ms: u64,
    /// Give up waiting after this long
    #[serde(default = "default_drain_timeout_secs")]
    pub timeout_secs: u64,
}

fn default_drain_event_count() -> u64 {
    20_000
}

fn default_drain_send_concurrency() -> u32 {
    50
}

fn default_drain_poll_interval_ms() -> u64 {
    250
}

fn default_drain_timeout_secs() -> u64 {
    600
}

impl Default for DrainConfig {
    fn default() -> Self {
        Self {
            event_count: default_drain_event_count(),
            send_concurrency: default_drain_send_concurrency(),
            poll_interval_ms: default_drain_poll_interval_ms(),
            timeout_secs: default_drain_timeout_secs(),
        }
    }
}

/// Read-path configuration.
///
/// The dashboard queries are the most database-bound thing Rustrak does:
/// sorting and paginating issues, aggregating event counts, fetching an issue's
/// latest event. This scenario seeds the database, then measures those queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadConfig {
    /// Events to ingest and fully digest before measuring
    #[serde(default = "default_read_seed_events")]
    pub seed_events: u64,
    /// How long to run the read load
    #[serde(default = "default_read_duration")]
    pub duration_secs: u64,
    /// Concurrent readers
    #[serde(default = "default_read_concurrency")]
    pub concurrency: u32,
    /// Distinct issue groups to spread the seed across.
    ///
    /// One group would make every query hit a single hot row and measure
    /// nothing; this controls how much the planner actually has to work.
    #[serde(default = "default_read_distinct_groups")]
    pub distinct_groups: u32,
}

fn default_read_seed_events() -> u64 {
    30_000
}

fn default_read_duration() -> u64 {
    60
}

fn default_read_concurrency() -> u32 {
    20
}

fn default_read_distinct_groups() -> u32 {
    500
}

impl Default for ReadConfig {
    fn default() -> Self {
        Self {
            seed_events: default_read_seed_events(),
            duration_secs: default_read_duration(),
            concurrency: default_read_concurrency(),
            distinct_groups: default_read_distinct_groups(),
        }
    }
}

/// Benchmark scenario configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioConfig {
    /// Scenario name
    pub name: String,
    /// Scenario description
    #[serde(default)]
    pub description: String,
    /// Scenario type
    #[serde(default)]
    pub scenario_type: ScenarioType,
    /// Test duration in seconds
    #[serde(default = "default_duration")]
    pub duration_secs: u64,
    /// Target requests per second (for sustained load)
    #[serde(default = "default_target_rps")]
    pub target_rps: u64,
    /// Number of concurrent connections
    #[serde(default = "default_concurrency")]
    pub concurrency: u32,
    /// Warmup period in seconds
    #[serde(default = "default_warmup")]
    pub warmup_secs: u64,
    /// Event configuration
    #[serde(default)]
    pub event: EventConfig,
    /// Burst-specific configuration
    #[serde(default)]
    pub burst: BurstConfig,
    /// Stress test configuration
    #[serde(default)]
    pub stress: StressConfig,
    /// Digest-drain configuration
    #[serde(default)]
    pub drain: DrainConfig,
    /// Read-path configuration
    #[serde(default)]
    pub read: ReadConfig,
}

fn default_duration() -> u64 {
    60
}

fn default_target_rps() -> u64 {
    100
}

fn default_concurrency() -> u32 {
    10
}

fn default_warmup() -> u64 {
    5
}

impl Default for ScenarioConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            description: "Default benchmark scenario".to_string(),
            scenario_type: ScenarioType::default(),
            duration_secs: default_duration(),
            target_rps: default_target_rps(),
            concurrency: default_concurrency(),
            warmup_secs: default_warmup(),
            event: EventConfig::default(),
            burst: BurstConfig::default(),
            stress: StressConfig::default(),
            drain: DrainConfig::default(),
            read: ReadConfig::default(),
        }
    }
}

impl ScenarioConfig {
    /// A scenario by name, `scenarios/<name>.toml`, or by path to any TOML.
    pub fn load(name_or_path: &str) -> Result<Self, ConfigError> {
        let path = Path::new(name_or_path);
        let path = if path.extension().is_some_and(|e| e == "toml") {
            path.to_path_buf()
        } else {
            scenarios_dir().join(format!("{name_or_path}.toml"))
        };
        let content = std::fs::read_to_string(&path)
            .map_err(|source| ConfigError::IoError { path, source })?;
        Ok(toml::from_str(&content)?)
    }
}

/// The scenarios `--scenario <name>` resolves to.
pub fn scenarios_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("scenarios")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every shipped scenario parses, including the matrix's own.
    #[test]
    fn every_scenario_file_loads() {
        let dir = scenarios_dir();
        let files: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .chain(std::fs::read_dir(dir.join("matrix")).unwrap())
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|e| e == "toml"))
            .collect();
        assert!(files.len() >= 6);
        for path in files {
            let config =
                ScenarioConfig::load(path.to_str().unwrap()).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(config.name, path.file_stem().unwrap().to_str().unwrap());
        }
    }

    #[test]
    fn a_name_resolves_to_its_file() {
        assert_eq!(
            ScenarioConfig::load("read").unwrap().scenario_type,
            ScenarioType::Read
        );
        assert!(ScenarioConfig::load("unknown").is_err());
    }

    #[test]
    fn test_parse_toml() {
        let toml_str = r#"
            name = "test"
            description = "Test scenario"
            scenario_type = "sustained"
            duration_secs = 120
            target_rps = 500
            concurrency = 20
            warmup_secs = 5

            [event]
            breadcrumb_count = 3
            stack_depth = 5
            include_user = true
            include_tags = false
        "#;

        let config: ScenarioConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.name, "test");
        assert_eq!(config.duration_secs, 120);
        assert_eq!(config.target_rps, 500);
        assert_eq!(config.event.breadcrumb_count, 3);
        assert!(!config.event.include_tags);
    }
}
