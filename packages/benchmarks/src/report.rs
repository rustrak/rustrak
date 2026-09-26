//! Benchmark report generation.

use crate::config::ScenarioConfig;
use crate::metrics::ContainerMetrics;
use crate::runner::StatsSnapshot;
use chrono::{DateTime, Utc};
use colored::Colorize;
use hdrhistogram::Histogram;
use rand::RngExt;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::time::Duration;

/// Throughput metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThroughputMetrics {
    /// Total requests sent
    pub total_requests: u64,
    /// Successful requests (2xx)
    pub successful: u64,
    /// Failed requests
    pub failed: u64,
    /// Achieved events per second
    pub events_per_second: f64,
}

/// Latency metrics in milliseconds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyMetrics {
    /// 50th percentile (median)
    pub p50: f64,
    /// 95th percentile
    pub p95: f64,
    /// 99th percentile
    pub p99: f64,
    /// Maximum latency
    pub max: f64,
    /// Minimum latency
    pub min: f64,
    /// Mean latency
    pub mean: f64,
}

/// Memory metrics in megabytes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMetricsReport {
    /// Memory at idle (before test)
    pub idle_mb: f64,
    /// Peak memory usage
    pub peak_mb: f64,
    /// Average memory usage
    pub average_mb: f64,
    /// Memory limit if set
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_mb: Option<f64>,
}

/// CPU metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuMetricsReport {
    /// Peak CPU usage percentage
    pub peak_percent: f64,
    /// Average CPU usage percentage
    pub average_percent: f64,
}

/// Error breakdown
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorMetrics {
    /// Rate limited requests (429)
    pub rate_limited_429: u64,
    /// Server errors (5xx)
    pub server_error_5xx: u64,
    /// Connection failures
    pub connection_failed: u64,
}

/// Digest pipeline metrics.
///
/// Rustrak acknowledges an event as soon as it is written to the filesystem and
/// does the database work afterwards in a spawned task. HTTP latency therefore
/// measures the acknowledgement, not the storage — these figures measure the
/// storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrainMetrics {
    /// Events accepted over HTTP
    pub events_sent: u64,
    /// Events actually persisted by the digest pipeline
    pub events_digested: u64,
    /// Wall time spent sending
    pub ingest_secs: f64,
    /// Wall time from the last send to a fully drained backlog
    pub drain_secs: f64,
    /// End-to-end wall time
    pub total_secs: f64,
    /// Sustained digest rate over the whole window
    pub digest_events_per_second: f64,
    /// Largest observed gap between accepted and persisted
    pub peak_backlog: u64,
    /// Whether the backlog reached zero before the timeout
    pub fully_drained: bool,
}

/// Latency for one read endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointMetrics {
    /// Human-readable endpoint label
    pub endpoint: String,
    /// Requests issued
    pub requests: u64,
    /// Requests that returned 2xx
    pub successful: u64,
    /// Latency percentiles in milliseconds
    pub latency_ms: LatencyMetrics,
    /// Achieved requests per second
    pub requests_per_second: f64,
}

/// Identifies what was under test, so a result file stands on its own.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentInfo {
    /// Free-form label for the variant, e.g. "pg16" or "pg18-io_uring"
    pub label: String,
    /// PostgreSQL major version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub postgres_major: Option<i32>,
    /// Full PostgreSQL version string
    #[serde(skip_serializing_if = "Option::is_none")]
    pub postgres_version: Option<String>,
    /// Repeat index when a scenario is run more than once
    #[serde(default)]
    pub repeat: u32,
}

/// Scenario configuration summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSummary {
    /// Test duration in seconds
    pub duration_secs: u64,
    /// Target requests per second
    pub target_rps: u64,
    /// Number of concurrent connections
    pub concurrency: u32,
    /// Warmup duration in seconds
    pub warmup_secs: u64,
}

/// Complete benchmark results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResults {
    /// Unique run identifier
    pub run_id: String,
    /// Timestamp of the run
    pub timestamp: DateTime<Utc>,
    /// Scenario name
    pub scenario: String,
    /// What was under test
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<EnvironmentInfo>,
    /// Configuration summary
    pub config: ConfigSummary,
    /// Results section
    pub results: ResultsSection,
}

