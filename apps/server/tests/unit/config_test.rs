//! Unit tests for configuration parsing
//!
//! Tests environment variable parsing and default values.
//!
//! Note: These tests modify global environment variables and must run serially.

use actix_web::cookie::Key;
use rustrak::config::{Config, RateLimitConfig, SecurityConfig};
use serial_test::serial;

// =============================================================================
// Rate Limit Config Tests
// =============================================================================

#[test]
#[serial]
fn test_rate_limit_config_defaults() {
    // Clear any env vars that might affect this test
    std::env::remove_var("MAX_EVENTS_PER_MINUTE");
    std::env::remove_var("MAX_EVENTS_PER_HOUR");
    std::env::remove_var("MAX_EVENTS_PER_PROJECT_PER_MINUTE");
    std::env::remove_var("MAX_EVENTS_PER_PROJECT_PER_HOUR");

    let config = RateLimitConfig::from_env();

    // A brake for a runaway loop, not a cap on ordinary spikes.
    assert_eq!(config.max_events_per_minute, 60_000);
    assert_eq!(config.max_events_per_hour, 1_000_000);
    assert_eq!(config.max_events_per_project_per_minute, 20_000);
    assert_eq!(config.max_events_per_project_per_hour, 300_000);
}

/// Telemetry reports whether an operator moved any limit, never the numbers:
/// that is what tells "hits the defaults" from "lowered them on purpose".
#[test]
fn rate_limit_config_knows_whether_it_was_customized() {
    assert!(!RateLimitConfig::default().is_customized());

    let raised = RateLimitConfig {
        max_events_per_project_per_hour: RateLimitConfig::default().max_events_per_project_per_hour
            + 1,
        ..RateLimitConfig::default()
    };
    assert!(raised.is_customized());
}

#[test]
#[serial]
fn test_rate_limit_config_custom_values() {
    // Set custom values
    std::env::set_var("MAX_EVENTS_PER_MINUTE", "100");
    std::env::set_var("MAX_EVENTS_PER_HOUR", "1000");
    std::env::set_var("MAX_EVENTS_PER_PROJECT_PER_MINUTE", "50");
    std::env::set_var("MAX_EVENTS_PER_PROJECT_PER_HOUR", "500");

    let config = RateLimitConfig::from_env();

    assert_eq!(config.max_events_per_minute, 100);
    assert_eq!(config.max_events_per_hour, 1000);
    assert_eq!(config.max_events_per_project_per_minute, 50);
    assert_eq!(config.max_events_per_project_per_hour, 500);

    // Clean up
    std::env::remove_var("MAX_EVENTS_PER_MINUTE");
    std::env::remove_var("MAX_EVENTS_PER_HOUR");
    std::env::remove_var("MAX_EVENTS_PER_PROJECT_PER_MINUTE");
    std::env::remove_var("MAX_EVENTS_PER_PROJECT_PER_HOUR");
}

#[test]
#[serial]
fn test_rate_limit_config_invalid_values_use_defaults() {
    // Set invalid (non-numeric) values
    std::env::set_var("MAX_EVENTS_PER_MINUTE", "not-a-number");
    std::env::set_var("MAX_EVENTS_PER_HOUR", "abc");

    let config = RateLimitConfig::from_env();

    // Should fall back to defaults
    let defaults = RateLimitConfig::default();
    assert_eq!(config.max_events_per_minute, defaults.max_events_per_minute);
    assert_eq!(config.max_events_per_hour, defaults.max_events_per_hour);

    // Clean up
    std::env::remove_var("MAX_EVENTS_PER_MINUTE");
    std::env::remove_var("MAX_EVENTS_PER_HOUR");
}

#[test]
#[serial]
fn test_rate_limit_config_zero_values() {
    std::env::set_var("MAX_EVENTS_PER_MINUTE", "0");
    std::env::set_var("MAX_EVENTS_PER_HOUR", "0");

    let config = RateLimitConfig::from_env();

    // Zero is a valid value (effectively disables rate limiting)
    assert_eq!(config.max_events_per_minute, 0);
    assert_eq!(config.max_events_per_hour, 0);

    // Clean up
    std::env::remove_var("MAX_EVENTS_PER_MINUTE");
    std::env::remove_var("MAX_EVENTS_PER_HOUR");
}

