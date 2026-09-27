//! Unit tests for the anonymous telemetry module: the on/off decision, the
//! counters, the report shape and the PostHog adapter.

use rustrak::config::TelemetryConfig;
use rustrak::telemetry::{decide, Disabled};

fn config(enabled: bool, do_not_track: bool) -> TelemetryConfig {
    TelemetryConfig {
        enabled,
        do_not_track,
    }
}

// =============================================================================
// The on/off decision
// =============================================================================

#[test]
fn telemetry_runs_when_switched_on_and_a_key_was_compiled_in() {
    assert_eq!(decide(&config(true, false), Some("phc_key")), Ok(()));
}

#[test]
fn the_switch_wins_over_everything_else() {
    assert_eq!(
        decide(&config(false, true), Some("phc_key")),
        Err(Disabled::SwitchedOff)
    );
    assert_eq!(
        decide(&config(false, false), None),
        Err(Disabled::SwitchedOff)
    );
}

#[test]
fn do_not_track_disables_it_even_with_a_key() {
    assert_eq!(
        decide(&config(true, true), Some("phc_key")),
        Err(Disabled::DoNotTrack)
    );
}

/// A binary built without `RUSTRAK_TELEMETRY_KEY` (a local build, a fork, a
/// contributor's `cargo run`) never sends anything, and says why.
#[test]
fn a_binary_without_a_compiled_in_key_stays_quiet() {
    assert_eq!(
        decide(&config(true, false), None),
        Err(Disabled::NoKeyCompiledIn)
    );
    assert_eq!(
        decide(&config(true, false), Some("")),
        Err(Disabled::NoKeyCompiledIn)
    );
}

#[test]
fn each_reason_names_itself_for_the_startup_log() {
    assert!(Disabled::SwitchedOff
        .to_string()
        .contains("RUSTRAK_TELEMETRY"));
    assert!(Disabled::DoNotTrack.to_string().contains("DO_NOT_TRACK"));
    assert!(Disabled::NoKeyCompiledIn.to_string().contains("key"));
}

// =============================================================================
// Counts are blurred to two significant digits
// =============================================================================

/// Qdrant's rule: big numbers lose their trailing digits so an exact count
/// cannot fingerprint an instance. Small numbers are left alone because
/// there is nothing to hide in "3 projects".
#[test]
fn counts_keep_two_significant_digits_and_lose_the_rest() {
    use rustrak::telemetry::blur_count;
    for (input, expected) in [
        (0, 0),
        (7, 7),
        (42, 42),
        (99, 99),
        (123, 120),
        (999, 990),
        (1_999, 1_900),
        (123_456_789, 120_000_000),
        (u64::MAX, 18_000_000_000_000_000_000),
    ] {
        assert_eq!(blur_count(input), expected, "blur_count({input})");
    }
}

// =============================================================================
// Health counters
// =============================================================================

mod counters {
    use rustrak::telemetry::{Counters, Rejection};
    use std::time::Duration;

    #[test]
    fn a_fresh_counter_set_reports_zero_everywhere() {
        let health = Counters::new().snapshot_and_reset();
        assert_eq!(health.ingest.accepted, 0);
        assert_eq!(health.ingest.rejected.rate_limit, 0);
        assert_eq!(health.ingest.latency_ms.p50, None);
        assert_eq!(health.ingest.latency_ms.p99, None);
        assert_eq!(health.digest.ok, 0);
        assert_eq!(health.digest.failed, 0);
        assert!(health.http_5xx_by_route.is_empty());
        assert!(health.alerts_failed_by_provider.is_empty());
        assert!(health.panics.is_empty());
    }

    #[test]
    fn accepted_ingests_are_counted_with_their_latency_percentiles() {
        let counters = Counters::new();
        for ms in [3, 4, 4, 5, 6, 7, 8, 9, 40, 400] {
            counters.ingest_accepted(Duration::from_millis(ms));
        }
        let health = counters.snapshot_and_reset();
        assert_eq!(health.ingest.accepted, 10);
        // Bucketed on a fixed log scale: the percentile is the bucket's upper bound.
        assert_eq!(health.ingest.latency_ms.p50, Some(10));
        assert_eq!(health.ingest.latency_ms.p99, Some(500));
    }

    #[test]
    fn rejections_are_counted_by_reason_and_nothing_else() {
        let counters = Counters::new();
        counters.ingest_rejected(Rejection::RateLimit);
        counters.ingest_rejected(Rejection::RateLimit);
        counters.ingest_rejected(Rejection::Auth);
        counters.ingest_rejected(Rejection::TooLarge);
        counters.ingest_rejected(Rejection::Malformed);
        counters.ingest_rejected(Rejection::Other);
        let r = counters.snapshot_and_reset().ingest.rejected;
        assert_eq!(
            (r.rate_limit, r.auth, r.too_large, r.malformed, r.other),
            (2, 1, 1, 1, 1)
        );
    }

