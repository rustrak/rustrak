//! Process-local Prometheus counters. Unlike the anonymous telemetry window,
//! these never reset during a process lifetime. A restart starts a new series.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::Mutex;
use std::time::Duration;

use super::counters::{Rejection, LATENCY_BOUNDS_MS};

#[derive(Clone, Copy, Default)]
pub struct Spool {
    pub pending: u64,
    pub bytes: u64,
    pub oldest_seconds: u64,
}

pub struct MetricsCounters {
    accepted: AtomicU64,
    rejected: [AtomicU64; 5],
    latency: [AtomicU64; LATENCY_BOUNDS_MS.len() + 1],
    latency_us: AtomicU64,
    digest_ok: AtomicU64,
    digest_failed: AtomicU64,
    digest_rate_limited: AtomicU64,
    http_5xx: Mutex<BTreeMap<String, u64>>,
    alert_failures: Mutex<BTreeMap<String, u64>>,
}

impl MetricsCounters {
    pub const fn new() -> Self {
        Self {
            accepted: AtomicU64::new(0),
            rejected: [const { AtomicU64::new(0) }; 5],
            latency: [const { AtomicU64::new(0) }; LATENCY_BOUNDS_MS.len() + 1],
            latency_us: AtomicU64::new(0),
            digest_ok: AtomicU64::new(0),
            digest_failed: AtomicU64::new(0),
            digest_rate_limited: AtomicU64::new(0),
            http_5xx: Mutex::new(BTreeMap::new()),
            alert_failures: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn accepted(&self, duration: Duration) {
        self.accepted.fetch_add(1, Relaxed);
        let bucket = LATENCY_BOUNDS_MS
            .iter()
            .position(|bound| duration <= Duration::from_millis(*bound))
            .unwrap_or(LATENCY_BOUNDS_MS.len());
        self.latency[bucket].fetch_add(1, Relaxed);
        self.latency_us.fetch_add(
            u64::try_from(duration.as_micros()).unwrap_or(u64::MAX),
            Relaxed,
        );
    }

    pub fn rejected(&self, reason: Rejection) {
        self.rejected[reason as usize].fetch_add(1, Relaxed);
    }

    pub fn digest_ok(&self) {
        self.digest_ok.fetch_add(1, Relaxed);
    }

    pub fn digest_failed(&self) {
        self.digest_failed.fetch_add(1, Relaxed);
    }

    pub fn digest_rate_limited(&self) {
        self.digest_rate_limited.fetch_add(1, Relaxed);
    }

    pub fn http_5xx(&self, route: &str) {
        bump(&self.http_5xx, route);
    }

    pub fn alert_failed(&self, provider: &str) {
        bump(&self.alert_failures, provider);
    }

    /// `spool` and `db_bytes` come from the endpoint's short-lived reading, not
    /// the six-hour anonymous telemetry window.
    pub fn render(&self, spool: Option<Spool>, db_bytes: Option<u64>) -> String {
        let mut out = String::new();
        out.push_str("# HELP rustrak_ingest_accepted_total Accepted ingest requests.\n# TYPE rustrak_ingest_accepted_total counter\n");
        let accepted = self.accepted.load(Relaxed);
        writeln!(out, "rustrak_ingest_accepted_total {accepted}").unwrap();

        out.push_str("# HELP rustrak_ingest_rejected_total Rejected ingest requests.\n# TYPE rustrak_ingest_rejected_total counter\n");
        for (reason, label) in [
            (Rejection::RateLimit, "rate_limit"),
            (Rejection::Auth, "auth"),
            (Rejection::TooLarge, "too_large"),
            (Rejection::Malformed, "malformed"),
            (Rejection::Other, "other"),
        ] {
            writeln!(
                out,
                "rustrak_ingest_rejected_total{{reason=\"{label}\"}} {}",
                self.rejected[reason as usize].load(Relaxed)
            )
            .unwrap();
        }

        out.push_str("# HELP rustrak_ingest_duration_seconds Time to accept an ingest request.\n# TYPE rustrak_ingest_duration_seconds histogram\n");
        let mut cumulative = 0;
        for (i, bound) in LATENCY_BOUNDS_MS.iter().enumerate() {
            cumulative += self.latency[i].load(Relaxed);
            writeln!(
                out,
                "rustrak_ingest_duration_seconds_bucket{{le=\"{}\"}} {cumulative}",
                *bound as f64 / 1000.0
            )
            .unwrap();
        }
        cumulative += self.latency[LATENCY_BOUNDS_MS.len()].load(Relaxed);
        writeln!(
            out,
            "rustrak_ingest_duration_seconds_bucket{{le=\"+Inf\"}} {cumulative}"
        )
        .unwrap();
        writeln!(
            out,
            "rustrak_ingest_duration_seconds_sum {}",
            self.latency_us.load(Relaxed) as f64 / 1_000_000.0
        )
        .unwrap();
        writeln!(out, "rustrak_ingest_duration_seconds_count {cumulative}").unwrap();

        out.push_str(
            "# HELP rustrak_digest_total Digest outcomes.\n# TYPE rustrak_digest_total counter\n",
        );
        for (result, counter) in [
            ("ok", &self.digest_ok),
            ("failed", &self.digest_failed),
            ("rate_limited", &self.digest_rate_limited),
        ] {
            writeln!(
                out,
                "rustrak_digest_total{{result=\"{result}\"}} {}",
                counter.load(Relaxed)
            )
            .unwrap();
        }

        write_map(
            &mut out,
            "rustrak_http_5xx_total",
            "HTTP 5xx responses by matched route.",
            "route",
            &self.http_5xx,
        );
        write_map(
            &mut out,
            "rustrak_alert_failures_total",
            "Failed alert deliveries by provider.",
            "provider",
            &self.alert_failures,
        );
        for (name, help, value) in [
            (
                "rustrak_spool_pending",
                "Pending ingest files.",
                spool.map(|s| s.pending),
            ),
            (
                "rustrak_spool_bytes",
                "Bytes in pending ingest files.",
                spool.map(|s| s.bytes),
            ),
            (
                "rustrak_spool_oldest_seconds",
                "Age of oldest pending ingest file in seconds.",
                spool.map(|s| s.oldest_seconds),
            ),
        ] {
            writeln!(out, "# HELP {name} {help}\n# TYPE {name} gauge").unwrap();
            if let Some(value) = value {
                writeln!(out, "{name} {value}").unwrap();
            }
        }
        out.push_str(
            "# HELP rustrak_db_bytes Database size in bytes.\n# TYPE rustrak_db_bytes gauge\n",
        );
        if let Some(bytes) = db_bytes {
            writeln!(out, "rustrak_db_bytes {bytes}").unwrap();
        }
        out
    }
}

impl Default for MetricsCounters {
    fn default() -> Self {
        Self::new()
    }
}

fn bump(map: &Mutex<BTreeMap<String, u64>>, key: &str) {
    let mut map = map.lock().unwrap_or_else(|e| e.into_inner());
    *map.entry(key.to_owned()).or_insert(0) += 1;
}

fn write_map(
    out: &mut String,
    name: &str,
    help: &str,
    label: &str,
    map: &Mutex<BTreeMap<String, u64>>,
) {
    writeln!(out, "# HELP {name} {help}\n# TYPE {name} counter").unwrap();
    for (key, value) in map.lock().unwrap_or_else(|e| e.into_inner()).iter() {
        let escaped = key
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n");
        writeln!(out, "{name}{{{label}=\"{escaped}\"}} {value}").unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slow_ingest_lands_in_infinite_bucket() {
        let counters = MetricsCounters::new();
        counters.accepted(Duration::from_secs(12));
        counters.rejected(Rejection::Auth);
        counters.digest_failed();
        counters.http_5xx("/api/issues/{id}");
        counters.alert_failed("email");
        let first = counters.render(
            Some(Spool {
                pending: 2,
                bytes: 512,
                oldest_seconds: 30,
            }),
            Some(1024),
        );
        assert!(first.contains("rustrak_ingest_accepted_total 1"));
        assert!(first.contains("rustrak_ingest_duration_seconds_bucket{le=\"10\"} 0"));
        assert!(first.contains("rustrak_ingest_duration_seconds_bucket{le=\"+Inf\"} 1"));
        assert!(first.contains("rustrak_ingest_duration_seconds_sum 12"));
        assert!(first.contains("rustrak_ingest_rejected_total{reason=\"auth\"} 1"));
        assert!(first.contains("rustrak_http_5xx_total{route=\"/api/issues/{id}\"} 1"));
        assert!(first.contains("rustrak_spool_bytes 512"));
        assert!(first.contains("rustrak_db_bytes 1024"));
    }

    #[test]
    fn latency_buckets_use_exact_bounds() {
        let counters = MetricsCounters::new();
        counters.accepted(Duration::from_millis(1) + Duration::from_nanos(1));
        let output = counters.render(Some(Spool::default()), None);
        assert!(output.contains("rustrak_ingest_duration_seconds_bucket{le=\"0.001\"} 0"));
        assert!(output.contains("rustrak_ingest_duration_seconds_bucket{le=\"0.002\"} 1"));
    }

    #[test]
    fn labels_are_escaped_and_missing_db_gauge_is_not_zero() {
        let counters = MetricsCounters::new();
        counters.http_5xx("a\"\\\nb");
        let output = counters.render(Some(Spool::default()), None);
        assert!(output.contains("rustrak_http_5xx_total{route=\"a\\\"\\\\\\nb\"} 1"));
        assert!(!output.contains("rustrak_db_bytes 0"));
    }

    #[test]
    fn unreadable_spool_omits_its_gauges_but_keeps_counters() {
        let counters = MetricsCounters::new();
        counters.digest_ok();
        let output = counters.render(None, Some(1));
        assert!(output.contains("rustrak_digest_total{result=\"ok\"} 1"));
        assert!(!output.contains("rustrak_spool_pending 0"));
        assert!(output.contains("rustrak_db_bytes 1"));
    }
}
