//! The event summary a GitHub issue opens with, the way Sentry writes it:
//! each exception as `Type: value` and its last five in-app frames.

use rustrak::services::github::event_text;
use serde_json::json;

fn frame(n: u32, in_app: bool) -> serde_json::Value {
    json!({ "filename": format!("app/f{n}.js"), "function": format!("f{n}"), "lineno": n, "in_app": in_app })
}

#[test]
fn shows_the_last_five_in_app_frames_of_each_exception() {
    let mut frames: Vec<_> = (1..=7).map(|n| frame(n, true)).collect();
    frames.push(frame(8, false));
    frames[6]["context_line"] = json!("    throw new TypeError(x)  ");
    let data = json!({ "exception": { "values": [
        { "type": "Error", "value": "cause" },
        { "type": "TypeError", "value": "x is undefined", "stacktrace": { "frames": frames } },
    ]}});

    assert_eq!(
        event_text(&data),
        "Error: cause\n\n\
         TypeError: x is undefined\n\
         (3 additional frame(s) were not displayed)\n\
         ...\n  \
         File \"app/f3.js\", line 3, in f3\n  \
         File \"app/f4.js\", line 4, in f4\n  \
         File \"app/f5.js\", line 5, in f5\n  \
         File \"app/f6.js\", line 6, in f6\n  \
         File \"app/f7.js\", line 7, in f7\n    \
         throw new TypeError(x)"
    );
}

#[test]
fn frames_without_a_filename_name_their_module() {
    let data = json!({ "exception": [
        { "type": "E", "value": "v", "stacktrace": { "frames": [{ "module": "app.db", "function": "q" }] } },
    ]});
    assert_eq!(event_text(&data), "E: v\n  Module \"app.db\", in q");
}

#[test]
fn a_message_event_shows_its_message() {
    assert_eq!(
        event_text(&json!({ "logentry": { "formatted": "disk full" } })),
        "disk full"
    );
    assert_eq!(event_text(&json!({ "message": "disk full" })), "disk full");
    assert_eq!(event_text(&json!({})), "");
}
