//! `rustrak-plugin`, the authors' framework, seen from the host: what a
//! plugin written with it registers, and how its routes behave.

use std::sync::Arc;
use std::time::Duration;

use rustrak::plugins::runtime::{Loaded, Request, Runtime};
use rustrak::plugins::store::PluginDb;
use serde_json::{json, Value};

use crate::common::plugins::hello_wasm;

struct Fixture {
    _dir: tempfile::TempDir,
    runtime: Arc<Runtime>,
    plugin: Loaded,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let runtime = Runtime::new().unwrap();
    let db = PluginDb::open(&dir.path().join("data.sqlite"), 16).unwrap();
    let migrations = crate::common::plugins::hello_migrations();
    db.migrate(&migrations).unwrap();
    let plugin = runtime
        .load("hello", hello_wasm(), dir.path(), Some(Arc::new(db)))
        .unwrap();
    Fixture {
        _dir: dir,
        runtime,
        plugin,
    }
}

fn call(f: &Fixture, method: &str, path: &str, query: &str, body: &[u8]) -> (u16, String) {
    let req = Request {
        method: method.into(),
        path: path.into(),
        query: query.into(),
        headers: vec![],
        body: body.to_vec(),
        project_id: Some(3),
        client_ip: None,
    };
    let r = f
        .runtime
        .handle_http(
            &f.plugin,
            req,
            Duration::from_secs(2),
            crate::common::plugins::hello_settings(),
        )
        .expect("the plugin answers");
    (r.status, String::from_utf8(r.body).unwrap())
}

#[test]
fn handlers_get_their_arguments_from_the_request() {
    let f = fixture();
    assert_eq!(call(&f, "POST", "/hit", "", b"/home").0, 204);
    assert_eq!(call(&f, "GET", "/count", "", b""), (200, "1".into()));
    assert_eq!(
        call(&f, "GET", "/greet", "name=ana", b""),
        (200, "hello ana".into())
    );
    let (status, body) = call(&f, "POST", "/echo", "", br#"{"message":"hi"}"#);
    assert_eq!(status, 200);
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap(),
        json!({ "message": "hi" })
    );
}

#[test]
fn bad_input_is_a_400_and_an_unknown_route_a_404() {
    let f = fixture();
    let (status, body) = call(&f, "POST", "/echo", "", b"not json");
    assert_eq!(status, 400);
    assert!(body.contains("invalid JSON body"), "{body}");
    assert_eq!(
        call(&f, "GET", "/greet", "", b"").0,
        400,
        "missing query field"
    );
    assert_eq!(call(&f, "GET", "/nope", "", b"").0, 404);
}

#[test]
fn hits_are_added_listed_newest_first_and_cleared() {
    let f = fixture();
    assert_eq!(call(&f, "POST", "/hits", "", br#"{"path":"/a"}"#).0, 201);
    assert_eq!(call(&f, "POST", "/hits", "", br#"{"path":"/b"}"#).0, 201);
    assert_eq!(
        call(&f, "POST", "/hits", "", br#"{"path":""}"#).0,
        400,
        "a path is required"
    );

    let (status, body) = call(&f, "GET", "/hits", "limit=10", b"");
    assert_eq!(status, 200);
    let hits: Value = serde_json::from_str(&body).unwrap();
    let paths: Vec<&str> = hits
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, ["/b", "/a"]);
    assert!(
        hits[0]["id"].is_i64() && hits[0]["createdAt"].is_i64(),
        "{hits}"
    );

    assert_eq!(call(&f, "DELETE", "/hits", "", b"").0, 204);
    assert_eq!(call(&f, "GET", "/count", "", b""), (200, "0".into()));
}

#[test]
fn settings_reach_the_code_typed() {
    let f = fixture();
    let (status, seen) = call(&f, "GET", "/config", "", b"");
    assert_eq!(status, 200, "{seen}");
    assert_eq!(
        serde_json::from_str::<Value>(&seen).unwrap(),
        json!({ "greeting": "Hello", "maxPathLength": 200, "hasToken": false })
    );
}
