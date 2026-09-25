//! Integration tests for serving the compiled dashboard.
//!
//! No database: the dashboard is static files and a fallback rule, and every
//! one of these drives the real Actix service stack over a temporary
//! directory shaped like a Vite build.
//!
//! What is actually under test is the boundary. A single-page application and
//! a REST API share one origin here, and the two ways that goes wrong are
//! symmetrical: the API swallowing a client route (a reload on `/projects`
//! 404s) or the shell swallowing an API path (a typo'd endpoint answers `200
//! text/html` and every client parses it as a bug in itself).

use actix_web::{test, App};
use rustrak::config::DashboardConfig;
use rustrak::middleware::auth::RequireAuth;
use rustrak::routes::dashboard::Dashboard;
use serde_json::Value;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

const SHELL: &str = "<!doctype html><html><body><div id=\"app\"></div></body></html>";

/// A directory shaped like `vite build` leaves one.
fn build_output() -> TempDir {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(dir.path().join("index.html"), SHELL).expect("write index.html");
    fs::create_dir(dir.path().join("assets")).expect("create assets");
    fs::write(
        dir.path().join("assets/main-a1b2c3d4.js"),
        "console.log('rustrak')",
    )
    .expect("write asset");
    fs::write(dir.path().join("favicon.ico"), b"\x00\x00\x01\x00").expect("write favicon");
    dir
}

fn dashboard(root: &Path) -> Dashboard {
    Dashboard::detect(root).expect("a directory with an index.html is a dashboard")
}

// =============================================================================
// The shell
// =============================================================================

#[actix_web::test]
async fn serves_the_shell_at_the_root() {
    let dir = build_output();
    let app = test::init_service(App::new().configure(dashboard(dir.path()).configure())).await;

    let resp = test::call_service(&app, test::TestRequest::get().uri("/").to_request()).await;

    assert_eq!(resp.status(), 200);
    let content_type = resp
        .headers()
        .get("content-type")
        .expect("Content-Type header missing")
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        content_type.starts_with("text/html"),
        "expected html, got {content_type}"
    );

    let body = test::read_body(resp).await;
    assert_eq!(body, SHELL);
}

#[actix_web::test]
async fn the_shell_is_never_cached() {
    let dir = build_output();
    let app = test::init_service(App::new().configure(dashboard(dir.path()).configure())).await;

    let resp = test::call_service(&app, test::TestRequest::get().uri("/").to_request()).await;

    let cache_control = resp
        .headers()
        .get("cache-control")
        .expect("Cache-Control header missing")
        .to_str()
        .unwrap()
        .to_string();
    // The shell names the hashed bundles. Cached, a deploy keeps pointing the
    // browser at chunks that no longer exist.
    assert!(
        cache_control.contains("no-cache"),
        "expected no-cache, got {cache_control}"
    );
}

/// A no-cache shell would be a full transfer on every navigation without an
/// identity to revalidate against.
#[actix_web::test]
async fn an_unchanged_shell_revalidates_to_304() {
    let dir = build_output();
    let app = test::init_service(App::new().configure(dashboard(dir.path()).configure())).await;

    let first = test::call_service(&app, test::TestRequest::get().uri("/").to_request()).await;
    let etag = first
        .headers()
        .get("etag")
        .expect("ETag header missing")
        .to_str()
        .unwrap()
        .to_string();

    let again = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/projects")
            .insert_header(("If-None-Match", etag.clone()))
            .to_request(),
    )
    .await;

    assert_eq!(again.status(), 304);
    assert!(test::read_body(again).await.is_empty());
}

/// The whole point of the fallback: a route only the router knows about,
/// reached by reload or by typing it in, still gets the shell.
#[actix_web::test]
async fn an_unknown_client_route_gets_the_shell() {
    let dir = build_output();
    let app = test::init_service(App::new().configure(dashboard(dir.path()).configure())).await;

    for path in ["/projects", "/projects/42/issues", "/settings/tokens"] {
        let resp = test::call_service(&app, test::TestRequest::get().uri(path).to_request()).await;

        assert_eq!(resp.status(), 200, "{path} should render the shell");
        let body = test::read_body(resp).await;
        assert_eq!(body, SHELL, "{path} should render the shell");
    }
}

// =============================================================================
// Assets
// =============================================================================

#[actix_web::test]
async fn a_hashed_asset_is_immutable() {
    let dir = build_output();
    let app = test::init_service(App::new().configure(dashboard(dir.path()).configure())).await;

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/assets/main-a1b2c3d4.js")
            .to_request(),
    )
    .await;

    assert_eq!(resp.status(), 200);
    let cache_control = resp
        .headers()
        .get("cache-control")
        .expect("Cache-Control header missing")
        .to_str()
        .unwrap()
        .to_string();
    // Vite puts the content hash in the file name, so the URL can never point
    // at different bytes and the browser never has to ask again.
    assert!(
        cache_control.contains("immutable"),
        "expected immutable, got {cache_control}"
    );
    assert!(
        cache_control.contains("max-age=31536000"),
        "expected a year, got {cache_control}"
    );
}

/// A missing asset is a missing asset. Answering it with the shell hands the
/// browser HTML under a `.js` URL, and the parse error that follows says
/// nothing about the deploy that dropped the file.
#[actix_web::test]
async fn a_missing_asset_is_a_404_and_not_the_shell() {
    let dir = build_output();
    let app = test::init_service(App::new().configure(dashboard(dir.path()).configure())).await;

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/assets/deleted-by-a-deploy.js")
            .to_request(),
    )
    .await;

    assert_eq!(resp.status(), 404);
    let body = test::read_body(resp).await;
    assert_ne!(body, SHELL);
}

