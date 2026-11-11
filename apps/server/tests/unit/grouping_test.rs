//! Unit tests for the grouping algorithm
//!
//! Tests event grouping logic, hash generation, and denormalized field extraction.

use rustrak::services::grouping::{
    calculate_grouping_key, get_denormalized_fields, get_title, get_type_and_value,
    hash_grouping_key,
};
use serde_json::json;

#[test]
fn test_grouping_key_with_exception() {
    let event = json!({
        "exception": {
            "values": [{
                "type": "TypeError",
                "value": "Cannot read property 'x' of undefined"
            }]
        },
        "transaction": "/api/users"
    });

    let key = calculate_grouping_key(&event);
    assert!(key.contains("TypeError"));
    assert!(key.contains("/api/users"));
}

#[test]
fn test_grouping_key_with_fingerprint() {
    let event = json!({
        "exception": {
            "values": [{
                "type": "Error",
                "value": "Something went wrong"
            }]
        },
        "fingerprint": ["custom-group", "{{ default }}"]
    });

    let key = calculate_grouping_key(&event);
    assert!(key.starts_with("custom-group"));
}

#[test]
fn test_hash_grouping_key() {
    let hash = hash_grouping_key("test");
    assert_eq!(hash.len(), 64); // SHA256 hex = 64 chars
}

#[test]
fn test_get_type_and_value_log_message() {
    let event = json!({
        "message": "Something happened"
    });

    let (type_, value) = get_type_and_value(&event);
    assert_eq!(type_, "Log Message");
    assert_eq!(value, "Something happened");
}

#[test]
fn test_exception_with_values_array() {
    let event = json!({
        "exception": {
            "values": [
                { "type": "InnerError", "value": "inner" },
                { "type": "OuterError", "value": "outer cause" }
            ]
        }
    });

    // Should use the LAST exception (most important in the chain)
    let (type_, value) = get_type_and_value(&event);
    assert_eq!(type_, "OuterError");
    assert_eq!(value, "outer cause");
}

#[test]
fn test_exception_direct_array() {
    // Some SDKs send exception as a direct array, not { values: [...] }
    let event = json!({
        "exception": [
            { "type": "FirstError", "value": "first" },
            { "type": "LastError", "value": "last" }
        ]
    });

    let (type_, value) = get_type_and_value(&event);
    assert_eq!(type_, "LastError");
    assert_eq!(value, "last");
}

#[test]
fn test_exception_missing_type() {
    let event = json!({
        "exception": {
            "values": [{
                "value": "error without type"
            }]
        }
    });

    let (type_, _) = get_type_and_value(&event);
    assert_eq!(type_, "Error"); // Default fallback
}

#[test]
fn test_exception_missing_value() {
    let event = json!({
        "exception": {
            "values": [{
                "type": "CustomError"
            }]
        }
    });

    let (type_, value) = get_type_and_value(&event);
    assert_eq!(type_, "CustomError");
    assert_eq!(value, "");
}

#[test]
fn test_exception_multiline_value() {
    let event = json!({
        "exception": {
            "values": [{
                "type": "Error",
                "value": "First line\nSecond line\nThird line"
            }]
        }
    });

    let key = calculate_grouping_key(&event);
    // Grouping should only use first line for the title portion
    assert!(key.contains("First line"));
    // But the full value is stored
    let (_, value) = get_type_and_value(&event);
    assert!(value.contains("Second line"));
}

#[test]
fn test_logentry_message() {
    let event = json!({
        "logentry": {
            "message": "User %s logged in",
            "params": ["john"]
        }
    });

    let (type_, value) = get_type_and_value(&event);
    assert_eq!(type_, "Log Message");
    assert_eq!(value, "User john logged in");
    assert!(calculate_grouping_key(&event).contains("User %s logged in"));
}

#[test]
fn test_logentry_formatted() {
    let event = json!({
        "logentry": {
            "formatted": "User john logged in"
        }
    });

    let (type_, value) = get_type_and_value(&event);
    assert_eq!(type_, "Log Message");
    assert_eq!(value, "User john logged in");
}

#[test]
fn test_grouping_prefers_message_over_formatted() {
    let john = json!({
        "logentry": {
            "message": "User %s logged in",
            "formatted": "User john logged in"
        }
    });
    let jane = json!({
        "logentry": {
            "message": "User %s logged in",
            "formatted": "User jane logged in"
        }
    });

    assert_eq!(calculate_grouping_key(&john), calculate_grouping_key(&jane));
    assert!(calculate_grouping_key(&john).contains("User %s logged in"));
}

#[test]
fn test_an_empty_formatted_falls_through_to_the_message() {
    // Sentry reads `formatted or message`, and an empty string is falsy there,
    // so it must not shadow a usable template.
    let event = json!({
        "logentry": { "formatted": "", "message": "User %s logged in" }
    });

    let (type_, value) = get_type_and_value(&event);
    assert_eq!(type_, "Log Message");
    assert_eq!(value, "User %s logged in");
}

#[test]
fn test_an_empty_message_falls_through_to_formatted_for_grouping() {
    // The same falsy rule on the grouping side: `message or formatted`.
    let event = json!({
        "logentry": { "message": "", "formatted": "User john logged in" }
    });

    let key = calculate_grouping_key(&event);
    assert!(key.contains("User john logged in"), "got {key}");
}

#[test]
fn test_an_empty_message_object_still_falls_back() {
    let event = json!({
        "message": { "formatted": "", "message": "", "params": null },
        "exception": []
    });

    let (type_, value) = get_type_and_value(&event);
    assert_eq!(type_, "Unknown");
    assert_eq!(value, "");
}

