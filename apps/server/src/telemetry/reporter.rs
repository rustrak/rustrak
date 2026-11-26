//! The background task: samples the process every minute, builds one report
//! per window and hands it to the sink. Runs beside the other workers and
//! never touches the request path.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::db::DbPool;
use crate::error::AppResult;

use super::{
    instance_id, probe, ConfigFacts, Counters, Plugins, Report, Sampler, Sink, Volume, SCHEMA,
};

/// What `main` knows and the reporter cannot find out by itself.
#[derive(Debug, Clone)]
pub struct Context {
    pub dashboard_served: bool,
    pub config: ConfigFacts,
    /// The SQLite file, when that is the backend, for its size.
    pub sqlite_path: Option<PathBuf>,
    pub ingest_dir: PathBuf,
}

#[derive(Debug, Clone, Copy)]
pub struct Schedule {
    /// Before the first report: long enough to read the startup notice and
    /// set the switch, and to skip a container stuck in a restart loop.
    pub initial_delay: Duration,
    pub interval: Duration,
    pub sample_every: Duration,
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            initial_delay: Duration::from_secs(10 * 60),
            interval: Duration::from_secs(6 * 60 * 60),
            sample_every: Duration::from_secs(60),
        }
    }
}

pub struct Reporter {
    pool: DbPool,
    sink: Arc<dyn Sink>,
    counters: &'static Counters,
    sampler: Sampler,
    context: Context,
    schedule: Schedule,
    boot: Instant,
    reported_once: AtomicBool,
}

impl Reporter {
    pub fn new(
        pool: DbPool,
        sink: Arc<dyn Sink>,
        counters: &'static Counters,
        context: Context,
    ) -> Self {
        Self {
            pool,
            sink,
            counters,
            sampler: Sampler::new(),
            context,
            schedule: Schedule::default(),
            boot: Instant::now(),
            reported_once: AtomicBool::new(false),
        }
    }

    pub fn with_schedule(mut self, schedule: Schedule) -> Self {
        self.schedule = schedule;
        self
    }

    pub fn sink_name(&self) -> &'static str {
        self.sink.name()
    }

    /// Everything the next heartbeat will say. Drains the counter window.
    pub async fn build_report(&self) -> AppResult<Report> {
        self.assemble(true).await
    }

    /// The same document, without touching the window or the boot flag.
    pub async fn preview(&self) -> AppResult<Report> {
        self.assemble(false).await
    }

    async fn assemble(&self, drain: bool) -> AppResult<Report> {
        let disk = self.read_disk().await;
        if let Some(mb) = disk.rss_mb {
            self.sampler.record_rss_mb(mb);
        }
        let (first_since_boot, rss_mb, health) = if drain {
            (
                !self.reported_once.swap(true, Relaxed),
                self.sampler.snapshot_and_reset(),
                self.counters.snapshot_and_reset(),
            )
        } else {
            (
                !self.reported_once.load(Relaxed),
                self.sampler.snapshot(),
                self.counters.snapshot(),
            )
        };
        Ok(Report {
            schema: SCHEMA,
            instance_id: instance_id(&self.pool).await?,
            version: env!("CARGO_PKG_VERSION").to_string(),
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            container: disk.container,
            db_backend: crate::db::backend_name().to_string(),
            db_version: crate::db::engine_version(&self.pool).await.ok(),
            dashboard_served: self.context.dashboard_served,
            uptime_secs: self.boot.elapsed().as_secs(),
            first_since_boot,
            cpu_count: std::thread::available_parallelism().map_or(1, |n| n.get() as u64),
            mem_total_mb: disk.mem_total_mb.map(super::blur_count),
            rss_mb,
            sqlite_db_mb: disk.sqlite_db_mb,
            ingest_dir_pending: super::blur_count(disk.ingest_dir_pending),
            volume: Volume::collect(&self.pool).await?,
            plugins: Plugins::collect(&self.pool).await?,
            health,
            config: ConfigFacts {
                alert_providers: super::alert_providers(&self.pool).await?,
                github_connected: super::github_connected(&self.pool).await?,
                ..self.context.config.clone()
            },
        })
    }

    /// Shared with the preview endpoint, hence the `Arc`.
    pub async fn run(self: Arc<Self>) {
        tokio::time::sleep(self.schedule.initial_delay).await;
        loop {
            self.report_once().await;
            let mut slept = Duration::ZERO;
            while slept < self.schedule.interval {
                let step = self
                    .schedule
                    .sample_every
                    .min(self.schedule.interval - slept);
                tokio::time::sleep(step).await;
                slept += step;
                self.sample_rss().await;
            }
        }
    }

    /// One attempt. A miss is a `debug` line and the next window's problem:
    /// an air-gapped box must not fill its logs every few hours.
    async fn report_once(&self) {
        let report = match self.build_report().await {
            Ok(report) => report,
            Err(e) => {
                log::debug!("telemetry: could not build the report: {e}");
                return;
            }
        };
        log::debug!(
            "telemetry: sending to {}: {}",
            self.sink.name(),
            serde_json::to_string(&report).unwrap_or_default()
        );
        if let Err(e) = self.sink.send(&report).await {
            log::debug!(
                "telemetry: {} did not take the report: {e}",
                self.sink.name()
            );
        }
    }

    /// Everything that touches the filesystem, on the blocking pool: the
    /// preview runs on a request and a large ingest backlog makes `read_dir`
    /// slow. Falls back to nothing rather than failing the report.
    async fn read_disk(&self) -> DiskFacts {
        let sqlite_path = self.context.sqlite_path.clone();
        let ingest_dir = self.context.ingest_dir.clone();
        tokio::task::spawn_blocking(move || DiskFacts {
            rss_mb: probe::rss_mb(),
            mem_total_mb: probe::mem_total_mb(),
            container: probe::container(),
            sqlite_db_mb: sqlite_path
                .and_then(|p| std::fs::metadata(p).ok())
                .map(|m| super::blur_count(m.len() / (1024 * 1024))),
            ingest_dir_pending: std::fs::read_dir(ingest_dir)
                .map(|entries| entries.count() as u64)
                .unwrap_or(0),
        })
        .await
        .unwrap_or_default()
    }

    /// The per-minute sample, also off the async thread.
    async fn sample_rss(&self) {
        if let Ok(Some(mb)) = tokio::task::spawn_blocking(probe::rss_mb).await {
            self.sampler.record_rss_mb(mb);
        }
    }
}

#[derive(Default)]
struct DiskFacts {
    rss_mb: Option<u64>,
    mem_total_mb: Option<u64>,
    container: bool,
    sqlite_db_mb: Option<u64>,
    ingest_dir_pending: u64,
}
