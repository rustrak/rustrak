//! The GitHub integration over HTTP, against a fake GitHub API that runs on
//! loopback inside the test and records every call Rustrak makes to it.

use crate::common::TestDb;
use actix_session::{storage::CookieSessionStore, SessionMiddleware};
use actix_web::cookie::Key;
use actix_web::{test, web, App, HttpRequest, HttpResponse, HttpServer};
use chrono::Utc;
use rustrak::config::{Config, DashboardConfig, DatabaseConfig, RateLimitConfig};
use rustrak::models::github::{CodeMapping, GitHubApp};
use rustrak::models::CreateProject;
use rustrak::routes;
use rustrak::services::github::GitHubService;
use rustrak::services::grouping::DenormalizedFields;
use rustrak::services::{AuthTokenService, IssueService, ProjectService};
use serde_json::{json, Value};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const KEY: &str = include_str!("../fixtures/github_app_test_key.txt");

// =============================================================================
// A fake GitHub
// =============================================================================

#[derive(Clone, Debug)]
struct Call {
    method: String,
    path: String,
    body: Value,
}

struct FakeGitHub {
    url: String,
    calls: Arc<Mutex<Vec<Call>>>,
}

impl FakeGitHub {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let calls = Arc::new(Mutex::new(Vec::new()));
        let data = web::Data::from(calls.clone());
        let server = HttpServer::new(move || {
            App::new()
                .app_data(data.clone())
                .default_service(web::to(answer))
        })
        .workers(1)
        .listen(listener)
        .unwrap()
        .run();
        actix_web::rt::spawn(server);
        FakeGitHub { url, calls }
    }

    fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }

    /// Waits for a call Rustrak makes in the background.
    async fn wait_for(&self, method: &str, path: &str) -> Call {
        for _ in 0..50 {
            if let Some(call) = self
                .calls()
                .into_iter()
                .find(|c| c.method == method && c.path == path)
            {
                return call;
            }
            actix_web::rt::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("GitHub never received {method} {path}: {:?}", self.calls());
    }
}

async fn answer(
    req: HttpRequest,
    body: web::Bytes,
    calls: web::Data<Mutex<Vec<Call>>>,
) -> HttpResponse {
    let method = req.method().to_string();
    let path = req.path().to_string();
    let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    calls.lock().unwrap().push(Call {
        method: method.clone(),
        path: path.clone(),
        body: body.clone(),
    });
    let repo = json!({
        "full_name": "acme/web", "default_branch": "trunk", "private": true,
        "html_url": "https://github.com/acme/web",
    });
    match (method.as_str(), path.as_str()) {
        ("POST", "/app-manifests/abc/conversions") => HttpResponse::Created().json(json!({
            "id": 7, "slug": "rustrak-test", "client_id": "Iv1.x", "client_secret": "cs",
            "webhook_secret": "whs", "pem": KEY, "html_url": "https://github.com/apps/rustrak-test",
            "owner": { "login": "acme" },
        })),
        // An installation removed on GitHub whose `deleted` webhook never came.
        ("POST", "/app/installations/13/access_tokens") => {
            HttpResponse::NotFound().json(json!({ "message": "Not Found" }))
        }
        // The token names its installation, so a call made with it says which.
        ("POST", p) if p.ends_with("/access_tokens") => HttpResponse::Created().json(json!({
            "token": format!("ghs_{}", p.split('/').nth(3).unwrap_or_default()),
            "expires_at": (Utc::now() + chrono::Duration::hours(1)).to_rfc3339(),
        })),
        ("GET", "/app/installations/99") => HttpResponse::Ok().json(json!({
            "account": { "login": "acme", "type": "Organization" },
        })),
        // Installation 50 reaches 250 repositories, a page of 100 at a time.
        ("GET", "/installation/repositories")
            if req
                .headers()
                .get("Authorization")
                .unwrap()
                .to_str()
                .unwrap()
                == "token ghs_50" =>
        {
            let page: usize = req
                .query_string()
                .split('&')
                .find_map(|kv| kv.strip_prefix("page="))
                .unwrap()
                .parse()
                .unwrap();
            let repositories: Vec<Value> = ((page - 1) * 100..(page * 100).min(250))
                .map(|i| json!({ "full_name": format!("big/r{i}"), "default_branch": "main", "private": true }))
                .collect();
            HttpResponse::Ok().json(json!({ "total_count": 250, "repositories": repositories }))
        }
        ("GET", "/installation/repositories") => {
            HttpResponse::Ok().json(json!({ "total_count": 1, "repositories": [repo] }))
        }
        ("GET", "/repos/acme/web") => HttpResponse::Ok().json(repo),
        // Fails the first time it is asked, the way GitHub has a bad minute.
        ("POST", "/repos/acme/flaky/issues")
            if calls
                .lock()
                .unwrap()
                .iter()
                .filter(|c| c.path == path)
                .count()
                == 1 =>
        {
            HttpResponse::BadGateway().json(json!({ "message": "Server Error" }))
        }
        ("GET", "/repos/acme/flaky/issues") => HttpResponse::Ok().json(json!([])),
        // Opens the issue, then loses the answer: GitHub has it, Rustrak does not.
        ("POST", "/repos/acme/lost/issues") => {
            HttpResponse::BadGateway().json(json!({ "message": "Server Error" }))
        }
        ("GET", "/repos/acme/lost/issues") => {
            let mut listed = Vec::new();
            for c in calls.lock().unwrap().iter() {
                if c.method != "POST" || c.path != path {
                    continue;
                }
                let body = c.body["body"].as_str().unwrap_or_default();
                // Someone pasted the Rustrak link into another issue since.
                let link = body
                    .split("](")
                    .nth(1)
                    .and_then(|rest| rest.split(')').next())
                    .unwrap_or_default();
                listed.push(json!({
                    "number": 5, "title": "Seen in prod?", "state": "open",
                    "body": format!("Looks like [this]({link})"),
                    "html_url": "https://github.com/acme/lost/issues/5",
                }));
                listed.push(json!({
                    "number": 9, "title": c.body["title"], "body": body, "state": "open",
                    "html_url": "https://github.com/acme/lost/issues/9",
                }));
            }
            HttpResponse::Ok().json(listed)
        }
        // Takes its time to answer, long enough for a replay to arrive meanwhile.
        ("POST", "/repos/acme/slow/issues") => {
            actix_web::rt::time::sleep(Duration::from_millis(300)).await;
            HttpResponse::Created().json(json!({
                "number": 6, "title": body["title"], "state": "open",
                "html_url": "https://github.com/acme/slow/issues/6",
            }))
        }
        // GitHub is slow to apply the first change, so a later one lands
        // before it. `APPLIED` records the order GitHub applied them in.
        ("PATCH", "/repos/acme/slow/issues/6") => {
            let first = calls
                .lock()
                .unwrap()
                .iter()
                .filter(|c| c.method == "PATCH" && c.path == path)
                .count()
                == 1;
            if first {
                actix_web::rt::time::sleep(Duration::from_millis(300)).await;
            }
            calls.lock().unwrap().push(Call {
                method: "APPLIED".into(),
                path,
                body,
            });
            HttpResponse::Ok().json(json!({}))
        }
        ("POST", "/repos/acme/flaky/issues") => HttpResponse::Created().json(json!({
            "number": 8, "title": body["title"], "state": "open",
            "html_url": "https://github.com/acme/flaky/issues/8",
        })),
        ("POST", "/repos/acme/web/issues") => HttpResponse::Created().json(json!({
            "number": 7, "title": body["title"], "state": "open",
            "html_url": "https://github.com/acme/web/issues/7",
        })),
        // The issues API answers for pull requests too.
        ("GET", "/repos/acme/web/issues/14") => HttpResponse::Ok().json(json!({
            "number": 14, "title": "Fix the login", "state": "open",
            "html_url": "https://github.com/acme/web/pull/14",
            "pull_request": { "url": "https://api.github.com/repos/acme/web/pulls/14" },
        })),
        ("GET", "/repos/acme/web/issues/12") => HttpResponse::Ok().json(json!({
            "number": 12, "title": "Login is broken", "state": "open",
            "html_url": "https://github.com/acme/web/issues/12",
        })),
        ("GET", "/repos/acme/web/assignees") => {
            HttpResponse::Ok().json(json!([{ "login": "alice" }, { "login": "octocat" }]))
        }
        ("GET", "/repos/acme/web/contents/.github/CODEOWNERS") => {
            HttpResponse::Ok().body("* @acme/all\n/src/api/ @acme/backend\n")
        }
        // A linked repository the installation no longer reaches.
        (_, p) if p.starts_with("/repos/gone/") => {
            HttpResponse::NotFound().json(json!({ "message": "Not Found" }))
        }
        ("PATCH", _) | ("POST", _) => HttpResponse::Ok().json(json!({})),
        _ => HttpResponse::NotFound().json(json!({ "message": "Not Found" })),
    }
}

// =============================================================================
// Rustrak under test
// =============================================================================

fn test_config() -> Config {
    Config {
        host: "127.0.0.1".to_string(),
        port: 0,
        database: DatabaseConfig {
            url: "sqlite::memory:".to_string(),
            max_connections: 5,
            min_connections: 1,
            acquire_timeout: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(60),
            max_lifetime: Duration::from_secs(300),
        },
        rate_limit: RateLimitConfig {
            max_events_per_minute: 1000,
            max_events_per_hour: 10000,
            max_events_per_project_per_minute: 500,
            max_events_per_project_per_hour: 5000,
        },
        security: rustrak::config::SecurityConfig {
            ssl_proxy: false,
            session_secret_key: None,
        },
        ingest_dir: None,
        public_url: Some("https://rustrak.example.com".to_string()),
        sourcemap_storage_path: "/tmp/test_sourcemaps".to_string(),
        sourcemap_cache_bytes: 64 * 1024 * 1024,
        max_chunk_size_bytes: 10 * 1024 * 1024,
        session_flush_interval_secs: 30,
        session_cardinality_cap: 10_000,
        dashboard: DashboardConfig {
            dir: "./static".to_string(),
            enabled: true,
            url: None,
        },
        telemetry: rustrak::config::TelemetryConfig {
            enabled: false,
            do_not_track: false,
        },
    }
}

