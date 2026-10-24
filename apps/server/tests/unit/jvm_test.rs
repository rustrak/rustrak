//! JVM deobfuscation: ProGuard remapping and source bundle context.
//!
//! Expectations are ported from Symbolicator's `symbolicator-proguard`
//! (`symbolication.rs` tests, 799f5d9696) and Sentry's
//! `sentry/lang/java/processing.py` (2458718b2c), which together decide what
//! a deobfuscated JVM event looks like.

use rustrak::services::jvm::{build_proguard_cache, is_jvm_event, symbolicate_event, JvmFiles};
use serde_json::{json, Value};

const MAPPING_UUID: &str = "11111111-2222-3333-4444-555555555555";
const BUNDLE_ID: &str = "66666666-7777-8888-9999-000000000000";

/// Symbolicator's `remap_filename` mapping: R8 full mode renaming sources,
/// with synthesized frames.
const R8_MAPPING: &[u8] = br#"# compiler: R8
# compiler_version: 8.3.36
# min_api: 24
# common_typos_disable
# {"id":"com.android.tools.r8.mapping","version":"2.2"}
# pg_map_id: 48ffd94
io.wzieba.r8fullmoderenamessources.Foobar -> a.a:
# {"id":"sourceFile","fileName":"Foobar.kt"}
    1:3:void <init>():3:3 -> <init>
    4:11:void <init>():5:5 -> <init>
    1:7:void foo():9:9 -> a
    8:15:void foo():10:10 -> a
io.wzieba.r8fullmoderenamessources.FoobarKt -> a.b:
# {"id":"sourceFile","fileName":"Foobar.kt"}
    1:5:void main():15:15 -> a
    6:9:void main():16:16 -> a
    1:4:void main(java.lang.String[]):0:0 -> b
io.wzieba.r8fullmoderenamessources.MainActivity -> io.wzieba.r8fullmoderenamessources.MainActivity:
# {"id":"sourceFile","fileName":"MainActivity.kt"}
    1:4:void <init>():7:7 -> <init>
    1:1:void $r8$lambda$pOQDVg57r6gG0-DzwbGf17BfNbs(android.view.View):0:0 -> a
      # {"id":"com.android.tools.r8.synthesized"}
    1:9:void onCreate$lambda$1$lambda$0(android.view.View):14:14 -> b
    1:3:void onCreate(android.os.Bundle):10:10 -> onCreate
    4:8:void onCreate(android.os.Bundle):12:12 -> onCreate
    9:16:void onCreate(android.os.Bundle):13:13 -> onCreate
    17:20:void onCreate(android.os.Bundle):12:12 -> onCreate
io.wzieba.r8fullmoderenamessources.MainActivity$$ExternalSyntheticLambda0 -> a.c:
# {"id":"sourceFile","fileName":"R8$$SyntheticClass"}
# {"id":"com.android.tools.r8.synthesized"}
    1:4:void onClick(android.view.View):0:0 -> onClick
      # {"id":"com.android.tools.r8.synthesized"}
io.wzieba.r8fullmoderenamessources.R -> a.d:
    void <init>() -> <init>
      # {"id":"com.android.tools.r8.synthesized"}"#;

/// Symbolicator's `remap_exception_simple` / `sets_in_app_after_resolving` mapping.
const SLF4J_MAPPING: &[u8] = b"org.slf4j.helpers.Util$ClassContextSecurityManager -> org.a.b.g$a:
    65:65:void <init>() -> <init>
    67:67:java.lang.Class[] getClassContext() -> a
    69:69:java.lang.Class[] getExtraClassContext() -> a
    68:68:java.lang.Class[] getContext() -> a
    65:65:void <init>(org.slf4j.helpers.Util$1) -> <init>
org.slf4j.helpers.Util$ClassContext -> org.a.b.g$b:
    65:65:void <init>() -> <init>
";

fn cache(mapping: &[u8]) -> Vec<u8> {
    build_proguard_cache(mapping).expect("valid mapping")
}

