//! The organization-level releases API sentry-cli drives (`releases new`,
//! `finalize`, `info`, `list`, `delete`, `deploys`, `set-commits`), shaped
//! like Sentry's (`organization_releases.py`, `organization_release_details.py`,
//! `release_deploys.py`). Rustrak has no organization: its slug is accepted
//! and ignored, and a release is one row per project.

use crate::common::{create_test_project, create_test_token, TestDb};
use actix_web::test;
use serde_json::{json, Value};

struct Setup {
    db: TestDb,
    token: String,
    web: rustrak::models::Project,
    api: rustrak::models::Project,
}

async fn setup() -> Setup {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let web = create_test_project(&db.pool, "Web").await;
    let api = create_test_project(&db.pool, "Api").await;
    Setup {
        db,
        token,
        web,
        api,
    }
}

/// Calls the app as sentry-cli does: Bearer token, JSON in, and a response
/// it only parses when its content type is `application/json`.
async fn call(s: &Setup, method: &str, uri: &str, body: Option<Value>) -> (u16, Value) {
    let app = crate::common::app(&s.db.pool).await;
    let mut req = match method {
        "POST" => test::TestRequest::post(),
        "PUT" => test::TestRequest::put(),
        "DELETE" => test::TestRequest::delete(),
        _ => test::TestRequest::get(),
    }
    .uri(uri)
    .insert_header(("Authorization", format!("Bearer {}", s.token)));
    if let Some(body) = body {
        req = req.set_json(body);
    }
    let resp = test::call_service(&app, req.to_request()).await;
    let status = resp.status().as_u16();
    let json_body = resp
        .headers()
        .get("content-type")
        .is_some_and(|v| v.to_str().unwrap_or("").starts_with("application/json"));
    let bytes = test::read_body(resp).await;
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        assert!(
            json_body,
            "{method} {uri} answered {status} without application/json"
        );
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, value)
}

fn slugs(release: &Value) -> Vec<String> {
    let mut slugs: Vec<String> = release["projects"]
        .as_array()
        .expect("projects")
        .iter()
        .map(|p| {
            assert!(
                p["name"].is_string(),
                "sentry-cli needs each project's name"
            );
            p["slug"].as_str().unwrap().to_string()
        })
        .collect();
    slugs.sort();
    slugs
}

/// `releases new`: one release per listed project, 201 then 208 when it
/// already existed, with `version`, `dateCreated` and `projects[{slug,name}]`,
/// the fields sentry-cli's `ReleaseInfo` requires.
#[actix_web::test]
async fn new_creates_the_release_in_each_project() {
    let s = setup().await;
    let body = json!({
        "version": "1.0.0",
        "projects": [s.web.slug, s.api.id],
        "dateStarted": "2026-11-23T10:00:00Z",
    });
    let (status, release) = call(
        &s,
        "POST",
        "/api/0/organizations/acme/releases/",
        Some(body.clone()),
    )
    .await;
    assert_eq!(status, 201, "{release}");
    assert_eq!(release["version"], "1.0.0");
    assert!(release["dateCreated"].is_string());
    assert_eq!(
        slugs(&release),
        vec![s.api.slug.clone(), s.web.slug.clone()]
    );

    let (status, _) = call(
        &s,
        "POST",
        "/api/0/organizations/acme/releases/",
        Some(body),
    )
    .await;
    assert_eq!(status, 208);
}