macro_rules! app {
    ($db:expr) => {
        test::init_service(
            App::new()
                .app_data(web::Data::new($db.pool.clone()))
                .app_data(web::Data::new(test_config()))
                .wrap(
                    SessionMiddleware::builder(CookieSessionStore::default(), Key::generate())
                        .cookie_secure(false)
                        .build(),
                )
                .configure(routes::github::configure)
                .configure(routes::issues::configure),
        )
        .await
    };
}

async fn token(db: &TestDb) -> String {
    AuthTokenService::create(
        &db.pool,
        rustrak::models::CreateAuthToken { description: None },
    )
    .await
    .unwrap()
    .token
}

fn bearer(token: &str) -> (&'static str, String) {
    ("Authorization", format!("Bearer {token}"))
}

async fn project(db: &TestDb) -> rustrak::models::Project {
    ProjectService::create(
        &db.pool,
        CreateProject {
            name: "Web".to_string(),
            slug: Some("web".to_string()),
            platform: None,
        },
    )
    .await
    .unwrap()
}

/// The app, installation 99 on `acme`, and the project pointed at acme/web.
async fn connect(db: &TestDb, github: &FakeGitHub, project_id: i32) {
    GitHubService::save_app(
        &db.pool,
        &GitHubApp {
            app_id: 7,
            slug: "rustrak-test".into(),
            client_id: "Iv1.x".into(),
            client_secret: "cs".into(),
            webhook_secret: "whs".into(),
            private_key: KEY.into(),
            html_url: "https://github.com/apps/rustrak-test".into(),
            owner_login: "acme".into(),
            web_url: "https://github.com".into(),
            api_url: github.url.clone(),
            last_webhook_at: None,
            created_at: Utc::now(),
        },
    )
    .await
    .unwrap();
    GitHubService::upsert_installation(&db.pool, 99, "acme", "Organization")
        .await
        .unwrap();
    GitHubService::save_project_repository(
        &db.pool,
        project_id,
        99,
        "acme/web",
        "trunk",
        &[CodeMapping {
            stack_root: "/app/".into(),
            source_root: "".into(),
        }],
        false,
        &json!({}),
    )
    .await
    .unwrap();
}

async fn issue(db: &TestDb, project_id: i32, culprit_file: &str) -> rustrak::models::Issue {
    IssueService::create(
        &db.pool,
        project_id,
        Utc::now(),
        &DenormalizedFields {
            calculated_type: "TypeError".into(),
            calculated_value: "x is undefined".into(),
            transaction: "/login".into(),
            last_frame_filename: culprit_file.into(),
            last_frame_module: String::new(),
            last_frame_function: "query".into(),
            culprit: "query".into(),
            logger: String::new(),
            release: String::new(),
        },
        Some("error"),
        Some("javascript"),
    )
    .await
    .unwrap()
}

// =============================================================================
// Connecting the app
// =============================================================================

#[actix_web::test]
async fn status_reports_no_app_until_one_is_connected() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let token = token(&db).await;
    let app = app!(db);

    let req = test::TestRequest::get()
        .uri("/api/github")
        .insert_header(bearer(&token))
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(body["app"], Value::Null);
    assert_eq!(
        body["webhook_url"],
        "https://rustrak.example.com/api/github/webhook"
    );

    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    let req = test::TestRequest::get()
        .uri("/api/github")
        .insert_header(bearer(&token))
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(body["app"]["slug"], "rustrak-test");
    assert_eq!(
        body["app"]["install_url"],
        "https://github.com/apps/rustrak-test/installations/new"
    );
    assert_eq!(body["installations"][0]["account_login"], "acme");
}

#[actix_web::test]
async fn manifest_flow_creates_the_app() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let token = token(&db).await;
    let app = app!(db);

    let req = test::TestRequest::post()
        .uri("/api/github/manifest")
        .insert_header(bearer(&token))
        .set_json(json!({ "organization": "acme" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let cookie = resp.response().cookies().next().unwrap().into_owned();
    let body: Value = test::read_body_json(resp).await;
    let action = body["action_url"].as_str().unwrap();
    assert!(action.starts_with("https://github.com/organizations/acme/settings/apps/new?state="));
    let state = action.split("state=").nth(1).unwrap();
    let manifest: Value = serde_json::from_str(body["manifest"].as_str().unwrap()).unwrap();
    assert_eq!(
        manifest["hook_attributes"]["url"],
        "https://rustrak.example.com/api/github/webhook"
    );
    assert_eq!(
        manifest["redirect_url"],
        "https://rustrak.example.com/api/github/callback"
    );

    // A callback whose state is not the one handed out is refused.
    let req = test::TestRequest::get()
        .uri("/api/github/callback?code=abc&state=forged")
        .insert_header(bearer(&token))
        .cookie(cookie.clone())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    // The real one trades the code for the app's credentials, at the API
    // `GITHUB_API_URL` names (api.github.com when unset).
    std::env::set_var("GITHUB_API_URL", &github.url);
    let req = test::TestRequest::get()
        .uri(&format!("/api/github/callback?code=abc&state={state}"))
        .insert_header(bearer(&token))
        .cookie(cookie)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 302);
    assert_eq!(
        resp.headers().get("Location").unwrap(),
        "/settings/integrations?github=connected"
    );
    let stored = GitHubService::app(&db.pool).await.unwrap().unwrap();
    assert_eq!(stored.app_id, 7);
    assert_eq!(stored.webhook_secret, "whs");
}

#[actix_web::test]
async fn manifest_sends_the_browser_to_the_dashboard_host() {
    let db = TestDb::new().await;
    let token = token(&db).await;
    let base = test_config();
    let config = rustrak::config::Config {
        dashboard: DashboardConfig {
            url: Some("https://errors.example.com".to_string()),
            ..base.dashboard.clone()
        },
        ..base
    };
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(config))
            .wrap(
                SessionMiddleware::builder(CookieSessionStore::default(), Key::generate())
                    .cookie_secure(false)
                    .build(),
            )
            .configure(routes::github::configure),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/github/manifest")
        .insert_header(bearer(&token))
        .set_json(json!({}))
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, req).await;
    let manifest: Value = serde_json::from_str(body["manifest"].as_str().unwrap()).unwrap();
    // The session cookie lives on the dashboard's host, so GitHub must send
    // the browser back there; only the webhook goes to PUBLIC_URL.
    assert_eq!(
        manifest["redirect_url"],
        "https://errors.example.com/api/github/callback"
    );
    assert_eq!(
        manifest["setup_url"],
        "https://errors.example.com/api/github/installed"
    );
    assert_eq!(
        manifest["hook_attributes"]["url"],
        "https://rustrak.example.com/api/github/webhook"
    );
}

#[actix_web::test]
async fn installing_the_app_records_the_installation() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let token = token(&db).await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    let app = app!(db);

    let req = test::TestRequest::get()
        .uri("/api/github/installed?installation_id=99&setup_action=install")
        .insert_header(bearer(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 302);
    let installations = GitHubService::installations(&db.pool).await.unwrap();
    assert_eq!(installations[0].account_type, "Organization");
}

#[actix_web::test]
async fn disconnecting_forgets_the_app_and_project_repositories() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let token = token(&db).await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    let app = app!(db);

    let req = test::TestRequest::delete()
        .uri("/api/github")
        .insert_header(bearer(&token))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 204);
    assert!(GitHubService::app(&db.pool).await.unwrap().is_none());
    assert!(GitHubService::project_repository(&db.pool, project.id)
        .await
        .unwrap()
        .is_none());
}

// =============================================================================
// A project's repository
// =============================================================================

#[actix_web::test]
async fn repositories_lists_what_the_installations_reach() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let token = token(&db).await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    let app = app!(db);

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/github/repositories?project_id={}",
            project.id
        ))
        .insert_header(bearer(&token))
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(body[0]["full_name"], "acme/web");
    assert_eq!(body[0]["installation_id"], 99);
}

#[actix_web::test]
async fn repositories_reads_every_page_of_a_large_installation() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let token = token(&db).await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    GitHubService::upsert_installation(&db.pool, 50, "big", "Organization")
        .await
        .unwrap();
    let app = app!(db);

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/github/repositories?project_id={}",
            project.id
        ))
        .insert_header(bearer(&token))
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, req).await;
    let names: Vec<&str> = body
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["full_name"].as_str())
        .filter(|n| n.starts_with("big/"))
        .collect();
    assert_eq!(names.len(), 250);
    assert_eq!(names[0], "big/r0");
    assert_eq!(names[249], "big/r249");
}

#[actix_web::test]
async fn repositories_skip_an_installation_github_no_longer_has() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let token = token(&db).await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    GitHubService::upsert_installation(&db.pool, 13, "gone", "User")
        .await
        .unwrap();
    let app = app!(db);

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/github/repositories?project_id={}",
            project.id
        ))
        .insert_header(bearer(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body[0]["full_name"], "acme/web");
}

#[actix_web::test]
async fn project_repository_refuses_a_name_that_is_not_owner_slash_repo() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let token = token(&db).await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    let app = app!(db);

    for name in [
        "../user",
        "acme/web/issues",
        "acme/..",
        "acme/web?x=1",
        "acme",
    ] {
        let req = test::TestRequest::put()
            .uri(&format!("/api/projects/{}/github", project.id))
            .insert_header(bearer(&token))
            .set_json(json!({ "installation_id": 99, "repo_full_name": name }))
            .to_request();
        assert_eq!(test::call_service(&app, req).await.status(), 400, "{name}");
    }
    assert!(!github.calls().iter().any(|c| c.method == "GET"));
}