/// What `sentry-cli debug-files bundle-jvm` (2.58.6) wrote for a `Greeter.kt`
/// whose line 6 throws: symbolic's `SYSB` header and the zip in one stream, so the zip's
/// offsets count the header.
const SOURCE_BUNDLE: &[u8] = include_bytes!("../fixtures/jvm_source_bundle.zip");

fn files<'a>(mapping: Option<&'a [u8]>, bundle: Option<&'a [u8]>) -> JvmFiles<'a> {
    JvmFiles {
        proguard: vec![(MAPPING_UUID.to_string(), mapping)],
        source_bundles: bundle.into_iter().collect(),
    }
}

fn jvm_event(frames: Value) -> Value {
    json!({
        "platform": "java",
        "exception": {"values": [{
            "type": "g$a",
            "module": "org.a.b",
            "stacktrace": {"frames": frames}
        }]},
        "debug_meta": {"images": [{"type": "proguard", "uuid": MAPPING_UUID}]}
    })
}

fn frames(event: &Value) -> &Vec<Value> {
    event["exception"]["values"][0]["stacktrace"]["frames"]
        .as_array()
        .unwrap()
}

fn field<'a>(frames: &'a [Value], key: &str) -> Vec<&'a Value> {
    frames.iter().map(|f| &f[key]).collect()
}

#[test]
fn remaps_frames_and_drops_synthesized_ones() {
    // Symbolicator `remap_filename`, in event order (caller first).
    let raw = json!([
        {"function": "performClickInternal", "abs_path": "Unknown Source", "module": "android.view.View.-$$Nest$m", "filename": "Unknown Source", "lineno": 0},
        {"function": "performClickInternal", "abs_path": "View.java", "module": "android.view.View", "filename": "View.java", "lineno": 7636},
        {"function": "performClick", "abs_path": "View.java", "module": "android.view.View", "filename": "View.java", "lineno": 7659},
        {"function": "onClick", "abs_path": "SourceFile", "module": "a.c", "filename": "SourceFile", "lineno": 1},
        {"function": "a", "abs_path": "SourceFile", "module": "io.wzieba.r8fullmoderenamessources.MainActivity", "filename": "SourceFile", "lineno": 1},
        {"function": "b", "abs_path": "SourceFile", "module": "io.wzieba.r8fullmoderenamessources.MainActivity", "filename": "SourceFile", "lineno": 6},
        {"function": "a", "abs_path": "SourceFile", "module": "a.a", "filename": "SourceFile", "lineno": 12}
    ]);
    let mut event = jvm_event(raw.clone());
    let cache = cache(R8_MAPPING);

    symbolicate_event(&mut event, &files(Some(&cache), None), None);

    let out = frames(&event);
    assert_eq!(
        field(out, "function"),
        [
            "performClickInternal",
            "performClickInternal",
            "performClick",
            "onCreate$lambda$1$lambda$0",
            "foo"
        ]
    );
    assert_eq!(
        out[3]["module"],
        "io.wzieba.r8fullmoderenamessources.MainActivity"
    );
    assert_eq!(out[3]["filename"], "MainActivity.kt");
    assert_eq!(out[3]["abs_path"], "MainActivity.kt");
    assert_eq!(out[3]["lineno"], 14);
    assert_eq!(
        out[4]["module"],
        "io.wzieba.r8fullmoderenamessources.Foobar"
    );
    assert_eq!(out[4]["lineno"], 10);
    // Sentry keeps what came in, frame for frame.
    assert_eq!(
        event["exception"]["values"][0]["raw_stacktrace"]["frames"],
        raw
    );
}

#[test]
fn remaps_exception_type_and_keeps_the_raw_one() {
    let mut event = jvm_event(json!([]));
    let cache = cache(SLF4J_MAPPING);

    symbolicate_event(&mut event, &files(Some(&cache), None), None);

    let exc = &event["exception"]["values"][0];
    assert_eq!(exc["type"], "Util$ClassContextSecurityManager");
    assert_eq!(exc["module"], "org.slf4j.helpers");
    assert_eq!(exc["raw_type"], "g$a");
    assert_eq!(exc["raw_module"], "org.a.b");
}