#[test]
#[serial]
fn test_rate_limit_config_negative_values() {
    std::env::set_var("MAX_EVENTS_PER_MINUTE", "-100");

    let config = RateLimitConfig::from_env();

    // Negative values are technically valid i64
    assert_eq!(config.max_events_per_minute, -100);

    // Clean up
    std::env::remove_var("MAX_EVENTS_PER_MINUTE");
}

// =============================================================================
// PUBLIC_URL Config Tests
// =============================================================================

#[test]
#[serial]
fn test_config_public_url_none_when_not_set() {
    let saved_db = std::env::var("DATABASE_URL").ok();
    let saved_pub = std::env::var("PUBLIC_URL").ok();
    std::env::remove_var("PUBLIC_URL");
    std::env::set_var("DATABASE_URL", "postgres://test:test@localhost/test");

    let config = Config::from_env().expect("Config::from_env() should succeed");
    assert!(config.public_url.is_none());

    std::env::remove_var("DATABASE_URL");
    if let Some(v) = saved_db {
        std::env::set_var("DATABASE_URL", v);
    }
    match saved_pub {
        Some(v) => std::env::set_var("PUBLIC_URL", v),
        None => std::env::remove_var("PUBLIC_URL"),
    }
}

#[test]
#[serial]
fn test_config_public_url_loaded_from_env() {
    let saved_db = std::env::var("DATABASE_URL").ok();
    let saved_pub = std::env::var("PUBLIC_URL").ok();
    std::env::set_var("PUBLIC_URL", "https://api.example.com");
    std::env::set_var("DATABASE_URL", "postgres://test:test@localhost/test");

    let config = Config::from_env().expect("Config::from_env() should succeed");
    assert_eq!(
        config.public_url,
        Some("https://api.example.com".to_string())
    );

    std::env::remove_var("DATABASE_URL");
    if let Some(v) = saved_db {
        std::env::set_var("DATABASE_URL", v);
    }
    match saved_pub {
        Some(v) => std::env::set_var("PUBLIC_URL", v),
        None => std::env::remove_var("PUBLIC_URL"),
    }
}

#[test]
#[serial]
fn test_config_public_url_strips_trailing_slash() {
    let saved_db = std::env::var("DATABASE_URL").ok();
    let saved_pub = std::env::var("PUBLIC_URL").ok();
    std::env::set_var("PUBLIC_URL", "https://api.example.com/");
    std::env::set_var("DATABASE_URL", "postgres://test:test@localhost/test");

    let config = Config::from_env().expect("Config::from_env() should succeed");
    assert_eq!(
        config.public_url,
        Some("https://api.example.com".to_string())
    );

    std::env::remove_var("DATABASE_URL");
    if let Some(v) = saved_db {
        std::env::set_var("DATABASE_URL", v);
    }
    match saved_pub {
        Some(v) => std::env::set_var("PUBLIC_URL", v),
        None => std::env::remove_var("PUBLIC_URL"),
    }
}

#[test]
#[serial]
fn test_config_public_url_empty_string_treated_as_none() {
    let saved_db = std::env::var("DATABASE_URL").ok();
    let saved_pub = std::env::var("PUBLIC_URL").ok();
    std::env::set_var("PUBLIC_URL", "");
    std::env::set_var("DATABASE_URL", "postgres://test:test@localhost/test");

    let config = Config::from_env().expect("Config::from_env() should succeed");
    assert!(config.public_url.is_none());

    std::env::remove_var("DATABASE_URL");
    if let Some(v) = saved_db {
        std::env::set_var("DATABASE_URL", v);
    }
    match saved_pub {
        Some(v) => std::env::set_var("PUBLIC_URL", v),
        None => std::env::remove_var("PUBLIC_URL"),
    }
}

// =============================================================================
// SESSION_SECRET_KEY Tests
// =============================================================================

/// Restores SESSION_SECRET_KEY and SSL_PROXY to their pre-test values on drop,
/// so a failing assertion cannot leak state into the next serial test.
struct SessionEnv {
    secret: Option<String>,
    ssl_proxy: Option<String>,
}

impl SessionEnv {
    fn set(secret: Option<&str>) -> Self {
        let guard = Self {
            secret: std::env::var("SESSION_SECRET_KEY").ok(),
            ssl_proxy: std::env::var("SSL_PROXY").ok(),
        };
        std::env::remove_var("SSL_PROXY");
        match secret {
            Some(value) => std::env::set_var("SESSION_SECRET_KEY", value),
            None => std::env::remove_var("SESSION_SECRET_KEY"),
        }
        guard
    }
}