/// Results section containing all metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultsSection {
    /// Throughput metrics
    pub throughput: ThroughputMetrics,
    /// Latency metrics
    pub latency_ms: LatencyMetrics,
    /// Memory metrics
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_mb: Option<MemoryMetricsReport>,
    /// CPU metrics
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_percent: Option<CpuMetricsReport>,
    /// Error breakdown
    pub errors: ErrorMetrics,
    /// Actual test duration
    pub actual_duration_secs: f64,
    /// Database container resource usage, when collected
    #[serde(skip_serializing_if = "Option::is_none")]
    pub postgres_memory_mb: Option<MemoryMetricsReport>,
    /// Database container CPU, when collected
    #[serde(skip_serializing_if = "Option::is_none")]
    pub postgres_cpu_percent: Option<CpuMetricsReport>,
    /// PostgreSQL engine statistics over the run
    #[serde(skip_serializing_if = "Option::is_none")]
    pub postgres: Option<crate::pgstats::PgReport>,
    /// Digest pipeline metrics (drain and read scenarios)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest: Option<DrainMetrics>,
    /// Per-endpoint read latency (read scenario)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoints: Option<Vec<EndpointMetrics>>,
}

impl BenchmarkResults {
    /// Create new benchmark results from raw data
    pub fn new(
        config: &ScenarioConfig,
        stats: StatsSnapshot,
        histogram: &Histogram<u64>,
        duration: Duration,
        container_metrics: Option<ContainerMetrics>,
    ) -> Self {
        let run_id = format!(
            "{}-{}-{:03}",
            Utc::now().format("%Y%m%d"),
            config.name,
            rand::rng().random_range(0..1000u16)
        );

        let duration_secs = duration.as_secs_f64();
        let events_per_second = if duration_secs > 0.0 {
            stats.successful as f64 / duration_secs
        } else {
            0.0
        };

        // Convert histogram values from microseconds to milliseconds
        let latency = if !histogram.is_empty() {
            LatencyMetrics {
                p50: histogram.value_at_percentile(50.0) as f64 / 1000.0,
                p95: histogram.value_at_percentile(95.0) as f64 / 1000.0,
                p99: histogram.value_at_percentile(99.0) as f64 / 1000.0,
                max: histogram.max() as f64 / 1000.0,
                min: histogram.min() as f64 / 1000.0,
                mean: histogram.mean() / 1000.0,
            }
        } else {
            LatencyMetrics {
                p50: 0.0,
                p95: 0.0,
                p99: 0.0,
                max: 0.0,
                min: 0.0,
                mean: 0.0,
            }
        };

        let (memory_mb, cpu_percent) = if let Some(ref metrics) = container_metrics {
            (
                Some(MemoryMetricsReport {
                    idle_mb: metrics.memory.idle_mb,
                    peak_mb: metrics.memory.peak_mb,
                    average_mb: metrics.memory.average_mb,
                    limit_mb: metrics.memory.limit_mb,
                }),
                Some(CpuMetricsReport {
                    peak_percent: metrics.cpu.peak_percent,
                    average_percent: metrics.cpu.average_percent,
                }),
            )
        } else {
            (None, None)
        };

        Self {
            run_id,
            timestamp: Utc::now(),
            scenario: config.name.clone(),
            environment: None,
            config: ConfigSummary {
                duration_secs: config.duration_secs,
                target_rps: config.target_rps,
                concurrency: config.concurrency,
                warmup_secs: config.warmup_secs,
            },
            results: ResultsSection {
                throughput: ThroughputMetrics {
                    total_requests: stats.total_requests,
                    successful: stats.successful,
                    failed: stats.failed,
                    events_per_second,
                },
                latency_ms: latency,
                memory_mb,
                cpu_percent,
                errors: ErrorMetrics {
                    rate_limited_429: stats.rate_limited,
                    server_error_5xx: stats.server_errors,
                    connection_failed: stats
                        .failed
                        .saturating_sub(stats.rate_limited)
                        .saturating_sub(stats.server_errors),
                },
                actual_duration_secs: duration_secs,
                postgres_memory_mb: None,
                postgres_cpu_percent: None,
                postgres: None,
                digest: None,
                endpoints: None,
            },
        }
    }

    /// Attach PostgreSQL container resource usage.
    pub fn with_postgres_container_metrics(mut self, metrics: ContainerMetrics) -> Self {
        self.results.postgres_memory_mb = Some(MemoryMetricsReport {
            idle_mb: metrics.memory.idle_mb,
            peak_mb: metrics.memory.peak_mb,
            average_mb: metrics.memory.average_mb,
            limit_mb: metrics.memory.limit_mb,
        });
        self.results.postgres_cpu_percent = Some(CpuMetricsReport {
            peak_percent: metrics.cpu.peak_percent,
            average_percent: metrics.cpu.average_percent,
        });
        self
    }

