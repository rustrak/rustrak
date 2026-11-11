//! Unit tests for authentication parsing
//!
//! Tests X-Sentry-Auth header parsing.

use rustrak::auth::sentry_auth::parse_sentry_auth_header;

#[test]
fn test_parse_sentry_auth_header() {
    let header = "Sentry sentry_key=abc123, sentry_version=7, sentry_client=test/1.0";
    let result = parse_sentry_auth_header(header);

    assert_eq!(result.get("sentry_key"), Some(&"abc123".to_string()));
    assert_eq!(result.get("sentry_version"), Some(&"7".to_string()));
    assert_eq!(result.get("sentry_client"), Some(&"test/1.0".to_string()));
}

#[test]
fn test_invalid_header() {
    let result = parse_sentry_auth_header("Bearer token123");
    assert!(result.is_empty());
}

#[test]
fn test_empty_header() {
    let result = parse_sentry_auth_header("");
    assert!(result.is_empty());
}

#[test]
fn test_parse_sentry_auth_header_with_spaces() {
    // Extra spaces around values
    let header = "Sentry sentry_key = abc123 , sentry_version = 7";
    let result = parse_sentry_auth_header(header);

    assert_eq!(result.get("sentry_key"), Some(&"abc123".to_string()));
    assert_eq!(result.get("sentry_version"), Some(&"7".to_string()));
}

#[test]
fn test_parse_sentry_auth_header_uuid_key() {
    let header = "Sentry sentry_key=9ec79c33-ec99-42ab-8353-589fcb2e04dc, sentry_version=7";
    let result = parse_sentry_auth_header(header);

    assert_eq!(
        result.get("sentry_key"),
        Some(&"9ec79c33-ec99-42ab-8353-589fcb2e04dc".to_string())
    );
}

#[test]
fn test_parse_sentry_auth_header_no_hyphens_uuid() {
    let header = "Sentry sentry_key=9ec79c33ec9942ab8353589fcb2e04dc, sentry_version=7";
    let result = parse_sentry_auth_header(header);

    assert_eq!(
        result.get("sentry_key"),
        Some(&"9ec79c33ec9942ab8353589fcb2e04dc".to_string())
    );
}

#[test]
fn test_parse_sentry_auth_header_all_fields() {
    let header = "Sentry sentry_key=abc, sentry_version=7, sentry_client=sentry.python/1.45.0, sentry_timestamp=1704801600.0, sentry_secret=deprecated";
    let result = parse_sentry_auth_header(header);

    assert_eq!(result.get("sentry_key"), Some(&"abc".to_string()));
    assert_eq!(result.get("sentry_version"), Some(&"7".to_string()));
    assert_eq!(
        result.get("sentry_client"),
        Some(&"sentry.python/1.45.0".to_string())
    );
    assert_eq!(
        result.get("sentry_timestamp"),
        Some(&"1704801600.0".to_string())
    );
    assert_eq!(result.get("sentry_secret"), Some(&"deprecated".to_string()));
}

#[test]
fn test_parse_sentry_auth_header_only_sentry_prefix() {
    let header = "Sentry ";
    let result = parse_sentry_auth_header(header);

    // Empty after "Sentry " prefix
    assert!(result.is_empty());
}

#[test]
fn test_parse_sentry_auth_header_missing_value() {
    let header = "Sentry sentry_key";
    let result = parse_sentry_auth_header(header);

    // No "=" means no value, should be skipped
    assert!(!result.contains_key("sentry_key"));
}

#[test]
fn test_parse_sentry_auth_header_empty_value() {
    let header = "Sentry sentry_key=";
    let result = parse_sentry_auth_header(header);

    // Empty value is valid
    assert_eq!(result.get("sentry_key"), Some(&"".to_string()));
}

#[test]
fn test_parse_sentry_auth_header_value_with_equals() {
    // Value containing equals sign
    let header = "Sentry sentry_key=abc=def, sentry_version=7";
    let result = parse_sentry_auth_header(header);

    // splitn(2, '=') ensures we only split on first '='
    assert_eq!(result.get("sentry_key"), Some(&"abc=def".to_string()));
}

#[test]
fn test_parse_sentry_auth_header_case_sensitive_prefix() {
    // "sentry " (lowercase) should not match
    let result = parse_sentry_auth_header("sentry sentry_key=abc");
    assert!(result.is_empty());

    // "SENTRY " (uppercase) should not match
    let result = parse_sentry_auth_header("SENTRY sentry_key=abc");
    assert!(result.is_empty());
}

#[test]
fn test_parse_sentry_auth_header_duplicate_keys() {
    // Last value wins
    let header = "Sentry sentry_key=first, sentry_key=second";
    let result = parse_sentry_auth_header(header);

    // HashMap will keep the last value (due to collect())
    assert_eq!(result.get("sentry_key"), Some(&"second".to_string()));
}

#[test]
fn test_parse_real_world_python_sdk() {
    let header = "Sentry sentry_key=9ec79c33ec9942ab8353589fcb2e04dc, sentry_version=7, sentry_client=sentry.python/1.45.0";
    let result = parse_sentry_auth_header(header);

    assert_eq!(result.len(), 3);
    assert_eq!(
        result.get("sentry_key"),
        Some(&"9ec79c33ec9942ab8353589fcb2e04dc".to_string())
    );
}

#[test]
fn test_parse_real_world_javascript_sdk() {
    let header = "Sentry sentry_key=abc123def456,sentry_version=7,sentry_client=sentry.javascript.browser/7.0.0";
    let result = parse_sentry_auth_header(header);

    // No spaces after commas is valid
    assert_eq!(result.get("sentry_key"), Some(&"abc123def456".to_string()));
    assert_eq!(
        result.get("sentry_client"),
        Some(&"sentry.javascript.browser/7.0.0".to_string())
    );
}