impl Drop for SessionEnv {
    fn drop(&mut self) {
        match &self.secret {
            Some(value) => std::env::set_var("SESSION_SECRET_KEY", value),
            None => std::env::remove_var("SESSION_SECRET_KEY"),
        }
        match &self.ssl_proxy {
            Some(value) => std::env::set_var("SSL_PROXY", value),
            None => std::env::remove_var("SSL_PROXY"),
        }
    }
}

#[test]
#[serial]
fn test_session_secret_shorter_than_64_bytes_is_rejected_at_load() {
    let _env = SessionEnv::set(Some("too-short-secret-key"));

    let result = SecurityConfig::from_env();

    assert!(
        result.is_err(),
        "a 20-byte secret must be rejected at load, not panic later in the cookie crate"
    );
}

#[test]
#[serial]
fn test_session_secret_too_short_error_names_the_variable_and_the_fix() {
    let _env = SessionEnv::set(Some("too-short-secret-key"));

    let message = SecurityConfig::from_env()
        .expect_err("a 20-byte secret must be rejected")
        .to_string();

    assert!(
        message.contains("SESSION_SECRET_KEY"),
        "the operator must be told which variable is wrong, got: {message}"
    );
    assert!(
        message.contains("20"),
        "the operator must be told the length we actually received, got: {message}"
    );
    assert!(
        message.contains("64"),
        "the operator must be told the minimum accepted length, got: {message}"
    );
    assert!(
        message.contains("openssl rand -hex 32"),
        "the operator must be told how to generate a valid key, got: {message}"
    );
}

#[test]
#[serial]
fn test_secret_of_64_bytes_keeps_the_key_existing_sessions_were_signed_with() {
    // What `openssl rand -hex 32` produces, and what every current deployment runs.
    let secret = "0123456789abcdef".repeat(4);
    assert_eq!(secret.len(), 64);
    let _env = SessionEnv::set(Some(&secret));

    let key = SecurityConfig::from_env()
        .expect("a 64-byte secret must load")
        .session_key()
        .expect("a 64-byte secret must build a key");

    // `Key` has no Debug impl, so compare with assert! rather than assert_eq!.
    assert!(
        key == Key::from(secret.as_bytes()),
        "keys of 64 bytes or more must be used verbatim, otherwise upgrading logs everyone out"
    );
}

/// Regression guard for rustrak/rustrak#298: the reporter set a 44-byte key,
/// which is what `openssl rand -base64 32` produces, and the server crash-looped
/// on `TooShort(44)` from the cookie crate after migrations had already run.
/// The key is still refused, but now it is refused in a way the operator can act on.
#[test]
#[serial]
fn test_secret_of_44_bytes_from_base64_is_refused_with_a_usable_message() {
    let secret = "sB5nQm0dK9xTfR2wV7yZaL4pC8jH1gE6uN3oI0kM5tQ=";
    assert_eq!(secret.len(), 44);
    let _env = SessionEnv::set(Some(secret));

    let message = SecurityConfig::from_env()
        .expect_err("a 44-byte secret must be refused, not panic in the cookie crate")
        .to_string();

    assert!(
        message.contains("SESSION_SECRET_KEY") && message.contains("44"),
        "the message must name the variable and the length received, got: {message}"
    );
}

#[test]
#[serial]
fn test_same_secret_builds_the_same_key_so_sessions_survive_a_restart() {
    let secret = "0123456789abcdef".repeat(4);
    let _env = SessionEnv::set(Some(&secret));

    let first = SecurityConfig::from_env().unwrap().session_key().unwrap();
    let second = SecurityConfig::from_env().unwrap().session_key().unwrap();

    assert!(first == second, "a fixed secret must be deterministic");
}

#[test]
#[serial]
fn test_no_secret_builds_a_random_key_per_start() {
    let _env = SessionEnv::set(None);

    let first = SecurityConfig::from_env().unwrap().session_key().unwrap();
    let second = SecurityConfig::from_env().unwrap().session_key().unwrap();

    assert!(
        first != second,
        "without a configured secret each start must get its own key"
    );
}

#[test]
#[serial]
fn test_debug_output_does_not_leak_the_session_secret() {
    let secret = "0123456789abcdef".repeat(4);
    let _env = SessionEnv::set(Some(&secret));

    let rendered = format!("{:?}", SecurityConfig::from_env().unwrap());

    assert!(
        !rendered.contains(&secret),
        "the session secret must never reach a log line, got: {rendered}"
    );
    assert!(
        rendered.contains("ssl_proxy"),
        "the rest of the config must still be inspectable, got: {rendered}"
    );
}