#[actix_web::test]
async fn project_repository_round_trip() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let token = token(&db).await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    GitHubService::delete_project_repository(&db.pool, project.id)
        .await
        .unwrap();
    let app = app!(db);
    let uri = format!("/api/projects/{}/github", project.id);

    let req = test::TestRequest::get()
        .uri(&uri)
        .insert_header(bearer(&token))
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(body, Value::Null);

    let req = test::TestRequest::put()
        .uri(&uri)
        .insert_header(bearer(&token))
        .set_json(json!({
            "installation_id": 99,
            "repo_full_name": "acme/web",
            "code_mappings": [{ "stack_root": "/app/", "source_root": "src/" }],
            "auto_create_issues": true,
            "auto_create_conditions": { "min_level": "error", "min_events": 10 },
        }))
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, req).await;
    // The default branch comes from GitHub, not from the request.
    assert_eq!(body["default_branch"], "trunk");
    assert_eq!(body["html_url"], "https://github.com/acme/web");
    assert_eq!(body["code_mappings"][0]["source_root"], "src/");
    assert_eq!(body["auto_create_issues"], true);
    assert_eq!(body["auto_create_conditions"]["min_events"], 10);

    let req = test::TestRequest::put()
        .uri(&uri)
        .insert_header(bearer(&token))
        .set_json(json!({
            "installation_id": 99,
            "repo_full_name": "acme/web",
            "auto_create_conditions": { "min_level": "loud" },
        }))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 400);

    let req = test::TestRequest::delete()
        .uri(&uri)
        .insert_header(bearer(&token))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 204);
}

#[actix_web::test]
async fn codeowners_sync_counts_the_rules() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let token = token(&db).await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    let app = app!(db);

    let req = test::TestRequest::post()
        .uri(&format!("/api/projects/{}/github/codeowners", project.id))
        .insert_header(bearer(&token))
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(body["codeowners_rules"], 2);

    let codeowners = || async {
        GitHubService::project_repository_row(&db.pool, project.id)
            .await
            .unwrap()
            .unwrap()
            .codeowners
    };
    // A sync that finishes after the project moved to another repository
    // does not write the old one's owners onto it.
    GitHubService::save_codeowners(&db.pool, project.id, "acme/old", Some("* @old"))
        .await
        .unwrap();
    assert_ne!(codeowners().await.as_deref(), Some("* @old"));
    // Saving the same repository, however GitHub cases it, keeps them.
    GitHubService::save_project_repository(
        &db.pool,
        project.id,
        99,
        "Acme/Web",
        "trunk",
        &[],
        false,
        &json!({}),
    )
    .await
    .unwrap();
    assert!(codeowners().await.is_some());
}

#[actix_web::test]
async fn suspect_commits_look_past_a_busy_window() {
    let db = TestDb::new().await;
    let commit = |sha: String, file: &str, minutes: i64| rustrak::models::github::RepoCommit {
        repo_full_name: "acme/web".into(),
        sha,
        message: "Change".into(),
        author_name: "Ana".into(),
        author_login: None,
        url: String::new(),
        files: json!([file]).to_string(),
        committed_at: Utc::now() - chrono::Duration::minutes(minutes),
    };
    GitHubService::save_commit(&db.pool, &commit("old".into(), "lib/db.ts", 2000))
        .await
        .unwrap();
    for i in 0..1001 {
        GitHubService::save_commit(&db.pool, &commit(format!("n{i}"), "README.md", 10))
            .await
            .unwrap();
    }
    let found =
        GitHubService::suspect_commits(&db.pool, "acme/web", "lib/db.ts", false, Utc::now())
            .await
            .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].sha, "old");

    // A name JSON escapes, or that LIKE would read as a pattern, still matches.
    for name in [r#"lib/a"b.ts"#, r"lib/a\b.ts", "lib/100%_!.ts"] {
        GitHubService::save_commit(&db.pool, &commit(name.into(), name, 5))
            .await
            .unwrap();
        let found = GitHubService::suspect_commits(&db.pool, "acme/web", name, true, Utc::now())
            .await
            .unwrap();
        assert_eq!(found.first().map(|c| c.sha.as_str()), Some(name));
    }
}

// =============================================================================
// Issues
// =============================================================================

async fn link_12(db: &TestDb, issue_id: uuid::Uuid) -> rustrak::models::github::IssueLink {
    GitHubService::add_link(
        &db.pool,
        issue_id,
        "acme/web",
        &rustrak::services::github::client::RemoteIssue {
            number: 12,
            title: "Login is broken".into(),
            url: "https://github.com/acme/web/issues/12".into(),
            state: "open".into(),
        },
    )
    .await
    .unwrap()
}

/// Links `acme/slow#6`, where GitHub applies the first change late.
async fn link_slow(db: &TestDb, issue_id: uuid::Uuid) {
    GitHubService::add_link(
        &db.pool,
        issue_id,
        "acme/slow",
        &rustrak::services::github::client::RemoteIssue {
            number: 6,
            title: "Slow".into(),
            url: "https://github.com/acme/slow/issues/6".into(),
            state: "open".into(),
        },
    )
    .await
    .unwrap();
}

/// The last change GitHub applied to `acme/slow#6`, once every request is in.
async fn applied_last(github: &FakeGitHub) -> Value {
    actix_web::rt::time::sleep(Duration::from_millis(600)).await;
    github
        .calls()
        .into_iter()
        .rfind(|c| c.method == "APPLIED")
        .expect("nothing applied")
        .body
}

#[actix_web::test]
async fn a_quick_reopen_leaves_github_open() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    let issue = issue(&db, project.id, "/app/lib/db.ts").await;
    link_slow(&db, issue.id).await;

    // Resolved, then reopened while GitHub is still closing it.
    IssueService::set_status(&db.pool, issue.id, "resolved", None)
        .await
        .unwrap();
    let closing = actix_web::rt::spawn(rustrak::services::github::sync::mirror_status(
        db.pool.clone(),
        issue.id,
    ));
    github.wait_for("PATCH", "/repos/acme/slow/issues/6").await;
    IssueService::set_status(&db.pool, issue.id, "unresolved", None)
        .await
        .unwrap();
    rustrak::services::github::sync::mirror_status(db.pool.clone(), issue.id)
        .await
        .unwrap();
    closing.await.unwrap().unwrap();

    assert_eq!(applied_last(&github).await["state"], "open");
}

#[actix_web::test]
async fn a_quick_reassignment_leaves_github_with_the_latest_assignee() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    let issue = issue(&db, project.id, "/app/lib/db.ts").await;
    link_slow(&db, issue.id).await;
    let mut users = Vec::new();
    for login in ["alice", "bob"] {
        let user = rustrak::services::UsersService::create_user_with_password_hash(
            &db.pool,
            &format!("{login}@acme.dev"),
            "unused",
            rustrak::models::UserRole::Member,
        )
        .await
        .unwrap();
        sqlx::query("UPDATE users SET github_login = $1 WHERE id = $2")
            .bind(login)
            .bind(user.id)
            .execute(&db.pool)
            .await
            .unwrap();
        users.push(user.id);
    }

    // Alice, then Bob while GitHub is still assigning Alice.
    IssueService::assign(&db.pool, issue.id, Some(users[0]), Some("user"))
        .await
        .unwrap();
    let first = actix_web::rt::spawn(rustrak::services::github::sync::mirror_assignee(
        db.pool.clone(),
        issue.id,
        None,
    ));
    github.wait_for("PATCH", "/repos/acme/slow/issues/6").await;
    IssueService::assign(&db.pool, issue.id, Some(users[1]), Some("user"))
        .await
        .unwrap();
    rustrak::services::github::sync::mirror_assignee(db.pool.clone(), issue.id, None)
        .await
        .unwrap();
    first.await.unwrap().unwrap();

    assert_eq!(applied_last(&github).await["assignees"], json!(["bob"]));
}

#[actix_web::test]
async fn a_github_issue_created_by_hand_closes_when_resolved_meanwhile() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    sqlx::query("UPDATE project_repositories SET repo_full_name = 'acme/slow'")
        .execute(&db.pool)
        .await
        .unwrap();
    let issue = issue(&db, project.id, "/app/lib/db.ts").await;

    let creating = {
        let (pool, project, issue) = (db.pool.clone(), project.clone(), issue.clone());
        actix_web::rt::spawn(async move {
            rustrak::services::github::sync::create_issue(
                &pool,
                &project,
                &issue,
                Default::default(),
                None,
                "http://rustrak",
            )
            .await
        })
    };
    // Resolved before GitHub answers: there is no link yet to close.
    github.wait_for("POST", "/repos/acme/slow/issues").await;
    IssueService::set_status(&db.pool, issue.id, "resolved", None)
        .await
        .unwrap();
    rustrak::services::github::sync::mirror_status(db.pool.clone(), issue.id)
        .await
        .unwrap();
    creating.await.unwrap().unwrap();

    let call = github.wait_for("PATCH", "/repos/acme/slow/issues/6").await;
    assert_eq!(call.body["state"], "closed");
}

#[actix_web::test]
async fn a_github_issue_created_by_hand_for_a_resolved_issue_stays_open() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    let issue = issue(&db, project.id, "/app/lib/db.ts").await;
    let issue = IssueService::set_status(&db.pool, issue.id, "resolved", None)
        .await
        .unwrap();

    rustrak::services::github::sync::create_issue(
        &db.pool,
        &project,
        &issue,
        Default::default(),
        None,
        "http://rustrak",
    )
    .await
    .unwrap();

    actix_web::rt::time::sleep(Duration::from_millis(200)).await;
    assert!(!github.calls().iter().any(|c| c.method == "PATCH"));
}

