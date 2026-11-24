//! Targeted trimming for oversized events, run only when an event's raw
//! payload exceeds [`crate::ingest::parser::TARGET_EVENT_SIZE`] (see
//! `digest/processors/event.rs`).
//!
//! Applies the same per-field byte/depth budgets Relay's schema actually
//! declares (`#[metastructure(max_bytes = N, max_depth = D)]` in
//! relay-event-schema), via [`generic_trim::trim_databag`] — a JSON-native
//! reimplementation of Relay's `TrimmingProcessor` behavior. Fields Relay's
//! schema doesn't annotate with a budget (`pre_context`/`post_context`, the
//! breadcrumbs array length) are deliberately left untouched here too — see
//! the corresponding tests below for the source citations.

use super::generic_trim::trim_databag;

/// `Frame.vars` — relay-event-schema/src/protocol/stacktrace.rs:129.
const VARS_MAX_BYTES: usize = 2048;
const VARS_MAX_DEPTH: usize = 5;

/// `Breadcrumb.data` — relay-event-schema/src/protocol/breadcrumb.rs:108.
const BREADCRUMB_DATA_MAX_BYTES: usize = 2048;
const BREADCRUMB_DATA_MAX_DEPTH: usize = 5;

/// `Event.extra` — relay-event-schema/src/protocol/event.rs:402.
const EXTRA_MAX_BYTES: usize = 262_144;
const EXTRA_MAX_DEPTH: usize = 7;

/// `Event.modules` — relay-event-schema/src/protocol/event.rs:242.
const MODULES_MAX_BYTES: usize = 8192;
const MODULES_MAX_DEPTH: usize = 7;

/// `Request.data` — relay-event-schema/src/protocol/request.rs:437.
const REQUEST_DATA_MAX_BYTES: usize = 8192;
const REQUEST_DATA_MAX_DEPTH: usize = 7;

/// `RawStacktrace::MAX_OLDEST_FRAMES` / `MAX_NEWEST_FRAMES` —
/// relay-event-schema/src/protocol/stacktrace.rs:386-390.
const MAX_OLDEST_FRAMES: usize = 50;
const MAX_NEWEST_FRAMES: usize = 200;

/// Relay's `enforce_frame_hard_limit` (relay-event-normalization/src/trimming.rs:377),
/// which it applies to every stack trace of every event, whatever its size:
/// keeps the 50 oldest and 200 newest frames. A frame that is not an object
/// fails Relay's schema and is dropped here too. Runs before symbolication,
/// so no stack trace costs more than 250 frames of work.
pub fn cap_stack_frames(event_data: &mut serde_json::Value) {
    cap_container(event_data);
    for key in ["exception", "threads"] {
        let Some(root) = event_data.get_mut(key) else {
            continue;
        };
        // Relay reads a bare array the same as `{"values": [...]}`.
        let values = if root.is_array() {
            root.as_array_mut()
        } else {
            root.get_mut("values")
                .and_then(serde_json::Value::as_array_mut)
        };
        if let Some(values) = values {
            values.iter_mut().for_each(cap_container);
        }
    }
}

fn cap_container(container: &mut serde_json::Value) {
    for key in ["stacktrace", "raw_stacktrace"] {
        if let Some(frames) = container
            .get_mut(key)
            .and_then(|s| s.get_mut("frames"))
            .and_then(serde_json::Value::as_array_mut)
        {
            frames.retain(serde_json::Value::is_object);
            if frames.len() > MAX_OLDEST_FRAMES + MAX_NEWEST_FRAMES {
                frames.drain(MAX_OLDEST_FRAMES..frames.len() - MAX_NEWEST_FRAMES);
            }
        }
    }
}

pub fn trim_oversized_event(event_data: &mut serde_json::Value) {
    trim_frames_under(event_data, "exception");
    trim_frames_under(event_data, "threads");
    trim_breadcrumb_data(event_data);

    if event_data.get("extra").is_some() {
        trim_databag(&mut event_data["extra"], EXTRA_MAX_BYTES, EXTRA_MAX_DEPTH);
    }
    if event_data.get("modules").is_some() {
        trim_databag(
            &mut event_data["modules"],
            MODULES_MAX_BYTES,
            MODULES_MAX_DEPTH,
        );
    }
    if event_data["request"].get("data").is_some() {
        trim_databag(
            &mut event_data["request"]["data"],
            REQUEST_DATA_MAX_BYTES,
            REQUEST_DATA_MAX_DEPTH,
        );
    }
}

