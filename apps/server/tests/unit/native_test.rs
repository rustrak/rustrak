//! Native (Apple) symbolication against a real dSYM.
//!
//! `fixtures/crash_arm64.dwarf` is the DWARF file of `crash.dSYM`, built from
//! `fixtures/crash.c` with `clang -g -O1 -arch arm64` and `dsymutil`;
//! `crash_arm64.src.zip` is what `sentry-cli debug-files bundle-sources`
//! made of it. Expected frames are what Apple's `atos -i` answers for the same
//! addresses, with the binary loaded at 0x104000000:
//!
//! - 0x10400041c (`sdiv`): divide (crash.c:5), inlined in compute (crash.c:10)
//! - 0x10400042c (`bl abort`): divide (crash.c:4), inlined in compute (crash.c:10)
//! - 0x104000430, the return address of that call: main (crash.c:14)
//!
//! Event handling follows Sentry's `sentry/lang/native/processing.py`
//! (2458718b2c) and Symbolicator's `symbolicator-native` (799f5d9696).

use rustrak::services::native::{
    build_symcaches, is_native_event, native_image_ids, symbolicate_event, NativeFiles,
};
use serde_json::{json, Value};

const DWARF: &[u8] = include_bytes!("../fixtures/crash_arm64.dwarf");
const SOURCE_BUNDLE: &[u8] = include_bytes!("../fixtures/crash_arm64.src.zip");
const DEBUG_ID: &str = "821b5334-aca1-3d75-bea8-579e8f53103a";
const APP: &str = "/private/var/containers/Bundle/Application/7A8E/Crash.app/Crash";

fn symcache() -> Vec<u8> {
    let caches = build_symcaches(DWARF).expect("valid dSYM");
    assert_eq!(caches.len(), 1);
    assert_eq!(caches[0].0.to_string(), DEBUG_ID);
    caches.into_iter().next().unwrap().2
}

/// A sentry-cocoa event: frames caller first, the crashing frame last.
fn event(frames: Value) -> Value {
    json!({
        "platform": "cocoa",
        "contexts": {"os": {"type": "os", "name": "iOS"}},
        "exception": {"values": [{
            "type": "SIGABRT",
            "mechanism": {"type": "mach", "meta": {"signal": {"number": 6}}},
            "stacktrace": {"frames": frames}
        }]},
        "debug_meta": {"images": [
            {"type": "macho", "debug_id": DEBUG_ID, "code_file": APP,
             "image_addr": "0x104000000", "image_size": 16384, "arch": "arm64"},
            {"type": "macho", "debug_id": "11111111-2222-3333-4444-555555555555",
             "code_file": "/usr/lib/system/libsystem_kernel.dylib",
             "image_addr": "0x180000000", "image_size": 65536, "arch": "arm64"}
        ]}
    })
}

fn frame(addr: &str) -> Value {
    json!({"instruction_addr": addr, "in_app": true})
}

fn files<'a>(cache: Option<&'a [u8]>, bundle: Option<&'a [u8]>) -> NativeFiles<'a> {
    NativeFiles {
        symcaches: vec![(DEBUG_ID.to_string(), cache)],
        source_bundles: bundle
            .map(|b| (DEBUG_ID.to_string(), b))
            .into_iter()
            .collect(),
    }
}

fn frames(event: &Value) -> &Vec<Value> {
    event["exception"]["values"][0]["stacktrace"]["frames"]
        .as_array()
        .unwrap()
}

fn summary(frames: &[Value]) -> Vec<String> {
    frames
        .iter()
        .map(|f| format!("{} {}:{}", f["function"], f["filename"], f["lineno"]))
        .collect()
}

#[test]
fn crashing_frame_expands_inlined_functions() {
    let cache = symcache();
    let mut event = event(json!([frame("0x10400041c")]));

    symbolicate_event(&mut event, &files(Some(&cache), None));

    let out = frames(&event);
    // Caller first, like the event: compute called the inlined divide.
    assert_eq!(
        summary(out),
        ["\"compute\" \"crash.c\":10", "\"divide\" \"crash.c\":5"]
    );
    for f in out {
        assert_eq!(f["package"], APP);
        assert_eq!(f["abs_path"], "/tmp/rustrak-fixture/crash.c");
        assert_eq!(f["instruction_addr"], "0x10400041c");
        assert_eq!(f["data"]["symbolicator_status"], "symbolicated");
        assert_eq!(f["in_app"], true);
    }
    assert_eq!(
        event["exception"]["values"][0]["raw_stacktrace"]["frames"],
        json!([frame("0x10400041c")])
    );
}

/// `rel:<index>` and `rel:<debug id>` name the image by its position in
/// `debug_meta.images`, which lookup sorts by address: listed after the
/// system image here, the app keeps index 1.
#[test]
fn relative_addresses_find_the_image_by_index_or_debug_id() {
    let cache = symcache();
    for mode in ["rel:1", &format!("rel:{DEBUG_ID}")] {
        let mut event = event(json!([
            {"instruction_addr": "0x41c", "addr_mode": mode, "in_app": true}
        ]));
        event["debug_meta"]["images"]
            .as_array_mut()
            .unwrap()
            .reverse();

        symbolicate_event(&mut event, &files(Some(&cache), None));

        assert_eq!(
            summary(frames(&event)),
            ["\"compute\" \"crash.c\":10", "\"divide\" \"crash.c\":5"],
            "{mode}"
        );
    }
}