#[actix_web::test]
async fn serves_a_file_from_the_root_of_the_build() {
    let dir = build_output();
    let app = test::init_service(App::new().configure(dashboard(dir.path()).configure())).await;

    let resp = test::call_service(
        &app,
        test::TestRequest::get().uri("/favicon.ico").to_request(),
    )
    .await;

    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn refuses_to_climb_out_of_the_build_directory() {
    let dir = build_output();
    let secret = dir.path().parent().unwrap().join("rustrak-secret.txt");
    fs::write(&secret, "not yours").expect("write secret");

    let app = test::init_service(App::new().configure(dashboard(dir.path()).configure())).await;

    for path in [
        "/assets/%2e%2e%2frustrak-secret.txt",
        "/%2e%2e%2frustrak-secret.txt",
        "/assets/../../rustrak-secret.txt",
    ] {
        let resp = test::call_service(&app, test::TestRequest::get().uri(path).to_request()).await;
        let status = resp.status();
        let body = test::read_body(resp).await;
        assert_ne!(body, "not yours", "{path} escaped the build directory");
        // 400 (Actix rejecting the encoded traversal outright), 404, or the
        // shell are all fine. What is not fine is the file.
        assert!(
            status == 400 || status == 404 || body == SHELL,
            "{path} answered {status} with something unexpected"
        );
    }

    let _ = fs::remove_file(&secret);
}

// =============================================================================
// The API keeps its own paths
// =============================================================================

/// An unknown endpoint under an API prefix must stay an API answer. This is
/// the failure that costs an afternoon: `@rustrak/client` asks for
/// `/api/prjects`, gets `200 text/html`, and reports a schema error.
#[actix_web::test]
async fn an_unknown_api_path_is_a_json_404() {
    let dir = build_output();
    let app = test::init_service(App::new().configure(dashboard(dir.path()).configure())).await;

    for path in [
        "/api/does-not-exist",
        "/auth/does-not-exist",
        "/health/does-not-exist",
    ] {
        let resp = test::call_service(&app, test::TestRequest::get().uri(path).to_request()).await;

        assert_eq!(resp.status(), 404, "{path} should be a 404");
        let content_type = resp
            .headers()
            .get("content-type")
            .expect("Content-Type header missing")
            .to_str()
            .unwrap()
            .to_string();
        assert!(
            content_type.starts_with("application/json"),
            "{path} answered {content_type}"
        );

        let body: Value = test::read_body_json(resp).await;
        assert_eq!(body["error"]["type"], "NotFound", "{path}");
    }
}

/// The dashboard mounts last, so a real route always wins over the fallback.
#[actix_web::test]
async fn a_real_route_still_wins_over_the_fallback() {
    let dir = build_output();
    let app = test::init_service(
        App::new()
            .route(
                "/health",
                actix_web::web::get().to(|| async { actix_web::HttpResponse::Ok().json("ok") }),
            )
            .configure(dashboard(dir.path()).configure()),
    )
    .await;

    let resp = test::call_service(&app, test::TestRequest::get().uri("/health").to_request()).await;

    assert_eq!(resp.status(), 200);
    let body = test::read_body(resp).await;
    assert_ne!(body, SHELL);
}

// =============================================================================
// Detection
// =============================================================================

#[actix_web::test]
async fn there_is_no_dashboard_without_an_index() {
    let empty = tempfile::tempdir().expect("temp dir");
    assert!(Dashboard::detect(empty.path()).is_none());
    assert!(Dashboard::detect(Path::new("/rustrak/definitely/not/here")).is_none());
}

/// `RUSTRAK_DASHBOARD=off` wins over a build that is right there. This is the
/// deployment that runs `rustrak-ui` on another host and wants the machine
/// holding the data to serve no frontend, with no image rebuilt and no
/// directory renamed to get there.
#[actix_web::test]
async fn the_switch_keeps_a_present_build_unmounted() {
    let dir = build_output();
    let root = dir.path().to_str().expect("utf-8 temp path").to_string();

    let off = DashboardConfig {
        dir: root.clone(),
        enabled: false,
        url: None,
    };
    assert!(Dashboard::from_config(&off).is_none());

    let on = DashboardConfig {
        dir: root,
        enabled: true,
        url: None,
    };
    assert!(Dashboard::from_config(&on).is_some());
}

// =============================================================================
// Authentication
// =============================================================================

/// The bundle is public. It has to be: it is the code that draws the login
/// form, and there is nothing in it a stranger could not fetch from the
/// release anyway. Every API route behind it still authenticates for itself.
#[actix_web::test]
async fn the_dashboard_is_reachable_without_a_session() {
    let dir = build_output();
    let app = test::init_service(
        App::new()
            .wrap(RequireAuth::new(true))
            .configure(dashboard(dir.path()).configure()),
    )
    .await;

    for path in ["/", "/projects", "/assets/main-a1b2c3d4.js"] {
        let resp = test::call_service(&app, test::TestRequest::get().uri(path).to_request()).await;
        assert_eq!(resp.status(), 200, "{path} should not need a session");
    }
}

/// Without a dashboard the middleware keeps behaving exactly as it did: a
/// path outside the API prefixes is not public, it is unauthenticated.
#[actix_web::test]
async fn without_a_dashboard_unknown_paths_still_need_a_session() {
    // One route, purely so the app has a body type. `/anything` is still a
    // path no service claims, which is the case under test.
    let app = test::init_service(App::new().wrap(RequireAuth::new(false)).route(
        "/health",
        actix_web::web::get().to(|| async { actix_web::HttpResponse::Ok().finish() }),
    ))
    .await;

    let resp =
        test::call_service(&app, test::TestRequest::get().uri("/anything").to_request()).await;

    assert_eq!(resp.status(), 401);
}