#[test]
fn test_title_prefers_formatted_over_message() {
    let event = json!({
        "logentry": {
            "message": "User %s logged in",
            "formatted": "User john logged in"
        }
    });

    let (type_, value) = get_type_and_value(&event);
    assert_eq!(
        get_title(&type_, &value),
        "Log Message: User john logged in"
    );
}

#[test]
fn test_title_prefers_formatted_in_deprecated_message_object() {
    let event = json!({
        "message": {
            "message": "User %s logged in",
            "formatted": "User john logged in"
        }
    });

    let (_type, value) = get_type_and_value(&event);
    assert_eq!(value, "User john logged in");
}

#[test]
fn test_title_interpolates_positional_params() {
    let event = json!({
        "logentry": {
            "message": "User %s logged in",
            "params": ["john"]
        }
    });

    let (_type, value) = get_type_and_value(&event);
    assert_eq!(value, "User john logged in");
}

#[test]
fn test_title_interpolates_named_params() {
    let event = json!({
        "logentry": {
            "message": "Hello, {name}!",
            "params": { "name": "World" }
        }
    });

    let (_type, value) = get_type_and_value(&event);
    assert_eq!(value, "Hello, World!");
}

#[test]
fn test_title_keeps_template_when_params_do_not_fit() {
    let event = json!({
        "logentry": {
            "message": "User %s logged in",
            "params": null
        }
    });

    let (_type, value) = get_type_and_value(&event);
    assert_eq!(value, "User %s logged in");
}

#[test]
fn test_title_leaves_a_literal_percent_alone() {
    let event = json!({
        "logentry": {
            "message": "Disk 90% full",
            "params": ["ignored"]
        }
    });

    let (_type, value) = get_type_and_value(&event);
    assert_eq!(value, "Disk 90% full");
}

#[test]
fn test_deprecated_message_field() {
    let event = json!({
        "message": "Direct message string"
    });

    let (type_, value) = get_type_and_value(&event);
    assert_eq!(type_, "Log Message");
    assert_eq!(value, "Direct message string");
}

#[test]
fn test_deprecated_message_object() {
    let event = json!({
        "message": {
            "message": "Nested message"
        }
    });

    let (type_, value) = get_type_and_value(&event);
    assert_eq!(type_, "Log Message");
    assert_eq!(value, "Nested message");
}

#[test]
fn test_deprecated_message_object_formatted() {
    let event = json!({
        "message": {
            "formatted": "Nested formatted message",
            "message": null,
            "params": null
        }
    });

    let (type_, value) = get_type_and_value(&event);
    assert_eq!(type_, "Log Message");
    assert_eq!(value, "Nested formatted message");
}

#[test]
fn test_message_only_events_do_not_all_group_together() {
    let foo = json!({ "message": { "formatted": "foo" } });
    let bar = json!({ "message": { "formatted": "bar" } });

    assert_ne!(calculate_grouping_key(&foo), calculate_grouping_key(&bar));
}

#[test]
fn test_log_multiline_uses_first_line() {
    let event = json!({
        "message": "First line of log\nSecond line\nThird line"
    });

    let (_, value) = get_type_and_value(&event);
    // For log messages, only first line is used
    assert_eq!(value, "First line of log");
}

/// Python 3.11 `ExceptionGroup` / .NET `AggregateException` / JS `AggregateError`:
/// a wrapper whose only job is to hold the real errors.
fn exception_group_event(inner: &[(&str, &str)]) -> serde_json::Value {
    let mut values = vec![];
    for (i, (ty, val)) in inner.iter().enumerate() {
        values.push(json!({
            "type": ty, "value": val,
            "mechanism": {
                "type": "chained",
                "exception_id": i + 1,
                "parent_id": 0,
                "source": format!("exceptions[{i}]")
            }
        }));
    }
    values.push(json!({
        "type": "ExceptionGroup",
        "value": format!("{} sub-exception(s)", inner.len()),
        "mechanism": { "type": "generic", "exception_id": 0, "is_exception_group": true }
    }));
    json!({ "exception": { "values": values } })
}

#[test]
fn test_exception_group_is_titled_by_its_only_inner_error() {
    let event = exception_group_event(&[("ValueError", "invalid literal for int()")]);

    let (type_, value) = get_type_and_value(&event);
    assert_eq!(type_, "ValueError");
    assert_eq!(value, "invalid literal for int()");
}

#[test]
fn test_exception_groups_with_different_inner_errors_group_apart() {
    // The wrapper's own value is just a count, so grouping by it would put
    // unrelated errors in one issue.
    let value_error = exception_group_event(&[("ValueError", "invalid literal for int()")]);
    let key_error = exception_group_event(&[("KeyError", "'user_id'")]);

    assert_ne!(
        calculate_grouping_key(&value_error),
        calculate_grouping_key(&key_error)
    );
}

#[test]
fn test_exception_group_groups_by_the_inner_error_not_the_wrapper() {
    let event = exception_group_event(&[("ValueError", "invalid literal for int()")]);
    let key = calculate_grouping_key(&event);

    assert!(key.contains("ValueError"), "got {key}");
    assert!(!key.contains("ExceptionGroup"), "got {key}");
}

#[test]
fn test_exception_group_with_several_distinct_errors_keeps_the_wrapper() {
    // Sentry keeps the root group when the children genuinely differ, so the
    // issue represents the group rather than one arbitrary child.
    let event = exception_group_event(&[("ValueError", "bad int"), ("KeyError", "'user_id'")]);

    let (type_, _value) = get_type_and_value(&event);
    assert_eq!(type_, "ExceptionGroup");
}

