//! Benchmark runner for executing load tests.

use crate::config::{ScenarioConfig, ScenarioType};
use crate::envelope::{EnvelopeGenerator, EventConfig, PayloadKind};
use crate::metrics::MetricsCollector;
use crate::report::{BenchmarkResults, DrainMetrics, EndpointMetrics, LatencyMetrics};
use colored::Colorize;
use futures::stream::{self, StreamExt};
use hdrhistogram::Histogram;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::time::interval;

/// Runner errors
#[derive(Debug, Error)]
pub enum RunnerError {
    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),
    #[error("Metrics error: {0}")]
    MetricsError(#[from] crate::metrics::MetricsError),
    #[error("Server not ready after {0} seconds")]
    ServerNotReady(u64),
    #[error("Invalid server URL: {0}")]
    InvalidUrl(String),
    #[error("The '{0}' scenario needs a PostgreSQL connection (pass --postgres-url)")]
    PostgresRequired(String),
}

/// Request result
#[derive(Debug, Clone, Copy)]
pub struct RequestResult {
    /// Latency in microseconds
    pub latency_us: u64,
    /// HTTP status code
    pub status: u16,
    /// Whether the request was successful (2xx)
    pub success: bool,
}

/// Live statistics during benchmark
#[derive(Debug, Default)]
pub struct LiveStats {
    pub total_requests: AtomicU64,
    pub successful: AtomicU64,
    pub failed: AtomicU64,
    pub rate_limited: AtomicU64,
    pub server_errors: AtomicU64,
}