/// An image without `image_size` spans up to the next image.
#[test]
fn an_image_without_a_size_spans_to_the_next_one() {
    let cache = symcache();
    let mut event = event(json!([frame("0x10400041c")]));
    event["debug_meta"]["images"][0]
        .as_object_mut()
        .unwrap()
        .remove("image_size");

    symbolicate_event(&mut event, &files(Some(&cache), None));

    assert_eq!(
        summary(frames(&event)),
        ["\"compute\" \"crash.c\":10", "\"divide\" \"crash.c\":5"]
    );
}

#[test]
fn caller_frames_resolve_the_call_not_the_return_address() {
    // The return address 0x430 is main's first instruction; the call that
    // left it is at 0x42c, in compute.
    let cache = symcache();
    let mut event = event(json!([frame("0x104000430"), frame("0x10400041c")]));

    symbolicate_event(&mut event, &files(Some(&cache), None));

    let out = frames(&event);
    assert_eq!(
        summary(out),
        [
            "\"compute\" \"crash.c\":10",
            "\"divide\" \"crash.c\":4",
            "\"compute\" \"crash.c\":10",
            "\"divide\" \"crash.c\":5",
        ]
    );
    assert_eq!(out[0]["instruction_addr"], "0x10400042c");
}

#[test]
fn adjustment_none_keeps_every_address() {
    let cache = symcache();
    let mut event = event(json!([frame("0x104000430"), frame("0x10400041c")]));
    event["exception"]["values"][0]["stacktrace"]["instruction_addr_adjustment"] = json!("none");

    symbolicate_event(&mut event, &files(Some(&cache), None));

    assert_eq!(frames(&event)[0]["function"], "main");
    assert_eq!(frames(&event)[0]["lineno"], 14);
}

#[test]
fn applies_source_context_from_the_source_bundle() {
    let cache = symcache();
    let mut event = event(json!([frame("0x10400041c")]));

    symbolicate_event(&mut event, &files(Some(&cache), Some(SOURCE_BUNDLE)));

    let divide = &frames(&event)[1];
    assert_eq!(divide["context_line"], "    return a / b;");
    assert_eq!(
        divide["pre_context"],
        json!([
            "#include <stdlib.h>",
            "",
            "static inline __attribute__((always_inline)) int divide(int a, int b) {",
            "    if (b == 0) abort();"
        ])
    );
    assert_eq!(divide["post_context"][0], "}");
}

#[test]
fn a_missing_dsym_for_the_app_is_an_error_but_not_for_the_system() {
    let mut event = event(json!([frame("0x180000100"), frame("0x10400041c")]));
    let files = NativeFiles {
        symcaches: vec![
            (DEBUG_ID.to_string(), None),
            ("11111111-2222-3333-4444-555555555555".to_string(), None),
        ],
        source_bundles: vec![],
    };

    symbolicate_event(&mut event, &files);

    assert_eq!(
        event["errors"],
        json!([{
            "type": "native_missing_dsym",
            "message": null,
            "image_path": APP,
            "image_uuid": DEBUG_ID,
            "image_arch": "arm64"
        }])
    );
    let out = frames(&event);
    assert_eq!(out[1]["data"]["symbolicator_status"], "missing");
    assert_eq!(out[1]["package"], APP);
    assert_eq!(out[1].get("function"), None);
    let images = &event["debug_meta"]["images"];
    assert_eq!(images[0]["debug_status"], "missing");
    assert_eq!(images[1]["debug_status"], "missing");
}

#[test]
fn image_statuses_say_what_was_used() {
    let cache = symcache();
    let mut event = event(json!([frame("0x10400041c")]));

    symbolicate_event(&mut event, &files(Some(&cache), None));

    let images = &event["debug_meta"]["images"];
    assert_eq!(images[0]["debug_status"], "found");
    assert_eq!(images[1]["debug_status"], "unused");
    assert_eq!(event.get("errors"), None);
}

#[test]
fn frames_outside_every_image_stay_and_say_so() {
    let cache = symcache();
    let mut event = event(json!([frame("0x200000000"), frame("0x10400041c")]));

    symbolicate_event(&mut event, &files(Some(&cache), None));

    let out = frames(&event);
    assert_eq!(out[0]["instruction_addr"], "0x200000000");
    assert_eq!(out[0]["data"]["symbolicator_status"], "unknown_image");
}

#[test]
fn a_trailing_null_frame_is_dropped() {
    // Symbolicator drops the outermost frame when it is null-ish and unmapped.
    let cache = symcache();
    let mut event = event(json!([frame("0x0"), frame("0x10400041c")]));

    symbolicate_event(&mut event, &files(Some(&cache), None));

    assert_eq!(frames(&event).len(), 2);
    assert_eq!(frames(&event)[0]["function"], "compute");
}

#[test]
fn leaves_other_frames_alone() {
    let cache = symcache();
    let js = json!({"function": "x", "platform": "javascript", "instruction_addr": "0x10400041c"});
    let done =
        json!({"instruction_addr": "0x10400041c", "data": {"symbolicator_status": "symbolicated"}});
    let mut event = event(json!([js.clone(), done.clone()]));

    symbolicate_event(&mut event, &files(Some(&cache), None));

    assert_eq!(*frames(&event), vec![js, done]);
}

#[test]
fn detects_native_events_and_their_images() {
    assert!(is_native_event(&json!({"platform": "cocoa"})));
    assert!(is_native_event(&json!({
        "platform": "javascript",
        "exception": {"values": [{"stacktrace": {"frames": [{"platform": "native"}]}}]}
    })));
    assert!(!is_native_event(&json!({"platform": "java"})));
    assert_eq!(
        native_image_ids(&event(json!([]))),
        [DEBUG_ID, "11111111-2222-3333-4444-555555555555"]
    );
}

#[test]
fn rejects_what_is_not_a_debug_file() {
    assert!(build_symcaches(b"\xcf\xfa\xed\xfe not really mach-o").is_err());
}