    /// Attach PostgreSQL engine statistics.
    pub fn with_postgres_stats(mut self, report: crate::pgstats::PgReport) -> Self {
        self.environment = Some(match self.environment.take() {
            Some(env) => EnvironmentInfo {
                postgres_major: Some(report.server.major_version),
                postgres_version: Some(report.server.version.clone()),
                ..env
            },
            None => EnvironmentInfo {
                label: format!("pg{}", report.server.major_version),
                postgres_major: Some(report.server.major_version),
                postgres_version: Some(report.server.version.clone()),
                repeat: 0,
            },
        });
        self.results.postgres = Some(report);
        self
    }

    /// Attach digest pipeline metrics.
    pub fn with_digest_metrics(mut self, metrics: DrainMetrics) -> Self {
        self.results.digest = Some(metrics);
        self
    }

    /// Attach per-endpoint read latencies.
    pub fn with_endpoint_metrics(mut self, metrics: Vec<EndpointMetrics>) -> Self {
        self.results.endpoints = Some(metrics);
        self
    }

    /// Label this run (variant name and repeat index).
    pub fn with_label(mut self, label: &str, repeat: u32) -> Self {
        self.environment = Some(match self.environment.take() {
            Some(env) => EnvironmentInfo {
                label: label.to_string(),
                repeat,
                ..env
            },
            None => EnvironmentInfo {
                label: label.to_string(),
                postgres_major: None,
                postgres_version: None,
                repeat,
            },
        });
        self
    }

    /// Add container metrics to results
    pub fn with_container_metrics(mut self, metrics: ContainerMetrics) -> Self {
        self.results.memory_mb = Some(MemoryMetricsReport {
            idle_mb: metrics.memory.idle_mb,
            peak_mb: metrics.memory.peak_mb,
            average_mb: metrics.memory.average_mb,
            limit_mb: metrics.memory.limit_mb,
        });
        self.results.cpu_percent = Some(CpuMetricsReport {
            peak_percent: metrics.cpu.peak_percent,
            average_percent: metrics.cpu.average_percent,
        });
        self
    }

    /// Save results to a JSON file
    pub fn save(&self, output_dir: impl AsRef<Path>) -> std::io::Result<String> {
        let output_dir = output_dir.as_ref();
        fs::create_dir_all(output_dir)?;

        let filename = format!("{}.json", self.run_id);
        let filepath = output_dir.join(&filename);

        let json = serde_json::to_string_pretty(self)?;
        fs::write(&filepath, &json)?;

        // Also save as latest.json for convenience
        let latest_path = output_dir.join("latest.json");
        fs::write(&latest_path, &json)?;

        Ok(filepath.to_string_lossy().to_string())
    }

