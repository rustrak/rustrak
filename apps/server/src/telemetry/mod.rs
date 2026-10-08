//! Anonymous usage telemetry.
//!
//! One small report every few hours, keyed by a random instance id, carrying
//! version, platform and aggregate counters. Nothing that came from an SDK, a
//! user or a project ever leaves the box. The full field list lives in the
//! docs under `configuration/telemetry`.

pub mod counters;
pub mod identity;
pub mod metrics;
pub mod posthog;
pub mod report;
pub mod reporter;
pub mod resources;
pub mod volume;

pub use counters::{Counters, Health, Rejection};
pub use identity::instance_id;
pub use report::{ConfigFacts, Report, Sink, SinkError, Volume, SCHEMA};
pub use reporter::{Context, Reporter, Schedule};
pub use resources::{probe, Rss, Sampler};
pub use volume::alert_providers;

use crate::config::TelemetryConfig;

/// Why nothing will be sent. Each variant reads as the startup log line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disabled {
    /// `RUSTRAK_TELEMETRY=off`.
    SwitchedOff,
    /// `DO_NOT_TRACK=1`, the cross-tool convention.
    DoNotTrack,
    /// The binary was built without `RUSTRAK_TELEMETRY_KEY`: a local build, a
    /// fork or a contributor's `cargo run`.
    NoKeyCompiledIn,
}

impl std::fmt::Display for Disabled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Disabled::SwitchedOff => write!(f, "RUSTRAK_TELEMETRY is off"),
            Disabled::DoNotTrack => write!(f, "DO_NOT_TRACK is set"),
            Disabled::NoKeyCompiledIn => {
                write!(f, "no telemetry key was compiled into this binary")
            }
        }
    }
}

/// The outcome of [`decide`], kept for the preview endpoint and the startup line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelemetryStatus {
    Enabled { sink: &'static str },
    Disabled(Disabled),
}

/// Whether to send anything at all. The operator's switch is checked first so
/// the log line names the reason the operator chose, not an incidental one.
pub fn decide(config: &TelemetryConfig, compiled_key: Option<&str>) -> Result<(), Disabled> {
    if !config.enabled {
        return Err(Disabled::SwitchedOff);
    }
    if config.do_not_track {
        return Err(Disabled::DoNotTrack);
    }
    match compiled_key {
        Some(key) if !key.is_empty() => Ok(()),
        _ => Err(Disabled::NoKeyCompiledIn),
    }
}

/// Keeps the two leading digits of a count and zeroes the rest, so an exact
/// figure cannot single out an instance. `123_456` reads as `120_000`.
pub fn blur_count(n: u64) -> u64 {
    if n < 100 {
        return n;
    }
    let magnitude = 10u64.pow(n.ilog10() - 1);
    n / magnitude * magnitude
}

/// `file:line` when `file` is a path inside this crate, which rustc records
/// relative to the crate root. Dependencies and the standard library come
/// through as absolute paths and are not ours to report.
pub fn own_location(file: &str, line: u32) -> Option<String> {
    let relative = !file.starts_with('/') && !file.starts_with('\\') && !file.contains(":\\");
    relative.then(|| format!("{file}:{line}"))
}

/// Counts panics by location on top of whatever hook was already installed,
/// so the default stderr report still happens. Installing twice chains twice,
/// so call it once at startup.
pub fn install_panic_hook(counters: &'static Counters) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(key) = info
            .location()
            .and_then(|l| own_location(l.file(), l.line()))
        {
            counters.panic(&key);
        }
        previous(info);
    }));
}

/// `3.46.1` reads as `3.46`, `16.4 (Debian 16.4-1)` as `16.4`.
pub fn major_minor(raw: &str) -> String {
    raw.split(|c: char| !c.is_ascii_digit() && c != '.')
        .next()
        .unwrap_or("")
        .split('.')
        .take(2)
        .collect::<Vec<_>>()
        .join(".")
}

/// The file behind a `sqlite:` `DATABASE_URL`, for its size. `None` for any
/// other engine and for an in-memory database.
pub fn sqlite_path_from_url(url: &str) -> Option<std::path::PathBuf> {
    let rest = url.strip_prefix("sqlite:")?;
    let rest = rest.strip_prefix("//").unwrap_or(rest);
    let path = rest.split('?').next().unwrap_or("");
    (!path.is_empty() && path != ":memory:").then(|| std::path::PathBuf::from(path))
}