#[test]
fn test_identical_siblings_collapse_to_one_inner_error() {
    // Group<['Da', 'Da', 'Da']> is just 'Da'.
    let event = exception_group_event(&[
        ("ValueError", "same"),
        ("ValueError", "same"),
        ("ValueError", "same"),
    ]);

    let (type_, value) = get_type_and_value(&event);
    assert_eq!(type_, "ValueError");
    assert_eq!(value, "same");
}

#[test]
fn test_a_chain_without_mechanism_ids_is_untouched() {
    // Most SDKs never set `exception_id`. Without it the chain cannot be
    // walked, so the last exception still wins and existing issues keep their
    // grouping keys.
    let event = json!({
        "exception": { "values": [
            { "type": "IOError", "value": "disk full" },
            { "type": "RuntimeError", "value": "save failed" }
        ]}
    });

    let (type_, value) = get_type_and_value(&event);
    assert_eq!(type_, "RuntimeError");
    assert_eq!(value, "save failed");
}

#[test]
fn test_a_chain_with_mechanism_ids_is_titled_by_its_root() {
    // With ids the tree is known, and Sentry titles by the root unless a
    // framework override moves the choice. The root is the outermost error;
    // `source: cause` marks the one it wrapped.
    let event = json!({
        "exception": { "values": [
            { "type": "IOError", "value": "disk full",
              "mechanism": { "exception_id": 0 } },
            { "type": "RuntimeError", "value": "save failed",
              "mechanism": { "exception_id": 1, "parent_id": 0, "source": "cause" } }
        ]}
    });

    let (type_, value) = get_type_and_value(&event);
    assert_eq!(type_, "IOError");
    assert_eq!(value, "disk full");
}

#[test]
fn test_duplicate_exception_ids_stop_the_filter() {
    // A well-formed root, but two children claim the same id. The tree cannot
    // be trusted, so the wrapper must survive rather than be collapsed away.
    let event = json!({
        "exception": { "values": [
            { "type": "ValueError", "value": "same",
              "mechanism": { "exception_id": 1, "parent_id": 0 } },
            { "type": "ValueError", "value": "same",
              "mechanism": { "exception_id": 1, "parent_id": 0 } },
            { "type": "ExceptionGroup", "value": "2 sub-exceptions",
              "mechanism": { "exception_id": 0, "is_exception_group": true } }
        ]}
    });

    let (type_, _value) = get_type_and_value(&event);
    assert_eq!(type_, "ExceptionGroup");
}

#[test]
fn test_a_malformed_exception_tree_changes_nothing() {
    // Duplicate ids: the chain cannot be trusted, so it is left alone.
    let event = json!({
        "exception": { "values": [
            { "type": "Inner", "value": "a", "mechanism": { "exception_id": 1, "parent_id": 0 } },
            { "type": "Outer", "value": "b",
              "mechanism": { "exception_id": 1, "is_exception_group": true } }
        ]}
    });

    let (type_, value) = get_type_and_value(&event);
    assert_eq!(type_, "Outer");
    assert_eq!(value, "b");
}

#[test]
fn test_rxjava_wrapper_does_not_title_the_issue() {
    // tests/sentry/event_manager/test_event_manager.py
    // ::test_java_rxjava_exceptions_correct_error_title_subtitle
    for wrapper in [
        "OnErrorNotImplementedException",
        "CompositeException",
        "UndeliverableException",
    ] {
        let event = json!({
            "exception": { "values": [
                { "type": "NullPointerException",
                  "value": "Attempt to read from field 'a.b.c' on a null object",
                  "module": "java.lang",
                  "mechanism": { "type": "chained", "exception_id": 1, "parent_id": 0 } },
                { "type": wrapper,
                  "value": "The exception was not handled due to missing onError handler in the subscribe() method call.",
                  "module": "io.reactivex.rxjava3.exceptions",
                  "mechanism": { "type": "chained", "handled": false, "exception_id": 0 } }
            ]}
        });

        let (type_, value) = get_type_and_value(&event);
        assert_eq!(type_, "NullPointerException", "wrapper {wrapper}");
        assert_eq!(value, "Attempt to read from field 'a.b.c' on a null object");
    }
}

#[test]
fn test_rxjava_wrapper_without_mechanism_data_still_titles_by_the_last() {
    // ::test_java_rxjava_incomplete_error_correct_title_subtitle — without
    // mechanism ids the chain cannot be walked, so the default stands.
    let event = json!({
        "exception": { "values": [
            { "type": "NullPointerException",
              "value": "Attempt to read from field 'a.b.c' on a null object" },
            { "type": "CompositeException", "value": "Can't call onError." }
        ]}
    });

    let (type_, value) = get_type_and_value(&event);
    assert_eq!(type_, "CompositeException");
    assert_eq!(value, "Can't call onError.");
}

#[test]
fn test_kotlin_diagnostic_wrapper_does_not_title_the_issue() {
    // ::test_kotlin_coroutine_diagnostic_exception_correct_title — here the
    // real error is the root and the wrapper is the child, the reverse of the
    // RxJava shape.
    let event = json!({
        "exception": { "values": [
            { "type": "RuntimeException", "value": "main exception", "module": "java.lang",
              "mechanism": { "type": "UncaughtExceptionHandler", "exception_id": 0 } },
            { "type": "DiagnosticCoroutineContextException",
              "value": "[StandaloneCoroutine{Cancelling}@1a2b3c]",
              "module": "kotlinx.coroutines.internal",
              "mechanism": { "type": "suppressed", "exception_id": 1, "parent_id": 0 } }
        ]}
    });

    let (type_, value) = get_type_and_value(&event);
    assert_eq!(type_, "RuntimeException");
    assert_eq!(value, "main exception");
}