#[actix_web::test]
async fn an_issue_assigned_to_something_other_than_a_person_leaves_github_alone() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    let issue = issue(&db, project.id, "/app/lib/db.ts").await;
    link_12(&db, issue.id).await;
    let user = rustrak::services::UsersService::create_user_with_password_hash(
        &db.pool,
        "octo@acme.dev",
        "unused",
        rustrak::models::UserRole::Member,
    )
    .await
    .unwrap();
    sqlx::query("UPDATE users SET github_login = 'octocat' WHERE id = $1")
        .bind(user.id)
        .execute(&db.pool)
        .await
        .unwrap();

    // A team that happens to share the person's id.
    IssueService::assign(&db.pool, issue.id, Some(user.id), Some("team"))
        .await
        .unwrap();
    rustrak::services::github::sync::mirror_assignee(db.pool.clone(), issue.id, None)
        .await
        .unwrap();

    assert!(!github
        .calls()
        .iter()
        .any(|c| c.method == "PATCH" && c.body.get("assignees").is_some()));
}

#[actix_web::test]
async fn clearing_an_assignee_of_any_type_clears_github() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    let issue = issue(&db, project.id, "/app/lib/db.ts").await;
    link_12(&db, issue.id).await;

    // Cleared, with the type the API was sent alongside.
    IssueService::assign(&db.pool, issue.id, None, Some("team"))
        .await
        .unwrap();
    rustrak::services::github::sync::mirror_assignee(db.pool.clone(), issue.id, None)
        .await
        .unwrap();

    let call = github.wait_for("PATCH", "/repos/acme/web/issues/12").await;
    assert_eq!(call.body["assignees"], json!([]));
}

/// A rename on GitHub stored by its webhook while the state change was on
/// its way out is not undone by the title that change started with.
#[actix_web::test]
async fn a_status_sync_keeps_a_title_renamed_meanwhile() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    let issue = issue(&db, project.id, "/app/lib/db.ts").await;
    link_slow(&db, issue.id).await;

    IssueService::set_status(&db.pool, issue.id, "resolved", None)
        .await
        .unwrap();
    let closing = actix_web::rt::spawn(rustrak::services::github::sync::mirror_status(
        db.pool.clone(),
        issue.id,
    ));
    github.wait_for("PATCH", "/repos/acme/slow/issues/6").await;
    GitHubService::update_remote(&db.pool, "acme/slow", 6, "Renamed", "open")
        .await
        .unwrap();
    closing.await.unwrap().unwrap();

    let links = GitHubService::links(&db.pool, issue.id).await.unwrap();
    assert_eq!(links[0].title, "Renamed");
    assert_eq!(links[0].state, "closed");
}

#[actix_web::test]
async fn creating_a_github_issue_links_it() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let token = token(&db).await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    let issue = issue(&db, project.id, "/app/lib/db.ts").await;
    let app = app!(db);
    let base = format!("/api/projects/{}/issues/{}/github", project.id, issue.id);

    let req = test::TestRequest::post()
        .uri(&format!("{base}/issues"))
        .insert_header(bearer(&token))
        .set_json(json!({}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let link: Value = test::read_body_json(resp).await;
    assert_eq!(link["number"], 7);

    let sent = github.wait_for("POST", "/repos/acme/web/issues").await;
    assert_eq!(sent.body["title"], "TypeError: x is undefined");
    let body = sent.body["body"].as_str().unwrap();
    assert!(body.contains("WEB-1"), "{body}");
    assert!(body.contains(&format!(
        "https://rustrak.example.com/projects/{}/issues/{}",
        project.id, issue.id
    )));

    let req = test::TestRequest::get()
        .uri(&base)
        .insert_header(bearer(&token))
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(
        body["links"][0]["url"],
        "https://github.com/acme/web/issues/7"
    );
}

/// The instance's template, then the project's on top of it: the project's
/// labels add to the instance's, its non-empty texts replace them.
async fn templates(db: &TestDb, project_id: i32, project_template: Value) {
    let token = token(db).await;
    let app = app!(db);
    let req = test::TestRequest::put()
        .uri("/api/github/issue-template")
        .insert_header(bearer(&token))
        .set_json(json!({
            "labels": ["bug"], "title_prefix": "[Rustrak] ", "body_footer": "  cc @acme/oncall\n",
        }))
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(body["labels"], json!(["bug"]));
    // Indentation is markdown; only the trailing blank is dropped.
    assert_eq!(body["body_footer"], "  cc @acme/oncall");
    let req = test::TestRequest::put()
        .uri(&format!("/api/projects/{project_id}/github"))
        .insert_header(bearer(&token))
        .set_json(json!({
            "installation_id": 99, "repo_full_name": "acme/web", "issue_template": project_template,
        }))
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(body["issue_template"], project_template);
}

/// A footer that alone fills GitHub's limit would fail every issue it opens.
#[actix_web::test]
async fn a_footer_too_long_for_github_is_refused() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let token = token(&db).await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    let app = app!(db);
    let footer = "x".repeat(10_001);

    let req = test::TestRequest::put()
        .uri("/api/github/issue-template")
        .insert_header(bearer(&token))
        .set_json(json!({ "body_footer": footer }))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 400);
    let req = test::TestRequest::put()
        .uri(&format!("/api/projects/{}/github", project.id))
        .insert_header(bearer(&token))
        .set_json(json!({
            "installation_id": 99, "repo_full_name": "acme/web",
            "issue_template": { "body_footer": footer },
        }))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 400);
}

#[actix_web::test]
async fn the_draft_carries_the_event_and_both_templates() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let token = token(&db).await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    templates(
        &db,
        project.id,
        json!({ "labels": ["web"], "title_prefix": "", "body_footer": "" }),
    )
    .await;
    digest(&db, project.id, "production").await;
    let issue_id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM issues WHERE project_id = $1")
        .bind(project.id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    let alice = rustrak::services::UsersService::create_user_with_password_hash(
        &db.pool,
        "alice@acme.dev",
        "unused",
        rustrak::models::UserRole::Member,
    )
    .await
    .unwrap();
    sqlx::query("UPDATE users SET github_login = 'alice' WHERE id = $1")
        .bind(alice.id)
        .execute(&db.pool)
        .await
        .unwrap();
    let issue = IssueService::assign(&db.pool, issue_id, Some(alice.id), Some("user"))
        .await
        .unwrap();
    let app = app!(db);

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{}/issues/{}/github/draft",
            project.id, issue.id
        ))
        .insert_header(bearer(&token))
        .to_request();
    let draft: Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(draft["title"], "[Rustrak] TypeError: x is undefined");
    assert_eq!(draft["labels"], json!(["bug", "web"]));
    let body = draft["body"].as_str().unwrap();
    assert!(
        body.starts_with(&format!(
            "Rustrak Issue: [WEB-1](https://rustrak.example.com/projects/{}/issues/{})",
            project.id, issue.id
        )),
        "{body}"
    );
    assert!(
        body.contains("```\nTypeError: x is undefined\n```"),
        "{body}"
    );
    assert!(body.ends_with("cc @acme/oncall"), "{body}");
    // The Rustrak assignee's login, among those GitHub lets the issue have.
    assert_eq!(draft["assignee"], "alice");
    assert_eq!(draft["assignees"], json!(["alice", "octocat"]));
}

#[actix_web::test]
async fn a_github_issue_opens_with_the_templates_unless_the_request_says_otherwise() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let token = token(&db).await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    templates(
        &db,
        project.id,
        json!({ "labels": [], "title_prefix": "[web] ", "body_footer": "" }),
    )
    .await;
    let issue = issue(&db, project.id, "/app/lib/db.ts").await;
    let app = app!(db);
    let uri = format!(
        "/api/projects/{}/issues/{}/github/issues",
        project.id, issue.id
    );

    let req = test::TestRequest::post()
        .uri(&uri)
        .insert_header(bearer(&token))
        .set_json(json!({}))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 201);
    let sent = github.wait_for("POST", "/repos/acme/web/issues").await;
    assert_eq!(sent.body["title"], "[web] TypeError: x is undefined");
    assert_eq!(sent.body["labels"], json!(["bug"]));
    assert!(sent.body.get("assignees").is_none(), "{:?}", sent.body);

    // Another issue: GitHub answers #7 again, and one issue links #7 once.
    let other = self::issue(&db, project.id, "/app/lib/db.ts").await;
    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/projects/{}/issues/{}/github/issues",
            project.id, other.id
        ))
        .insert_header(bearer(&token))
        .set_json(json!({ "title": "Mine", "body": "Text", "labels": [], "assignee": "octocat" }))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 201);
    let sent = github.calls().pop().unwrap();
    assert_eq!(sent.body["title"], "Mine");
    assert_eq!(sent.body["body"], "Text");
    assert!(sent.body.get("labels").is_none(), "{:?}", sent.body);
    assert_eq!(sent.body["assignees"], json!(["octocat"]));

    // A description the person cleared stays empty.
    let third = self::issue(&db, project.id, "/app/lib/db.ts").await;
    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/projects/{}/issues/{}/github/issues",
            project.id, third.id
        ))
        .insert_header(bearer(&token))
        .set_json(json!({ "title": "Mine", "body": "" }))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 201);
    assert_eq!(github.calls().pop().unwrap().body["body"], "");
}

