//! The Prometheus exposition of the process counters. They never reset during
//! a process lifetime; a restart starts a new series.

use std::collections::BTreeMap;
use std::fmt::Write;

use super::counters::{Counters, Rejection, LATENCY_BOUNDS_MS};

#[derive(Clone, Copy, Default)]
pub struct Spool {
    pub pending: u64,
    pub bytes: u64,
    pub oldest_seconds: u64,
}

impl Counters {
    /// `spool` and `db_bytes` come from the endpoint's short-lived reading, not
    /// the six-hour anonymous telemetry window.
    pub fn render(&self, spool: Option<Spool>, db_bytes: Option<u64>) -> String {
        let totals = self.totals();
        let mut out = String::new();
        out.push_str("# HELP rustrak_ingest_accepted_total Accepted ingest requests.\n# TYPE rustrak_ingest_accepted_total counter\n");
        writeln!(out, "rustrak_ingest_accepted_total {}", totals.accepted).unwrap();

        out.push_str("# HELP rustrak_ingest_rejected_total Rejected ingest requests.\n# TYPE rustrak_ingest_rejected_total counter\n");
        for (reason, label) in Rejection::ALL {
            writeln!(
                out,
                "rustrak_ingest_rejected_total{{reason=\"{label}\"}} {}",
                totals.rejected[reason as usize]
            )
            .unwrap();
        }

        out.push_str("# HELP rustrak_ingest_duration_seconds Time to accept an ingest request.\n# TYPE rustrak_ingest_duration_seconds histogram\n");
        let mut cumulative = 0;
        for (i, bound) in LATENCY_BOUNDS_MS.iter().enumerate() {
            cumulative += totals.latency[i];
            writeln!(
                out,
                "rustrak_ingest_duration_seconds_bucket{{le=\"{}\"}} {cumulative}",
                *bound as f64 / 1000.0
            )
            .unwrap();
        }
        cumulative += totals.latency[LATENCY_BOUNDS_MS.len()];
        writeln!(
            out,
            "rustrak_ingest_duration_seconds_bucket{{le=\"+Inf\"}} {cumulative}"
        )
        .unwrap();
        writeln!(
            out,
            "rustrak_ingest_duration_seconds_sum {}",
            totals.latency_us as f64 / 1_000_000.0
        )
        .unwrap();
        writeln!(out, "rustrak_ingest_duration_seconds_count {cumulative}").unwrap();

        out.push_str(
            "# HELP rustrak_digest_total Digest outcomes.\n# TYPE rustrak_digest_total counter\n",
        );
        for (result, count) in [
            ("ok", totals.digest_ok),
            ("failed", totals.digest_failed),
            ("rate_limited", totals.digest_rate_limited),
        ] {
            writeln!(out, "rustrak_digest_total{{result=\"{result}\"}} {count}").unwrap();
        }

        write_map(
            &mut out,
            "rustrak_http_5xx_total",
            "HTTP 5xx responses by matched route.",
            "route",
            &totals.http_5xx_by_route,
        );
        write_map(
            &mut out,
            "rustrak_alert_failures_total",
            "Failed alert deliveries by provider.",
            "provider",
            &totals.alerts_failed_by_provider,
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

fn write_map(out: &mut String, name: &str, help: &str, label: &str, map: &BTreeMap<String, u64>) {
    writeln!(out, "# HELP {name} {help}\n# TYPE {name} counter").unwrap();
    for (key, value) in map {
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
    use std::time::Duration;

    #[test]
    fn slow_ingest_lands_in_infinite_bucket() {
        let counters = Counters::new();
        counters.ingest_accepted(Duration::from_secs(12));
        counters.ingest_rejected(Rejection::Auth);
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
        let counters = Counters::new();
        counters.ingest_accepted(Duration::from_millis(1) + Duration::from_nanos(1));
        let output = counters.render(Some(Spool::default()), None);
        assert!(output.contains("rustrak_ingest_duration_seconds_bucket{le=\"0.001\"} 0"));
        assert!(output.contains("rustrak_ingest_duration_seconds_bucket{le=\"0.002\"} 1"));
    }

    #[test]
    fn labels_are_escaped_and_missing_db_gauge_is_not_zero() {
        let counters = Counters::new();
        counters.http_5xx("a\"\\\nb");
        let output = counters.render(Some(Spool::default()), None);
        assert!(output.contains("rustrak_http_5xx_total{route=\"a\\\"\\\\\\nb\"} 1"));
        assert!(!output.contains("rustrak_db_bytes 0"));
    }

    #[test]
    fn unreadable_spool_omits_its_gauges_but_keeps_counters() {
        let counters = Counters::new();
        counters.digest_ok();
        let output = counters.render(None, Some(1));
        assert!(output.contains("rustrak_digest_total{result=\"ok\"} 1"));
        assert!(!output.contains("rustrak_spool_pending 0"));
        assert!(output.contains("rustrak_db_bytes 1"));
    }
}
