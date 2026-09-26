//! Fixtures for the anonymous telemetry tests.

use rustrak::telemetry::{ConfigFacts, Counters, Report, Rss, Volume};

/// A fully populated report with recognisable values.
pub fn sample_report() -> Report {
    Report {
        schema: 1,
        instance_id: "0b6d3a3e-1111-4a5b-8c9d-0e0f10111213".to_string(),
        version: "0.15.0".to_string(),
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        container: true,
        db_backend: "sqlite".to_string(),
        db_version: Some("3.46".to_string()),
        dashboard_served: true,
        uptime_secs: 86_412,
        first_since_boot: false,
        cpu_count: 4,
        mem_total_mb: Some(7_900),
        rss_mb: Rss {
            min: Some(60),
            max: Some(95),
            last: Some(70),
        },
        sqlite_db_mb: Some(120),
        ingest_dir_pending: 0,
        volume: Volume {
            projects: 3,
            users: 2,
            issues_open: 120,
            events_24h: 4_500,
            transactions_24h: 0,
            sessions_24h: 0,
            logs_24h: 0,
        },
        health: Counters::new().snapshot_and_reset(),
        config: ConfigFacts {
            ssl_proxy: true,
            public_url_set: true,
            smtp_configured: false,
            session_secret_set: true,
            alert_providers: vec!["slack".to_string()],
            quota_customized: true,
        },
    }
}
