//! Configuration read from the environment: parsing and defaults.
//!
//! Every test that touches the environment holds an [`EnvGuard`], which
//! serializes them and restores what they changed.

use crate::common::EnvGuard;
use actix_web::cookie::Key;
use rustrak::config::{Config, OidcConfig, RateLimitConfig, SecurityConfig};

#[test]
fn test_rate_limit_config_defaults() {
    let mut env = EnvGuard::new();
    env.remove("MAX_EVENTS_PER_MINUTE");
    env.remove("MAX_EVENTS_PER_HOUR");
    env.remove("MAX_EVENTS_PER_PROJECT_PER_MINUTE");
    env.remove("MAX_EVENTS_PER_PROJECT_PER_HOUR");

    let config = RateLimitConfig::from_env().unwrap();

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
fn test_rate_limit_config_custom_values() {
    let mut env = EnvGuard::new();
    env.set("MAX_EVENTS_PER_MINUTE", "100");
    env.set("MAX_EVENTS_PER_HOUR", "1000");
    env.set("MAX_EVENTS_PER_PROJECT_PER_MINUTE", "50");
    env.set("MAX_EVENTS_PER_PROJECT_PER_HOUR", "500");

    let config = RateLimitConfig::from_env().unwrap();

    assert_eq!(config.max_events_per_minute, 100);
    assert_eq!(config.max_events_per_hour, 1000);
    assert_eq!(config.max_events_per_project_per_minute, 50);
    assert_eq!(config.max_events_per_project_per_hour, 500);
}

/// A typo in a number stops startup and names the variable: falling back to
/// the default silently ran a server the operator did not configure.
#[test]
fn an_unparsable_number_is_refused_and_named() {
    for name in [
        "MAX_EVENTS_PER_MINUTE",
        "DATABASE_MAX_CONNECTIONS",
        "DATABASE_ACQUIRE_TIMEOUT_SECS",
        "SOURCEMAP_CACHE_MB",
        "MAX_CHUNK_SIZE_BYTES",
        "SESSION_FLUSH_INTERVAL_SECS",
        "SMTP_PORT",
        "INGEST_MAX_PENDING_EVENTS",
        "INGEST_MIN_FREE_DISK_MB",
    ] {
        let mut env = EnvGuard::new();
        env.set("DATABASE_URL", "postgres://test:test@localhost/test")
            .set(name, "ten");

        let message = Config::from_env()
            .expect_err("a value that is not a number must not start the server")
            .to_string();

        assert!(message.contains(name), "{name}: {message}");
        assert!(message.contains("\"ten\""), "{name}: {message}");
    }
}

/// Unset or blank keeps the default.
#[test]
fn a_blank_number_keeps_its_default() {
    let mut env = EnvGuard::new();
    env.set("MAX_EVENTS_PER_MINUTE", " ");
    env.remove("MAX_EVENTS_PER_HOUR");

    let config = RateLimitConfig::from_env().unwrap();

    let defaults = RateLimitConfig::default();
    assert_eq!(config.max_events_per_minute, defaults.max_events_per_minute);
    assert_eq!(config.max_events_per_hour, defaults.max_events_per_hour);
}

/// Load shedding is on by default with limits no ordinary workload meets.
#[test]
fn ingest_load_shedding_defaults() {
    let mut env = EnvGuard::new();
    env.set("DATABASE_URL", "postgres://test:test@localhost/test");
    env.remove("INGEST_MAX_PENDING_EVENTS");
    env.remove("INGEST_MIN_FREE_DISK_MB");

    let config = Config::from_env().unwrap();

    assert_eq!(config.ingest_max_pending_events, 100_000);
    assert_eq!(config.ingest_min_free_disk_bytes, 64 * 1024 * 1024);
}

#[test]
fn test_rate_limit_config_zero_values() {
    let mut env = EnvGuard::new();
    env.set("MAX_EVENTS_PER_MINUTE", "0");
    env.set("MAX_EVENTS_PER_HOUR", "0");

    let config = RateLimitConfig::from_env().unwrap();

    // Zero is a valid value (effectively disables rate limiting)
    assert_eq!(config.max_events_per_minute, 0);
    assert_eq!(config.max_events_per_hour, 0);
}

#[test]
fn test_rate_limit_config_negative_values() {
    let mut env = EnvGuard::new();
    env.set("MAX_EVENTS_PER_MINUTE", "-100");

    let config = RateLimitConfig::from_env().unwrap();

    // Negative values are technically valid i64
    assert_eq!(config.max_events_per_minute, -100);
}

#[test]
fn test_config_public_url() {
    for (value, expected) in [
        (None, None),
        (
            Some("https://api.example.com"),
            Some("https://api.example.com"),
        ),
        (
            Some("https://api.example.com/"),
            Some("https://api.example.com"),
        ),
        (Some(""), None),
    ] {
        let mut env = EnvGuard::new();
        env.set("DATABASE_URL", "postgres://test:test@localhost/test")
            .set_opt("PUBLIC_URL", value);

        let config = Config::from_env().expect("Config::from_env() should succeed");
        assert_eq!(
            config.public_url.as_deref(),
            expected,
            "PUBLIC_URL={value:?}"
        );
    }
}

/// `SESSION_SECRET_KEY` as given and `SSL_PROXY` unset, until dropped.
fn session_env(secret: Option<&str>) -> EnvGuard {
    let mut env = EnvGuard::new();
    env.remove("SSL_PROXY")
        .set_opt("SESSION_SECRET_KEY", secret);
    env
}

#[test]
fn test_session_secret_shorter_than_64_bytes_is_rejected_at_load() {
    let _env = session_env(Some("too-short-secret-key"));

    let result = SecurityConfig::from_env();

    assert!(
        result.is_err(),
        "a 20-byte secret must be rejected at load, not panic later in the cookie crate"
    );
}

#[test]
fn test_session_secret_too_short_error_names_the_variable_and_the_fix() {
    let _env = session_env(Some("too-short-secret-key"));

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
fn test_secret_of_64_bytes_keeps_the_key_existing_sessions_were_signed_with() {
    // What `openssl rand -hex 32` produces, and what every current deployment runs.
    let secret = "0123456789abcdef".repeat(4);
    assert_eq!(secret.len(), 64);
    let _env = session_env(Some(&secret));

    let key = SecurityConfig::from_env()
        .expect("a 64-byte secret must load")
        .session_key;

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
fn test_secret_of_44_bytes_from_base64_is_refused_with_a_usable_message() {
    let secret = "sB5nQm0dK9xTfR2wV7yZaL4pC8jH1gE6uN3oI0kM5tQ=";
    assert_eq!(secret.len(), 44);
    let _env = session_env(Some(secret));

    let message = SecurityConfig::from_env()
        .expect_err("a 44-byte secret must be refused, not panic in the cookie crate")
        .to_string();

    assert!(
        message.contains("SESSION_SECRET_KEY") && message.contains("44"),
        "the message must name the variable and the length received, got: {message}"
    );
}

#[test]
fn test_same_secret_builds_the_same_key_so_sessions_survive_a_restart() {
    let secret = "0123456789abcdef".repeat(4);
    let _env = session_env(Some(&secret));

    let first = SecurityConfig::from_env().unwrap().session_key;
    let second = SecurityConfig::from_env().unwrap().session_key;

    assert!(first == second, "a fixed secret must be deterministic");
}

#[test]
fn test_no_secret_builds_a_random_key_per_start() {
    let _env = session_env(None);

    let first = SecurityConfig::from_env().unwrap().session_key;
    let second = SecurityConfig::from_env().unwrap().session_key;

    assert!(
        first != second,
        "without a configured secret each start must get its own key"
    );
}

#[test]
fn test_debug_output_does_not_leak_the_session_secret() {
    let secret = "0123456789abcdef".repeat(4);
    let _env = session_env(Some(&secret));

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
fn test_ssl_proxy_still_requires_a_session_secret() {
    let mut env = session_env(None);
    env.set("SSL_PROXY", "true");

    let result = SecurityConfig::from_env();

    assert!(
        result.is_err(),
        "a secure-cookie deployment must not fall back to a per-start random key"
    );
}

/// Runs `body` with `DATABASE_URL` set and `RUSTRAK_DASHBOARD` as given.
fn with_dashboard_env<T>(value: Option<&str>, body: impl FnOnce() -> T) -> T {
    let mut env = EnvGuard::new();
    env.set("DATABASE_URL", "postgres://test:test@localhost/test")
        .set_opt("RUSTRAK_DASHBOARD", value);
    body()
}

#[test]
fn the_dashboard_is_enabled_unless_switched_off() {
    with_dashboard_env(None, || {
        let config = Config::from_env().expect("Config::from_env() should succeed");
        assert!(config.dashboard.enabled);
    });
}

#[test]
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
fn the_dashboard_switch_refuses_what_it_does_not_recognise() {
    with_dashboard_env(Some("of"), || {
        let error = Config::from_env().expect_err("a typo must not be read as on or off");
        assert!(
            matches!(error, rustrak::config::ConfigError::InvalidBoolean { ref name, ref value } if name == "RUSTRAK_DASHBOARD" && value == "of"),
            "got {error:?}"
        );
        assert!(error.to_string().contains("RUSTRAK_DASHBOARD"), "{error}");
    });
}

/// Runs `body` with `DATABASE_URL` set and the telemetry variables as given.
fn with_telemetry_env<T>(
    switch: Option<&str>,
    do_not_track: Option<&str>,
    body: impl FnOnce() -> T,
) -> T {
    let mut env = EnvGuard::new();
    env.set("DATABASE_URL", "postgres://test:test@localhost/test")
        .set_opt("RUSTRAK_TELEMETRY", switch)
        .set_opt("DO_NOT_TRACK", do_not_track);
    body()
}

#[test]
fn telemetry_is_enabled_unless_switched_off() {
    with_telemetry_env(None, None, || {
        let config = Config::from_env().expect("Config::from_env() should succeed");
        assert!(config.telemetry.enabled);
        assert!(!config.telemetry.do_not_track);
    });
}

#[test]
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
fn the_telemetry_switch_refuses_what_it_does_not_recognise() {
    with_telemetry_env(Some("of"), None, || {
        let error = Config::from_env().expect_err("a typo must not be read as on or off");
        assert!(
            matches!(error, rustrak::config::ConfigError::InvalidBoolean { ref name, ref value } if name == "RUSTRAK_TELEMETRY" && value == "of"),
            "got {error:?}"
        );
        assert!(error.to_string().contains("RUSTRAK_TELEMETRY"), "{error}");
    });
}

/// `DO_NOT_TRACK=1` is the cross-tool convention from consoledonottrack.com.
#[test]
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

/// Runs `body` with `DASHBOARD_URL` and `PUBLIC_URL` as given, bound to
/// `0.0.0.0:8080`.
fn with_link_env<T>(
    dashboard_url: Option<&str>,
    public_url: Option<&str>,
    body: impl FnOnce() -> T,
) -> T {
    let mut env = EnvGuard::new();
    env.set("DATABASE_URL", "postgres://test:test@localhost/test")
        .set("HOST", "0.0.0.0")
        .set("PORT", "8080")
        .set_opt("DASHBOARD_URL", dashboard_url)
        .set_opt("PUBLIC_URL", public_url);
    body()
}

/// An empty `PORT` keeps the default, like every other number.
#[test]
fn an_empty_port_keeps_the_default() {
    let mut env = EnvGuard::new();
    env.set("DATABASE_URL", "postgres://test:test@localhost/test")
        .set("PORT", "");
    let config = Config::from_env().expect("an empty PORT is unset");
    assert_eq!(config.port, Config::default().port);
}

/// The server serves the dashboard, so the address SDKs reach is also the one
/// people open: without `DASHBOARD_URL`, alert links follow `PUBLIC_URL`.
#[test]
fn alert_links_follow_public_url_when_dashboard_url_is_unset() {
    with_link_env(None, Some("https://rustrak.example.com/"), || {
        let config = Config::from_env().expect("Config::from_env() should succeed");
        assert_eq!(config.dashboard_url(), "https://rustrak.example.com");
    });
}

#[test]
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
fn alert_links_fall_back_to_the_bind_address() {
    with_link_env(Some("  "), None, || {
        let config = Config::from_env().expect("Config::from_env() should succeed");
        assert_eq!(config.dashboard_url(), "http://0.0.0.0:8080");
    });
}

/// Every `OIDC_*` variable unset, until dropped.
fn oidc_env() -> EnvGuard {
    let mut env = EnvGuard::new();
    for name in [
        "OIDC_ISSUER_URL",
        "OIDC_CLIENT_ID",
        "OIDC_CLIENT_SECRET",
        "OIDC_REDIRECT_URL",
        "OIDC_PROVIDER_NAME",
        "OIDC_SCOPES",
        "OIDC_ALLOWED_DOMAINS",
        "OIDC_AUTO_PROVISION",
        "OIDC_REQUIRE_EMAIL_VERIFIED",
        "OIDC_LINK_EXISTING_ACCOUNTS",
        "OIDC_GROUPS_CLAIM",
        "OIDC_ADMIN_GROUPS",
        "OIDC_DISABLE_PASSWORD_LOGIN",
    ] {
        env.remove(name);
    }
    env
}

#[test]
fn test_oidc_is_disabled_without_issuer() {
    let _env = oidc_env();
    assert!(OidcConfig::from_env().unwrap().is_none());
}

#[test]
fn test_oidc_config_defaults_and_domain_allowlist() {
    let mut env = oidc_env();
    env.set("OIDC_ISSUER_URL", "https://id.example.com");
    env.set("OIDC_CLIENT_ID", "rustrak");
    env.set("OIDC_CLIENT_SECRET", "secret");
    env.set(
        "OIDC_REDIRECT_URL",
        "https://rustrak.example.com/auth/sso/callback",
    );
    env.set("OIDC_ALLOWED_DOMAINS", "Example.com, staff.example.org ");

    let config = OidcConfig::from_env().unwrap().unwrap();
    assert_eq!(config.provider_name, "SSO");
    assert_eq!(config.scopes, ["openid", "email", "profile"]);
    assert_eq!(config.allowed_domains, ["example.com", "staff.example.org"]);
    assert!(
        !config.auto_provision,
        "a public provider must not open the instance to anyone with an account there"
    );
    assert!(
        !config.link_existing_accounts,
        "an existing account must not be taken over on the provider's email claim alone"
    );
    assert!(config.require_email_verified);
}

#[test]
fn test_oidc_account_linking_and_provisioning_can_be_enabled() {
    let mut env = oidc_env();
    env.set("OIDC_ISSUER_URL", "https://id.example.com");
    env.set("OIDC_CLIENT_ID", "rustrak");
    env.set("OIDC_CLIENT_SECRET", "secret");
    env.set(
        "OIDC_REDIRECT_URL",
        "https://rustrak.example.com/auth/sso/callback",
    );
    env.set("OIDC_AUTO_PROVISION", "true");
    env.set("OIDC_LINK_EXISTING_ACCOUNTS", "true");

    let config = OidcConfig::from_env().unwrap().unwrap();
    assert!(config.auto_provision);
    assert!(config.link_existing_accounts);

    env.set("OIDC_LINK_EXISTING_ACCOUNTS", "maybe");
    let message = OidcConfig::from_env().unwrap_err().to_string();
    assert!(message.contains("OIDC_LINK_EXISTING_ACCOUNTS"));
}

#[test]
fn test_oidc_group_mapping_defaults_to_the_groups_claim() {
    let mut env = oidc_env();
    env.set("OIDC_ISSUER_URL", "https://id.example.com");
    env.set("OIDC_CLIENT_ID", "rustrak");
    env.set("OIDC_CLIENT_SECRET", "secret");
    env.set(
        "OIDC_REDIRECT_URL",
        "https://rustrak.example.com/auth/sso/callback",
    );

    let config = OidcConfig::from_env().unwrap().unwrap();
    assert_eq!(config.groups_claim, "groups");
    assert!(
        config.admin_groups.is_empty(),
        "no mapping means SSO must not touch roles"
    );
    assert!(!config.disable_password_login);

    env.set("OIDC_GROUPS_CLAIM", " memberOf ");
    env.set("OIDC_ADMIN_GROUPS", "rustrak-admins, ops ,,");

    let config = OidcConfig::from_env().unwrap().unwrap();
    assert_eq!(config.groups_claim, "memberOf");
    assert_eq!(config.admin_groups, ["rustrak-admins", "ops"]);
}

#[test]
fn test_oidc_password_login_can_only_be_disabled_with_sso() {
    let mut env = oidc_env();
    env.set("OIDC_DISABLE_PASSWORD_LOGIN", "true");

    let error = OidcConfig::from_env().unwrap_err().to_string();
    assert!(
        error.contains("OIDC_DISABLE_PASSWORD_LOGIN"),
        "a switch that would lock everyone out must be named: {error}"
    );

    env.set("OIDC_ISSUER_URL", "https://id.example.com");
    env.set("OIDC_CLIENT_ID", "rustrak");
    env.set("OIDC_CLIENT_SECRET", "secret");
    env.set(
        "OIDC_REDIRECT_URL",
        "https://rustrak.example.com/auth/sso/callback",
    );
    env.set("OIDC_DISABLE_PASSWORD_LOGIN", "true");

    let config = OidcConfig::from_env().unwrap().unwrap();
    assert!(config.disable_password_login);
}

#[test]
fn test_oidc_rejects_partial_configuration() {
    let mut env = oidc_env();
    env.set("OIDC_ISSUER_URL", "https://id.example.com");
    let error = OidcConfig::from_env().unwrap_err().to_string();
    assert!(error.contains("OIDC_CLIENT_ID"));
}

#[test]
fn test_oidc_rejects_an_unrecognized_boolean_instead_of_using_the_default() {
    let mut env = oidc_env();
    env.set("OIDC_ISSUER_URL", "https://id.example.com");
    env.set("OIDC_CLIENT_ID", "rustrak");
    env.set("OIDC_CLIENT_SECRET", "secret");
    env.set(
        "OIDC_REDIRECT_URL",
        "https://rustrak.example.com/auth/sso/callback",
    );

    // Falling back to the default on a typo would silently pick a value the
    // operator did not write. The typo has to stop startup.
    env.set("OIDC_AUTO_PROVISION", "ture");
    let message = OidcConfig::from_env()
        .expect_err("an unrecognized boolean must be refused")
        .to_string();
    assert!(
        message.contains("OIDC_AUTO_PROVISION") && message.contains("ture"),
        "the operator must be told which variable is wrong and what it held, got: {message}"
    );

    env.set("OIDC_AUTO_PROVISION", "false");
    env.set("OIDC_REQUIRE_EMAIL_VERIFIED", "typo");
    let message = OidcConfig::from_env()
        .expect_err("an unrecognized boolean must be refused on either switch")
        .to_string();
    assert!(
        message.contains("OIDC_REQUIRE_EMAIL_VERIFIED"),
        "the operator must be told which variable is wrong, got: {message}"
    );

    // An empty value means "unset", like OIDC_ISSUER_URL, and unset keeps the
    // documented defaults.
    env.set("OIDC_AUTO_PROVISION", "");
    env.remove("OIDC_REQUIRE_EMAIL_VERIFIED");
    let config = OidcConfig::from_env().unwrap().unwrap();
    assert!(
        !config.auto_provision,
        "an empty value is unset, and unset keeps the documented default"
    );
    assert!(config.require_email_verified);

    env.set("OIDC_AUTO_PROVISION", "on");
    env.set("OIDC_REQUIRE_EMAIL_VERIFIED", "no");
    let config = OidcConfig::from_env().unwrap().unwrap();
    assert!(config.auto_provision);
    assert!(!config.require_email_verified);
}

#[test]
fn test_oidc_reports_when_any_provider_account_can_join() {
    let mut env = oidc_env();
    env.set("OIDC_ISSUER_URL", "https://id.example.com");
    env.set("OIDC_CLIENT_ID", "rustrak");
    env.set("OIDC_CLIENT_SECRET", "secret");
    env.set(
        "OIDC_REDIRECT_URL",
        "https://rustrak.example.com/auth/sso/callback",
    );

    let closed = OidcConfig::from_env().unwrap().unwrap();
    assert!(!closed.admits_any_provider_account());

    env.set("OIDC_AUTO_PROVISION", "true");
    let open = OidcConfig::from_env().unwrap().unwrap();
    assert!(
        open.admits_any_provider_account(),
        "provisioning without a domain allowlist lets every account at the provider in"
    );

    env.set("OIDC_ALLOWED_DOMAINS", "example.com");
    let restricted = OidcConfig::from_env().unwrap().unwrap();
    assert!(!restricted.admits_any_provider_account());
}

#[test]
fn alerts_reach_private_networks_only_when_switched_on() {
    for (value, expected) in [(None, false), (Some("true"), true), (Some("false"), false)] {
        let mut env = EnvGuard::new();
        env.set("DATABASE_URL", "postgres://test:test@localhost/test")
            .set_opt("RUSTRAK_ALERTS_ALLOW_PRIVATE_NETWORKS", value);

        let config = Config::from_env().expect("Config::from_env() should succeed");
        assert_eq!(
            config.alerts_allow_private_networks, expected,
            "RUSTRAK_ALERTS_ALLOW_PRIVATE_NETWORKS={value:?}"
        );
    }
}