    #[test]
    fn a_snapshot_starts_the_next_window_from_zero() {
        let counters = Counters::new();
        counters.ingest_accepted(Duration::from_millis(1));
        counters.digest_ok();
        counters.digest_failed();
        counters.http_5xx("/api/projects/{id}");
        counters.alert_failed("slack");
        counters.panic("src/digest/mod.rs:212");

        let first = counters.snapshot_and_reset();
        assert_eq!(first.ingest.accepted, 1);
        assert_eq!((first.digest.ok, first.digest.failed), (1, 1));
        assert_eq!(first.http_5xx_by_route["/api/projects/{id}"], 1);
        assert_eq!(first.alerts_failed_by_provider["slack"], 1);
        assert_eq!(first.panics["src/digest/mod.rs:212"], 1);

        let second = counters.snapshot_and_reset();
        assert_eq!(second.ingest.accepted, 0);
        assert_eq!(second.ingest.latency_ms.p50, None);
        assert_eq!((second.digest.ok, second.digest.failed), (0, 0));
        assert!(second.http_5xx_by_route.is_empty());
        assert!(second.alerts_failed_by_provider.is_empty());
        assert!(second.panics.is_empty());
    }

    #[test]
    fn repeated_keys_accumulate() {
        let counters = Counters::new();
        counters.http_5xx("/api/issues/{id}");
        counters.http_5xx("/api/issues/{id}");
        counters.http_5xx("/api/projects");
        let health = counters.snapshot_and_reset();
        assert_eq!(health.http_5xx_by_route["/api/issues/{id}"], 2);
        assert_eq!(health.http_5xx_by_route["/api/projects"], 1);
    }

    /// The preview endpoint looks without taking, so the next heartbeat
    /// still carries the whole window.
    #[test]
    fn a_peek_leaves_the_window_intact() {
        let counters = Counters::new();
        counters.ingest_accepted(Duration::from_millis(3));
        counters.http_5xx("/api/projects");
        let peeked = counters.snapshot();
        assert_eq!(peeked.ingest.accepted, 1);
        assert_eq!(peeked.http_5xx_by_route["/api/projects"], 1);
        assert_eq!(counters.snapshot_and_reset(), peeked);
    }

    /// Every hook in the request path bumps the same process-wide set.
    #[test]
    fn the_global_set_is_one_instance() {
        assert!(std::ptr::eq(Counters::global(), Counters::global()));
    }
}

// =============================================================================
// Panic hook
// =============================================================================

mod panics {
    use rustrak::telemetry::{install_panic_hook, own_location, Counters};

    /// The hook records where our code panicked and nothing about why: the
    /// message may carry a path, a query or a DSN.
    #[test]
    fn a_panic_in_our_own_tree_is_counted_by_location_only() {
        install_panic_hook(Counters::global());
        let line = line!() + 1;
        let _ = std::panic::catch_unwind(|| panic!("secret /home/alice/app.db"));
        let health = Counters::global().snapshot_and_reset();
        let key = format!("{}:{line}", file!());
        assert_eq!(health.panics.get(&key), Some(&1), "{:?}", health.panics);
        assert!(health.panics.keys().all(|k| !k.contains("secret")));
    }

    /// Dependencies and the standard library report absolute paths
    /// (`/rustc/…`, `~/.cargo/registry/…`); those are not ours to report.
    #[test]
    fn only_relative_paths_count_as_our_own_code() {
        assert_eq!(
            own_location("src/digest/mod.rs", 212).as_deref(),
            Some("src/digest/mod.rs:212")
        );
        assert_eq!(
            own_location("/rustc/abc/library/core/src/option.rs", 1),
            None
        );
        assert_eq!(
            own_location("/Users/x/.cargo/registry/src/foo/lib.rs", 1),
            None
        );
    }
}

// =============================================================================
// Resource sampler
// =============================================================================

mod resources {
    use rustrak::telemetry::{probe, Sampler};

    #[test]
    fn nothing_sampled_reads_as_absent() {
        let rss = Sampler::new().snapshot_and_reset();
        assert_eq!((rss.min, rss.max, rss.last), (None, None, None));
    }

    #[test]
    fn samples_fold_into_min_max_and_last() {
        let sampler = Sampler::new();
        for mb in [120, 95, 140, 110] {
            sampler.record_rss_mb(mb);
        }
        let rss = sampler.snapshot_and_reset();
        assert_eq!(
            (rss.min, rss.max, rss.last),
            (Some(95), Some(140), Some(110))
        );
    }

    /// The next window starts from the last reading rather than from
    /// nothing: a quiet 6 hours still reports the memory the process holds.
    #[test]
    fn a_peek_leaves_the_readings_intact() {
        let sampler = Sampler::new();
        sampler.record_rss_mb(120);
        sampler.record_rss_mb(80);
        let peeked = sampler.snapshot();
        assert_eq!(
            (peeked.min, peeked.max, peeked.last),
            (Some(80), Some(120), Some(80))
        );
        assert_eq!(sampler.snapshot_and_reset(), peeked);
    }

