//! Unit tests for configuration parsing
//!
//! Tests environment variable parsing and default values.
//!
//! Note: These tests modify global environment variables and must run serially.

use actix_web::cookie::Key;
use rustrak::config::{Config, OidcConfig, RateLimitConfig, SecurityConfig};
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

    assert_eq!(config.max_events_per_minute, 1000);
    assert_eq!(config.max_events_per_hour, 10000);
    assert_eq!(config.max_events_per_project_per_minute, 500);
    assert_eq!(config.max_events_per_project_per_hour, 5000);
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
    assert_eq!(config.max_events_per_minute, 1000);
    assert_eq!(config.max_events_per_hour, 10000);

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
// OpenID Connect Config Tests
// =============================================================================

fn clear_oidc_env() {
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
    ] {
        std::env::remove_var(name);
    }
}

#[test]
#[serial]
fn test_oidc_is_disabled_without_issuer() {
    clear_oidc_env();
    assert!(OidcConfig::from_env().unwrap().is_none());
}

#[test]
#[serial]
fn test_oidc_config_defaults_and_domain_allowlist() {
    clear_oidc_env();
    std::env::set_var("OIDC_ISSUER_URL", "https://id.example.com");
    std::env::set_var("OIDC_CLIENT_ID", "rustrak");
    std::env::set_var("OIDC_CLIENT_SECRET", "secret");
    std::env::set_var(
        "OIDC_REDIRECT_URL",
        "https://rustrak.example.com/auth/sso/callback",
    );
    std::env::set_var("OIDC_ALLOWED_DOMAINS", "Example.com, staff.example.org ");

    let config = OidcConfig::from_env().unwrap().unwrap();
    assert_eq!(config.provider_name, "SSO");
    assert_eq!(config.scopes, ["openid", "email", "profile"]);
    assert_eq!(config.allowed_domains, ["example.com", "staff.example.org"]);
    assert!(config.auto_provision);
    assert!(config.require_email_verified);
    clear_oidc_env();
}

#[test]
#[serial]
fn test_oidc_rejects_partial_configuration() {
    clear_oidc_env();
    std::env::set_var("OIDC_ISSUER_URL", "https://id.example.com");
    let error = OidcConfig::from_env().unwrap_err().to_string();
    assert!(error.contains("OIDC_CLIENT_ID"));
    clear_oidc_env();
}

#[test]
#[serial]
fn test_oidc_security_defaults_do_not_fail_open_on_invalid_booleans() {
    clear_oidc_env();
    std::env::set_var("OIDC_ISSUER_URL", "https://id.example.com");
    std::env::set_var("OIDC_CLIENT_ID", "rustrak");
    std::env::set_var("OIDC_CLIENT_SECRET", "secret");
    std::env::set_var(
        "OIDC_REDIRECT_URL",
        "https://rustrak.example.com/auth/sso/callback",
    );
    std::env::set_var("OIDC_AUTO_PROVISION", "typo");
    std::env::set_var("OIDC_REQUIRE_EMAIL_VERIFIED", "typo");

    let config = OidcConfig::from_env().unwrap().unwrap();
    assert!(config.auto_provision);
    assert!(config.require_email_verified);

    std::env::set_var("OIDC_AUTO_PROVISION", "false");
    std::env::set_var("OIDC_REQUIRE_EMAIL_VERIFIED", "off");
    let config = OidcConfig::from_env().unwrap().unwrap();
    assert!(!config.auto_provision);
    assert!(!config.require_email_verified);
    clear_oidc_env();
}