#[test]
fn test_kotlin_diagnostic_wrapper_with_chained_mechanism() {
    // ::test_kotlin_coroutine_diagnostic_exception_chained_mechanism_correct_title
    let event = json!({
        "exception": { "values": [
            { "type": "IllegalStateException", "value": "coroutine error", "module": "java.lang",
              "mechanism": { "type": "UncaughtExceptionHandler", "handled": false,
                             "exception_id": 0 } },
            { "type": "DiagnosticCoroutineContextException",
              "module": "kotlinx.coroutines.internal",
              "mechanism": { "type": "chained", "exception_id": 1, "parent_id": 0 } }
        ]}
    });

    let (type_, value) = get_type_and_value(&event);
    assert_eq!(type_, "IllegalStateException");
    assert_eq!(value, "coroutine error");
}

#[test]
fn test_a_lone_diagnostic_wrapper_keeps_the_default() {
    // ::test_kotlin_coroutine_diagnostic_exception_no_parent_keeps_default_behavior
    let event = json!({
        "exception": { "values": [
            { "type": "DiagnosticCoroutineContextException",
              "module": "kotlinx.coroutines.internal",
              "mechanism": { "type": "generic", "exception_id": 0 } }
        ]}
    });

    let (type_, _value) = get_type_and_value(&event);
    assert_eq!(type_, "DiagnosticCoroutineContextException");
}

#[test]
fn test_react_concurrent_rendering_is_titled_by_its_cause() {
    // tests/sentry/grouping/grouping_inputs/react-concurrent-rendering.json
    let event = json!({
        "exception": { "values": [
            { "type": "TypeError", "value": "Load failed",
              "mechanism": { "type": "onerror", "handled": false, "source": "cause",
                             "exception_id": 1, "parent_id": 0 } },
            { "type": "Error",
              "value": "There was an error during concurrent rendering but React was able to recover by instead synchronously rendering the entire root.",
              "mechanism": { "type": "generic", "handled": true, "exception_id": 0 } }
        ]}
    });

    let (type_, value) = get_type_and_value(&event);
    assert_eq!(type_, "TypeError");
    assert_eq!(value, "Load failed");
}

#[test]
fn test_groups_with_different_children_do_not_collapse() {
    // Sentry hashes a `chained_exception` component holding every surviving
    // exception, so the children decide the issue even when the wrapper text is
    // the same constant every time.
    let group = |a: &str, b: &str| {
        json!({
            "exception": { "values": [
                { "type": a, "value": "boom",
                  "mechanism": { "type": "chained", "exception_id": 2, "parent_id": 0 } },
                { "type": b, "value": "boom",
                  "mechanism": { "type": "chained", "exception_id": 1, "parent_id": 0 } },
                { "type": "System.AggregateException", "value": "One or more errors occurred.",
                  "mechanism": { "type": "generic", "exception_id": 0,
                                 "is_exception_group": true } }
            ]}
        })
    };

    assert_ne!(
        calculate_grouping_key(&group("MyApp.SuchWowException", "MyApp.AmazingException")),
        calculate_grouping_key(&group("MyApp.FooException", "MyApp.BarException"))
    );
}

#[test]
fn test_a_chain_groups_by_every_exception_in_it() {
    let chain = |cause: &str| {
        json!({
            "exception": { "values": [
                { "type": "IOError", "value": cause },
                { "type": "RuntimeError", "value": "save failed" }
            ]}
        })
    };

    assert_ne!(
        calculate_grouping_key(&chain("disk full")),
        calculate_grouping_key(&chain("permission denied"))
    );
}

#[test]
fn test_a_single_exception_keeps_its_grouping_key() {
    // The common case must not move: one exception, one component.
    let event = json!({
        "exception": { "values": [{ "type": "TypeError", "value": "boom" }] },
        "transaction": "/api/users"
    });

    assert_eq!(
        calculate_grouping_key(&event),
        "TypeError: boom ⋄ /api/users"
    );
}

#[test]
fn test_the_same_bug_with_different_ids_is_one_issue() {
    let err = |value: &str| {
        json!({
            "exception": { "values": [{ "type": "KeyError", "value": value }] }
        })
    };

    let keys: Vec<String> = ["user_4213", "user_9981", "user_1"]
        .iter()
        .map(|id| calculate_grouping_key(&err(&format!("missing key {id}"))))
        .collect();

    assert_eq!(keys[0], keys[1]);
    assert_eq!(keys[1], keys[2]);
    assert!(
        keys[0].contains("missing key user_<int>"),
        "got {}",
        keys[0]
    );
}

#[test]
fn test_log_messages_with_ids_group_together() {
    let warn = |order: u32| {
        json!({
            "logentry": { "formatted": format!("Payment failed for order {order}") }
        })
    };

    assert_eq!(
        calculate_grouping_key(&warn(4213)),
        calculate_grouping_key(&warn(9981))
    );
}

#[test]
fn test_the_title_keeps_the_real_message() {
    // Normalization is for grouping only. A human reads the actual values.
    let event = json!({
        "exception": { "values": [{ "type": "KeyError", "value": "missing key user_4213" }] }
    });

    let (type_, value) = get_type_and_value(&event);
    assert_eq!(get_title(&type_, &value), "KeyError: missing key user_4213");
}

