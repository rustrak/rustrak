//! Metrics collection from Docker containers.

use bollard::query_parameters::StatsOptions;
use bollard::Docker;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::time::Duration;

/// Metrics collection errors
#[derive(Debug, Error)]
pub enum MetricsError {
    #[error("Docker connection failed: {0}")]
    DockerError(#[from] bollard::errors::Error),
}

/// CPU metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CpuMetrics {
    /// Peak CPU usage percentage
    pub peak_percent: f64,
    /// Average CPU usage percentage
    pub average_percent: f64,
    /// Number of samples collected
    pub samples: u64,
}

/// Memory metrics in megabytes
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryMetrics {
    /// Memory usage at start (idle)
    pub idle_mb: f64,
    /// Peak memory usage
    pub peak_mb: f64,
    /// Average memory usage
    pub average_mb: f64,
    /// Memory limit (if set)
    pub limit_mb: Option<f64>,
    /// Number of samples collected
    pub samples: u64,
}

/// I/O metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IoMetrics {
    /// Total bytes read
    pub read_bytes: u64,
    /// Total bytes written
    pub write_bytes: u64,
}

/// Network metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetworkMetrics {
    /// Total bytes received
    pub rx_bytes: u64,
    /// Total bytes transmitted
    pub tx_bytes: u64,
}

/// Collected metrics for a container
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContainerMetrics {
    /// Container name
    pub container_name: String,
    /// CPU metrics
    pub cpu: CpuMetrics,
    /// Memory metrics
    pub memory: MemoryMetrics,
    /// I/O metrics
    pub io: IoMetrics,
    /// Network metrics
    pub network: NetworkMetrics,
}

/// Internal state for accumulating metrics
#[derive(Debug, Default)]
struct MetricsAccumulator {
    cpu_total: f64,
    cpu_peak: f64,
    cpu_samples: u64,
    memory_total: f64,
    memory_peak: f64,
    memory_idle: Option<f64>,
    memory_limit: Option<f64>,
    memory_samples: u64,
    io_read: u64,
    io_write: u64,
    network_rx: u64,
    network_tx: u64,
    previous_cpu: u64,
    previous_system_cpu: u64,
}

impl MetricsAccumulator {
    fn add_sample(&mut self, stats: &bollard::models::ContainerStatsResponse) {
        // CPU calculation — all sub-structs are Option<T> in bollard 0.21
        if let Some(cpu_stats) = &stats.cpu_stats {
            let total_usage = cpu_stats
                .cpu_usage
                .as_ref()
                .and_then(|u| u.total_usage)
                .unwrap_or(0);
            let system_usage = cpu_stats.system_cpu_usage.unwrap_or(0);

            let cpu_delta = total_usage.saturating_sub(self.previous_cpu);
            let system_delta = system_usage.saturating_sub(self.previous_system_cpu);

            if system_delta > 0 && self.previous_cpu > 0 {
                let num_cpus = cpu_stats.online_cpus.unwrap_or(1) as f64;
                let cpu_percent = (cpu_delta as f64 / system_delta as f64) * num_cpus * 100.0;
                self.cpu_total += cpu_percent;
                self.cpu_peak = self.cpu_peak.max(cpu_percent);
                self.cpu_samples += 1;
            }

            self.previous_cpu = total_usage;
            self.previous_system_cpu = system_usage;
        }

        // Memory calculation
        if let Some(memory_stats) = &stats.memory_stats {
            let memory_bytes = memory_stats.usage.unwrap_or(0) as f64;
            let memory_mb = memory_bytes / (1024.0 * 1024.0);

            if self.memory_idle.is_none() {
                self.memory_idle = Some(memory_mb);
            }
            self.memory_total += memory_mb;
            self.memory_peak = self.memory_peak.max(memory_mb);
            self.memory_samples += 1;

            if self.memory_limit.is_none() {
                if let Some(limit) = memory_stats.limit {
                    if limit > 0 && limit < u64::MAX / 2 {
                        self.memory_limit = Some(limit as f64 / (1024.0 * 1024.0));
                    }
                }
            }
        }

        // I/O metrics
        if let Some(blkio_stats) = &stats.blkio_stats {
            if let Some(ref io_stats) = blkio_stats.io_service_bytes_recursive {
                for stat in io_stats {
                    let value = stat.value.unwrap_or(0);
                    match stat.op.as_deref().unwrap_or("") {
                        "read" | "Read" => self.io_read = value,
                        "write" | "Write" => self.io_write = value,
                        _ => {}
                    }
                }
            }
        }

        // Network metrics
        if let Some(ref networks) = stats.networks {
            let (rx, tx) = networks.values().fold((0u64, 0u64), |(rx, tx), net| {
                (
                    rx + net.rx_bytes.unwrap_or(0),
                    tx + net.tx_bytes.unwrap_or(0),
                )
            });
            self.network_rx = rx;
            self.network_tx = tx;
        }
    }