/// Sentry requires `projects`, and refuses a project the caller does not
/// have with `{"projects": ["Invalid project ids or slugs"]}`.
#[actix_web::test]
async fn new_refuses_missing_or_unknown_projects() {
    let s = setup().await;
    let uri = "/api/0/organizations/acme/releases/";
    let (status, _) = call(&s, "POST", uri, Some(json!({ "version": "1.0.0" }))).await;
    assert_eq!(status, 400);
    let (status, body) = call(
        &s,
        "POST",
        uri,
        Some(json!({ "version": "1.0.0", "projects": ["nope"] })),
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(body["projects"][0], "Invalid project ids or slugs");
    let (status, _) = call(
        &s,
        "POST",
        uri,
        Some(json!({ "version": "a/b", "projects": [s.web.slug] })),
    )
    .await;
    assert_eq!(status, 400, "an invalid version");
}

async fn new_release(s: &Setup, version: &str) {
    let (status, body) = call(
        s,
        "POST",
        "/api/0/organizations/acme/releases/",
        Some(json!({ "version": version, "projects": [s.web.slug, s.api.slug] })),
    )
    .await;
    assert_eq!(status, 201, "{body}");
}

/// `releases finalize`: `PUT .../releases/{version}/` with `dateReleased`
/// (and the `projects` sentry-cli adds, which Sentry ignores) releases it in
/// every project that has it.
#[actix_web::test]
async fn finalize_sets_the_release_date_everywhere() {
    let s = setup().await;
    new_release(&s, "1.0.0").await;
    let (status, release) = call(
        &s,
        "PUT",
        "/api/0/organizations/acme/releases/1.0.0/",
        Some(json!({ "dateReleased": "2026-11-23T12:00:00Z", "projects": [s.web.slug] })),
    )
    .await;
    assert_eq!(status, 200, "{release}");
    assert_eq!(release["dateReleased"], "2026-11-23T12:00:00Z");
    assert_eq!(slugs(&release).len(), 2);

    let (status, _) = call(
        &s,
        "PUT",
        "/api/0/organizations/acme/releases/9.9.9/",
        Some(json!({ "dateReleased": "2026-11-23T12:00:00Z" })),
    )
    .await;
    assert_eq!(status, 404);
}

/// `releases info`: the release, or 404 so sentry-cli says it does not exist.
#[actix_web::test]
async fn info_returns_the_release_or_404() {
    let s = setup().await;
    new_release(&s, "1.0.0").await;
    let (status, release) =
        call(&s, "GET", "/api/0/organizations/acme/releases/1.0.0/", None).await;
    assert_eq!(status, 200);
    assert_eq!(release["version"], "1.0.0");
    assert_eq!(slugs(&release).len(), 2);
    let (status, _) = call(&s, "GET", "/api/0/organizations/acme/releases/9.9.9/", None).await;
    assert_eq!(status, 404);
}

/// `releases list`: the organization's releases, or with a project its own
/// (`GET /api/0/projects/{org}/{project}/releases/`), newest first.
#[actix_web::test]
async fn list_returns_releases_newest_first() {
    let s = setup().await;
    new_release(&s, "1.0.0").await;
    let (status, body) = call(
        &s,
        "POST",
        "/api/0/organizations/acme/releases/",
        Some(json!({ "version": "2.0.0", "projects": [s.api.slug] })),
    )
    .await;
    assert_eq!(status, 201, "{body}");

    let versions = |list: &Value| -> Vec<String> {
        list.as_array()
            .expect("a list")
            .iter()
            .map(|r| r["version"].as_str().unwrap().to_string())
            .collect()
    };
    let (status, list) = call(&s, "GET", "/api/0/organizations/acme/releases/", None).await;
    assert_eq!(status, 200);
    assert_eq!(versions(&list), vec!["2.0.0", "1.0.0"]);

    let uri = format!("/api/0/projects/acme/{}/releases/", s.web.slug);
    let (status, list) = call(&s, "GET", &uri, None).await;
    assert_eq!(status, 200);
    assert_eq!(versions(&list), vec!["1.0.0"]);
}

/// `releases delete`: 204, and the release is gone from every project.
#[actix_web::test]
async fn delete_removes_the_release() {
    let s = setup().await;
    new_release(&s, "1.0.0").await;
    let uri = "/api/0/organizations/acme/releases/1.0.0/";
    let (status, _) = call(&s, "DELETE", uri, None).await;
    assert_eq!(status, 204);
    let (status, _) = call(&s, "GET", uri, None).await;
    assert_eq!(status, 404);
    let (status, _) = call(&s, "DELETE", uri, None).await;
    assert_eq!(status, 404, "sentry-cli reads 404 as \"did nothing\"");
}

/// Sentry's `Release.safe_delete` refuses a release an issue started in
/// (`first_release`) or that has health data, with a 400 `{detail}`.
#[actix_web::test]
async fn delete_refuses_a_release_in_use() {
    let s = setup().await;
    new_release(&s, "1.0.0").await;
    new_release(&s, "2.0.0").await;
    let fields = rustrak::services::grouping::DenormalizedFields {
        calculated_type: "Error".into(),
        calculated_value: "boom".into(),
        transaction: String::new(),
        last_frame_filename: String::new(),
        last_frame_module: String::new(),
        last_frame_function: String::new(),
        culprit: String::new(),
        logger: String::new(),
        release: String::new(),
    };
    let issue = rustrak::services::IssueService::create(
        &s.db.pool,
        s.web.id,
        chrono::Utc::now(),
        &fields,
        Some("error"),
        None,
    )
    .await
    .unwrap();
    sqlx::query("UPDATE issues SET first_release = '1.0.0' WHERE id = $1")
        .bind(issue.id)
        .execute(&s.db.pool)
        .await
        .unwrap();
    let (status, body) = call(
        &s,
        "DELETE",
        "/api/0/organizations/acme/releases/1.0.0/",
        None,
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(
        body["detail"],
        "This release is referenced by active issues and cannot be removed."
    );

    sqlx::query(
        "INSERT INTO session_counts (project_id, release, environment, bucket, total, errored, crashed, abnormal) \
         VALUES ($1, '2.0.0', '', '2026-11-23 10:00:00', 1, 0, 0, 0)",
    )
    .bind(s.api.id)
    .execute(&s.db.pool)
    .await
    .unwrap();
    let (status, body) = call(
        &s,
        "DELETE",
        "/api/0/organizations/acme/releases/2.0.0/",
        None,
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(
        body["detail"],
        "This release has health data and cannot be removed."
    );
}

/// `releases deploys new`: 201 with Sentry's deploy shape. `id` is a string
/// and `dateFinished` is always set (sentry-cli's `deploys list` unwraps it).
#[actix_web::test]
async fn a_deploy_is_recorded_and_listed() {
    let s = setup().await;
    new_release(&s, "1.0.0").await;
    let uri = "/api/0/organizations/acme/releases/1.0.0/deploys/";
    let (status, deploy) = call(
        &s,
        "POST",
        uri,
        Some(json!({
            "environment": "production", "name": null, "url": null,
            "dateStarted": null, "dateFinished": "2026-11-23T12:00:00Z",
        })),
    )
    .await;
    assert_eq!(status, 201, "{deploy}");
    assert!(deploy["id"].is_string());
    assert_eq!(deploy["environment"], "production");
    assert_eq!(deploy["dateFinished"], "2026-11-23T12:00:00Z");

    let (status, deploy) = call(
        &s,
        "POST",
        uri,
        Some(json!({ "environment": "staging", "projects": [s.web.slug] })),
    )
    .await;
    assert_eq!(status, 201, "{deploy}");
    assert!(deploy["dateFinished"].is_string(), "defaults to now");

    let (status, list) = call(&s, "GET", uri, None).await;
    assert_eq!(status, 200);
    let mut seen: Vec<String> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["environment"].as_str().unwrap().to_string())
        .collect();
    seen.sort();
    // production in both projects, staging in one: the latest per project
    // and environment.
    assert_eq!(seen, vec!["production", "production", "staging"]);
}

/// Sentry requires `environment` (at most 64 characters, no `/` or line
/// breaks), and a deploy belongs to a release that exists.
#[actix_web::test]
async fn a_deploy_needs_a_valid_environment_and_release() {
    let s = setup().await;
    new_release(&s, "1.0.0").await;
    let uri = "/api/0/organizations/acme/releases/1.0.0/deploys/";
    for body in [
        json!({}),
        json!({ "environment": "" }),
        json!({ "environment": "a/b" }),
    ] {
        let (status, _) = call(&s, "POST", uri, Some(body.clone())).await;
        assert_eq!(status, 400, "{body}");
    }
    let (status, _) = call(
        &s,
        "POST",
        "/api/0/organizations/acme/releases/9.9.9/deploys/",
        Some(json!({ "environment": "production" })),
    )
    .await;
    assert_eq!(status, 404);
}

fn commit(id: &str, message: &str) -> Value {
    json!({
        "id": id, "repository": "acme/web", "message": message,
        "author_name": "Ada", "author_email": "ada@example.com",
        "timestamp": "2026-11-23T09:00:00Z",
        "patch_set": [{ "path": "src/main.rs", "type": "M" }],
    })
}

async fn commit_ids(s: &Setup, version: &str) -> Vec<String> {
    let uri = format!("/api/0/organizations/acme/releases/{version}/commits/");
    let (status, list) = call(s, "GET", &uri, None).await;
    assert_eq!(status, 200, "{list}");
    list.as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap().to_string())
        .collect()
}

/// `releases set-commits` with no repository integration (what Rustrak's
/// `repos` answers): the commits sentry-cli read from git, newest first,
/// replace the release's. The first is its `lastCommit`, as in Sentry's
/// `set_commits`.
#[actix_web::test]
async fn set_commits_replaces_the_release_commits() {
    let s = setup().await;
    new_release(&s, "1.0.0").await;
    let uri = "/api/0/organizations/acme/releases/1.0.0/";
    let (status, release) = call(
        &s,
        "PUT",
        uri,
        Some(json!({ "commits": [commit("bbb", "second"), commit("aaa", "first")] })),
    )
    .await;
    assert_eq!(status, 200, "{release}");
    assert_eq!(release["lastCommit"]["id"], "bbb");
    assert_eq!(release["commitCount"], 2);
    assert_eq!(commit_ids(&s, "1.0.0").await, vec!["bbb", "aaa"]);

    let (status, _) = call(
        &s,
        "PUT",
        uri,
        Some(json!({ "commits": [commit("ccc", "third")] })),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(commit_ids(&s, "1.0.0").await, vec!["ccc"]);

    let (status, _) = call(&s, "PUT", uri, Some(json!({ "refs": [] }))).await;
    assert_eq!(status, 200);
    assert!(
        commit_ids(&s, "1.0.0").await.is_empty(),
        "refs: [] clears them"
    );
}

/// Refs need a repository integration Rustrak does not have: Sentry's
/// `InvalidRepository` answer.
#[actix_web::test]
async fn refs_to_an_unknown_repository_are_refused() {
    let s = setup().await;
    new_release(&s, "1.0.0").await;
    let (status, body) = call(
        &s,
        "PUT",
        "/api/0/organizations/acme/releases/1.0.0/",
        Some(json!({ "refs": [{ "repository": "acme/web", "commit": "abc", "previousCommit": null }] })),
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(body["refs"][0], "Invalid repository names: acme/web");
}

/// `set-commits` asks for the previous release with commits to know where
/// to start reading git: `{}` when there is none.
#[actix_web::test]
async fn previous_with_commits_finds_the_last_release_with_commits() {
    let s = setup().await;
    new_release(&s, "1.0.0").await;
    let (status, _) = call(
        &s,
        "PUT",
        "/api/0/organizations/acme/releases/1.0.0/",
        Some(json!({ "commits": [commit("aaa", "first")] })),
    )
    .await;
    assert_eq!(status, 200);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    new_release(&s, "2.0.0").await;

    let (status, previous) = call(
        &s,
        "GET",
        "/api/0/organizations/acme/releases/2.0.0/previous-with-commits/",
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(previous["version"], "1.0.0");
    assert_eq!(previous["lastCommit"]["id"], "aaa");

    let (status, none) = call(
        &s,
        "GET",
        "/api/0/organizations/acme/releases/1.0.0/previous-with-commits/",
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(none, json!({}));
}