#[test]
#[serial]
fn test_ssl_proxy_still_requires_a_session_secret() {
    let _env = SessionEnv::set(None);
    std::env::set_var("SSL_PROXY", "true");

    let result = SecurityConfig::from_env();

    assert!(
        result.is_err(),
        "a secure-cookie deployment must not fall back to a per-start random key"
    );
}

// =============================================================================
// RUSTRAK_DASHBOARD switch
// =============================================================================

/// Runs `body` with `DATABASE_URL` set and `RUSTRAK_DASHBOARD` as given,
/// restoring both afterwards so the serial tests around it see the same
/// environment they started with.
fn with_dashboard_env<T>(value: Option<&str>, body: impl FnOnce() -> T) -> T {
    let saved_db = std::env::var("DATABASE_URL").ok();
    let saved_switch = std::env::var("RUSTRAK_DASHBOARD").ok();
    std::env::set_var("DATABASE_URL", "postgres://test:test@localhost/test");
    match value {
        Some(v) => std::env::set_var("RUSTRAK_DASHBOARD", v),
        None => std::env::remove_var("RUSTRAK_DASHBOARD"),
    }

    let out = body();

    match saved_db {
        Some(v) => std::env::set_var("DATABASE_URL", v),
        None => std::env::remove_var("DATABASE_URL"),
    }
    match saved_switch {
        Some(v) => std::env::set_var("RUSTRAK_DASHBOARD", v),
        None => std::env::remove_var("RUSTRAK_DASHBOARD"),
    }
    out
}

#[test]
#[serial]
fn the_dashboard_is_enabled_unless_switched_off() {
    with_dashboard_env(None, || {
        let config = Config::from_env().expect("Config::from_env() should succeed");
        assert!(config.dashboard.enabled);
    });
}

#[test]
#[serial]
fn the_dashboard_switch_reads_the_usual_spellings() {
    for (value, expected) in [
        ("off", false),
        ("false", false),
        ("0", false),
        ("OFF", false),
        (" off ", false),
        ("on", true),
        ("true", true),
        ("1", true),
    ] {
        with_dashboard_env(Some(value), || {
            let config = Config::from_env().expect("Config::from_env() should succeed");
            assert_eq!(
                config.dashboard.enabled, expected,
                "RUSTRAK_DASHBOARD={value:?}"
            );
        });
    }
}

/// A value the switch does not recognise is refused rather than read as
/// either side. The asymmetry is the point: an operator who typed `of` meant
/// to keep the dashboard off the box, and a default of "on" would expose it.
#[test]
#[serial]
fn the_dashboard_switch_refuses_what_it_does_not_recognise() {
    with_dashboard_env(Some("of"), || {
        let error = Config::from_env().expect_err("a typo must not be read as on or off");
        assert!(
            matches!(error, rustrak::config::ConfigError::InvalidDashboardSwitch { ref value } if value == "of"),
            "got {error:?}"
        );
        assert!(error.to_string().contains("RUSTRAK_DASHBOARD"), "{error}");
    });
}

// =============================================================================
// RUSTRAK_TELEMETRY switch
// =============================================================================

/// Runs `body` with `DATABASE_URL` set and the telemetry variables as given,
/// restoring everything afterwards.
fn with_telemetry_env<T>(
    switch: Option<&str>,
    do_not_track: Option<&str>,
    body: impl FnOnce() -> T,
) -> T {
    const VARS: [&str; 3] = ["DATABASE_URL", "RUSTRAK_TELEMETRY", "DO_NOT_TRACK"];
    let saved: Vec<Option<String>> = VARS.iter().map(|v| std::env::var(v).ok()).collect();

    std::env::set_var("DATABASE_URL", "postgres://test:test@localhost/test");
    for (name, value) in [
        ("RUSTRAK_TELEMETRY", switch),
        ("DO_NOT_TRACK", do_not_track),
    ] {
        match value {
            Some(v) => std::env::set_var(name, v),
            None => std::env::remove_var(name),
        }
    }

    let out = body();

    for (name, value) in VARS.iter().zip(saved) {
        match value {
            Some(v) => std::env::set_var(name, v),
            None => std::env::remove_var(name),
        }
    }
    out
}