    fn finalize(&self, container_name: &str) -> ContainerMetrics {
        ContainerMetrics {
            container_name: container_name.to_string(),
            cpu: CpuMetrics {
                peak_percent: self.cpu_peak,
                average_percent: if self.cpu_samples > 0 {
                    self.cpu_total / self.cpu_samples as f64
                } else {
                    0.0
                },
                samples: self.cpu_samples,
            },
            memory: MemoryMetrics {
                idle_mb: self.memory_idle.unwrap_or(0.0),
                peak_mb: self.memory_peak,
                average_mb: if self.memory_samples > 0 {
                    self.memory_total / self.memory_samples as f64
                } else {
                    0.0
                },
                limit_mb: self.memory_limit,
                samples: self.memory_samples,
            },
            io: IoMetrics {
                read_bytes: self.io_read,
                write_bytes: self.io_write,
            },
            network: NetworkMetrics {
                rx_bytes: self.network_rx,
                tx_bytes: self.network_tx,
            },
        }
    }
}

/// Metrics collector for Docker containers
pub struct MetricsCollector {
    docker: Docker,
    container_name: String,
    accumulator: Arc<Mutex<MetricsAccumulator>>,
    running: Arc<AtomicBool>,
}

impl MetricsCollector {
    /// Create a new metrics collector for a container
    pub async fn new(container_name: &str) -> Result<Self, MetricsError> {
        let docker = Docker::connect_with_socket_defaults()?;

        // Verify container exists
        docker.inspect_container(container_name, None).await?;

        Ok(Self {
            docker,
            container_name: container_name.to_string(),
            accumulator: Arc::new(Mutex::new(MetricsAccumulator::default())),
            running: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Start collecting metrics in the background
    pub fn start(&self) -> tokio::task::JoinHandle<()> {
        let docker = self.docker.clone();
        let container_name = self.container_name.clone();
        let accumulator = self.accumulator.clone();
        let running = self.running.clone();

        running.store(true, Ordering::SeqCst);

        tokio::spawn(async move {
            let options = StatsOptions {
                stream: true,
                one_shot: false,
            };

            let mut stats_stream = docker.stats(&container_name, Some(options));

            while running.load(Ordering::SeqCst) {
                tokio::select! {
                    Some(Ok(stats)) = stats_stream.next() => {
                        let mut acc = accumulator.lock().await;
                        acc.add_sample(&stats);
                    }
                    _ = tokio::time::sleep(Duration::from_millis(100)) => {
                        if !running.load(Ordering::SeqCst) {
                            break;
                        }
                    }
                }
            }
        })
    }

    /// Stop collecting metrics and return the final results
    pub async fn stop(&self) -> ContainerMetrics {
        self.running.store(false, Ordering::SeqCst);

        // Give time for the collector to stop
        tokio::time::sleep(Duration::from_millis(200)).await;

        let acc = self.accumulator.lock().await;
        acc.finalize(&self.container_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accumulator_default() {
        let acc = MetricsAccumulator::default();
        assert_eq!(acc.cpu_samples, 0);
        assert_eq!(acc.memory_samples, 0);
    }

    #[test]
    fn test_metrics_finalize() {
        let acc = MetricsAccumulator {
            cpu_total: 150.0,
            cpu_peak: 80.0,
            cpu_samples: 3,
            memory_total: 300.0,
            memory_peak: 120.0,
            memory_idle: Some(90.0),
            memory_samples: 3,
            ..Default::default()
        };

        let metrics = acc.finalize("test-container");

        assert_eq!(metrics.container_name, "test-container");
        assert_eq!(metrics.cpu.peak_percent, 80.0);
        assert!((metrics.cpu.average_percent - 50.0).abs() < 0.01);
        assert_eq!(metrics.memory.peak_mb, 120.0);
        assert!((metrics.memory.average_mb - 100.0).abs() < 0.01);
        assert_eq!(metrics.memory.idle_mb, 90.0);
    }
}