    #[test]
    fn the_next_window_starts_from_the_last_reading() {
        let sampler = Sampler::new();
        sampler.record_rss_mb(120);
        sampler.record_rss_mb(80);
        sampler.snapshot_and_reset();
        let rss = sampler.snapshot_and_reset();
        assert_eq!((rss.min, rss.max, rss.last), (Some(80), Some(80), Some(80)));
    }

    /// The probes answer on Linux and say so honestly elsewhere.
    #[test]
    fn the_process_probes_answer_on_linux_only() {
        let rss = probe::rss_mb();
        let total = probe::mem_total_mb();
        if cfg!(target_os = "linux") {
            assert!(rss.is_some_and(|mb| mb > 0), "{rss:?}");
            assert!(total.is_some_and(|mb| mb > 0), "{total:?}");
        } else {
            assert_eq!((rss, total), (None, None));
        }
    }
}

// =============================================================================
// The report and the PostHog adapter
// =============================================================================

mod posthog {
    use crate::common::telemetry::sample_report;
    use rustrak::telemetry::posthog::PostHogSink;
    use serde_json::Value;

    fn envelope() -> Value {
        PostHogSink::new("https://eu.i.posthog.com/i/v0/e/", "phc_test").envelope(&sample_report())
    }

    #[test]
    fn the_envelope_is_one_heartbeat_keyed_by_the_instance() {
        let e = envelope();
        assert_eq!(e["api_key"], "phc_test");
        assert_eq!(e["event"], "heartbeat");
        assert_eq!(e["distinct_id"], "0b6d3a3e-1111-4a5b-8c9d-0e0f10111213");
        assert!(e["timestamp"].as_str().is_some_and(|t| t.ends_with('Z')));
    }

    /// PostHog derives country and city from the request IP unless told not
    /// to. It is told not to on every event, not only in the project settings.
    #[test]
    fn geoip_is_switched_off_on_every_event() {
        assert_eq!(envelope()["properties"]["$geoip_disable"], true);
    }

    #[test]
    fn the_report_travels_as_the_event_properties() {
        let p = &envelope()["properties"];
        assert_eq!(p["schema"], 1);
        assert_eq!(p["version"], "0.15.0");
        assert_eq!(p["rss_mb"]["max"], 95);
        assert_eq!(p["volume"]["events_24h"], 4500);
        assert_eq!(p["health"]["ingest"]["accepted"], 0);
        assert_eq!(p["config"]["alert_providers"][0], "slack");
        assert_eq!(p["config"]["quota_customized"], true);
        assert_eq!(p["health"]["digest"]["rate_limited"], 0);
        assert!(
            p.get("instance_id").is_none(),
            "distinct_id already carries it"
        );
    }

    /// Person properties give "which version is this instance on now"
    /// without scanning events.
    #[test]
    fn the_current_build_facts_are_set_on_the_instance() {
        let set = &envelope()["properties"]["$set"];
        assert_eq!(set["version"], "0.15.0");
        assert_eq!(set["db_backend"], "sqlite");
        assert_eq!(set["os"], "linux");
        assert_eq!(set["arch"], "x86_64");
        assert_eq!(set["container"], true);
    }
}

// =============================================================================
// Engine version
// =============================================================================

/// Only the first two components travel: enough to spot an engine that
/// misbehaves, not enough to fingerprint a build.
#[test]
fn the_engine_version_is_cut_to_major_and_minor() {
    use rustrak::telemetry::major_minor;
    assert_eq!(major_minor("3.46.1"), "3.46");
    assert_eq!(major_minor("16.4 (Debian 16.4-1.pgdg120+1)"), "16.4");
    assert_eq!(major_minor("17"), "17");
    assert_eq!(major_minor(""), "");
}

// =============================================================================
// The SQLite file behind DATABASE_URL
// =============================================================================

#[test]
fn the_sqlite_file_is_read_from_the_database_url() {
    use rustrak::telemetry::sqlite_path_from_url;
    use std::path::PathBuf;
    assert_eq!(
        sqlite_path_from_url("sqlite:///data/rustrak.db"),
        Some(PathBuf::from("/data/rustrak.db"))
    );
    assert_eq!(
        sqlite_path_from_url("sqlite://rustrak.db?mode=rwc"),
        Some(PathBuf::from("rustrak.db"))
    );
    assert_eq!(
        sqlite_path_from_url("sqlite:rustrak.db"),
        Some(PathBuf::from("rustrak.db"))
    );
    assert_eq!(sqlite_path_from_url("sqlite::memory:"), None);
    assert_eq!(sqlite_path_from_url("sqlite://:memory:"), None);
    assert_eq!(sqlite_path_from_url("postgres://u:p@h/db"), None);
}