/// Its status and assignees do not sync (the app is not subscribed to pull
/// requests), and closing one through the issues API would close it unmerged.
#[actix_web::test]
async fn a_pull_request_cannot_be_linked() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let token = token(&db).await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    let issue = issue(&db, project.id, "/app/lib/db.ts").await;
    let app = app!(db);

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/projects/{}/issues/{}/github/links",
            project.id, issue.id
        ))
        .insert_header(bearer(&token))
        .set_json(json!({ "reference": "#14" }))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 400);
    assert!(GitHubService::links(&db.pool, issue.id)
        .await
        .unwrap()
        .is_empty());
    assert!(!github
        .calls()
        .iter()
        .any(|c| c.method == "POST" && c.path == "/repos/acme/web/issues/14/comments"));
}

#[actix_web::test]
async fn linking_an_existing_issue_comments_back_and_refuses_duplicates() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let token = token(&db).await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    let issue = issue(&db, project.id, "/app/lib/db.ts").await;
    let app = app!(db);
    let uri = format!(
        "/api/projects/{}/issues/{}/github/links",
        project.id, issue.id
    );

    let req = test::TestRequest::post()
        .uri(&uri)
        .insert_header(bearer(&token))
        .set_json(json!({ "reference": "#12" }))
        .to_request();
    let link: Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(link["title"], "Login is broken");
    github
        .wait_for("POST", "/repos/acme/web/issues/12/comments")
        .await;

    let req = test::TestRequest::post()
        .uri(&uri)
        .insert_header(bearer(&token))
        .set_json(json!({ "reference": "https://github.com/acme/web/issues/12" }))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 409);

    // GitHub names repositories case-insensitively; so does a duplicate.
    let again = GitHubService::add_link(
        &db.pool,
        issue.id,
        "Acme/Web",
        &rustrak::services::github::client::RemoteIssue {
            number: 12,
            title: "Login is broken".into(),
            url: "https://github.com/acme/web/issues/12".into(),
            state: "open".into(),
        },
    )
    .await;
    assert!(matches!(again, Err(rustrak::error::AppError::Conflict(_))));

    let req = test::TestRequest::delete()
        .uri(&format!("{uri}/{}", link["id"]))
        .insert_header(bearer(&token))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 204);
    assert!(GitHubService::links(&db.pool, issue.id)
        .await
        .unwrap()
        .is_empty());
}

#[actix_web::test]
async fn comments_reach_github_without_the_authors_email() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    let issue = issue(&db, project.id, "/app/lib/db.ts").await;
    link_12(&db, issue.id).await;
    let user = rustrak::services::UsersService::create_user_with_password_hash(
        &db.pool,
        "octo@acme.dev",
        "unused",
        rustrak::models::UserRole::Admin,
    )
    .await
    .unwrap();
    let token = AuthTokenService::create_for_user(
        &db.pool,
        rustrak::models::CreateAuthToken { description: None },
        Some(user.id),
    )
    .await
    .unwrap()
    .token;
    let app = app!(db);
    let comment = |text: &str| {
        test::TestRequest::post()
            .uri(&format!(
                "/api/projects/{}/issues/{}/comments",
                project.id, issue.id
            ))
            .insert_header(bearer(&token))
            .set_json(json!({ "text": text }))
            .to_request()
    };
    let posted = |text: &'static str| {
        let github = &github;
        async move {
            for _ in 0..50 {
                if let Some(c) = github.calls().into_iter().find(|c| {
                    c.path == "/repos/acme/web/issues/12/comments"
                        && c.body["body"].as_str().unwrap_or("").contains(text)
                }) {
                    return c.body["body"].as_str().unwrap().to_string();
                }
                actix_web::rt::time::sleep(Duration::from_millis(20)).await;
            }
            panic!("comment never mirrored: {:?}", github.calls());
        }
    };

    assert_eq!(
        test::call_service(&app, comment("first")).await.status(),
        201
    );
    let body = posted("first").await;
    assert!(!body.contains("octo@acme.dev"), "{body}");
    assert!(body.contains("A Rustrak user"), "{body}");

    sqlx::query("UPDATE users SET github_login = 'octocat' WHERE id = $1")
        .bind(user.id)
        .execute(&db.pool)
        .await
        .unwrap();
    assert_eq!(
        test::call_service(&app, comment("second")).await.status(),
        201
    );
    let body = posted("second").await;
    assert!(body.contains("@octocat"), "{body}");
    assert!(!body.contains("octo@acme.dev"), "{body}");
}

#[actix_web::test]
async fn one_unreachable_link_does_not_stop_the_others() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    let issue = issue(&db, project.id, "/app/lib/db.ts").await;
    GitHubService::add_link(
        &db.pool,
        issue.id,
        "gone/old",
        &rustrak::services::github::client::RemoteIssue {
            number: 1,
            title: "Old".into(),
            url: "https://github.com/gone/old/issues/1".into(),
            state: "open".into(),
        },
    )
    .await
    .unwrap();
    link_12(&db, issue.id).await;
    IssueService::set_status(&db.pool, issue.id, "resolved", None)
        .await
        .unwrap();

    let result = rustrak::services::github::sync::mirror_status(db.pool.clone(), issue.id).await;
    assert!(result.is_ok(), "{result:?}");
    assert!(github
        .calls()
        .iter()
        .any(|c| c.method == "PATCH" && c.path == "/repos/acme/web/issues/12"));
}

#[actix_web::test]
async fn resolving_commenting_and_assigning_mirror_onto_linked_issues() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let token = token(&db).await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    let issue = issue(&db, project.id, "/app/lib/db.ts").await;
    link_12(&db, issue.id).await;
    let app = app!(db);
    let uri = format!("/api/projects/{}/issues/{}", project.id, issue.id);

    let req = test::TestRequest::patch()
        .uri(&uri)
        .insert_header(bearer(&token))
        .set_json(json!({ "status": "resolved" }))
        .to_request();
    assert!(test::call_service(&app, req).await.status().is_success());
    let call = github.wait_for("PATCH", "/repos/acme/web/issues/12").await;
    assert_eq!(call.body["state"], "closed");

    let req = test::TestRequest::post()
        .uri(&format!("{uri}/comments"))
        .insert_header(bearer(&token))
        .set_json(json!({ "text": "Deployed the fix" }))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 201);
    let call = github
        .wait_for("POST", "/repos/acme/web/issues/12/comments")
        .await;
    assert!(call.body["body"]
        .as_str()
        .unwrap()
        .contains("Deployed the fix"));

    let user = rustrak::services::UsersService::create_user_with_password_hash(
        &db.pool,
        "octo@acme.dev",
        "unused",
        rustrak::models::UserRole::Member,
    )
    .await
    .unwrap();
    sqlx::query("UPDATE users SET github_login = 'octocat' WHERE id = $1")
        .bind(user.id)
        .execute(&db.pool)
        .await
        .unwrap();
    let req = test::TestRequest::patch()
        .uri(&uri)
        .insert_header(bearer(&token))
        .set_json(json!({ "assigned_to": user.id, "assignee_type": "user" }))
        .to_request();
    assert!(test::call_service(&app, req).await.status().is_success());
    for _ in 0..50 {
        if github
            .calls()
            .iter()
            .any(|c| c.method == "PATCH" && c.body["assignees"] == json!(["octocat"]))
        {
            return;
        }
        actix_web::rt::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("assignee never mirrored: {:?}", github.calls());
}

#[actix_web::test]
async fn issue_code_block_names_suspect_commits_and_owners() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let token = token(&db).await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    let issue = issue(&db, project.id, "/app/lib/db.ts").await;
    GitHubService::save_codeowners(
        &db.pool,
        project.id,
        "acme/web",
        Some("* @acme/all\n/lib/ @acme/db\n"),
    )
    .await
    .unwrap();
    // The mapping says the frame is `lib/db.ts` at the root, so another
    // package's `lib/db.ts` is a different file.
    for (sha, file, hours_before) in [
        ("aaa", "lib/db.ts", 2),
        ("bbb", "README.md", 1),
        ("ccc", "packages/server/lib/db.ts", 1),
    ] {
        GitHubService::save_commit(
            &db.pool,
            &rustrak::models::github::RepoCommit {
                repo_full_name: "acme/web".into(),
                sha: sha.into(),
                message: "Refactor pool".into(),
                author_name: "Ana".into(),
                author_login: Some("ana".into()),
                url: format!("https://github.com/acme/web/commit/{sha}"),
                files: json!([file]).to_string(),
                committed_at: issue.first_seen - chrono::Duration::hours(hours_before),
            },
        )
        .await
        .unwrap();
    }
    let app = app!(db);

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{}/issues/{}/github",
            project.id, issue.id
        ))
        .insert_header(bearer(&token))
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(body["suspect_commits"].as_array().unwrap().len(), 1);
    assert_eq!(body["suspect_commits"][0]["sha"], "aaa");
    assert_eq!(body["owners"], json!(["@acme/db"]));
}

#[actix_web::test]
async fn bulk_resolving_closes_linked_issues_of_this_project_only() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let token = token(&db).await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    let issue = issue(&db, project.id, "/app/lib/db.ts").await;
    link_12(&db, issue.id).await;
    let app = app!(db);

    // Another project's issue, resolved while its GitHub issue stayed open.
    let other = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "Other".to_string(),
            slug: Some("other".to_string()),
            platform: None,
        },
    )
    .await
    .unwrap();
    GitHubService::save_project_repository(
        &db.pool,
        other.id,
        99,
        "acme/api",
        "main",
        &[],
        false,
        &json!({}),
    )
    .await
    .unwrap();
    let foreign = self::issue(&db, other.id, "/app/lib/db.ts").await;
    GitHubService::add_link(
        &db.pool,
        foreign.id,
        "acme/api",
        &rustrak::services::github::client::RemoteIssue {
            number: 5,
            title: "Other".into(),
            url: "https://github.com/acme/api/issues/5".into(),
            state: "open".into(),
        },
    )
    .await
    .unwrap();
    IssueService::set_status(&db.pool, foreign.id, "resolved", None)
        .await
        .unwrap();

    let req = test::TestRequest::put()
        .uri(&format!("/api/projects/{}/issues", project.id))
        .insert_header(bearer(&token))
        .set_json(json!({ "ids": [issue.id, foreign.id], "status": "resolved" }))
        .to_request();
    assert!(test::call_service(&app, req).await.status().is_success());
    let call = github.wait_for("PATCH", "/repos/acme/web/issues/12").await;
    assert_eq!(call.body["state"], "closed");
    actix_web::rt::time::sleep(Duration::from_millis(200)).await;
    assert!(!github
        .calls()
        .iter()
        .any(|c| c.path == "/repos/acme/api/issues/5"));
}