#[test]
fn test_genuinely_different_bugs_still_split() {
    let a = json!({ "exception": { "values": [{ "type": "KeyError", "value": "missing key user_1" }] } });
    let b = json!({ "exception": { "values": [{ "type": "KeyError", "value": "missing key order_1" }] } });

    assert_ne!(calculate_grouping_key(&a), calculate_grouping_key(&b));
}

#[test]
fn test_transaction_included_in_grouping() {
    let event1 = json!({
        "exception": { "values": [{ "type": "Error", "value": "same error" }] },
        "transaction": "/api/v1/users"
    });
    let event2 = json!({
        "exception": { "values": [{ "type": "Error", "value": "same error" }] },
        "transaction": "/api/v2/users"
    });

    let key1 = calculate_grouping_key(&event1);
    let key2 = calculate_grouping_key(&event2);

    // Same error, different transaction = different groups
    assert_ne!(key1, key2);
    assert!(key1.contains("/api/v1/users"));
    assert!(key2.contains("/api/v2/users"));
}

#[test]
fn test_missing_transaction() {
    let event = json!({
        "exception": { "values": [{ "type": "Error", "value": "test" }] }
    });

    let key = calculate_grouping_key(&event);
    assert!(key.contains("<no transaction>"));
}

#[test]
fn test_custom_fingerprint_only() {
    let event = json!({
        "exception": { "values": [{ "type": "Error", "value": "ignored" }] },
        "fingerprint": ["custom-key-1", "custom-key-2"]
    });

    let key = calculate_grouping_key(&event);
    // Should NOT contain the exception info, only the fingerprint
    assert!(!key.contains("Error"));
    assert!(key.contains("custom-key-1"));
    assert!(key.contains("custom-key-2"));
}

#[test]
fn test_fingerprint_with_default_placeholder() {
    let event = json!({
        "exception": { "values": [{ "type": "MyError", "value": "my message" }] },
        "transaction": "/endpoint",
        "fingerprint": ["prefix", "{{ default }}", "suffix"]
    });

    let key = calculate_grouping_key(&event);
    assert!(key.contains("prefix"));
    assert!(key.contains("MyError"));
    assert!(key.contains("suffix"));
}

#[test]
fn test_fingerprint_multiple_defaults() {
    let event = json!({
        "exception": { "values": [{ "type": "Error", "value": "test" }] },
        "transaction": "/api",
        "fingerprint": ["{{ default }}", "{{ default }}"]
    });

    let key = calculate_grouping_key(&event);
    // Default is expanded twice
    let default_key = "Error: test ⋄ /api";
    assert!(key.contains(default_key));
}

#[test]
fn test_empty_fingerprint_array() {
    let event = json!({
        "exception": { "values": [{ "type": "Error", "value": "test" }] },
        "transaction": "/api",
        "fingerprint": []
    });

    let key = calculate_grouping_key(&event);
    // Empty fingerprint = falls back to default. sentry-ruby always sends
    // `"fingerprint": []` and Sentry treats it as "no custom fingerprint"
    // (sentry/grouping/api.py: `event.data.get("fingerprint") or ["{{ default }}"]`).
    assert_eq!(key, "Error: test ⋄ /api");
}

#[test]
fn test_empty_fingerprint_does_not_collapse_distinct_errors() {
    // Issue #290: two different exceptions with `fingerprint: []` must not
    // land in the same issue.
    let event1 = json!({
        "exception": { "values": [{ "type": "BoundaryTooLongError", "value": "a" }] },
        "transaction": "/upload",
        "fingerprint": []
    });
    let event2 = json!({
        "exception": { "values": [{ "type": "EmptyContentError", "value": "b" }] },
        "transaction": "/upload",
        "fingerprint": []
    });

    assert_ne!(
        calculate_grouping_key(&event1),
        calculate_grouping_key(&event2)
    );
}

#[test]
fn test_fingerprint_of_only_dropped_elements_falls_back_to_default() {
    // Relay turns `[null]` into an empty fingerprint, which serializes as
    // absent; Sentry then uses default grouping.
    let event = json!({
        "exception": { "values": [{ "type": "Error", "value": "test" }] },
        "transaction": "/api",
        "fingerprint": [null, [1, 2], {"k": "v"}]
    });

    assert_eq!(calculate_grouping_key(&event), "Error: test ⋄ /api");
}

// =============================================================================
// Fingerprint Coercion Tests (Relay LenientString parity)
// relay-event-schema/src/protocol/types.rs:722-747 @97f9c4b
// =============================================================================

#[test]
fn test_fingerprint_coerces_bool_true_to_capital_true() {
    let event = json!({
        "fingerprint": [true]
    });

    let key = calculate_grouping_key(&event);
    // Relay coerces Bool(true) -> "True" (capital, python legacy), not "".
    assert_eq!(key, "True");
}

#[test]
fn test_fingerprint_coerces_bool_false_to_capital_false() {
    let event = json!({ "fingerprint": [false] });
    assert_eq!(calculate_grouping_key(&event), "False");
}

#[test]
fn test_fingerprint_coerces_integer_to_string() {
    let event = json!({ "fingerprint": [42] });
    assert_eq!(calculate_grouping_key(&event), "42");
}

#[test]
fn test_fingerprint_coerces_negative_integer_to_string() {
    let event = json!({ "fingerprint": [-7] });
    assert_eq!(calculate_grouping_key(&event), "-7");
}

#[test]
fn test_fingerprint_coerces_float_by_truncating() {
    // Relay truncates toward zero: 3.99 -> "3".
    let event = json!({ "fingerprint": [3.99] });
    assert_eq!(calculate_grouping_key(&event), "3");
}

