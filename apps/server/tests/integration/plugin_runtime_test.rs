//! The plugin runtime against the real `hello` component: calls, its own
//! SQLite file, the compile cache, and a runaway plugin being stopped.

use std::sync::Arc;
use std::time::{Duration, Instant};

use rustrak::plugins::package::Package;
use rustrak::plugins::runtime::{CallError, Request, Runtime};
use rustrak::plugins::store::PluginDb;

use crate::common::plugins::hello_rtp;

fn request(method: &str, path: &str, project: Option<u64>, body: &[u8]) -> Request {
    Request {
        method: method.into(),
        path: path.into(),
        query: String::new(),
        headers: vec![],
        body: body.to_vec(),
        project_id: project,
        client_ip: None,
    }
}

struct Fixture {
    _dir: tempfile::TempDir,
    runtime: Arc<Runtime>,
    plugin: rustrak::plugins::runtime::Loaded,
}

fn fixture(with_sql: bool) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let package = Package::read(&hello_rtp()).expect("hello package");
    let runtime = Runtime::new().expect("runtime");
    let db = with_sql.then(|| {
        let db = PluginDb::open(&dir.path().join("data.sqlite"), 16).unwrap();
        db.migrate(&package.migrations).unwrap();
        Arc::new(db)
    });
    let plugin = runtime
        .load("hello", &package.wasm, dir.path(), db)
        .expect("hello loads");
    Fixture {
        _dir: dir,
        runtime,
        plugin,
    }
}

#[test]
fn serves_requests_with_its_own_database() {
    let f = fixture(true);
    let hit = f.runtime.handle_http(
        &f.plugin,
        request("POST", "/hit", Some(7), b"/home"),
        Duration::from_secs(2),
        crate::common::plugins::hello_settings(),
    );
    assert_eq!(hit.expect("hit").status, 204);

    let count = |project| {
        let r = f
            .runtime
            .handle_http(
                &f.plugin,
                request("GET", "/count", Some(project), b""),
                Duration::from_secs(2),
                crate::common::plugins::hello_settings(),
            )
            .expect("count");
        String::from_utf8(r.body).unwrap()
    };
    assert_eq!(count(7), "1");
    assert_eq!(count(8), "0");
}

#[test]
fn without_the_sql_capability_queries_fail_inside_the_plugin() {
    let f = fixture(false);
    let r = f
        .runtime
        .handle_http(
            &f.plugin,
            request("POST", "/hit", Some(7), b"/"),
            Duration::from_secs(2),
            crate::common::plugins::hello_settings(),
        )
        .expect("the plugin answers");
    assert_eq!(r.status, 500);
    // The reason is logged by the host; the caller only learns it failed.
    let body = String::from_utf8_lossy(&r.body);
    assert!(!body.contains("capability"), "{body}");
}

#[test]
fn a_runaway_plugin_is_stopped_at_its_budget() {
    let f = fixture(true);
    let started = Instant::now();
    let result = f.runtime.handle_http(
        &f.plugin,
        request("GET", "/spin", None, b""),
        Duration::from_millis(200),
        crate::common::plugins::hello_settings(),
    );
    assert!(matches!(result, Err(CallError::Timeout)), "{result:?}");
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "took {:?}",
        started.elapsed()
    );

    // The same plugin keeps answering afterwards.
    let ok = f.runtime.handle_http(
        &f.plugin,
        request("GET", "/count", Some(1), b""),
        Duration::from_secs(2),
        crate::common::plugins::hello_settings(),
    );
    assert_eq!(ok.expect("count").status, 200);
}

#[test]
fn the_compiled_code_is_cached_and_reused() {
    let dir = tempfile::tempdir().unwrap();
    let package = Package::read(&hello_rtp()).unwrap();
    let runtime = Runtime::new().unwrap();
    runtime
        .load("hello", &package.wasm, dir.path(), None)
        .unwrap();
    let cached: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.ends_with(".cwasm"))
        .collect();
    assert_eq!(cached.len(), 1, "{cached:?}");

    // Loaded again, it reads the cache rather than compiling and writing it.
    let file = dir.path().join(&cached[0]);
    let written = std::fs::metadata(&file).unwrap().modified().unwrap();
    std::thread::sleep(Duration::from_millis(20));
    runtime
        .load("hello", &package.wasm, dir.path(), None)
        .unwrap();
    assert_eq!(
        std::fs::metadata(&file).unwrap().modified().unwrap(),
        written
    );

    // A corrupt cache is rebuilt, not trusted.
    std::fs::write(dir.path().join(&cached[0]), b"garbage").unwrap();
    let plugin = runtime
        .load("hello", &package.wasm, dir.path(), None)
        .expect("recompiles");
    let r = runtime.handle_http(
        &plugin,
        request("GET", "/nope", None, b""),
        Duration::from_secs(2),
        crate::common::plugins::hello_settings(),
    );
    assert_eq!(r.expect("answers").status, 404);
}