    /// Print a summary to the console
    pub fn print_summary(&self) {
        println!("\n{}", "═".repeat(60).cyan());
        println!(
            "{} {}",
            "Benchmark Results:".bold(),
            self.scenario.cyan().bold()
        );
        println!("{}", "═".repeat(60).cyan());

        println!("\n{}", "Throughput".yellow().bold());
        println!(
            "  Total requests:    {}",
            self.results.throughput.total_requests.to_string().white()
        );
        println!(
            "  Successful:        {}",
            self.results.throughput.successful.to_string().green()
        );
        println!(
            "  Failed:            {}",
            if self.results.throughput.failed > 0 {
                self.results.throughput.failed.to_string().red()
            } else {
                self.results.throughput.failed.to_string().green()
            }
        );
        println!(
            "  Events/sec:        {}",
            format!("{:.2}", self.results.throughput.events_per_second)
                .cyan()
                .bold()
        );

        println!("\n{}", "Latency".yellow().bold());
        println!(
            "  P50:               {}",
            format!("{:.2}ms", self.results.latency_ms.p50).white()
        );
        println!(
            "  P95:               {}",
            format!("{:.2}ms", self.results.latency_ms.p95).white()
        );
        println!(
            "  P99:               {}",
            format!("{:.2}ms", self.results.latency_ms.p99)
                .yellow()
                .bold()
        );
        println!(
            "  Max:               {}",
            format!("{:.2}ms", self.results.latency_ms.max).white()
        );
        println!(
            "  Mean:              {}",
            format!("{:.2}ms", self.results.latency_ms.mean).white()
        );

        if let Some(ref memory) = self.results.memory_mb {
            println!("\n{}", "Memory".yellow().bold());
            println!(
                "  Idle:              {}",
                format!("{:.1} MB", memory.idle_mb).white()
            );
            println!(
                "  Peak:              {}",
                format!("{:.1} MB", memory.peak_mb).cyan().bold()
            );
            println!(
                "  Average:           {}",
                format!("{:.1} MB", memory.average_mb).white()
            );
            if let Some(limit) = memory.limit_mb {
                println!(
                    "  Limit:             {}",
                    format!("{:.1} MB", limit).dimmed()
                );
            }
        }

        if let Some(ref cpu) = self.results.cpu_percent {
            println!("\n{}", "CPU".yellow().bold());
            println!(
                "  Peak:              {}",
                format!("{:.1}%", cpu.peak_percent).cyan().bold()
            );
            println!(
                "  Average:           {}",
                format!("{:.1}%", cpu.average_percent).white()
            );
        }

        if let Some(ref digest) = self.results.digest {
            println!("\n{}", "Digest Pipeline".yellow().bold());
            println!(
                "  Events sent:       {}",
                digest.events_sent.to_string().white()
            );
            println!(
                "  Events digested:   {}",
                if digest.fully_drained {
                    digest.events_digested.to_string().green()
                } else {
                    digest.events_digested.to_string().red()
                }
            );
            println!(
                "  Ingest time:       {}",
                format!("{:.2}s", digest.ingest_secs).white()
            );
            println!(
                "  Drain time:        {}",
                format!("{:.2}s", digest.drain_secs).white()
            );
            println!(
                "  Digest events/s:   {}",
                format!("{:.2}", digest.digest_events_per_second)
                    .cyan()
                    .bold()
            );
            println!(
                "  Peak backlog:      {}",
                digest.peak_backlog.to_string().yellow()
            );
            if !digest.fully_drained {
                println!(
                    "  {}",
                    "Backlog did not reach zero before the timeout".red()
                );
            }
        }

        if let Some(ref endpoints) = self.results.endpoints {
            println!("\n{}", "Read Endpoints".yellow().bold());
            println!(
                "  {:<28} {:>9} {:>9} {:>9} {:>9}",
                "endpoint".dimmed(),
                "rps".dimmed(),
                "p50".dimmed(),
                "p95".dimmed(),
                "p99".dimmed()
            );
            for ep in endpoints {
                println!(
                    "  {:<28} {:>9.1} {:>8.2}m {:>8.2}m {:>8.2}m",
                    ep.endpoint,
                    ep.requests_per_second,
                    ep.latency_ms.p50,
                    ep.latency_ms.p95,
                    ep.latency_ms.p99
                );
            }
        }

        if let Some(ref pg) = self.results.postgres {
            println!("\n{}", "PostgreSQL".yellow().bold());
            println!(
                "  Version:           {}",
                format!("{}", pg.server.major_version).cyan().bold()
            );
            if let Some(io_method) = pg.server.settings.get("io_method") {
                println!("  io_method:         {}", io_method.cyan());
            }
            for (key, label) in [
                ("cache_hit_ratio", "Cache hit ratio"),
                ("wal_mb", "WAL generated"),
                ("transactions", "Transactions"),
                ("blk_io_time_ms", "Block I/O time"),
                ("idx_scan_ratio", "Index scan ratio"),
                ("temp_mb", "Temp spill"),
            ] {
                if let Some(value) = pg.delta.derived.get(key) {
                    let formatted = match key {
                        "cache_hit_ratio" | "idx_scan_ratio" => format!("{:.2}%", value),
                        "wal_mb" | "temp_mb" => format!("{:.1} MB", value),
                        "blk_io_time_ms" => format!("{:.0} ms", value),
                        _ => format!("{:.0}", value),
                    };
                    println!("  {:<18} {}", format!("{}:", label), formatted.white());
                }
            }
            println!(
                "  {:<18} {}",
                "Database size:",
                format!("{:.1} MB", pg.database_bytes as f64 / (1024.0 * 1024.0)).white()
            );
        }

        if let (Some(ref memory), Some(ref cpu)) = (
            &self.results.postgres_memory_mb,
            &self.results.postgres_cpu_percent,
        ) {
            println!("\n{}", "PostgreSQL Container".yellow().bold());
            println!(
                "  Peak memory:       {}",
                format!("{:.1} MB", memory.peak_mb).cyan().bold()
            );
            println!(
                "  Avg memory:        {}",
                format!("{:.1} MB", memory.average_mb).white()
            );
            println!(
                "  Peak CPU:          {}",
                format!("{:.1}%", cpu.peak_percent).cyan().bold()
            );
            println!(
                "  Avg CPU:           {}",
                format!("{:.1}%", cpu.average_percent).white()
            );
        }

        if self.results.errors.rate_limited_429 > 0
            || self.results.errors.server_error_5xx > 0
            || self.results.errors.connection_failed > 0
        {
            println!("\n{}", "Errors".red().bold());
            if self.results.errors.rate_limited_429 > 0 {
                println!(
                    "  Rate limited (429): {}",
                    self.results.errors.rate_limited_429.to_string().yellow()
                );
            }
            if self.results.errors.server_error_5xx > 0 {
                println!(
                    "  Server errors (5xx): {}",
                    self.results.errors.server_error_5xx.to_string().red()
                );
            }
            if self.results.errors.connection_failed > 0 {
                println!(
                    "  Connection failed:  {}",
                    self.results.errors.connection_failed.to_string().red()
                );
            }
        }

        println!("\n{}", "═".repeat(60).cyan());
        println!("{} {}", "Run ID:".dimmed(), self.run_id.dimmed());
        println!(
            "{} {}s",
            "Duration:".dimmed(),
            format!("{:.1}", self.results.actual_duration_secs).dimmed()
        );
        println!("{}", "═".repeat(60).cyan());
    }
}