#[test]
fn test_fingerprint_skips_null_elements() {
    // null is dropped entirely, not turned into an empty component.
    let event = json!({ "fingerprint": ["a", null, "b"] });
    let key = calculate_grouping_key(&event);
    assert_eq!(key, format!("a{}b", " ⋄ "));
}

#[test]
fn test_fingerprint_skips_nested_array_and_object() {
    // Arrays and objects are dropped (Relay cannot coerce them).
    let event = json!({ "fingerprint": ["keep", [1, 2], {"k": "v"}] });
    assert_eq!(calculate_grouping_key(&event), "keep");
}

#[test]
fn test_fingerprint_numeric_values_produce_distinct_groups() {
    // The core bug: distinct numeric fingerprints used to both collapse to ""
    // and merge into one issue. They must now group separately.
    let event1 = json!({ "fingerprint": [1] });
    let event2 = json!({ "fingerprint": [2] });
    assert_ne!(
        calculate_grouping_key(&event1),
        calculate_grouping_key(&event2)
    );
}

#[test]
fn test_fingerprint_coercion_preserves_default_placeholder() {
    // Coercion must not break the "{{ default }}" substitution.
    let event = json!({
        "exception": { "values": [{ "type": "MyError", "value": "boom" }] },
        "transaction": "/x",
        "fingerprint": [7, "{{ default }}"]
    });
    let key = calculate_grouping_key(&event);
    assert!(key.starts_with("7"));
    assert!(key.contains("MyError: boom"));
}

// =============================================================================
// Synthetic Exception Tests (Relay mechanism.synthetic parity)
// relay-event-schema/src/protocol/mechanism.rs:113 @97f9c4b
// =============================================================================

#[test]
fn test_synthetic_exception_ignored_falls_back_to_log_message() {
    // A synthetic exception (e.g. signal/segfault wrapper) must not drive
    // grouping by its type/value; Relay falls through to the next component.
    let event = json!({
        "exception": {
            "values": [{
                "type": "SIGSEGV",
                "value": "Segmentation fault",
                "mechanism": { "synthetic": true }
            }]
        },
        "logentry": { "message": "real grouping message" }
    });

    let (type_, value) = get_type_and_value(&event);
    assert_eq!(type_, "Log Message");
    assert_eq!(value, "real grouping message");
}

#[test]
fn test_synthetic_exception_without_message_is_unknown() {
    let event = json!({
        "exception": {
            "values": [{
                "type": "SIGSEGV",
                "value": "Segmentation fault",
                "mechanism": { "synthetic": true }
            }]
        }
    });

    let (type_, value) = get_type_and_value(&event);
    assert_eq!(type_, "Unknown");
    assert_eq!(value, "");
}

#[test]
fn test_a_synthetic_exception_drops_out_but_its_chain_still_groups() {
    // Only the synthetic component stops contributing; the real error next to
    // it still decides the issue.
    let with_cause = |cause: &str| {
        json!({
            "exception": { "values": [
                { "type": "ValueError", "value": cause },
                { "type": "SIGSEGV", "value": "Segfault",
                  "mechanism": { "synthetic": true } }
            ]}
        })
    };

    let key = calculate_grouping_key(&with_cause("bad input"));
    assert!(key.contains("ValueError: bad input"), "got {key}");
    assert!(!key.contains("SIGSEGV"), "got {key}");
    assert_ne!(key, calculate_grouping_key(&with_cause("worse input")));
}

#[test]
fn test_non_synthetic_exception_still_groups_by_type() {
    // synthetic:false must keep the existing exception-based grouping.
    let event = json!({
        "exception": {
            "values": [{
                "type": "TypeError",
                "value": "boom",
                "mechanism": { "synthetic": false }
            }]
        }
    });

    let (type_, value) = get_type_and_value(&event);
    assert_eq!(type_, "TypeError");
    assert_eq!(value, "boom");
}

#[test]
fn test_no_exception_no_message() {
    let event = json!({
        "timestamp": 12345,
        "platform": "python"
    });

    let (type_, value) = get_type_and_value(&event);
    assert_eq!(type_, "Unknown");
    assert_eq!(value, "");
}

#[test]
fn test_empty_event() {
    let event = json!({});

    let key = calculate_grouping_key(&event);
    assert!(key.contains("Unknown"));
    assert!(key.contains("<no transaction>"));
}

#[test]
fn test_null_values_in_exception() {
    let event = json!({
        "exception": {
            "values": [{
                "type": null,
                "value": null
            }]
        }
    });

    let (type_, value) = get_type_and_value(&event);
    assert_eq!(type_, "Error"); // fallback
    assert_eq!(value, "");
}

#[test]
fn test_hash_is_deterministic() {
    let key = "Error: something ⋄ /api";
    let hash1 = hash_grouping_key(key);
    let hash2 = hash_grouping_key(key);
    assert_eq!(hash1, hash2);
}