// =============================================================================
// Webhooks
// =============================================================================

fn webhook(event: &str, payload: &Value, secret: &str) -> test::TestRequest {
    use hmac::{Hmac, KeyInit, Mac};
    let body = payload.to_string();
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(body.as_bytes());
    let signature = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));
    test::TestRequest::post()
        .uri("/api/github/webhook")
        .insert_header(("X-GitHub-Event", event))
        .insert_header(("X-Hub-Signature-256", signature))
        .insert_header(("Content-Type", "application/json"))
        .set_payload(body)
}

fn github_issue_event(action: &str, sender: &str) -> Value {
    json!({
        "action": action,
        "issue": { "number": 12, "title": "Login is broken", "state": if action == "closed" { "closed" } else { "open" },
                   "html_url": "https://github.com/acme/web/issues/12" },
        "repository": { "full_name": "acme/web" },
        "sender": { "login": sender },
    })
}

#[actix_web::test]
async fn webhooks_with_a_wrong_signature_are_refused() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    let app = app!(db);

    let req = webhook("issues", &github_issue_event("closed", "ana"), "wrong").to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 401);
}

#[actix_web::test]
async fn closing_and_reopening_on_github_follows_through() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    let issue = issue(&db, project.id, "/app/lib/db.ts").await;
    link_12(&db, issue.id).await;
    let app = app!(db);

    let req = webhook("issues", &github_issue_event("closed", "ana"), "whs").to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 204);
    let stored = IssueService::get_by_id(&db.pool, issue.id).await.unwrap();
    assert_eq!(stored.status, "resolved");
    assert_eq!(
        GitHubService::links(&db.pool, issue.id).await.unwrap()[0].state,
        "closed"
    );
    assert!(GitHubService::app(&db.pool)
        .await
        .unwrap()
        .unwrap()
        .last_webhook_at
        .is_some());

    let req = webhook("issues", &github_issue_event("reopened", "ana"), "whs").to_request();
    test::call_service(&app, req).await;
    let stored = IssueService::get_by_id(&db.pool, issue.id).await.unwrap();
    assert_eq!(stored.status, "unresolved");
}

#[actix_web::test]
async fn closing_one_linked_github_issue_closes_the_others() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    let issue = issue(&db, project.id, "/app/lib/db.ts").await;
    link_12(&db, issue.id).await;
    GitHubService::add_link(
        &db.pool,
        issue.id,
        "acme/web",
        &rustrak::services::github::client::RemoteIssue {
            number: 13,
            title: "Login is broken, again".into(),
            url: "https://github.com/acme/web/issues/13".into(),
            state: "open".into(),
        },
    )
    .await
    .unwrap();
    let app = app!(db);

    let req = webhook("issues", &github_issue_event("closed", "ana"), "whs").to_request();
    test::call_service(&app, req).await;

    let call = github.wait_for("PATCH", "/repos/acme/web/issues/13").await;
    assert_eq!(call.body["state"], "closed");
    // The one GitHub closed itself needs no telling.
    assert!(!github
        .calls()
        .iter()
        .any(|c| c.method == "PATCH" && c.path == "/repos/acme/web/issues/12"));
}

#[actix_web::test]
async fn a_disconnected_project_ignores_its_old_github_issues() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    let issue = issue(&db, project.id, "/app/lib/db.ts").await;
    link_12(&db, issue.id).await;
    GitHubService::delete_project_repository(&db.pool, project.id)
        .await
        .unwrap();
    let app = app!(db);

    let req = webhook("issues", &github_issue_event("closed", "ana"), "whs").to_request();
    test::call_service(&app, req).await;
    let mut comment = github_issue_event("created", "ana");
    comment["comment"] = json!({ "body": "Looking into it", "user": { "login": "ana" },
                                 "html_url": "https://github.com/acme/web/issues/12#c1" });
    test::call_service(&app, webhook("issue_comment", &comment, "whs").to_request()).await;

    let now = IssueService::get_by_id(&db.pool, issue.id).await.unwrap();
    assert_eq!(now.status, "unresolved");
    let activity = rustrak::services::IssueSocialService::list_activity(&db.pool, issue.id)
        .await
        .unwrap();
    assert!(activity.iter().all(|a| a.activity_type != "note"));
    // The link stays, as the record of what it was linked to.
    assert_eq!(
        GitHubService::links(&db.pool, issue.id)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[actix_web::test]
async fn an_issue_that_comes_back_reopens_its_github_issue() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    digest(&db, project.id, "production").await;
    let (issue_id,): (uuid::Uuid,) = sqlx::query_as("SELECT id FROM issues")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    link_12(&db, issue_id).await;
    IssueService::resolve(&db.pool, issue_id).await.unwrap();
    GitHubService::update_remote(&db.pool, "acme/web", 12, "Login is broken", "closed")
        .await
        .unwrap();

    digest(&db, project.id, "production").await;

    let call = github.wait_for("PATCH", "/repos/acme/web/issues/12").await;
    assert_eq!(call.body["state"], "open");
}

#[actix_web::test]
async fn assigning_one_linked_github_issue_assigns_the_others() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    let issue = issue(&db, project.id, "/app/lib/db.ts").await;
    link_12(&db, issue.id).await;
    GitHubService::add_link(
        &db.pool,
        issue.id,
        "acme/web",
        &rustrak::services::github::client::RemoteIssue {
            number: 13,
            title: "Login is broken, again".into(),
            url: "https://github.com/acme/web/issues/13".into(),
            state: "open".into(),
        },
    )
    .await
    .unwrap();
    let user = rustrak::services::UsersService::create_user_with_password_hash(
        &db.pool,
        "octo@acme.dev",
        "unused",
        rustrak::models::UserRole::Member,
    )
    .await
    .unwrap();
    sqlx::query("UPDATE users SET github_login = 'octocat' WHERE id = $1")
        .bind(user.id)
        .execute(&db.pool)
        .await
        .unwrap();
    let app = app!(db);

    let mut assigned = github_issue_event("assigned", "ana");
    assigned["issue"]["assignees"] = json!([{ "login": "octocat" }, { "login": "hubot" }]);
    test::call_service(&app, webhook("issues", &assigned, "whs").to_request()).await;

    let call = github.wait_for("PATCH", "/repos/acme/web/issues/13").await;
    assert_eq!(call.body["assignees"], json!(["octocat"]));
    // The one assigned on GitHub keeps all of its assignees.
    assert!(!github
        .calls()
        .iter()
        .any(|c| c.method == "PATCH" && c.path == "/repos/acme/web/issues/12"));
}

#[actix_web::test]
async fn an_assignment_leaves_its_own_github_issue_alone_however_the_link_spells_it() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    let issue = issue(&db, project.id, "/app/lib/db.ts").await;
    GitHubService::add_link(
        &db.pool,
        issue.id,
        "Acme/Web",
        &rustrak::services::github::client::RemoteIssue {
            number: 12,
            title: "Login is broken".into(),
            url: "https://github.com/acme/web/issues/12".into(),
            state: "open".into(),
        },
    )
    .await
    .unwrap();
    let user = rustrak::services::UsersService::create_user_with_password_hash(
        &db.pool,
        "octo@acme.dev",
        "unused",
        rustrak::models::UserRole::Member,
    )
    .await
    .unwrap();
    sqlx::query("UPDATE users SET github_login = 'octocat' WHERE id = $1")
        .bind(user.id)
        .execute(&db.pool)
        .await
        .unwrap();
    let app = app!(db);

    let mut assigned = github_issue_event("assigned", "ana");
    assigned["issue"]["assignees"] = json!([{ "login": "octocat" }, { "login": "hubot" }]);
    test::call_service(&app, webhook("issues", &assigned, "whs").to_request()).await;

    let stored = IssueService::get_by_id(&db.pool, issue.id).await.unwrap();
    assert_eq!(stored.assigned_to, Some(user.id));
    actix_web::rt::time::sleep(Duration::from_millis(300)).await;
    assert!(!github
        .calls()
        .iter()
        .any(|c| c.method == "PATCH" && c.path.ends_with("/issues/12")));
}