/// Compare two benchmark results
pub fn compare(old: &BenchmarkResults, new: &BenchmarkResults) {
    println!("\n{}", "═".repeat(60).cyan());
    println!(
        "{} {} {} {}",
        "Comparison:".bold(),
        old.run_id.dimmed(),
        "→".dimmed(),
        new.run_id.cyan()
    );
    println!("{}", "═".repeat(60).cyan());

    let throughput_change = if old.results.throughput.events_per_second > 0.0 {
        (new.results.throughput.events_per_second - old.results.throughput.events_per_second)
            / old.results.throughput.events_per_second
            * 100.0
    } else {
        0.0
    };

    let latency_change = if old.results.latency_ms.p99 > 0.0 {
        (new.results.latency_ms.p99 - old.results.latency_ms.p99) / old.results.latency_ms.p99
            * 100.0
    } else {
        0.0
    };

    println!("\n{}", "Throughput".yellow().bold());
    println!(
        "  Events/sec:  {:.2} → {:.2} ({})",
        old.results.throughput.events_per_second,
        new.results.throughput.events_per_second,
        format_change(throughput_change, true)
    );

    println!("\n{}", "Latency P99".yellow().bold());
    println!(
        "  {:.2}ms → {:.2}ms ({})",
        old.results.latency_ms.p99,
        new.results.latency_ms.p99,
        format_change(latency_change, false)
    );

    if let (Some(old_mem), Some(new_mem)) = (&old.results.memory_mb, &new.results.memory_mb) {
        let memory_change = (new_mem.peak_mb - old_mem.peak_mb) / old_mem.peak_mb * 100.0;

        println!("\n{}", "Peak Memory".yellow().bold());
        println!(
            "  {:.1}MB → {:.1}MB ({})",
            old_mem.peak_mb,
            new_mem.peak_mb,
            format_change(memory_change, false)
        );
    }

    println!("{}", "═".repeat(60).cyan());
}

fn format_change(percent: f64, higher_is_better: bool) -> colored::ColoredString {
    let is_improvement = if higher_is_better {
        percent > 0.0
    } else {
        percent < 0.0
    };

    let sign = if percent > 0.0 { "+" } else { "" };
    let text = format!("{}{:.1}%", sign, percent);

    if is_improvement {
        text.green()
    } else if percent.abs() < 1.0 {
        text.white()
    } else {
        text.red()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hdrhistogram::Histogram;

    #[test]
    fn test_results_creation() {
        let config = ScenarioConfig::sustained();
        let stats = StatsSnapshot {
            total_requests: 1000,
            successful: 950,
            failed: 50,
            rate_limited: 30,
            server_errors: 20,
        };

        let mut histogram = Histogram::<u64>::new_with_bounds(1, 60_000_000, 3).unwrap();
        for i in 0..100 {
            histogram.record(i * 1000).unwrap(); // 0-99ms in microseconds
        }

        let results =
            BenchmarkResults::new(&config, stats, &histogram, Duration::from_secs(10), None);

        assert_eq!(results.scenario, "sustained");
        assert_eq!(results.results.throughput.total_requests, 1000);
        assert_eq!(results.results.throughput.successful, 950);
        assert!((results.results.throughput.events_per_second - 95.0).abs() < 0.1);
    }

    #[test]
    fn test_json_serialization() {
        let config = ScenarioConfig::baseline();
        let stats = StatsSnapshot::default();
        let histogram = Histogram::<u64>::new_with_bounds(1, 60_000_000, 3).unwrap();

        let results =
            BenchmarkResults::new(&config, stats, &histogram, Duration::from_secs(60), None);

        let json = serde_json::to_string(&results).unwrap();
        assert!(json.contains("baseline"));
        assert!(json.contains("throughput"));
        assert!(json.contains("latency_ms"));
    }
}