#[test]
fn test_hash_is_hex() {
    let hash = hash_grouping_key("test input");
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn test_different_keys_different_hashes() {
    let hash1 = hash_grouping_key("key1");
    let hash2 = hash_grouping_key("key2");
    assert_ne!(hash1, hash2);
}

#[test]
fn test_title_with_value() {
    let title = get_title("TypeError", "undefined is not a function");
    assert_eq!(title, "TypeError: undefined is not a function");
}

#[test]
fn test_title_without_value() {
    let title = get_title("GenericError", "");
    assert_eq!(title, "GenericError");
}

#[test]
fn test_title_multiline_value() {
    let title = get_title("Error", "first line\nsecond line\nthird line");
    assert_eq!(title, "Error: first line");
}

#[test]
fn test_denormalized_fields_basic() {
    let event = json!({
        "exception": {
            "values": [{
                "type": "ValueError",
                "value": "invalid value",
                "stacktrace": {
                    "frames": [{
                        "filename": "app/main.py",
                        "module": "main",
                        "function": "run",
                        "in_app": true
                    }]
                }
            }]
        },
        "transaction": "/process"
    });

    let fields = get_denormalized_fields(&event);
    assert_eq!(fields.calculated_type, "ValueError");
    assert_eq!(fields.calculated_value, "invalid value");
    assert_eq!(fields.transaction, "/process");
    assert_eq!(fields.last_frame_filename, "app/main.py");
    assert_eq!(fields.last_frame_module, "main");
    assert_eq!(fields.last_frame_function, "run");
}

#[test]
fn test_denormalized_fields_prefers_in_app_frame() {
    let event = json!({
        "exception": {
            "values": [{
                "type": "Error",
                "value": "test",
                "stacktrace": {
                    "frames": [
                        { "filename": "library/code.py", "function": "lib_func", "in_app": false },
                        { "filename": "app/handler.py", "function": "handle", "in_app": true },
                        { "filename": "framework/base.py", "function": "dispatch", "in_app": false }
                    ]
                }
            }]
        }
    });

    let fields = get_denormalized_fields(&event);
    // Should pick the last in_app=true frame
    assert_eq!(fields.last_frame_filename, "app/handler.py");
    assert_eq!(fields.last_frame_function, "handle");
}

#[test]
fn test_denormalized_fields_falls_back_to_last_frame() {
    let event = json!({
        "exception": {
            "values": [{
                "type": "Error",
                "value": "test",
                "stacktrace": {
                    "frames": [
                        { "filename": "first.py", "function": "first" },
                        { "filename": "last.py", "function": "last" }
                    ]
                }
            }]
        }
    });

    let fields = get_denormalized_fields(&event);
    // No in_app frames, should use the last frame
    assert_eq!(fields.last_frame_filename, "last.py");
    assert_eq!(fields.last_frame_function, "last");
}

#[test]
fn test_denormalized_fields_no_stacktrace() {
    let event = json!({
        "exception": {
            "values": [{
                "type": "Error",
                "value": "no stack"
            }]
        }
    });

    let fields = get_denormalized_fields(&event);
    assert_eq!(fields.last_frame_filename, "");
    assert_eq!(fields.last_frame_module, "");
    assert_eq!(fields.last_frame_function, "");
}

#[test]
fn test_type_truncation() {
    let long_type = "A".repeat(200);
    let event = json!({
        "exception": {
            "values": [{
                "type": long_type,
                "value": "test"
            }]
        }
    });

    let (type_, _) = get_type_and_value(&event);
    assert_eq!(type_.len(), 128); // Truncated to 128
}

#[test]
fn test_value_truncation() {
    let long_value = "B".repeat(2000);
    let event = json!({
        "exception": {
            "values": [{
                "type": "Error",
                "value": long_value
            }]
        }
    });

    let (_, value) = get_type_and_value(&event);
    assert_eq!(value.len(), 1024); // Truncated to 1024
}

#[test]
fn test_logger_truncation_matches_relay_limit() {
    // Relay's `logger` field limit is 64 chars
    // (relay-event-schema/src/protocol/event.rs @97f9c4b), not 128.
    let long_logger = "L".repeat(200);
    let event = json!({
        "exception": { "values": [{ "type": "Error", "value": "test" }] },
        "logger": long_logger
    });

    let fields = get_denormalized_fields(&event);
    assert_eq!(fields.logger.len(), 64);
}

#[test]
fn test_release_truncation_matches_relay_limit() {
    // Relay's `release` field limit is 200 chars
    // (relay-event-schema/src/protocol/event.rs @97f9c4b), not 250.
    let long_release = "R".repeat(300);
    let event = json!({
        "exception": { "values": [{ "type": "Error", "value": "test" }] },
        "release": long_release
    });

    let fields = get_denormalized_fields(&event);
    assert_eq!(fields.release.len(), 200);
}

#[test]
fn test_transaction_truncation() {
    let long_transaction = "/".to_string() + &"x".repeat(300);
    let event = json!({
        "exception": { "values": [{ "type": "Error", "value": "test" }] },
        "transaction": long_transaction
    });

    let key = calculate_grouping_key(&event);
    // Transaction is truncated to 200 chars
    assert!(key.len() < long_transaction.len() + 200);
}

#[test]
fn test_unicode_in_error_message() {
    let event = json!({
        "exception": {
            "values": [{
                "type": "Error",
                "value": "Error en español: ¡Hola! 你好 🎉"
            }]
        }
    });

    let key = calculate_grouping_key(&event);
    assert!(key.contains("¡Hola!"));
    assert!(key.contains("你好"));
}

#[test]
fn test_diamond_separator_in_message() {
    // Edge case: what if the message contains the separator?
    let event = json!({
        "exception": {
            "values": [{
                "type": "Error",
                "value": "Contains ⋄ separator"
            }]
        },
        "transaction": "/api"
    });

    let key = calculate_grouping_key(&event);
    // Should still work, but grouping might be affected
    assert!(key.contains("Contains ⋄ separator"));
}

#[test]
fn test_newlines_and_tabs_in_value() {
    let event = json!({
        "exception": {
            "values": [{
                "type": "Error",
                "value": "Line 1\n\tTabbed line 2\r\nWindows line 3"
            }]
        }
    });

    let (_, value) = get_type_and_value(&event);
    assert!(value.contains('\n'));
    assert!(value.contains('\t'));
}

/// A cyclic `parent_id` chain is malformed input an SDK can send: Relay passes
/// `exception_id` and `parent_id` through without validating them. Sentry walks
/// the same tree unguarded but its caller catches `RecursionError` and falls
/// back to the unfiltered chain; nothing catches a blown Rust stack, so the
/// cycle has to be found rather than survived.
#[test]
fn test_cyclic_exception_group_keeps_the_chain_instead_of_recursing() {
    let event = json!({
        "exception": { "values": [
            { "type": "A", "value": "a", "mechanism": {
                "exception_id": 0, "parent_id": 1, "is_exception_group": true } },
            { "type": "B", "value": "b", "mechanism": {
                "exception_id": 1, "parent_id": 0, "is_exception_group": true } }
        ]}
    });

    let key = calculate_grouping_key(&event);

    assert!(key.contains("A: a"), "key was {key}");
    assert!(key.contains("B: b"), "key was {key}");
}

/// A cycle below a collapsed wrapper reaches the first-path walk instead of the
/// top-level one, and has to stop there too. The wrapper naming the inner error
/// as its own parent is what closes the loop: ids stay unique and neither node
/// parents itself, so the tree reconstruction accepts the chain and the walk is
/// the only thing left to catch it.
#[test]
fn test_cyclic_first_path_keeps_the_chain_instead_of_recursing() {
    let event = json!({
        "exception": { "values": [
            { "type": "Wrapper", "value": "1 sub-exception(s)", "mechanism": {
                "exception_id": 0, "parent_id": 1, "is_exception_group": true } },
            { "type": "Inner", "value": "boom", "mechanism": {
                "exception_id": 1, "parent_id": 0 } }
        ]}
    });

    let key = calculate_grouping_key(&event);

    // Falling back keeps both; collapsing the wrapper would leave only "Inner".
    assert!(key.contains("Wrapper: "), "key was {key}");
    assert!(key.contains("Inner: boom"), "key was {key}");
}

/// `type_and_value` truncates to the limits Sentry applies to an issue's title
/// (`eventtypes/error.py`), which the grouping path must not inherit before
/// normalizing: a value cut mid-pattern no longer matches it, so two events
/// that differ only in that value open two issues instead of one.
#[test]
fn test_long_message_normalizes_before_the_length_limit_applies() {
    let padded = |id: &str| json!({ "message": format!("{} id={id}", "x".repeat(1010)) });

    let first = padded("550e8400-e29b-41d4-a716-446655440000");
    let second = padded("6ba7b810-9dad-11d1-80b4-00c04fd430c8");

    assert_eq!(
        calculate_grouping_key(&first),
        calculate_grouping_key(&second)
    );
}

/// A chain of exception groups nested deeper than the walk can follow is the
/// acyclic twin of the cycle above: distinct ids, so nothing repeats, and
/// ~75 bytes per entry, so tens of thousands fit under the 4 MB an event item
/// is allowed. Sentry's own walk is just as unguarded, but Python raises
/// `RecursionError` at its recursion limit and the caller's blanket `except`
/// degrades to the unfiltered chain; a blown Rust stack aborts the process,
/// so the depth has to be bounded here.
#[test]
fn test_deep_exception_group_chain_keeps_the_chain_instead_of_recursing() {
    let depth = 15_000u64;
    let mut values = vec![json!({
        "type": "E0", "value": "boom",
        "mechanism": { "exception_id": 0, "is_exception_group": true }
    })];
    for id in 1..depth {
        values.push(json!({
            "type": format!("E{id}"), "value": "boom",
            "mechanism": {
                "exception_id": id, "parent_id": id - 1, "is_exception_group": true }
        }));
    }
    values.push(json!({
        "type": "Inner", "value": "boom",
        "mechanism": { "exception_id": depth, "parent_id": depth - 1 }
    }));
    let event = json!({ "exception": { "values": values } });

    let key = grouping_key_on_a_bounded_stack(event);

    assert!(
        key.contains("E0: boom"),
        "the wrapper chain was collapsed away"
    );
    assert!(
        key.contains("Inner: boom"),
        "key was truncated at the wrapper"
    );
}

/// A deep chain below a collapsed wrapper reaches the first-path walk instead
/// of the top-level one, and has to stop there too.
#[test]
fn test_deep_first_path_keeps_the_chain_instead_of_recursing() {
    let depth = 15_000u64;
    let mut values = vec![json!({
        "type": "Wrapper", "value": "1 sub-exception(s)",
        "mechanism": { "exception_id": 0, "is_exception_group": true }
    })];
    for id in 1..=depth {
        values.push(json!({
            "type": format!("Cause{id}"), "value": "boom",
            "mechanism": { "exception_id": id, "parent_id": id - 1 }
        }));
    }
    let event = json!({ "exception": { "values": values } });

    let key = grouping_key_on_a_bounded_stack(event);

    assert!(key.contains("Wrapper: "), "the wrapper was collapsed away");
    assert!(
        key.contains("Cause15000: boom"),
        "key was truncated at the wrapper"
    );
}

/// Runs the grouping on a thread with the stack a Tokio worker gets, so the
/// budget under test is the server's rather than whatever the harness hands
/// this test.
fn grouping_key_on_a_bounded_stack(event: serde_json::Value) -> String {
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || calculate_grouping_key(&event))
        .expect("spawn")
        .join()
        .expect("the grouping walk aborted the process")
}