impl LiveStats {
    pub fn record(&self, result: &RequestResult) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);

        if result.success {
            self.successful.fetch_add(1, Ordering::Relaxed);
        } else {
            self.failed.fetch_add(1, Ordering::Relaxed);

            if result.status == 429 {
                self.rate_limited.fetch_add(1, Ordering::Relaxed);
            } else if result.status >= 500 {
                self.server_errors.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    pub fn snapshot(&self) -> StatsSnapshot {
        StatsSnapshot {
            total_requests: self.total_requests.load(Ordering::Relaxed),
            successful: self.successful.load(Ordering::Relaxed),
            failed: self.failed.load(Ordering::Relaxed),
            rate_limited: self.rate_limited.load(Ordering::Relaxed),
            server_errors: self.server_errors.load(Ordering::Relaxed),
        }
    }
}

/// Result of a bulk send phase.
pub struct SendOutcome {
    pub stats: StatsSnapshot,
    pub histogram: Histogram<u64>,
    pub elapsed: Duration,
    /// Successfully accepted error events (these become `events` rows)
    pub errors_ok: u64,
}

/// Snapshot of statistics at a point in time
#[derive(Debug, Clone, Default)]
pub struct StatsSnapshot {
    pub total_requests: u64,
    pub successful: u64,
    pub failed: u64,
    pub rate_limited: u64,
    pub server_errors: u64,
}

/// Benchmark runner
pub struct BenchmarkRunner {
    config: ScenarioConfig,
    server_url: String,
    project_id: u32,
    sentry_key: String,
    client: Client,
    container_name: Option<String>,
    postgres_container: Option<String>,
    postgres_url: Option<String>,
    api_token: Option<String>,
    label: Option<String>,
    repeat: u32,
}

impl BenchmarkRunner {
    /// Create a new benchmark runner
    pub fn new(
        config: ScenarioConfig,
        server_url: &str,
        project_id: u32,
        sentry_key: &str,
    ) -> Result<Self, RunnerError> {
        // Validate URL
        if !server_url.starts_with("http://") && !server_url.starts_with("https://") {
            return Err(RunnerError::InvalidUrl(server_url.to_string()));
        }

        let client = Client::builder()
            .pool_max_idle_per_host(config.concurrency as usize)
            .timeout(Duration::from_secs(30))
            .build()?;

        Ok(Self {
            config,
            server_url: server_url.trim_end_matches('/').to_string(),
            project_id,
            sentry_key: sentry_key.to_string(),
            client,
            container_name: None,
            postgres_container: None,
            postgres_url: None,
            api_token: None,
            label: None,
            repeat: 0,
        })
    }

    /// Set the container name for metrics collection
    pub fn with_container(mut self, container_name: &str) -> Self {
        self.container_name = Some(container_name.to_string());
        self
    }

    /// Set the PostgreSQL container name for resource metrics
    pub fn with_postgres_container(mut self, container_name: &str) -> Self {
        self.postgres_container = Some(container_name.to_string());
        self
    }

    /// Set the PostgreSQL connection string for engine statistics
    pub fn with_postgres_url(mut self, url: &str) -> Self {
        self.postgres_url = Some(url.to_string());
        self
    }

    /// Set the API token used by read-path requests
    pub fn with_api_token(mut self, token: &str) -> Self {
        self.api_token = Some(token.to_string());
        self
    }

    /// Label this run for later comparison
    pub fn with_label(mut self, label: &str, repeat: u32) -> Self {
        self.label = Some(label.to_string());
        self.repeat = repeat;
        self
    }

    /// Get the envelope endpoint URL
    fn envelope_url(&self) -> String {
        format!(
            "{}/api/{}/envelope/?sentry_key={}",
            self.server_url, self.project_id, self.sentry_key
        )
    }

    /// Wait for server to be ready
    pub async fn wait_for_server(&self, timeout_secs: u64) -> Result<(), RunnerError> {
        let health_url = format!("{}/health", self.server_url);
        let start = Instant::now();

        println!("{}", "Waiting for server to be ready...".dimmed());

        while start.elapsed() < Duration::from_secs(timeout_secs) {
            match self.client.get(&health_url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    println!("{}", "Server is ready!".green());
                    return Ok(());
                }
                _ => {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
        }

        Err(RunnerError::ServerNotReady(timeout_secs))
    }

    /// Send a single request and measure latency
    async fn send_request(&self, envelope: Vec<u8>) -> RequestResult {
        let start = Instant::now();

        let result = self
            .client
            .post(self.envelope_url())
            .header("Content-Type", "application/x-sentry-envelope")
            .header("Content-Encoding", "gzip")
            .body(envelope)
            .send()
            .await;

        let latency_us = start.elapsed().as_micros() as u64;

        match result {
            Ok(resp) => {
                let status = resp.status().as_u16();
                RequestResult {
                    latency_us,
                    status,
                    success: resp.status().is_success(),
                }
            }
            Err(_) => RequestResult {
                latency_us,
                status: 0,
                success: false,
            },
        }
    }

    /// Run warmup phase
    async fn warmup(&self, generator: &mut EnvelopeGenerator) {
        if self.config.warmup_secs == 0 {
            return;
        }

        let pb = ProgressBar::new(self.config.warmup_secs);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.yellow} {msg} [{bar:40.yellow}] {pos}/{len}s")
                .unwrap()
                .progress_chars("=> "),
        );
        pb.set_message("Warming up");

        let duration = Duration::from_secs(self.config.warmup_secs);
        let start = Instant::now();

        while start.elapsed() < duration {
            let envelope = generator.generate_compressed_payload(None);
            let _ = self.send_request(envelope).await;
            pb.set_position(start.elapsed().as_secs());
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        pb.finish_with_message("Warmup complete");
    }

    /// Run sustained load scenario
    async fn run_sustained(&self, generator: Arc<Mutex<EnvelopeGenerator>>) -> BenchmarkResults {
        let stats = Arc::new(LiveStats::default());
        let histogram = Arc::new(Mutex::new(
            Histogram::<u64>::new_with_bounds(1, 60_000_000, 3).unwrap(),
        ));

        let duration = Duration::from_secs(self.config.duration_secs);
        // 1 RPS if misconfigured with zero.
        let interval_ns = 1_000_000_000u64
            .checked_div(self.config.target_rps)
            .unwrap_or(1_000_000_000);

        let pb = ProgressBar::new(self.config.duration_secs);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} {msg} [{bar:40.green}] {pos}/{len}s | {per_sec}")
                .unwrap()
                .progress_chars("=> "),
        );
        pb.set_message("Running sustained load");

        let start = Instant::now();

        // Spawn worker tasks
        let mut handles = Vec::new();

        for _ in 0..self.config.concurrency {
            let client = self.client.clone();
            let url = self.envelope_url();
            let stats = stats.clone();
            let histogram = histogram.clone();
            let generator = generator.clone();
            let rate_limit = Duration::from_nanos(interval_ns * self.config.concurrency as u64);

            let handle = tokio::spawn(async move {
                let mut interval = interval(rate_limit);

                while start.elapsed() < duration {
                    interval.tick().await;

                    let envelope = {
                        let mut gen = generator.lock().await;
                        gen.generate_compressed_payload(None)
                    };

                    let req_start = Instant::now();
                    let result = client
                        .post(&url)
                        .header("Content-Type", "application/x-sentry-envelope")
                        .header("Content-Encoding", "gzip")
                        .body(envelope)
                        .send()
                        .await;

                    let latency_us = req_start.elapsed().as_micros() as u64;

                    let request_result = match result {
                        Ok(resp) => {
                            let status = resp.status().as_u16();
                            RequestResult {
                                latency_us,
                                status,
                                success: resp.status().is_success(),
                            }
                        }
                        Err(_) => RequestResult {
                            latency_us,
                            status: 0,
                            success: false,
                        },
                    };

                    stats.record(&request_result);

                    if let Ok(mut hist) = histogram.try_lock() {
                        let _ = hist.record(latency_us);
                    }
                }
            });

            handles.push(handle);
        }

        // Progress updates
        while start.elapsed() < duration {
            pb.set_position(start.elapsed().as_secs());
            let snapshot = stats.snapshot();
            let rps = snapshot.total_requests as f64 / start.elapsed().as_secs_f64();
            pb.set_message(format!(
                "RPS: {:.0} | OK: {} | Fail: {}",
                rps, snapshot.successful, snapshot.failed
            ));
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        // Wait for all workers
        for handle in handles {
            handle.abort();
        }

        pb.finish_with_message("Sustained load complete");

        let total_duration = start.elapsed();
        let snapshot = stats.snapshot();
        let hist = histogram.lock().await;

        BenchmarkResults::new(
            &self.config,
            snapshot,
            &hist,
            total_duration,
            None, // Metrics collected separately
        )
    }

    /// Run burst scenario
    async fn run_burst(&self, generator: Arc<Mutex<EnvelopeGenerator>>) -> BenchmarkResults {
        let stats = Arc::new(LiveStats::default());
        let histogram = Arc::new(Mutex::new(
            Histogram::<u64>::new_with_bounds(1, 60_000_000, 3).unwrap(),
        ));

        let burst_config = &self.config.burst;
        let total_bursts = burst_config.cycles;

        println!(
            "{} bursts of {} events with {}s pause",
            total_bursts.to_string().cyan(),
            burst_config.burst_size.to_string().cyan(),
            burst_config.pause_secs.to_string().cyan()
        );

        let start = Instant::now();

        for cycle in 0..total_bursts {
            let pb = ProgressBar::new(burst_config.burst_size);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template(&format!(
                        "{{spinner:.cyan}} Burst {}/{} [{{bar:40.cyan}}] {{pos}}/{{len}}",
                        cycle + 1,
                        total_bursts
                    ))
                    .unwrap()
                    .progress_chars("=> "),
            );

            // Send burst
            let requests: Vec<_> = (0..burst_config.burst_size)
                .map(|_| {
                    let generator = generator.clone();
                    async move {
                        let mut gen = generator.lock().await;
                        gen.generate_compressed_payload(None)
                    }
                })
                .collect();

            let envelopes: Vec<Vec<u8>> = futures::future::join_all(requests).await;

            let results: Vec<RequestResult> = stream::iter(envelopes)
                .map(|envelope| {
                    let client = self.client.clone();
                    let url = self.envelope_url();
                    async move {
                        let req_start = Instant::now();
                        let result = client
                            .post(&url)
                            .header("Content-Type", "application/x-sentry-envelope")
                            .header("Content-Encoding", "gzip")
                            .body(envelope)
                            .send()
                            .await;

                        let latency_us = req_start.elapsed().as_micros() as u64;

                        match result {
                            Ok(resp) => {
                                let status = resp.status().as_u16();
                                RequestResult {
                                    latency_us,
                                    status,
                                    success: resp.status().is_success(),
                                }
                            }
                            Err(_) => RequestResult {
                                latency_us,
                                status: 0,
                                success: false,
                            },
                        }
                    }
                })
                .buffer_unordered(self.config.concurrency as usize)
                .inspect(|_| pb.inc(1))
                .collect()
                .await;

            // Record results
            for result in &results {
                stats.record(result);
                if let Ok(mut hist) = histogram.try_lock() {
                    let _ = hist.record(result.latency_us);
                }
            }

            pb.finish();

            // Pause between bursts (except after last)
            if cycle < total_bursts - 1 {
                println!(
                    "{}",
                    format!("Pausing for {}s...", burst_config.pause_secs).dimmed()
                );
                tokio::time::sleep(Duration::from_secs(burst_config.pause_secs)).await;
            }
        }

        let total_duration = start.elapsed();
        let snapshot = stats.snapshot();
        let hist = histogram.lock().await;

        BenchmarkResults::new(&self.config, snapshot, &hist, total_duration, None)
    }

    /// Run baseline scenario
    async fn run_baseline(&self, generator: Arc<Mutex<EnvelopeGenerator>>) -> BenchmarkResults {
        let stats = Arc::new(LiveStats::default());
        let histogram = Arc::new(Mutex::new(
            Histogram::<u64>::new_with_bounds(1, 60_000_000, 3).unwrap(),
        ));

        let duration = Duration::from_secs(self.config.duration_secs);

        let pb = ProgressBar::new(self.config.duration_secs);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.blue} {msg} [{bar:40.blue}] {pos}/{len}s")
                .unwrap()
                .progress_chars("=> "),
        );
        pb.set_message("Baseline measurement");

        let start = Instant::now();

        while start.elapsed() < duration {
            let envelope = {
                let mut gen = generator.lock().await;
                gen.generate_compressed_payload(None)
            };

            let result = self.send_request(envelope).await;
            stats.record(&result);

            {
                let mut hist = histogram.lock().await;
                let _ = hist.record(result.latency_us);
            }

            pb.set_position(start.elapsed().as_secs());
            pb.set_message(format!(
                "Latency: {:.2}ms",
                result.latency_us as f64 / 1000.0
            ));

            // One request per second for baseline
            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        pb.finish_with_message("Baseline complete");

        let total_duration = start.elapsed();
        let snapshot = stats.snapshot();
        let hist = histogram.lock().await;

        BenchmarkResults::new(&self.config, snapshot, &hist, total_duration, None)
    }

    /// Run stress test scenario
    async fn run_stress(&self, generator: Arc<Mutex<EnvelopeGenerator>>) -> BenchmarkResults {
        let stats = Arc::new(LiveStats::default());
        let histogram = Arc::new(Mutex::new(
            Histogram::<u64>::new_with_bounds(1, 60_000_000, 3).unwrap(),
        ));

        let stress_config = &self.config.stress;
        let mut current_rps = stress_config.initial_rps;

        println!(
            "Starting stress test: {} -> {} RPS (step: {}, duration: {}s per step)",
            stress_config.initial_rps.to_string().cyan(),
            stress_config.max_rps.to_string().cyan(),
            stress_config.rps_increment.to_string().cyan(),
            stress_config.step_duration_secs.to_string().cyan()
        );

        let start = Instant::now();

        while current_rps <= stress_config.max_rps {
            let step_start = Instant::now();
            let step_duration = Duration::from_secs(stress_config.step_duration_secs);
            let interval_ns = 1_000_000_000 / current_rps;

            let pb = ProgressBar::new(stress_config.step_duration_secs);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template(&format!(
                        "{{spinner:.magenta}} {} RPS [{{bar:40.magenta}}] {{pos}}/{{len}}s | OK: {{msg}}",
                        current_rps
                    ))
                    .unwrap()
                    .progress_chars("=> "),
            );

            let step_stats = Arc::new(LiveStats::default());

            // Spawn workers for this step
            let mut handles = Vec::new();

            for _ in 0..self.config.concurrency {
                let client = self.client.clone();
                let url = self.envelope_url();
                let local_step_stats = step_stats.clone();
                let global_stats = stats.clone();
                let histogram = histogram.clone();
                let generator = generator.clone();
                let rate_limit = Duration::from_nanos(interval_ns * self.config.concurrency as u64);

                let handle = tokio::spawn(async move {
                    let mut interval = interval(rate_limit);

                    while step_start.elapsed() < step_duration {
                        interval.tick().await;

                        let envelope = {
                            let mut gen = generator.lock().await;
                            gen.generate_compressed_payload(None)
                        };

                        let req_start = Instant::now();
                        let result = client
                            .post(&url)
                            .header("Content-Type", "application/x-sentry-envelope")
                            .header("Content-Encoding", "gzip")
                            .body(envelope)
                            .send()
                            .await;

                        let latency_us = req_start.elapsed().as_micros() as u64;

                        let request_result = match result {
                            Ok(resp) => {
                                let status = resp.status().as_u16();
                                RequestResult {
                                    latency_us,
                                    status,
                                    success: resp.status().is_success(),
                                }
                            }
                            Err(_) => RequestResult {
                                latency_us,
                                status: 0,
                                success: false,
                            },
                        };

                        local_step_stats.record(&request_result);
                        global_stats.record(&request_result);

                        if let Ok(mut hist) = histogram.try_lock() {
                            let _ = hist.record(latency_us);
                        }
                    }
                });

                handles.push(handle);
            }

            // Progress updates
            while step_start.elapsed() < step_duration {
                pb.set_position(step_start.elapsed().as_secs());
                let snapshot = step_stats.snapshot();
                pb.set_message(format!(
                    "{} / fail: {}",
                    snapshot.successful, snapshot.failed
                ));
                tokio::time::sleep(Duration::from_millis(500)).await;
            }

            // Stop workers
            for handle in handles {
                handle.abort();
            }

            pb.finish();

            // Check error rate
            let snapshot = step_stats.snapshot();
            let error_rate = if snapshot.total_requests > 0 {
                snapshot.failed as f64 / snapshot.total_requests as f64
            } else {
                0.0
            };

            if error_rate > stress_config.error_threshold {
                println!(
                    "{}",
                    format!(
                        "Error threshold exceeded ({:.1}% > {:.1}%), stopping stress test",
                        error_rate * 100.0,
                        stress_config.error_threshold * 100.0
                    )
                    .red()
                );
                break;
            }

            current_rps += stress_config.rps_increment;
        }

        let total_duration = start.elapsed();
        let snapshot = stats.snapshot();
        let hist = histogram.lock().await;

        BenchmarkResults::new(&self.config, snapshot, &hist, total_duration, None)
    }

    /// Send a fixed number of events as fast as `concurrency` allows.
    ///
    /// Shared by the drain scenario and the read scenario's seeding phase.
    async fn send_events(
        &self,
        generator: Arc<Mutex<EnvelopeGenerator>>,
        count: u64,
        concurrency: u32,
        label: &str,
    ) -> SendOutcome {
        let stats = Arc::new(LiveStats::default());
        let histogram = Arc::new(Mutex::new(
            Histogram::<u64>::new_with_bounds(1, 60_000_000, 3).unwrap(),
        ));
        // Only error events: the drain wait needs to know how many rows to
        // expect in `events`, and transactions do not land there.
        let errors_ok = Arc::new(AtomicU64::new(0));

        let pb = ProgressBar::new(count);
        pb.set_style(
            ProgressStyle::default_bar()
                .template(&format!(
                    "{{spinner:.cyan}} {} [{{bar:40.cyan}}] {{pos}}/{{len}} | {{per_sec}}",
                    label
                ))
                .unwrap()
                .progress_chars("=> "),
        );

        let start = Instant::now();

        stream::iter(0..count)
            .map(|_| {
                let client = self.client.clone();
                let url = self.envelope_url();
                let generator = generator.clone();
                let stats = stats.clone();
                let histogram = histogram.clone();
                let pb = pb.clone();
                let errors_ok = errors_ok.clone();

                async move {
                    let (kind, envelope) = {
                        let mut gen = generator.lock().await;
                        gen.generate_compressed_payload_kinded(None)
                    };

                    let req_start = Instant::now();
                    let result = client
                        .post(&url)
                        .header("Content-Type", "application/x-sentry-envelope")
                        .header("Content-Encoding", "gzip")
                        .body(envelope)
                        .send()
                        .await;

                    let latency_us = req_start.elapsed().as_micros() as u64;

                    let request_result = match result {
                        Ok(resp) => {
                            let status = resp.status().as_u16();
                            RequestResult {
                                latency_us,
                                status,
                                success: resp.status().is_success(),
                            }
                        }
                        Err(_) => RequestResult {
                            latency_us,
                            status: 0,
                            success: false,
                        },
                    };

                    stats.record(&request_result);
                    if request_result.success && matches!(kind, PayloadKind::Error) {
                        errors_ok.fetch_add(1, Ordering::Relaxed);
                    }
                    if let Ok(mut hist) = histogram.try_lock() {
                        let _ = hist.record(latency_us);
                    }
                    pb.inc(1);
                }
            })
            .buffer_unordered(concurrency as usize)
            .collect::<Vec<_>>()
            .await;

        let elapsed = start.elapsed();
        pb.finish_and_clear();

        // Bound the guard so it is dropped before the struct is returned.
        let hist = {
            let guard = histogram.lock().await;
            guard.clone()
        };

        SendOutcome {
            stats: stats.snapshot(),
            histogram: hist,
            elapsed,
            errors_ok: errors_ok.load(Ordering::Relaxed),
        }
    }

    /// Wait until the digest pipeline has persisted `expected` rows.
    ///
    /// Returns (digested, drain_duration, peak_backlog, fully_drained).
    ///
    /// The wait ends early if the count stops advancing for long enough that the
    /// pipeline is clearly not going to catch up — otherwise a dropped event
    /// would burn the entire timeout on every run.
    async fn wait_for_drain(
        &self,
        pg: &crate::pgstats::PgStatsCollector,
        baseline: i64,
        expected: u64,
        timeout: Duration,
        poll_interval: Duration,
    ) -> (u64, Duration, u64, bool) {
        let target = baseline + expected as i64;
        let start = Instant::now();
        let mut peak_backlog: u64 = 0;
        let mut last_count = baseline;
        let mut last_progress = Instant::now();

        // If nothing lands for this long, treat the pipeline as settled. Events
        // can legitimately be dropped (rate limit re-check inside the digest),
        // so "target reached" cannot be the only exit condition.
        let stall_limit = Duration::from_secs(20);

        let pb = ProgressBar::new(expected);
        pb.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{spinner:.magenta} Draining backlog [{bar:40.magenta}] {pos}/{len} | {msg}",
                )
                .unwrap()
                .progress_chars("=> "),
        );

        loop {
            let current = pg
                .count_rows("events", Some(self.project_id))
                .await
                .unwrap_or(last_count);

            let digested = (current - baseline).max(0) as u64;
            let backlog = expected.saturating_sub(digested);
            peak_backlog = peak_backlog.max(backlog);

            pb.set_position(digested);
            pb.set_message(format!("backlog: {}", backlog));

            if current >= target {
                pb.finish_and_clear();
                return (digested, start.elapsed(), peak_backlog, true);
            }

            if current > last_count {
                last_count = current;
                last_progress = Instant::now();
            } else if last_progress.elapsed() > stall_limit {
                pb.finish_and_clear();
                println!(
                    "{}",
                    format!(
                        "Digest stalled at {}/{} events (no progress for {}s)",
                        digested,
                        expected,
                        stall_limit.as_secs()
                    )
                    .yellow()
                );
                return (digested, start.elapsed(), peak_backlog, false);
            }

            if start.elapsed() > timeout {
                pb.finish_and_clear();
                println!(
                    "{}",
                    format!("Digest timed out at {}/{} events", digested, expected).red()
                );
                return (digested, start.elapsed(), peak_backlog, false);
            }

            tokio::time::sleep(poll_interval).await;
        }
    }

    /// Run the digest-drain scenario.
    async fn run_drain(
        &self,
        generator: Arc<Mutex<EnvelopeGenerator>>,
        pg: &crate::pgstats::PgStatsCollector,
    ) -> BenchmarkResults {
        let drain_config = &self.config.drain;

        println!(
            "Sending {} events at concurrency {}, then waiting for the digest to drain",
            drain_config.event_count.to_string().cyan(),
            drain_config.send_concurrency.to_string().cyan()
        );

        let baseline = pg
            .count_rows("events", Some(self.project_id))
            .await
            .unwrap_or(0);

        let overall_start = Instant::now();
        let outcome = self
            .send_events(
                generator,
                drain_config.event_count,
                drain_config.send_concurrency,
                "Ingesting",
            )
            .await;

        // Wait only on the error events: transactions are written to a different
        // table, so counting them toward the `events` target would guarantee the
        // wait never completes.
        let (digested, drain_duration, peak_backlog, fully_drained) = self
            .wait_for_drain(
                pg,
                baseline,
                outcome.errors_ok,
                Duration::from_secs(drain_config.timeout_secs),
                Duration::from_millis(drain_config.poll_interval_ms),
            )
            .await;

        let total_duration = overall_start.elapsed();
        let total_secs = total_duration.as_secs_f64();

        let digest_metrics = DrainMetrics {
            events_sent: outcome.errors_ok,
            events_digested: digested,
            ingest_secs: outcome.elapsed.as_secs_f64(),
            drain_secs: drain_duration.as_secs_f64(),
            total_secs,
            digest_events_per_second: if total_secs > 0.0 {
                digested as f64 / total_secs
            } else {
                0.0
            },
            peak_backlog,
            fully_drained,
        };

        BenchmarkResults::new(
            &self.config,
            outcome.stats,
            &outcome.histogram,
            total_duration,
            None,
        )
        .with_digest_metrics(digest_metrics)
    }

    /// Run the read-path scenario.
    async fn run_read(
        &self,
        generator: Arc<Mutex<EnvelopeGenerator>>,
        pg: &crate::pgstats::PgStatsCollector,
    ) -> BenchmarkResults {
        let read_config = &self.config.read;

        // ---- Seed phase (not measured) ----
        println!(
            "Seeding {} events across ~{} issue groups...",
            read_config.seed_events.to_string().cyan(),
            read_config.distinct_groups.to_string().cyan()
        );

        let baseline = pg
            .count_rows("events", Some(self.project_id))
            .await
            .unwrap_or(0);
        let seed = self
            .send_events(
                generator,
                read_config.seed_events,
                self.config.concurrency.max(20),
                "Seeding",
            )
            .await;

        let (digested, _, _, drained) = self
            .wait_for_drain(
                pg,
                baseline,
                seed.errors_ok,
                Duration::from_secs(600),
                Duration::from_millis(250),
            )
            .await;

        println!(
            "{}",
            format!(
                "Seeded {} events ({})",
                digested,
                if drained { "drained" } else { "partial" }
            )
            .dimmed()
        );

        // Equalise planner statistics and flush dirty buffers, so the read
        // comparison measures the engine rather than which run happened to get
        // an autovacuum or a checkpoint mid-flight.
        println!("{}", "Running ANALYZE and CHECKPOINT...".dimmed());
        let _ = pg.analyze().await;
        let _ = pg.checkpoint().await;

        let issue_ids = self.fetch_issue_ids().await;
        println!(
            "{}",
            format!("Querying against {} issues", issue_ids.len()).dimmed()
        );

        // ---- Measured phase ----
        // Probe every endpoint once and drop the ones that do not return 2xx.
        // An endpoint that 404s answers in microseconds without touching the
        // tables it was meant to exercise, so leaving it in would quietly pull
        // the aggregate latency down and make the database look faster than it
        // is. Better to measure fewer queries than to measure error pages.
        let endpoints = self
            .validate_endpoints(self.read_endpoints(&issue_ids))
            .await;
        if endpoints.is_empty() {
            // Returning early rather than continuing: the worker loop selects an
            // endpoint with `index % endpoints.len()`, which would divide by
            // zero. An empty result is also the honest output here — measuring
            // nothing is not the same as measuring zero latency.
            eprintln!(
                "{}",
                "No read endpoints responded successfully; nothing to measure".red()
            );
            let empty = Histogram::<u64>::new_with_bounds(1, 60_000_000, 3).unwrap();
            return BenchmarkResults::new(
                &self.config,
                StatsSnapshot::default(),
                &empty,
                Duration::from_secs(0),
                None,
            )
            .with_endpoint_metrics(Vec::new());
        }

        let duration = Duration::from_secs(read_config.duration_secs);

        let stats = Arc::new(LiveStats::default());
        let combined = Arc::new(Mutex::new(
            Histogram::<u64>::new_with_bounds(1, 60_000_000, 3).unwrap(),
        ));
        let per_endpoint: Arc<Vec<Arc<Mutex<Histogram<u64>>>>> = Arc::new(
            endpoints
                .iter()
                .map(|_| {
                    Arc::new(Mutex::new(
                        Histogram::<u64>::new_with_bounds(1, 60_000_000, 3).unwrap(),
                    ))
                })
                .collect(),
        );
        let per_endpoint_counts: Arc<Vec<(AtomicU64, AtomicU64)>> = Arc::new(
            endpoints
                .iter()
                .map(|_| (AtomicU64::new(0), AtomicU64::new(0)))
                .collect(),
        );

        let pb = ProgressBar::new(read_config.duration_secs);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.blue} Read load [{bar:40.blue}] {pos}/{len}s | {msg}")
                .unwrap()
                .progress_chars("=> "),
        );

        let start = Instant::now();
        let endpoints = Arc::new(endpoints);
        let mut handles = Vec::new();

        for worker in 0..read_config.concurrency {
            let client = self.client.clone();
            let endpoints = endpoints.clone();
            let stats = stats.clone();
            let combined = combined.clone();
            let per_endpoint = per_endpoint.clone();
            let per_endpoint_counts = per_endpoint_counts.clone();
            let token = self.api_token.clone();

            handles.push(tokio::spawn(async move {
                // Stagger the starting endpoint per worker so all readers are
                // not hitting the same query at the same instant.
                let mut index = worker as usize % endpoints.len();

                while start.elapsed() < duration {
                    let (_, url) = &endpoints[index];

                    let req_start = Instant::now();
                    let mut request = client.get(url);
                    if let Some(ref token) = token {
                        request = request.bearer_auth(token);
                    }
                    let result = request.send().await;
                    let latency_us = req_start.elapsed().as_micros() as u64;

                    let request_result = match result {
                        Ok(resp) => RequestResult {
                            latency_us,
                            status: resp.status().as_u16(),
                            success: resp.status().is_success(),
                        },
                        Err(_) => RequestResult {
                            latency_us,
                            status: 0,
                            success: false,
                        },
                    };

                    stats.record(&request_result);
                    per_endpoint_counts[index].0.fetch_add(1, Ordering::Relaxed);
                    if request_result.success {
                        per_endpoint_counts[index].1.fetch_add(1, Ordering::Relaxed);
                    }

                    if let Ok(mut hist) = combined.try_lock() {
                        let _ = hist.record(latency_us);
                    }
                    if let Ok(mut hist) = per_endpoint[index].try_lock() {
                        let _ = hist.record(latency_us);
                    }

                    index = (index + 1) % endpoints.len();
                }
            }));
        }

        while start.elapsed() < duration {
            pb.set_position(start.elapsed().as_secs());
            let snapshot = stats.snapshot();
            pb.set_message(format!(
                "RPS: {:.0} | OK: {} | Fail: {}",
                snapshot.total_requests as f64 / start.elapsed().as_secs_f64().max(0.001),
                snapshot.successful,
                snapshot.failed
            ));
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        for handle in handles {
            handle.abort();
        }
        pb.finish_and_clear();

        let total_duration = start.elapsed();
        let snapshot = stats.snapshot();

        let mut endpoint_metrics = Vec::new();
        for (index, (name, _)) in endpoints.iter().enumerate() {
            let hist = per_endpoint[index].lock().await;
            let requests = per_endpoint_counts[index].0.load(Ordering::Relaxed);
            let successful = per_endpoint_counts[index].1.load(Ordering::Relaxed);

            endpoint_metrics.push(EndpointMetrics {
                endpoint: name.clone(),
                requests,
                successful,
                latency_ms: histogram_to_latency(&hist),
                requests_per_second: requests as f64 / total_duration.as_secs_f64().max(0.001),
            });
        }

        let combined_hist = combined.lock().await;

        BenchmarkResults::new(&self.config, snapshot, &combined_hist, total_duration, None)
            .with_endpoint_metrics(endpoint_metrics)
    }

    /// Fetch issue IDs to query during the read scenario.
    ///
    /// Issue IDs are UUID strings, and the list endpoint wraps results in an
    /// `items` array.
    async fn fetch_issue_ids(&self) -> Vec<String> {
        let url = format!(
            "{}/api/projects/{}/issues?limit=100",
            self.server_url, self.project_id
        );

        let mut request = self.client.get(&url);
        if let Some(ref token) = self.api_token {
            request = request.bearer_auth(token);
        }

        let Ok(resp) = request.send().await else {
            return Vec::new();
        };
        let Ok(body) = resp.json::<serde_json::Value>().await else {
            return Vec::new();
        };

        let items = body
            .get("items")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        items
            .iter()
            .filter_map(|item| {
                item.get("id")
                    .and_then(|id| id.as_str())
                    .map(str::to_string)
            })
            .collect()
    }

    /// Probe each endpoint once, keeping only those that return 2xx.
    async fn validate_endpoints(&self, endpoints: Vec<(String, String)>) -> Vec<(String, String)> {
        let mut valid = Vec::new();

        for (name, url) in endpoints {
            let mut request = self.client.get(&url);
            if let Some(ref token) = self.api_token {
                request = request.bearer_auth(token);
            }

            match request.send().await {
                Ok(resp) if resp.status().is_success() => valid.push((name, url)),
                Ok(resp) => {
                    eprintln!(
                        "{}",
                        format!(
                            "Skipping endpoint '{}': HTTP {} ({})",
                            name,
                            resp.status().as_u16(),
                            url
                        )
                        .yellow()
                    );
                }
                Err(e) => {
                    eprintln!(
                        "{}",
                        format!("Skipping endpoint '{}': {}", name, e).yellow()
                    );
                }
            }
        }

        valid
    }

    /// The read endpoints exercised by the read scenario.
    ///
    /// These are the queries the dashboard actually issues, chosen to cover
    /// different access shapes: a sorted+paginated scan, a deep page, a
    /// single-row lookup, a time-bucketed aggregate, and an event list.
    fn read_endpoints(&self, issue_ids: &[String]) -> Vec<(String, String)> {
        let base = format!("{}/api/projects/{}", self.server_url, self.project_id);
        let mut endpoints = vec![
            (
                "issues:digest_order".to_string(),
                format!("{}/issues?limit=50&sort=digest_order&order=desc", base),
            ),
            (
                "issues:last_seen".to_string(),
                format!("{}/issues?limit=50&sort=last_seen&order=desc", base),
            ),
            (
                "issues:event_count".to_string(),
                format!("{}/issues?limit=50&sort=event_count&order=desc", base),
            ),
            (
                "issues:deep_page".to_string(),
                format!("{}/issues?limit=50&offset=200&sort=last_seen", base),
            ),
        ];

        // Single-issue lookups only make sense once there is an issue to look
        // up, and the events list is nested under one.
        if let Some(issue_id) = issue_ids.first() {
            endpoints.push((
                "issue:detail".to_string(),
                format!("{}/issues/{}", base, issue_id),
            ));
            endpoints.push((
                "issue:stats".to_string(),
                format!("{}/issues/{}/stats", base, issue_id),
            ));
            endpoints.push((
                "events:list".to_string(),
                format!("{}/issues/{}/events?limit=50", base, issue_id),
            ));
        }

        endpoints
    }

    /// Run the benchmark scenario
    pub async fn run(&self) -> Result<BenchmarkResults, RunnerError> {
        println!(
            "\n{} {} {}",
            "Running scenario:".bold(),
            self.config.name.cyan().bold(),
            format!("({})", self.config.scenario_type).dimmed()
        );
        println!("{}", self.config.description.dimmed());
        println!();

        // Create event generator
        let event_config = EventConfig {
            breadcrumb_count: self.config.event.breadcrumb_count,
            stack_depth: self.config.event.stack_depth,
            include_user: self.config.event.include_user,
            include_tags: self.config.event.include_tags,
            include_extra: self.config.event.include_extra,
            environment: "benchmark".to_string(),
            release: "rustrak-bench@0.1.0".to_string(),
            error_type: "BenchmarkError".to_string(),
            distinct_groups: self.config.event.distinct_groups,
            transaction_ratio: self.config.event.transaction_ratio,
            spans_per_transaction: self.config.event.spans_per_transaction,
        };
        let generator = Arc::new(Mutex::new(EnvelopeGenerator::new(event_config)));

        // Warmup
        {
            let mut gen = generator.lock().await;
            self.warmup(&mut gen).await;
        }

        // Connect to PostgreSQL for engine statistics. The drain and read
        // scenarios require it (they need to observe digest progress); the
        // others degrade gracefully without it.
        let mut pg_collector = match self.postgres_url {
            Some(ref url) => match crate::pgstats::PgStatsCollector::connect(url).await {
                Ok(collector) => {
                    println!(
                        "{}",
                        format!(
                            "Connected to PostgreSQL {} for engine statistics",
                            collector.server().major_version
                        )
                        .dimmed()
                    );
                    Some(collector)
                }
                Err(e) => {
                    eprintln!(
                        "{}",
                        format!("Warning: could not connect to PostgreSQL: {}", e).yellow()
                    );
                    None
                }
            },
            None => None,
        };

        let needs_pg = matches!(
            self.config.scenario_type,
            ScenarioType::Drain | ScenarioType::Read
        );
        if needs_pg && pg_collector.is_none() {
            return Err(RunnerError::PostgresRequired(
                self.config.scenario_type.to_string(),
            ));
        }

        // Start container metrics collection
        let server_metrics = self
            .start_container_metrics(self.container_name.as_deref())
            .await;
        let postgres_metrics = self
            .start_container_metrics(self.postgres_container.as_deref())
            .await;

        if let Some(ref mut collector) = pg_collector {
            collector.begin().await;
        }

        // Run the appropriate scenario
        let mut results = match self.config.scenario_type {
            ScenarioType::Baseline => self.run_baseline(generator).await,
            ScenarioType::Burst => self.run_burst(generator).await,
            ScenarioType::Sustained => self.run_sustained(generator).await,
            ScenarioType::Stress => self.run_stress(generator).await,
            ScenarioType::Drain => {
                let pg = pg_collector.as_ref().expect("checked above");
                self.run_drain(generator, pg).await
            }
            ScenarioType::Read => {
                let pg = pg_collector.as_ref().expect("checked above");
                self.run_read(generator, pg).await
            }
        };

        // Stop metrics collection and attach results
        if let Some(collector) = server_metrics {
            results = results.with_container_metrics(collector.stop().await);
        }
        if let Some(collector) = postgres_metrics {
            results = results.with_postgres_container_metrics(collector.stop().await);
        }
        if let Some(ref collector) = pg_collector {
            results = results.with_postgres_stats(collector.finish().await);
        }
        if let Some(ref label) = self.label {
            results = results.with_label(label, self.repeat);
        }

        Ok(results)
    }

    /// Start a metrics collector for a container, warning but not failing if it
    /// is unavailable.
    async fn start_container_metrics(&self, container: Option<&str>) -> Option<MetricsCollector> {
        let container = container?;

        match MetricsCollector::new(container).await {
            Ok(collector) => {
                collector.start();
                Some(collector)
            }
            Err(e) => {
                eprintln!(
                    "{}",
                    format!(
                        "Warning: could not collect metrics for '{}': {}",
                        container, e
                    )
                    .yellow()
                );
                None
            }
        }
    }
}

/// Convert an HDR histogram of microsecond samples into millisecond percentiles.
fn histogram_to_latency(histogram: &Histogram<u64>) -> LatencyMetrics {
    if histogram.is_empty() {
        return LatencyMetrics {
            p50: 0.0,
            p95: 0.0,
            p99: 0.0,
            max: 0.0,
            min: 0.0,
            mean: 0.0,
        };
    }

    LatencyMetrics {
        p50: histogram.value_at_percentile(50.0) as f64 / 1000.0,
        p95: histogram.value_at_percentile(95.0) as f64 / 1000.0,
        p99: histogram.value_at_percentile(99.0) as f64 / 1000.0,
        max: histogram.max() as f64 / 1000.0,
        min: histogram.min() as f64 / 1000.0,
        mean: histogram.mean() / 1000.0,
    }
}