#[test]
fn remaps_root_package_exception() {
    // Symbolicator `remap_exception_root_package`: R8 aggressive mode, no module.
    let mapping = b"androidx.compose.runtime.tooling.DiagnosticComposeException -> kj:
    1:3:void <init>(androidx.compose.runtime.tooling.ComposeStackTrace):18:18 -> <init>
";
    let mut event = json!({
        "platform": "java",
        "exception": {"values": [{"type": "kj"}]}
    });
    let cache = cache(mapping);

    symbolicate_event(&mut event, &files(Some(&cache), None), None);

    let exc = &event["exception"]["values"][0];
    assert_eq!(exc["type"], "DiagnosticComposeException");
    assert_eq!(exc["module"], "androidx.compose.runtime.tooling");
}

#[test]
fn sets_in_app_from_the_release_package_only_when_remapped() {
    // Symbolicator `sets_in_app_after_resolving` + `doesnt_set_in_app_if_not_resolved`.
    let raw = json!([
        {"function": "a", "module": "org.a.b.g$b", "lineno": 70},
        {"function": "init", "module": "com.android.Zygote", "lineno": 62},
        {"function": "a", "module": "org.a.b.g$a", "lineno": 68, "in_app": true},
        {"function": "a", "module": "org.a.b.g$a", "lineno": 69, "in_app": false},
        {"function": "a", "module": "org.a.b.g$a", "lineno": 67}
    ]);
    let mut event = jvm_event(raw);
    let cache = cache(SLF4J_MAPPING);

    symbolicate_event(&mut event, &files(Some(&cache), None), Some("org.slf4j"));

    let out = frames(&event);
    assert_eq!(out[4]["in_app"], true);
    assert_eq!(out[3]["in_app"], false);
    assert_eq!(out[2]["in_app"], true);
    assert_eq!(
        out[1].get("in_app"),
        None,
        "unresolved frames keep no in_app"
    );
    assert_eq!(out[0]["in_app"], true);
}

#[test]
fn applies_source_context_from_a_source_bundle() {
    let mapping = b"com.example.Greeter -> a.a:
# {\"id\":\"sourceFile\",\"fileName\":\"Greeter.kt\"}
    1:1:java.lang.String greet(java.lang.String):6:6 -> a
";
    let cache = cache(mapping);
    let mut event = jvm_event(json!([
        {"function": "a", "module": "a.a", "filename": "SourceFile", "lineno": 1}
    ]));

    symbolicate_event(&mut event, &files(Some(&cache), Some(SOURCE_BUNDLE)), None);

    let frame = &frames(&event)[0];
    assert_eq!(frame["module"], "com.example.Greeter");
    assert_eq!(frame["function"], "greet");
    assert_eq!(frame["lineno"], 6);
    assert_eq!(
        frame["context_line"],
        "        if (n == 0) throw IllegalStateException(\"empty name\")"
    );
    assert_eq!(frame["pre_context"].as_array().unwrap().len(), 5);
    assert_eq!(frame["pre_context"][0], "package com.example");
    assert_eq!(
        frame["post_context"],
        json!(["        return \"Hello $name\"", "    }", "}"])
    );
}

#[test]
fn source_context_works_without_a_mapping() {
    // Unminified builds upload only a source bundle (a `jvm` image).
    let mut event = json!({
        "platform": "java",
        "exception": {"values": [{"type": "IllegalStateException", "module": "java.lang", "stacktrace": {"frames": [
            {"function": "greet", "module": "com.example.Greeter", "filename": "Greeter.kt", "abs_path": "Greeter.kt", "lineno": 5}
        ]}}]},
        "debug_meta": {"images": [{"type": "jvm", "debug_id": BUNDLE_ID}]}
    });
    let files = JvmFiles {
        proguard: vec![],
        source_bundles: vec![SOURCE_BUNDLE],
    };

    symbolicate_event(&mut event, &files, None);

    assert_eq!(
        frames(&event)[0]["context_line"],
        "        val n = name.length"
    );
}