#[actix_web::test]
async fn github_comments_and_assignees_come_back_into_rustrak() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    let issue = issue(&db, project.id, "/app/lib/db.ts").await;
    link_12(&db, issue.id).await;
    let user = rustrak::services::UsersService::create_user_with_password_hash(
        &db.pool,
        "octo@acme.dev",
        "unused",
        rustrak::models::UserRole::Member,
    )
    .await
    .unwrap();
    sqlx::query("UPDATE users SET github_login = 'octocat' WHERE id = $1")
        .bind(user.id)
        .execute(&db.pool)
        .await
        .unwrap();
    let app = app!(db);

    let mut comment = github_issue_event("created", "ana");
    comment["comment"] = json!({ "body": "Looking into it", "user": { "login": "ana" },
                                 "html_url": "https://github.com/acme/web/issues/12#c1" });
    test::call_service(&app, webhook("issue_comment", &comment, "whs").to_request()).await;
    // Rustrak's own comments come back as the app's bot and are not copied twice.
    comment["sender"]["login"] = json!("rustrak-test[bot]");
    test::call_service(&app, webhook("issue_comment", &comment, "whs").to_request()).await;
    // GitHub redelivers webhooks; the same comment is not copied twice.
    comment["sender"]["login"] = json!("ana");
    test::call_service(&app, webhook("issue_comment", &comment, "whs").to_request()).await;

    let notes: Vec<_> = rustrak::services::IssueSocialService::list_activity(&db.pool, issue.id)
        .await
        .unwrap()
        .into_iter()
        .filter(|a| a.activity_type == "note")
        .collect();
    assert_eq!(notes.len(), 1);
    let data: Value = serde_json::from_str(&notes[0].data).unwrap();
    assert_eq!(data["text"], "Looking into it");
    assert_eq!(data["author"], "ana");
    assert_eq!(data["source"], "github");

    let mut assigned = github_issue_event("assigned", "ana");
    assigned["assignee"] = json!({ "login": "octocat" });
    assigned["issue"]["assignees"] = json!([{ "login": "octocat" }]);
    test::call_service(&app, webhook("issues", &assigned, "whs").to_request()).await;
    let stored = IssueService::get_by_id(&db.pool, issue.id).await.unwrap();
    assert_eq!(stored.assigned_to, Some(user.id));

    // Someone else leaving the GitHub issue keeps the Rustrak assignee, the
    // way Sentry reads `issue.assignees` rather than the one that changed.
    let mut unassigned = github_issue_event("unassigned", "ana");
    unassigned["assignee"] = json!({ "login": "ana" });
    unassigned["issue"]["assignees"] = json!([{ "login": "octocat" }]);
    test::call_service(&app, webhook("issues", &unassigned, "whs").to_request()).await;
    let stored = IssueService::get_by_id(&db.pool, issue.id).await.unwrap();
    assert_eq!(stored.assigned_to, Some(user.id));

    unassigned["assignee"] = json!({ "login": "octocat" });
    unassigned["issue"]["assignees"] = json!([]);
    test::call_service(&app, webhook("issues", &unassigned, "whs").to_request()).await;
    let stored = IssueService::get_by_id(&db.pool, issue.id).await.unwrap();
    assert_eq!(stored.assigned_to, None);
}

#[actix_web::test]
async fn webhooks_larger_than_the_default_body_limit_are_read() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    let app = app!(db);

    // `installation.created` carries every repository; a large account
    // passes actix's 256 KiB default.
    let repositories: Vec<Value> = (0..10_000)
        .map(|i| json!({ "id": i, "full_name": format!("big/repository-{i}") }))
        .collect();
    let payload = json!({
        "action": "created",
        "installation": { "id": 42, "account": { "login": "big", "type": "Organization" } },
        "repositories": repositories,
    });
    assert!(payload.to_string().len() > 256 * 1024);
    let req = webhook("installation", &payload, "whs").to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 204);
}

#[actix_web::test]
async fn a_push_with_fixes_resolves_the_issue_and_keeps_the_commit() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    let issue = issue(&db, project.id, "/app/lib/db.ts").await;
    let app = app!(db);

    link_12(&db, issue.id).await;
    // GitHub renamed the default branch since the project was connected
    // (`trunk` then); the push says which one it is now.
    let push = |branch: &str, sha: &str| {
        json!({
            "ref": format!("refs/heads/{branch}"),
            "repository": { "full_name": "acme/web", "default_branch": "main" },
            "commits": [{
                "id": sha, "message": "Guard the pool\n\nFixes WEB-1",
                "timestamp": (Utc::now() - chrono::Duration::minutes(5)).to_rfc3339(),
                "url": format!("https://github.com/acme/web/commit/{sha}"),
                "author": { "name": "Ana", "username": "ana" },
                "added": [], "removed": [], "modified": ["lib/db.ts"],
            }],
        })
    };

    // Other branches are not the code that ships.
    test::call_service(
        &app,
        webhook("push", &push("feature", "f00"), "whs").to_request(),
    )
    .await;
    let stored = IssueService::get_by_id(&db.pool, issue.id).await.unwrap();
    assert_eq!(stored.status, "unresolved");
    test::call_service(
        &app,
        webhook("push", &push("trunk", "0ld"), "whs").to_request(),
    )
    .await;
    let stored = IssueService::get_by_id(&db.pool, issue.id).await.unwrap();
    assert_eq!(stored.status, "unresolved");

    let req = webhook("push", &push("main", "abc123"), "whs").to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 204);
    let stored = IssueService::get_by_id(&db.pool, issue.id).await.unwrap();
    assert_eq!(stored.status, "resolved");
    let activity = rustrak::services::IssueSocialService::list_activity(&db.pool, issue.id)
        .await
        .unwrap();
    let data: Value = serde_json::from_str(&activity[0].data).unwrap();
    assert_eq!(data["commit"]["sha"], "abc123");
    // The linked GitHub issue closes too.
    let call = github.wait_for("PATCH", "/repos/acme/web/issues/12").await;
    assert_eq!(call.body["state"], "closed");

    let commits: Vec<(String,)> = sqlx::query_as("SELECT sha FROM repo_commits")
        .fetch_all(&db.pool)
        .await
        .unwrap();
    assert_eq!(commits, vec![("abc123".to_string(),)]);

    // A redelivered push does not resolve the issue again once it came back.
    IssueService::set_status(&db.pool, issue.id, "unresolved", None)
        .await
        .unwrap();
    test::call_service(
        &app,
        webhook("push", &push("main", "abc123"), "whs").to_request(),
    )
    .await;
    let stored = IssueService::get_by_id(&db.pool, issue.id).await.unwrap();
    assert_eq!(stored.status, "unresolved");

    // A push that fails halfway leaves nothing behind, so its redelivery
    // resolves the issue and records why.
    let failing = push("main", "def456");
    sqlx::query("ALTER TABLE issue_activity RENAME TO issue_activity_gone")
        .execute(&db.pool)
        .await
        .unwrap();
    let req = webhook("push", &failing, "whs").to_request();
    assert!(test::call_service(&app, req)
        .await
        .status()
        .is_server_error());
    sqlx::query("ALTER TABLE issue_activity_gone RENAME TO issue_activity")
        .execute(&db.pool)
        .await
        .unwrap();
    let stored = IssueService::get_by_id(&db.pool, issue.id).await.unwrap();
    assert_eq!(stored.status, "unresolved");
    test::call_service(&app, webhook("push", &failing, "whs").to_request()).await;
    let stored = IssueService::get_by_id(&db.pool, issue.id).await.unwrap();
    assert_eq!(stored.status, "resolved");
    let activity = rustrak::services::IssueSocialService::list_activity(&db.pool, issue.id)
        .await
        .unwrap();
    let data: Value = serde_json::from_str(&activity[0].data).unwrap();
    assert_eq!(data["commit"]["sha"], "def456");
}

#[actix_web::test]
async fn uninstalling_the_app_on_github_removes_the_installation() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    let app = app!(db);

    let payload = json!({ "action": "deleted", "installation": { "id": 99, "account": { "login": "acme" } } });
    test::call_service(&app, webhook("installation", &payload, "whs").to_request()).await;
    assert!(GitHubService::installations(&db.pool)
        .await
        .unwrap()
        .is_empty());
}

// =============================================================================
// New issues
// =============================================================================

/// Digests one `TypeError` event, always the same group, in `environment`.
async fn digest(db: &TestDb, project_id: i32, environment: &str) {
    digest_id(
        db,
        project_id,
        environment,
        &uuid::Uuid::new_v4().simple().to_string(),
    )
    .await;
}

/// Digests the event with this id; a second call replays it.
async fn digest_id(db: &TestDb, project_id: i32, environment: &str, event_id: &str) {
    let ingest = tempfile::TempDir::new().unwrap();
    let event_id = event_id.to_string();
    let event = crate::common::EventBuilder::new()
        .with_event_id(&event_id)
        .with_exception("TypeError", "x is undefined")
        .with_environment(environment)
        .build();
    rustrak::ingest::store_event(
        ingest.path(),
        &event_id,
        &serde_json::to_vec(&event).unwrap(),
    )
    .await
    .unwrap();
    crate::common::process_error_event(
        &db.pool,
        &rustrak::ingest::EventMetadata {
            event_id,
            project_id,
            ingested_at: Utc::now(),
            remote_addr: None,
        },
        ingest.path(),
        &test_config().rate_limit,
        crate::common::null_sourcemap_provider(),
    )
    .await
    .unwrap();
}

/// Turns on automatic GitHub issues for the project, with these filters.
async fn auto_create(db: &TestDb, project_id: i32, conditions: Value) {
    sqlx::query(
        "UPDATE project_repositories SET auto_create_issues = TRUE, auto_create_conditions = $1 WHERE project_id = $2",
    )
    .bind(conditions.to_string())
    .bind(project_id)
    .execute(&db.pool)
    .await
    .unwrap();
}

fn issues_opened(github: &FakeGitHub) -> usize {
    github
        .calls()
        .iter()
        .filter(|c| c.method == "POST" && c.path == "/repos/acme/web/issues")
        .count()
}

#[actix_web::test]
async fn a_new_issue_opens_a_github_issue_when_the_project_asks() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    auto_create(&db, project.id, json!({})).await;

    digest(&db, project.id, "production").await;

    let sent = github.wait_for("POST", "/repos/acme/web/issues").await;
    assert_eq!(sent.body["title"], "TypeError: x is undefined");
}