fn trim_breadcrumb_data(event_data: &mut serde_json::Value) {
    let count = event_data["breadcrumbs"]["values"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);

    for i in 0..count {
        let crumb = &mut event_data["breadcrumbs"]["values"][i];
        if crumb.get("data").is_some() {
            trim_databag(
                &mut crumb["data"],
                BREADCRUMB_DATA_MAX_BYTES,
                BREADCRUMB_DATA_MAX_DEPTH,
            );
        }
    }
}

fn trim_frames_under(event_data: &mut serde_json::Value, root_key: &str) {
    let value_count = event_data[root_key]["values"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);

    for value_idx in 0..value_count {
        let frame_count = event_data[root_key]["values"][value_idx]["stacktrace"]["frames"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0);

        for frame_idx in 0..frame_count {
            let frame =
                &mut event_data[root_key]["values"][value_idx]["stacktrace"]["frames"][frame_idx];
            if frame.get("vars").is_some() {
                trim_databag(&mut frame["vars"], VARS_MAX_BYTES, VARS_MAX_DEPTH);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Relay reads `exception` and `threads` given as a bare array the same
    /// as `{"values": [...]}`, so the frame cap applies to both forms.
    #[test]
    fn the_frame_cap_applies_to_the_bare_array_form() {
        let frames: Vec<serde_json::Value> = (0..300).map(|i| json!({ "lineno": i })).collect();
        let mut event = json!({
            "exception": [{ "type": "E", "stacktrace": { "frames": frames.clone() } }],
            "threads": [{ "id": 1, "stacktrace": { "frames": frames } }],
        });

        cap_stack_frames(&mut event);

        for key in ["exception", "threads"] {
            let kept = event[key][0]["stacktrace"]["frames"].as_array().unwrap();
            assert_eq!(kept.len(), 250, "{key}");
            assert_eq!(
                kept[50]["lineno"], 100,
                "{key}: the 200 newest follow the 50 oldest"
            );
        }
    }

    #[test]
    fn trim_frame_vars_uses_the_real_relay_budget_of_2048_bytes() {
        let mut vars = serde_json::Map::new();
        for i in 0..50 {
            vars.insert(format!("var{i:03}"), json!("x".repeat(100)));
        }
        let mut event = json!({
            "exception": { "values": [{ "type": "Error", "value": "boom", "stacktrace": { "frames": [{
                "filename": "app.py",
                "lineno": 10,
                "vars": vars,
            }]}}]}
        });

        trim_oversized_event(&mut event);

        let frame = &event["exception"]["values"][0]["stacktrace"]["frames"][0];
        let trimmed = serde_json::to_string(&frame["vars"]).unwrap();
        assert!(
            trimmed.len() <= 2048 + 64, // small slack for the last item's own truncation
            "vars should be trimmed to Relay's real 2048-byte budget (stacktrace.rs:129), got {} bytes",
            trimmed.len()
        );
        assert!(
            frame["vars"]["var000"].is_string(),
            "leading vars within budget should survive"
        );
    }

    #[test]
    fn trim_applies_to_thread_frame_vars_too() {
        let mut vars = serde_json::Map::new();
        for i in 0..50 {
            vars.insert(format!("var{i:03}"), json!("x".repeat(100)));
        }
        let mut event = json!({
            "threads": { "values": [{
                "id": "0",
                "crashed": true,
                "stacktrace": { "frames": [{
                    "filename": "worker.js",
                    "lineno": 10,
                    "vars": vars,
                }]}
            }]}
        });

        trim_oversized_event(&mut event);

        let frame = &event["threads"]["values"][0]["stacktrace"]["frames"][0];
        let trimmed = serde_json::to_string(&frame["vars"]).unwrap();
        assert!(
            trimmed.len() <= 2048 + 64,
            "thread frame vars should get the same budget as exception frame vars, got {} bytes",
            trimmed.len()
        );
    }

    #[test]
    fn trim_leaves_context_lines_untouched_no_matter_how_many() {
        // Relay's schema has no size/count limit on pre_context/post_context
        // (verified: no #[metastructure(max_bytes/max_depth)] on those
        // fields in relay-event-schema/src/protocol/stacktrace.rs) — so
        // Rustrak shouldn't invent one either.
        let pre: Vec<String> = (0..500).map(|i| format!("line{i}")).collect();
        let mut event = json!({
            "exception": { "values": [{ "type": "Error", "value": "boom", "stacktrace": { "frames": [{
                "filename": "app.py",
                "lineno": 10,
                "pre_context": pre,
            }]}}]}
        });

        trim_oversized_event(&mut event);

        let frame = &event["exception"]["values"][0]["stacktrace"]["frames"][0];
        assert_eq!(
            frame["pre_context"].as_array().unwrap().len(),
            500,
            "pre_context must be left untouched — Relay doesn't cap it either"
        );
    }

    #[test]
    fn trim_leaves_breadcrumb_count_untouched_no_matter_how_many() {
        // Relay's schema has no count/size limit on the breadcrumbs array
        // itself either (verified: no metastructure attrs on
        // Event.breadcrumbs in event.rs) — only per-breadcrumb `data` has a
        // budget (see trim_breadcrumb_data_uses_the_real_relay_budget below).
        let crumbs: Vec<serde_json::Value> = (0..500)
            .map(|i| json!({ "message": format!("crumb{i}") }))
            .collect();
        let mut event = json!({ "breadcrumbs": { "values": crumbs } });

        trim_oversized_event(&mut event);

        assert_eq!(
            event["breadcrumbs"]["values"].as_array().unwrap().len(),
            500,
            "breadcrumb count must be left untouched — Relay doesn't cap it either"
        );
    }

    #[test]
    fn trim_breadcrumb_data_uses_the_real_relay_budget_of_2048_bytes() {
        let mut data = serde_json::Map::new();
        for i in 0..50 {
            data.insert(format!("k{i:03}"), json!("x".repeat(100)));
        }
        let mut event = json!({
            "breadcrumbs": { "values": [{ "message": "big", "data": data }] }
        });

        trim_oversized_event(&mut event);

        let trimmed = serde_json::to_string(&event["breadcrumbs"]["values"][0]["data"]).unwrap();
        assert!(
            trimmed.len() <= 2048 + 64,
            "breadcrumb.data should be trimmed to Relay's real 2048-byte budget (breadcrumb.rs:108), got {} bytes",
            trimmed.len()
        );
    }

    #[test]
    fn trim_extra_uses_the_real_relay_budget_of_256kb() {
        let mut extra = serde_json::Map::new();
        extra.insert("huge".to_string(), json!("x".repeat(300_000)));
        let mut event = json!({ "extra": extra });

        trim_oversized_event(&mut event);

        let trimmed = serde_json::to_string(&event["extra"]).unwrap();
        assert!(
            trimmed.len() <= 262_144 + 64,
            "extra should be trimmed to Relay's real 262144-byte budget (event.rs:402), got {} bytes",
            trimmed.len()
        );
    }

    #[test]
    fn trim_modules_uses_the_real_relay_budget_of_8kb() {
        let mut modules = serde_json::Map::new();
        for i in 0..500 {
            modules.insert(format!("pkg{i:03}"), json!("1.0.0"));
        }
        let mut event = json!({ "modules": modules });

        trim_oversized_event(&mut event);

        let trimmed = serde_json::to_string(&event["modules"]).unwrap();
        assert!(
            trimmed.len() <= 8192 + 64,
            "modules should be trimmed to Relay's real 8192-byte budget (event.rs:242), got {} bytes",
            trimmed.len()
        );
    }

    #[test]
    fn trim_request_data_uses_the_real_relay_budget_of_8kb() {
        let mut event = json!({
            "request": { "method": "POST", "data": "x".repeat(20_000) }
        });

        trim_oversized_event(&mut event);

        let trimmed = serde_json::to_string(&event["request"]["data"]).unwrap();
        assert!(
            trimmed.len() <= 8192 + 8,
            "request.data should be trimmed to Relay's real 8192-byte budget (request.rs:437), got {} bytes",
            trimmed.len()
        );
    }

    #[test]
    fn trim_is_noop_for_a_normal_sized_event() {
        let original = json!({
            "exception": { "values": [{ "type": "Error", "value": "boom", "stacktrace": { "frames": [{
                "filename": "app.py",
                "lineno": 10,
                "pre_context": ["a", "b"],
                "post_context": ["c", "d"],
                "vars": { "x": "1", "y": "2" },
            }]}}]},
            "breadcrumbs": { "values": [{ "message": "only one" }] },
            "extra": { "k": "v" },
            "modules": { "pkg": "1.0.0" },
            "request": { "method": "GET", "data": "small" }
        });
        let mut event = original.clone();

        trim_oversized_event(&mut event);

        assert_eq!(
            event, original,
            "a within-bounds event must be left untouched"
        );
    }
}