#[test]
fn reports_a_missing_mapping_as_an_event_error() {
    let raw = json!([{"function": "a", "module": "a.a", "lineno": 1}]);
    let mut event = jvm_event(raw.clone());

    symbolicate_event(&mut event, &files(None, None), None);

    assert_eq!(
        event["errors"],
        json!([{
            "type": "proguard_missing_mapping",
            "symbolicator_type": "missing",
            "mapping_uuid": MAPPING_UUID
        }])
    );
    assert_eq!(*frames(&event), *raw.as_array().unwrap());
}

#[test]
fn reports_an_unreadable_mapping_as_missing_line_info() {
    let mut event = jvm_event(json!([{"function": "a", "module": "a.a", "lineno": 1}]));

    symbolicate_event(&mut event, &files(Some(b"not a cache"), None), None);

    assert_eq!(event["errors"][0]["type"], "proguard_missing_lineno");
    assert_eq!(event["errors"][0]["symbolicator_type"], "invalid");
}

#[test]
fn leaves_non_java_frames_alone() {
    let native = json!({"function": "a", "module": "a.a", "lineno": 1, "platform": "native"});
    let mut event = jvm_event(json!([native.clone()]));
    let cache = cache(R8_MAPPING);

    symbolicate_event(&mut event, &files(Some(&cache), None), None);

    assert_eq!(frames(&event)[0], native);
}

#[test]
fn remaps_thread_stacktraces_too() {
    let mut event = json!({
        "platform": "java",
        "threads": {"values": [{"stacktrace": {"frames": [
            {"function": "a", "module": "a.a", "lineno": 12}
        ]}}]}
    });
    let cache = cache(R8_MAPPING);

    symbolicate_event(&mut event, &files(Some(&cache), None), None);

    let thread = &event["threads"]["values"][0];
    assert_eq!(thread["stacktrace"]["frames"][0]["function"], "foo");
    assert_eq!(thread["raw_stacktrace"]["frames"][0]["function"], "a");
}

#[test]
fn deobfuscates_class_names_in_the_exception_value() {
    // Sentry `Exceptions.deobfuscate_and_save`, including the cast message form.
    let mut event = json!({
        "platform": "java",
        "exception": {"values": [
            {"type": "ClassCastException", "module": "java.lang", "value": "org.a.b.g$a cannot be cast to org.a.b.g$b"},
            {"type": "IllegalStateException", "module": "java.lang", "value": "bad state in org.a.b.g$a"}
        ]}
    });
    let cache = cache(SLF4J_MAPPING);

    symbolicate_event(&mut event, &files(Some(&cache), None), None);

    let values = &event["exception"]["values"];
    assert_eq!(
        values[0]["value"],
        "org.slf4j.helpers.Util$ClassContextSecurityManager cannot be cast to org.slf4j.helpers.Util$ClassContext"
    );
    assert_eq!(
        values[0]["raw_value"],
        "org.a.b.g$a cannot be cast to org.a.b.g$b"
    );
    assert_eq!(
        values[1]["value"],
        "bad state in org.slf4j.helpers.Util$ClassContextSecurityManager"
    );
}

#[test]
fn detects_jvm_events_like_sentry() {
    assert!(is_jvm_event(&json!({"platform": "java"})));
    assert!(is_jvm_event(&json!({
        "platform": "other",
        "exception": {"values": [{"stacktrace": {"frames": [{"platform": "java"}]}}]}
    })));
    assert!(is_jvm_event(&json!({
        "debug_meta": {"images": [{"type": "proguard", "uuid": MAPPING_UUID}]}
    })));
    assert!(!is_jvm_event(&json!({"platform": "javascript"})));
    assert!(!is_jvm_event(&json!({
        "debug_meta": {"images": [{"type": "proguard"}]}
    })));
}

#[test]
fn rejects_an_invalid_mapping() {
    assert!(build_proguard_cache(b"this is not a proguard mapping").is_err());
}