#[actix_web::test]
async fn an_automatic_github_issue_uses_the_templates() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    templates(
        &db,
        project.id,
        json!({ "labels": ["web"], "title_prefix": "", "body_footer": "" }),
    )
    .await;
    auto_create(&db, project.id, json!({})).await;

    digest(&db, project.id, "production").await;

    let sent = github.wait_for("POST", "/repos/acme/web/issues").await;
    assert_eq!(sent.body["title"], "[Rustrak] TypeError: x is undefined");
    assert_eq!(sent.body["labels"], json!(["bug", "web"]));
    let body = sent.body["body"].as_str().unwrap();
    assert!(body.contains("TypeError: x is undefined\n```"), "{body}");
    assert!(body.contains("cc @acme/oncall"), "{body}");
}

#[actix_web::test]
async fn an_automatic_github_issue_github_refused_is_tried_again() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    sqlx::query("UPDATE project_repositories SET repo_full_name = 'acme/flaky'")
        .execute(&db.pool)
        .await
        .unwrap();
    auto_create(&db, project.id, json!({})).await;

    digest(&db, project.id, "production").await;

    let issue: (uuid::Uuid,) = sqlx::query_as("SELECT id FROM issues")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    for _ in 0..100 {
        if !GitHubService::links(&db.pool, issue.0)
            .await
            .unwrap()
            .is_empty()
        {
            return;
        }
        actix_web::rt::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("never linked: {:?}", github.calls());
}

#[actix_web::test]
async fn an_automatic_github_issue_whose_answer_was_lost_is_linked_not_opened_again() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    sqlx::query("UPDATE project_repositories SET repo_full_name = 'acme/lost'")
        .execute(&db.pool)
        .await
        .unwrap();
    auto_create(&db, project.id, json!({})).await;

    digest(&db, project.id, "production").await;

    let issue: (uuid::Uuid,) = sqlx::query_as("SELECT id FROM issues")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    for _ in 0..100 {
        let links = GitHubService::links(&db.pool, issue.0).await.unwrap();
        if let Some(link) = links.first() {
            assert_eq!(link.number, 9);
            let opened = github
                .calls()
                .iter()
                .filter(|c| c.method == "POST" && c.path == "/repos/acme/lost/issues")
                .count();
            assert_eq!(opened, 1);
            return;
        }
        actix_web::rt::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("never linked: {:?}", github.calls());
}

#[actix_web::test]
async fn an_event_replayed_while_its_github_issue_is_opening_does_not_open_another() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    sqlx::query("UPDATE project_repositories SET repo_full_name = 'acme/slow'")
        .execute(&db.pool)
        .await
        .unwrap();
    auto_create(&db, project.id, json!({})).await;

    // The SDK sends the same event again before GitHub has answered the first.
    let event_id = uuid::Uuid::new_v4().simple().to_string();
    digest_id(&db, project.id, "production", &event_id).await;
    digest_id(&db, project.id, "production", &event_id).await;

    github.wait_for("POST", "/repos/acme/slow/issues").await;
    actix_web::rt::time::sleep(Duration::from_millis(800)).await;
    let opened = github
        .calls()
        .iter()
        .filter(|c| c.method == "POST" && c.path == "/repos/acme/slow/issues")
        .count();
    assert_eq!(opened, 1);
}

#[actix_web::test]
async fn an_issue_resolved_while_its_github_issue_is_opening_closes_it() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    sqlx::query("UPDATE project_repositories SET repo_full_name = 'acme/slow'")
        .execute(&db.pool)
        .await
        .unwrap();
    auto_create(&db, project.id, json!({})).await;

    digest(&db, project.id, "production").await;
    // Resolved before GitHub answers: there is no link yet to close.
    let (issue_id,): (uuid::Uuid,) = sqlx::query_as("SELECT id FROM issues")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    IssueService::set_status(&db.pool, issue_id, "resolved", None)
        .await
        .unwrap();
    rustrak::services::github::sync::mirror_status(db.pool.clone(), issue_id)
        .await
        .unwrap();

    let call = github.wait_for("PATCH", "/repos/acme/slow/issues/6").await;
    assert_eq!(call.body["state"], "closed");
}

#[actix_web::test]
async fn the_filters_decide_when_the_github_issue_opens() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    auto_create(
        &db,
        project.id,
        json!({ "min_events": 3, "environments": ["production"] }),
    )
    .await;

    digest(&db, project.id, "production").await;
    digest(&db, project.id, "staging").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(issues_opened(&github), 0, "two events are not three");

    // The third event, but from staging: the count is right, the place is not.
    digest(&db, project.id, "staging").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(issues_opened(&github), 0, "staging is filtered out");

    digest(&db, project.id, "production").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        issues_opened(&github),
        0,
        "the fourth event is past the count"
    );
}

#[actix_web::test]
async fn the_github_issue_opens_on_the_event_that_reaches_the_count() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    auto_create(&db, project.id, json!({ "min_events": 2 })).await;

    digest(&db, project.id, "production").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(issues_opened(&github), 0);

    digest(&db, project.id, "production").await;
    github.wait_for("POST", "/repos/acme/web/issues").await;
}

#[actix_web::test]
async fn a_regression_that_reaches_the_count_opens_the_github_issue() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    auto_create(&db, project.id, json!({ "min_events": 2 })).await;

    digest(&db, project.id, "production").await;
    let (issue_id,): (uuid::Uuid,) = sqlx::query_as("SELECT id FROM issues WHERE project_id = $1")
        .bind(project.id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    IssueService::set_status(&db.pool, issue_id, "resolved", None)
        .await
        .unwrap();

    digest(&db, project.id, "production").await;
    github.wait_for("POST", "/repos/acme/web/issues").await;
}

#[actix_web::test]
async fn a_replayed_event_that_reached_the_count_opens_the_github_issue() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;

    // The second event reached the count, but the server stopped before it
    // acted on it; a third event came in since.
    let second = uuid::Uuid::new_v4().simple().to_string();
    digest(&db, project.id, "production").await;
    digest_id(&db, project.id, "production", &second).await;
    digest(&db, project.id, "production").await;
    auto_create(&db, project.id, json!({ "min_events": 2 })).await;

    digest_id(&db, project.id, "production", &second).await;
    github.wait_for("POST", "/repos/acme/web/issues").await;
}

#[actix_web::test]
async fn a_replayed_event_digested_while_muted_stays_quiet_after_unmuting() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;
    auto_create(&db, project.id, json!({ "min_events": 2 })).await;

    digest(&db, project.id, "production").await;
    let (issue_id,): (uuid::Uuid,) = sqlx::query_as("SELECT id FROM issues")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    IssueService::mute(&db.pool, issue_id).await.unwrap();
    // Reaches the count while muted, then the server stops before deleting it.
    let second = uuid::Uuid::new_v4().simple().to_string();
    digest_id(&db, project.id, "production", &second).await;
    IssueService::unmute(&db.pool, issue_id).await.unwrap();

    digest_id(&db, project.id, "production", &second).await;
    actix_web::rt::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(issues_opened(&github), 0);
}

#[actix_web::test]
async fn a_replayed_first_event_stored_before_counts_were_kept_opens_the_github_issue() {
    let db = TestDb::new().await;
    let github = FakeGitHub::start().await;
    let project = project(&db).await;
    connect(&db, &github, project.id).await;

    let first = uuid::Uuid::new_v4().simple().to_string();
    digest_id(&db, project.id, "production", &first).await;
    digest(&db, project.id, "production").await;
    sqlx::query("UPDATE events SET issue_event_count = NULL")
        .execute(&db.pool)
        .await
        .unwrap();
    auto_create(&db, project.id, json!({})).await;

    digest_id(&db, project.id, "production", &first).await;
    github.wait_for("POST", "/repos/acme/web/issues").await;
}

// =============================================================================
// A person's GitHub login
// =============================================================================

#[actix_web::test]
async fn people_set_their_own_github_login() {
    let db = TestDb::new().await;
    let user = rustrak::services::UsersService::create_user_with_password_hash(
        &db.pool,
        "octo@acme.dev",
        "unused",
        rustrak::models::UserRole::Member,
    )
    .await
    .unwrap();
    let token = AuthTokenService::create_for_user(
        &db.pool,
        rustrak::models::CreateAuthToken { description: None },
        Some(user.id),
    )
    .await
    .unwrap()
    .token;
    let app = app!(db);

    let get = || {
        test::TestRequest::get()
            .uri("/api/github/me")
            .insert_header(bearer(&token))
            .to_request()
    };
    let body: Value = test::call_and_read_body_json(&app, get()).await;
    assert_eq!(body["login"], Value::Null);

    let req = test::TestRequest::put()
        .uri("/api/github/me")
        .insert_header(bearer(&token))
        .set_json(json!({ "login": "@octocat" }))
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(body["login"], "octocat");

    // One login, one person: inbound assignments must not have to guess.
    sqlx::query("UPDATE users SET github_login = NULL WHERE id = $1")
        .bind(user.id)
        .execute(&db.pool)
        .await
        .unwrap();
    let other = rustrak::services::UsersService::create_user_with_password_hash(
        &db.pool,
        "cat@acme.dev",
        "unused",
        rustrak::models::UserRole::Member,
    )
    .await
    .unwrap();
    sqlx::query("UPDATE users SET github_login = 'octocat' WHERE id = $1")
        .bind(other.id)
        .execute(&db.pool)
        .await
        .unwrap();
    let req = test::TestRequest::put()
        .uri("/api/github/me")
        .insert_header(bearer(&token))
        .set_json(json!({ "login": "OctoCat" }))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 409);

    let req = test::TestRequest::put()
        .uri("/api/github/me")
        .insert_header(bearer(&token))
        .set_json(json!({ "login": "not a login!" }))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 400);

    let req = test::TestRequest::put()
        .uri("/api/github/me")
        .insert_header(bearer(&token))
        .set_json(json!({ "login": null }))
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(body["login"], Value::Null);
}