#[test]
#[serial]
fn telemetry_is_enabled_unless_switched_off() {
    with_telemetry_env(None, None, || {
        let config = Config::from_env().expect("Config::from_env() should succeed");
        assert!(config.telemetry.enabled);
        assert!(!config.telemetry.do_not_track);
    });
}

#[test]
#[serial]
fn the_telemetry_switch_reads_the_usual_spellings() {
    for (value, expected) in [
        ("off", false),
        ("false", false),
        ("0", false),
        ("OFF", false),
        (" off ", false),
        ("on", true),
        ("true", true),
        ("1", true),
    ] {
        with_telemetry_env(Some(value), None, || {
            let config = Config::from_env().expect("Config::from_env() should succeed");
            assert_eq!(
                config.telemetry.enabled, expected,
                "RUSTRAK_TELEMETRY={value:?}"
            );
        });
    }
}

/// Same asymmetry as the dashboard switch: a typo must not be read as "on"
/// when the operator meant to keep data from leaving the box.
#[test]
#[serial]
fn the_telemetry_switch_refuses_what_it_does_not_recognise() {
    with_telemetry_env(Some("of"), None, || {
        let error = Config::from_env().expect_err("a typo must not be read as on or off");
        assert!(
            matches!(error, rustrak::config::ConfigError::InvalidTelemetrySwitch { ref value } if value == "of"),
            "got {error:?}"
        );
        assert!(error.to_string().contains("RUSTRAK_TELEMETRY"), "{error}");
    });
}

/// `DO_NOT_TRACK=1` is the cross-tool convention from consoledonottrack.com.
/// Only `1` and `true` count; anything else leaves it unset.
#[test]
#[serial]
fn do_not_track_is_read_from_the_conventional_variable() {
    for (value, expected) in [("1", true), ("true", true), ("0", false), ("", false)] {
        with_telemetry_env(None, Some(value), || {
            let config = Config::from_env().expect("Config::from_env() should succeed");
            assert_eq!(
                config.telemetry.do_not_track, expected,
                "DO_NOT_TRACK={value:?}"
            );
        });
    }
}

// =============================================================================
// Where alert notifications link to
// =============================================================================

/// Runs `body` with `DASHBOARD_URL`, `PUBLIC_URL`, `HOST` and `PORT` as given,
/// restoring all of them and `DATABASE_URL` afterwards.
fn with_link_env<T>(
    dashboard_url: Option<&str>,
    public_url: Option<&str>,
    body: impl FnOnce() -> T,
) -> T {
    const VARS: [&str; 5] = [
        "DATABASE_URL",
        "DASHBOARD_URL",
        "PUBLIC_URL",
        "HOST",
        "PORT",
    ];
    let saved: Vec<_> = VARS.iter().map(|v| std::env::var(v).ok()).collect();
    std::env::set_var("DATABASE_URL", "postgres://test:test@localhost/test");
    std::env::set_var("HOST", "0.0.0.0");
    std::env::set_var("PORT", "8080");
    for (var, value) in [("DASHBOARD_URL", dashboard_url), ("PUBLIC_URL", public_url)] {
        match value {
            Some(v) => std::env::set_var(var, v),
            None => std::env::remove_var(var),
        }
    }

    let out = body();

    for (var, value) in VARS.iter().zip(saved) {
        match value {
            Some(v) => std::env::set_var(var, v),
            None => std::env::remove_var(var),
        }
    }
    out
}

/// The server serves the dashboard, so the address SDKs reach is also the one
/// people open: without `DASHBOARD_URL`, alert links follow `PUBLIC_URL`.
#[test]
#[serial]
fn alert_links_follow_public_url_when_dashboard_url_is_unset() {
    with_link_env(None, Some("https://rustrak.example.com/"), || {
        let config = Config::from_env().expect("Config::from_env() should succeed");
        assert_eq!(config.dashboard_url(), "https://rustrak.example.com");
    });
}

#[test]
#[serial]
fn dashboard_url_wins_for_a_dashboard_on_its_own_host() {
    with_link_env(
        Some("https://ui.example.com/"),
        Some("https://api.example.com"),
        || {
            let config = Config::from_env().expect("Config::from_env() should succeed");
            assert_eq!(config.dashboard_url(), "https://ui.example.com");
        },
    );
}

#[test]
#[serial]
fn alert_links_fall_back_to_the_bind_address() {
    with_link_env(Some("  "), None, || {
        let config = Config::from_env().expect("Config::from_env() should succeed");
        assert_eq!(config.dashboard_url(), "http://0.0.0.0:8080");
    });
}
