//! Integration tests for the Sentry management API subset (`/api/0/...`)
//! that GitLab's error tracking consumes. Shapes follow Sentry's serializers;
//! see `routes/sentry_api.rs` for the source references.

use crate::common::{null_sourcemap_provider, process_error_event, TestDb};
use actix_web::test;
use chrono::{DateTime, Utc};
use rustrak::config::RateLimitConfig;
use rustrak::db::DbPool;
use rustrak::ingest::EventMetadata;
use rustrak::models::{CreateAuthToken, CreateProject, CreateUserRequest, UserRole};
use rustrak::services::{
    AuthTokenService, EventService, IssueService, ProjectService, UsersService,
};
use serde_json::{json, Value};
use uuid::Uuid;

/// The dashboard is at `https://rustrak.example.com`.
fn config() -> rustrak::config::Config {
    config_at(Some("https://rustrak.example.com"))
}

fn config_at(public_url: Option<&str>) -> rustrak::config::Config {
    rustrak::config::Config {
        rate_limit: rustrak::config::RateLimitConfig {
            max_events_per_project_per_minute: 1000,
            max_events_per_project_per_hour: 10000,
            ..crate::common::rate_limit()
        },
        public_url: public_url.map(str::to_string),
        sourcemap_cache_bytes: 1024,
        max_chunk_size_bytes: 1024,
        session_cardinality_cap: 10,
        ..crate::common::config()
    }
}

async fn admin_token(pool: &DbPool) -> String {
    AuthTokenService::create(pool, CreateAuthToken::default())
        .await
        .expect("token")
        .token
}

/// A token for a non-admin user holding `role` on `project_id`.
async fn member_token(pool: &DbPool, project_id: i32, role: &str) -> String {
    let req = CreateUserRequest {
        email: format!(
            "member-{project_id}-{}@example.com",
            role.replace(' ', "-").to_lowercase()
        ),
        password: "password123".to_string(),
    };
    let user = UsersService::create_user(pool, &req, UserRole::Member)
        .await
        .expect("user");
    crate::common::grant_project_role(pool, project_id, user.id, role)
        .await
        .expect("member");
    AuthTokenService::create_for_user(pool, CreateAuthToken::default(), Some(user.id))
        .await
        .expect("token")
        .token
}

async fn project(pool: &DbPool, name: &str) -> rustrak::models::Project {
    ProjectService::create(
        pool,
        CreateProject {
            name: name.to_string(),
            slug: None,
            platform: Some("python".to_string()),
        },
    )
    .await
    .expect("project")
}

/// Digests an error event through the real pipeline and returns its issue.
async fn ingest(
    pool: &DbPool,
    project_id: i32,
    exc_type: &str,
    exc_value: &str,
    timestamp: DateTime<Utc>,
) -> rustrak::models::Issue {
    ingest_with(pool, project_id, exc_type, exc_value, timestamp, json!({})).await
}

/// [`ingest`], with `extra` merged over the event's top-level fields.
async fn ingest_with(
    pool: &DbPool,
    project_id: i32,
    exc_type: &str,
    exc_value: &str,
    timestamp: DateTime<Utc>,
    extra: Value,
) -> rustrak::models::Issue {
    let dir = tempfile::TempDir::new().unwrap();
    let event_id = Uuid::new_v4();
    let mut event = json!({
        "event_id": event_id.simple().to_string(),
        "timestamp": timestamp.timestamp() as f64,
        "platform": "python",
        "level": "error",
        "user": {"id": exc_value},
        "exception": {"values": [{
            "type": exc_type,
            "value": exc_value,
            "stacktrace": {"frames": [{
                "filename": "app.py",
                "abs_path": "/srv/app.py",
                "function": "main",
                "lineno": 10,
                "colno": 4,
                "in_app": true,
                "pre_context": ["def main():"],
                "context_line": "    raise ValueError()",
                "post_context": [""]
            }]}
        }]}
    });
    for (key, value) in extra.as_object().unwrap() {
        event[key] = value.clone();
    }
    crate::common::store_event(
        dir.path(),
        project_id,
        &event_id.simple().to_string(),
        &serde_json::to_vec(&event).unwrap(),
    )
    .await
    .unwrap();
    let metadata = EventMetadata {
        event_id: event_id.simple().to_string(),
        project_id,
        ingested_at: timestamp,
        remote_addr: None,
    };
    let limits = RateLimitConfig {
        max_events_per_minute: 1000,
        max_events_per_hour: 10000,
        max_events_per_project_per_minute: 1000,
        max_events_per_project_per_hour: 10000,
    };
    process_error_event(
        pool,
        &metadata,
        dir.path(),
        &limits,
        null_sourcemap_provider(),
    )
    .await
    .expect("digest");
    let issue_id = EventService::issue_id_for_event(pool, project_id, event_id)
        .await
        .unwrap()
        .expect("event has an issue");
    IssueService::get_by_id(pool, issue_id).await.unwrap()
}

async fn get_json(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    uri: &str,
    token: &str,
) -> (u16, Value) {
    let req = test::TestRequest::get()
        .uri(uri)
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let resp = test::call_service(app, req).await;
    let status = resp.status().as_u16();
    let body = test::read_body(resp).await;
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

#[actix_web::test]
async fn projects_lists_projects_with_their_organization() {
    let db = TestDb::new().await;
    let token = admin_token(&db.pool).await;
    let p = project(&db.pool, "Demo App").await;
    let app = crate::common::app_with_config(&db.pool, config()).await;

    let (status, body) = get_json(&app, "/api/0/projects/", &token).await;

    assert_eq!(status, 200);
    let item = &body.as_array().expect("bare list")[0];
    assert_eq!(item["id"], p.id.to_string());
    assert_eq!(item["slug"], "demo-app");
    assert_eq!(item["name"], "Demo App");
    assert_eq!(item["platform"], "python");
    assert_eq!(item["status"], "active");
    assert!(item["dateCreated"].is_string());
    assert_eq!(item["organization"]["id"], "1");
    assert_eq!(item["organization"]["slug"], "rustrak");
    assert_eq!(item["organization"]["name"], "Rustrak");
    assert_eq!(item["organization"]["status"]["id"], "active");
}

#[actix_web::test]
async fn projects_shows_a_member_only_their_projects() {
    let db = TestDb::new().await;
    let mine = project(&db.pool, "Mine").await;
    project(&db.pool, "Not Mine").await;
    let token = member_token(&db.pool, mine.id, "Viewer").await;
    let app = crate::common::app_with_config(&db.pool, config()).await;

    let (status, body) = get_json(&app, "/api/0/projects/", &token).await;

    assert_eq!(status, 200);
    let slugs: Vec<&str> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["slug"].as_str().unwrap())
        .collect();
    assert_eq!(slugs, vec!["mine"]);
}

#[actix_web::test]
async fn projects_paginate_with_per_page_and_the_link_header() {
    let db = TestDb::new().await;
    let token = admin_token(&db.pool).await;
    project(&db.pool, "First").await;
    project(&db.pool, "Second").await;
    let app = crate::common::app_with_config(&db.pool, config()).await;

    let page = |cursor: &str| {
        test::TestRequest::get()
            .uri(&format!("/api/0/projects/?per_page=1&cursor={cursor}"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request()
    };

    let first: Value = test::call_and_read_body_json(&app, page("1:0:0")).await;
    let resp = test::call_service(&app, page("1:1:0")).await;
    let link = resp
        .headers()
        .get("Link")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let second: Value = test::read_body_json(resp).await;

    let mut slugs = vec![first[0]["slug"].clone(), second[0]["slug"].clone()];
    slugs.sort_by_key(|s| s.to_string());
    assert_eq!(slugs, ["first", "second"]);
    assert!(
        link.contains(r#"rel="previous"; results="true"; cursor="1:0:1""#),
        "{link}"
    );
    assert!(
        link.contains(r#"rel="next"; results="false"; cursor="1:2:0""#),
        "{link}"
    );
}

#[actix_web::test]
async fn organizations_lists_the_single_synthetic_org() {
    let db = TestDb::new().await;
    let token = admin_token(&db.pool).await;
    let app = crate::common::app_with_config(&db.pool, config()).await;

    let (status, body) = get_json(&app, "/api/0/organizations/", &token).await;

    assert_eq!(status, 200);
    let orgs = body.as_array().expect("bare list");
    assert_eq!(orgs.len(), 1);
    assert_eq!(orgs[0]["slug"], "rustrak");
    assert_eq!(orgs[0]["id"], "1");
}

#[actix_web::test]
async fn issues_lists_unresolved_issues_in_sentry_shape() {
    let db = TestDb::new().await;
    let token = admin_token(&db.pool).await;
    let p = project(&db.pool, "Demo App").await;
    let issue = ingest(&db.pool, p.id, "ValueError", "boom", Utc::now()).await;
    let resolved = ingest(&db.pool, p.id, "KeyError", "gone", Utc::now()).await;
    IssueService::set_status(&db.pool, resolved.id, "resolved", None)
        .await
        .unwrap();
    let app = crate::common::app_with_config(&db.pool, config()).await;

    let (status, body) = get_json(&app, "/api/0/projects/rustrak/demo-app/issues/", &token).await;

    assert_eq!(status, 200);
    let items = body.as_array().expect("bare list");
    assert_eq!(items.len(), 1, "query defaults to is:unresolved");
    let item = &items[0];
    assert_eq!(item["id"], issue.numeric_id.to_string());
    assert_eq!(item["shortId"], "DEMO-APP-1");
    assert_eq!(item["title"], "ValueError: boom");
    assert_eq!(item["culprit"], issue.culprit);
    assert_eq!(item["level"], "error");
    assert_eq!(item["status"], "unresolved");
    assert_eq!(item["type"], "error");
    assert_eq!(item["metadata"]["type"], "ValueError");
    assert_eq!(item["metadata"]["value"], "boom");
    assert_eq!(item["count"], "1");
    assert_eq!(item["userCount"], 1);
    assert!(item["firstSeen"].is_string());
    assert!(item["lastSeen"].is_string());
    assert_eq!(item["project"]["id"], p.id.to_string());
    assert_eq!(item["project"]["slug"], "demo-app");
    let hourly = item["stats"]["24h"].as_array().expect("24h stats");
    assert_eq!(hourly.len(), 24);
    assert_eq!(
        hourly[23][1], 1,
        "this hour's event lands in the last bucket"
    );
}

#[actix_web::test]
async fn issues_query_filters_by_status_and_text() {
    let db = TestDb::new().await;
    let token = admin_token(&db.pool).await;
    let p = project(&db.pool, "Demo App").await;
    ingest(&db.pool, p.id, "ValueError", "boom", Utc::now()).await;
    let resolved = ingest(&db.pool, p.id, "KeyError", "gone", Utc::now()).await;
    IssueService::set_status(&db.pool, resolved.id, "resolved", None)
        .await
        .unwrap();
    let ignored = ingest(&db.pool, p.id, "OSError", "disk", Utc::now()).await;
    IssueService::set_status(&db.pool, ignored.id, "ignored", None)
        .await
        .unwrap();
    let app = crate::common::app_with_config(&db.pool, config()).await;
    let titles = |body: &Value| -> Vec<String> {
        let mut t: Vec<String> = body
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["title"].as_str().unwrap().to_string())
            .collect();
        t.sort();
        t
    };
    let base = "/api/0/projects/rustrak/demo-app/issues/";

    let (_, body) = get_json(&app, &format!("{base}?query=is:resolved"), &token).await;
    assert_eq!(titles(&body), ["KeyError: gone"]);
    let (_, body) = get_json(&app, &format!("{base}?query=is:ignored"), &token).await;
    assert_eq!(titles(&body), ["OSError: disk"]);
    let (_, body) = get_json(&app, &format!("{base}?query="), &token).await;
    assert_eq!(
        body.as_array().unwrap().len(),
        3,
        "an empty query lists every issue"
    );
    let (_, body) = get_json(&app, &format!("{base}?query=is:unresolved%20boom"), &token).await;
    assert_eq!(titles(&body), ["ValueError: boom"]);
    let (_, body) = get_json(&app, &format!("{base}?query=is:unresolved%20gone"), &token).await;
    assert!(body.as_array().unwrap().is_empty());

    let (status, _) = get_json(&app, &format!("{base}?query=is:bookmarked"), &token).await;
    assert_eq!(status, 400);
}

#[actix_web::test]
async fn issues_list_feedback_only_under_its_category() {
    let db = TestDb::new().await;
    let token = admin_token(&db.pool).await;
    let p = project(&db.pool, "Demo App").await;
    ingest(&db.pool, p.id, "ValueError", "boom", Utc::now()).await;
    let feedback = json!({"type": "feedback", "contexts": {"feedback": {"message": "Love it"}}});
    ingest_with(&db.pool, p.id, "", "", Utc::now(), feedback).await;
    let app = crate::common::app_with_config(&db.pool, config()).await;
    let base = "/api/0/projects/rustrak/demo-app/issues/";
    let titles = |body: &Value| -> Vec<String> {
        body.as_array()
            .unwrap()
            .iter()
            .map(|i| i["title"].as_str().unwrap().to_string())
            .collect()
    };

    // Sentry keeps feedback out of the default search (`in_default_search`).
    let (_, body) = get_json(&app, base, &token).await;
    assert_eq!(titles(&body), ["ValueError: boom"]);
    // Its own feedback UI asks with `issue.category:feedback`.
    let (_, body) = get_json(
        &app,
        &format!("{base}?query=is:unresolved%20issue.category:feedback"),
        &token,
    )
    .await;
    assert_eq!(titles(&body), ["User Feedback: Love it"]);
    assert_eq!(body[0]["issueCategory"], "feedback");
}

#[actix_web::test]
async fn issues_sort_follows_sentry_sort_keys() {
    let db = TestDb::new().await;
    let token = admin_token(&db.pool).await;
    let p = project(&db.pool, "Demo App").await;
    let old = ingest(
        &db.pool,
        p.id,
        "OldError",
        "x",
        Utc::now() - chrono::Duration::hours(2),
    )
    .await;
    ingest(
        &db.pool,
        p.id,
        "OldError",
        "x",
        Utc::now() - chrono::Duration::hours(2),
    )
    .await;
    let new = ingest(
        &db.pool,
        p.id,
        "NewError",
        "y",
        Utc::now() - chrono::Duration::hours(1),
    )
    .await;
    let app = crate::common::app_with_config(&db.pool, config()).await;
    let first = |body: &Value| body[0]["id"].as_str().unwrap().to_string();
    let base = "/api/0/projects/rustrak/demo-app/issues/";

    let (_, body) = get_json(&app, base, &token).await;
    assert_eq!(
        first(&body),
        new.numeric_id.to_string(),
        "date: last seen first"
    );
    let (_, body) = get_json(&app, &format!("{base}?sort=new"), &token).await;
    assert_eq!(
        first(&body),
        new.numeric_id.to_string(),
        "new: first seen first"
    );
    let (_, body) = get_json(&app, &format!("{base}?sort=freq"), &token).await;
    assert_eq!(
        first(&body),
        old.numeric_id.to_string(),
        "freq: most events first"
    );

    let (status, body) = get_json(&app, &format!("{base}?sort=user"), &token).await;
    assert_eq!(status, 400);
    assert!(body.to_string().contains("Sort key 'user' not supported."));
}

#[actix_web::test]
async fn issues_paginate_with_sentry_cursors_in_the_link_header() {
    let db = TestDb::new().await;
    let token = admin_token(&db.pool).await;
    let p = project(&db.pool, "Demo App").await;
    for n in 0..3 {
        ingest(&db.pool, p.id, &format!("E{n}"), "x", Utc::now()).await;
    }
    let app = crate::common::app_with_config(&db.pool, config()).await;
    let page = |uri: String| {
        test::TestRequest::get()
            .uri(&uri)
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request()
    };
    let base = "/api/0/projects/rustrak/demo-app/issues/?limit=2";

    let resp = test::call_service(&app, page(base.to_string())).await;
    let link = resp
        .headers()
        .get("Link")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body.as_array().unwrap().len(), 2);
    assert!(
        link.contains(r#"rel="previous"; results="false"; cursor="2:-1:1""#),
        "{link}"
    );
    assert!(
        link.contains(r#"rel="next"; results="true"; cursor="2:1:0""#),
        "{link}"
    );
    assert!(link.contains("/api/0/projects/rustrak/demo-app/issues/?limit=2&cursor=2:1:0>"));

    let resp = test::call_service(&app, page(format!("{base}&cursor=2:1:0"))).await;
    let link = resp
        .headers()
        .get("Link")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert!(
        link.contains(r#"rel="previous"; results="true"; cursor="2:0:1""#),
        "{link}"
    );
    assert!(
        link.contains(r#"rel="next"; results="false"; cursor="2:2:0""#),
        "{link}"
    );

    let (status, _) = get_json(&app, &format!("{base}&cursor=bogus"), &token).await;
    assert_eq!(status, 400);
}

#[actix_web::test]
async fn issues_stats_period_picks_the_series() {
    let db = TestDb::new().await;
    let token = admin_token(&db.pool).await;
    let p = project(&db.pool, "Demo App").await;
    ingest(
        &db.pool,
        p.id,
        "ValueError",
        "boom",
        Utc::now() - chrono::Duration::days(2),
    )
    .await;
    let app = crate::common::app_with_config(&db.pool, config()).await;
    let base = "/api/0/projects/rustrak/demo-app/issues/";

    let (_, body) = get_json(&app, &format!("{base}?statsPeriod=14d"), &token).await;
    let daily = body[0]["stats"]["14d"].as_array().unwrap();
    assert_eq!(daily.len(), 14);
    assert_eq!(
        daily[11][1], 1,
        "two days ago lands two buckets before today"
    );
    assert_eq!(
        daily[13][0].as_i64().unwrap() % 86_400,
        0,
        "buckets align to the day"
    );

    let (_, body) = get_json(&app, &format!("{base}?statsPeriod="), &token).await;
    assert!(body[0].get("stats").is_none());

    let (status, _) = get_json(&app, &format!("{base}?statsPeriod=7d"), &token).await;
    assert_eq!(status, 400);
}

#[actix_web::test]
async fn issue_details_adds_releases_and_both_stats_series() {
    let db = TestDb::new().await;
    let token = admin_token(&db.pool).await;
    let p = project(&db.pool, "Demo App").await;
    let issue = ingest(&db.pool, p.id, "ValueError", "boom", Utc::now()).await;
    let app = crate::common::app_with_config(&db.pool, config()).await;

    for uri in [
        format!("/api/0/issues/{}/", issue.numeric_id),
        format!("/api/0/groups/{}/", issue.numeric_id),
        format!("/api/0/organizations/rustrak/issues/{}/", issue.numeric_id),
        format!("/api/0/organizations/rustrak/groups/{}/", issue.numeric_id),
    ] {
        let (status, body) = get_json(&app, &uri, &token).await;
        assert_eq!(status, 200, "{uri}");
        assert_eq!(body["id"], issue.numeric_id.to_string());
        assert_eq!(body["title"], "ValueError: boom");
        assert_eq!(body["project"]["id"], p.id.to_string());
        assert_eq!(body["annotations"], json!([]));
        assert_eq!(body["userReportCount"], 0);
        assert_eq!(body["userCount"], 1);
        assert!(body["firstRelease"].is_null(), "the event named no release");
        assert_eq!(body["stats"]["24h"].as_array().unwrap().len(), 24);
        assert_eq!(body["stats"]["30d"].as_array().unwrap().len(), 30);
    }
}

#[actix_web::test]
async fn issue_details_404s_for_unknown_or_inaccessible_issues() {
    let db = TestDb::new().await;
    let p = project(&db.pool, "Demo App").await;
    let other = project(&db.pool, "Other").await;
    let issue = ingest(&db.pool, p.id, "ValueError", "boom", Utc::now()).await;
    let outsider = member_token(&db.pool, other.id, "Project Admin").await;
    let token = admin_token(&db.pool).await;
    let app = crate::common::app_with_config(&db.pool, config()).await;

    let (status, _) = get_json(
        &app,
        &format!("/api/0/issues/{}/", issue.numeric_id),
        &outsider,
    )
    .await;
    assert_eq!(status, 404);
    let (status, _) = get_json(
        &app,
        &format!("/api/0/issues/{}/", issue.numeric_id + 1),
        &token,
    )
    .await;
    assert_eq!(status, 404);
    let (status, _) = get_json(&app, &format!("/api/0/issues/{}/", issue.id), &token).await;
    assert_eq!(status, 404, "a UUID is not a Sentry issue id");
}

#[actix_web::test]
async fn issue_details_names_first_and_last_release() {
    let db = TestDb::new().await;
    let token = admin_token(&db.pool).await;
    let p = project(&db.pool, "Demo App").await;
    let release = |v: &str| json!({"release": v});
    ingest_with(
        &db.pool,
        p.id,
        "ValueError",
        "boom",
        Utc::now(),
        release("1.0.0"),
    )
    .await;
    let issue = ingest_with(
        &db.pool,
        p.id,
        "ValueError",
        "boom",
        Utc::now(),
        release("1.1.0"),
    )
    .await;
    let app = crate::common::app_with_config(&db.pool, config()).await;

    let (_, body) = get_json(
        &app,
        &format!("/api/0/issues/{}/", issue.numeric_id),
        &token,
    )
    .await;

    assert_eq!(
        body["firstRelease"],
        json!({"version": "1.0.0", "shortVersion": "1.0.0"})
    );
    assert_eq!(
        body["lastRelease"],
        json!({"version": "1.1.0", "shortVersion": "1.1.0"})
    );
}

#[actix_web::test]
async fn latest_event_carries_the_stack_trace_as_sentry_entries() {
    let db = TestDb::new().await;
    let token = admin_token(&db.pool).await;
    let p = project(&db.pool, "Demo App").await;
    ingest(
        &db.pool,
        p.id,
        "ValueError",
        "older",
        Utc::now() - chrono::Duration::hours(1),
    )
    .await;
    let issue = ingest_with(
        &db.pool,
        p.id,
        "ValueError",
        "older",
        Utc::now(),
        json!({"logentry": {"formatted": "it broke"}, "tags": {"env": "prod"}}),
    )
    .await;
    let app = crate::common::app_with_config(&db.pool, config()).await;

    let (status, body) = get_json(
        &app,
        &format!("/api/0/issues/{}/events/latest/", issue.numeric_id),
        &token,
    )
    .await;

    assert_eq!(status, 200);
    assert_eq!(body["groupID"], issue.numeric_id.to_string());
    assert_eq!(body["projectID"], p.id.to_string());
    assert_eq!(body["eventID"].as_str().unwrap().len(), 32, "dashless hex");
    assert_eq!(body["id"], body["eventID"]);
    assert!(body["dateReceived"].is_string());
    assert!(body["dateCreated"].is_string());
    assert_eq!(body["message"], "it broke");
    assert_eq!(body["tags"], json!([{"key": "env", "value": "prod"}]));

    // Message outranks exception (display scores 2050 and 2000).
    let entries = body["entries"].as_array().unwrap();
    assert_eq!(entries[0]["type"], "message");
    assert_eq!(entries[0]["data"]["formatted"], "it broke");
    assert_eq!(entries[1]["type"], "exception");
    let value = &entries[1]["data"]["values"][0];
    assert_eq!(value["type"], "ValueError");
    assert_eq!(value["value"], "older");
    let frame = &value["stacktrace"]["frames"][0];
    assert_eq!(frame["filename"], "app.py");
    assert_eq!(frame["absPath"], "/srv/app.py");
    assert_eq!(frame["function"], "main");
    assert_eq!(frame["lineNo"], 10);
    assert_eq!(frame["colNo"], 4);
    assert_eq!(frame["inApp"], true);
    assert_eq!(
        frame["context"],
        json!([[9, "def main():"], [10, "    raise ValueError()"], [11, ""]])
    );
    assert_eq!(value["stacktrace"]["hasSystemFrames"], true);
}

#[actix_web::test]
async fn latest_event_frame_without_source_has_an_empty_context() {
    let db = TestDb::new().await;
    let token = admin_token(&db.pool).await;
    let p = project(&db.pool, "Demo App").await;
    let issue = ingest_with(
        &db.pool,
        p.id,
        "ValueError",
        "x",
        Utc::now(),
        json!({"exception": {"values": [{"type": "ValueError", "value": "x",
            "stacktrace": {"frames": [{"function": "f", "lineno": 3}]}}]}}),
    )
    .await;
    let app = crate::common::app_with_config(&db.pool, config()).await;

    let (_, body) = get_json(
        &app,
        &format!("/api/0/groups/{}/events/latest/", issue.numeric_id),
        &token,
    )
    .await;

    let frame = &body["entries"][0]["data"]["values"][0]["stacktrace"]["frames"][0];
    assert_eq!(frame["context"], json!([]));
}

async fn put_status(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    uri: &str,
    token: &str,
    status: &str,
) -> (u16, Value) {
    let req = test::TestRequest::put()
        .uri(uri)
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(json!({ "status": status }))
        .to_request();
    let resp = test::call_service(app, req).await;
    let code = resp.status().as_u16();
    let body = test::read_body(resp).await;
    (code, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

#[actix_web::test]
async fn update_issue_writes_the_status_back() {
    let db = TestDb::new().await;
    let token = admin_token(&db.pool).await;
    let p = project(&db.pool, "Demo App").await;
    let issue = ingest(&db.pool, p.id, "ValueError", "boom", Utc::now()).await;
    let app = crate::common::app_with_config(&db.pool, config()).await;
    let uri = format!("/api/0/issues/{}/", issue.numeric_id);

    for (sent, stored) in [
        ("resolved", "resolved"),
        ("unresolved", "unresolved"),
        ("ignored", "ignored"),
        ("muted", "ignored"),
        ("resolvedInNextRelease", "resolved"),
    ] {
        let (status, body) = put_status(&app, &uri, &token, sent).await;
        assert_eq!(status, 200, "{sent}");
        assert_eq!(body["id"], issue.numeric_id.to_string());
        assert_eq!(body["status"], stored, "{sent}");
        let reloaded = IssueService::get_by_id(&db.pool, issue.id).await.unwrap();
        assert_eq!(reloaded.status, stored, "{sent}");
    }

    let (status, _) = put_status(&app, &uri, &token, "bogus").await;
    assert_eq!(status, 400);
}

#[actix_web::test]
async fn update_issue_needs_an_editor() {
    let db = TestDb::new().await;
    let p = project(&db.pool, "Demo App").await;
    let issue = ingest(&db.pool, p.id, "ValueError", "boom", Utc::now()).await;
    let viewer = member_token(&db.pool, p.id, "Viewer").await;
    let app = crate::common::app_with_config(&db.pool, config()).await;

    let (status, _) = put_status(
        &app,
        &format!("/api/0/issues/{}/", issue.numeric_id),
        &viewer,
        "resolved",
    )
    .await;

    assert_eq!(status, 403);
}

#[actix_web::test]
async fn repos_is_an_empty_list() {
    let db = TestDb::new().await;
    let token = admin_token(&db.pool).await;
    let app = crate::common::app_with_config(&db.pool, config()).await;

    let (status, body) = get_json(&app, "/api/0/organizations/rustrak/repos/", &token).await;

    assert_eq!(status, 200);
    assert_eq!(body, json!([]));
}

#[actix_web::test]
async fn issue_link_redirects_to_the_dashboard() {
    let db = TestDb::new().await;
    let token = admin_token(&db.pool).await;
    let p = project(&db.pool, "Demo App").await;
    let issue = ingest(&db.pool, p.id, "ValueError", "boom", Utc::now()).await;
    let app = crate::common::app_with_config(&db.pool, config()).await;
    let location = |resp: &actix_web::dev::ServiceResponse| {
        resp.headers()
            .get("Location")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string()
    };

    for uri in [
        format!("/rustrak/demo-app/issues/{}", issue.numeric_id),
        format!("/rustrak/demo-app/issues/{}/", issue.numeric_id),
    ] {
        let req = test::TestRequest::get()
            .uri(&uri)
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 302, "{uri}");
        assert_eq!(
            location(&resp),
            format!(
                "https://rustrak.example.com/projects/{}/issues/{}",
                p.id, issue.id
            )
        );
    }

    let req = test::TestRequest::get()
        .uri(&format!("/rustrak/demo-app/issues/{}", issue.numeric_id))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 302);
    assert_eq!(
        location(&resp),
        "https://rustrak.example.com/login",
        "no issue leaks to a stranger"
    );
}

/// Sentry builds the `Link` from its configured URL prefix
/// (`utils/http.py::absolute_uri`), never from the request's `Host`.
#[actix_web::test]
async fn link_header_uses_the_public_url_not_the_host_header() {
    let db = TestDb::new().await;
    let token = admin_token(&db.pool).await;
    project(&db.pool, "Demo App").await;
    let app = crate::common::app_with_config(&db.pool, config()).await;

    let req = test::TestRequest::get()
        .uri("/api/0/projects/rustrak/demo-app/issues/")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .insert_header(("Host", "evil.example"))
        .insert_header(("X-Forwarded-Host", "evil.example"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let link = resp.headers().get("Link").unwrap().to_str().unwrap();

    assert!(
        link.starts_with("<https://rustrak.example.com/api/0/projects/rustrak/demo-app/issues/?"),
        "{link}"
    );
    assert!(!link.contains("evil.example"), "{link}");
}

/// The cursor's page size wins over `limit`, so a cursor followed without
/// repeating `limit` neither skips nor repeats issues.
#[actix_web::test]
async fn cursor_carries_the_page_size() {
    let db = TestDb::new().await;
    let token = admin_token(&db.pool).await;
    let p = project(&db.pool, "Demo App").await;
    for n in 0..3 {
        ingest(&db.pool, p.id, &format!("E{n}"), "x", Utc::now()).await;
    }
    let app = crate::common::app_with_config(&db.pool, config()).await;

    let req = test::TestRequest::get()
        .uri("/api/0/projects/rustrak/demo-app/issues/?cursor=2:1:0")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let link = resp
        .headers()
        .get("Link")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let body: Value = test::read_body_json(resp).await;

    assert_eq!(
        body.as_array().unwrap().len(),
        1,
        "page 2 of size 2 holds the third issue"
    );
    assert!(link.contains(r#"cursor="2:0:1""#), "{link}");
    assert!(link.contains(r#"cursor="2:2:0""#), "{link}");
}

#[actix_web::test]
async fn out_of_range_cursors_are_rejected() {
    let db = TestDb::new().await;
    let token = admin_token(&db.pool).await;
    project(&db.pool, "Demo App").await;
    let app = crate::common::app_with_config(&db.pool, config()).await;

    for cursor in ["100:9223372036854775807:0", "-5:1:0", "0:1:0", "101:0:0"] {
        for base in [
            "/api/0/projects/rustrak/demo-app/issues/",
            "/api/0/projects/",
        ] {
            let (status, _) = get_json(&app, &format!("{base}?cursor={cursor}"), &token).await;
            assert_eq!(status, 400, "{base} cursor={cursor}");
        }
    }
}

/// Without a dashboard a path no route claims is a 404; the issue link
/// authenticates itself and must still reach its login redirect.
#[actix_web::test]
async fn issue_link_redirects_a_stranger_even_without_a_dashboard() {
    let db = TestDb::new().await;
    let p = project(&db.pool, "Demo App").await;
    let issue = ingest(&db.pool, p.id, "ValueError", "boom", Utc::now()).await;
    let app = crate::common::app_with_config(&db.pool, config()).await;

    let req = test::TestRequest::get()
        .uri(&format!("/rustrak/demo-app/issues/{}", issue.numeric_id))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 302);
    assert_eq!(
        resp.headers().get("Location").unwrap(),
        "https://rustrak.example.com/login"
    );

    let req = test::TestRequest::get()
        .uri("/rustrak/demo-app/other/1")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404, "only the issue link is routed");
}

/// An empty `cursor` is no cursor, as in Sentry (`if request.GET.get("cursor")`).
#[actix_web::test]
async fn an_empty_cursor_is_the_first_page() {
    let db = TestDb::new().await;
    let token = admin_token(&db.pool).await;
    let p = project(&db.pool, "Demo App").await;
    ingest(&db.pool, p.id, "ValueError", "boom", Utc::now()).await;
    let app = crate::common::app_with_config(&db.pool, config()).await;

    for uri in [
        "/api/0/projects/?cursor=",
        "/api/0/projects/rustrak/demo-app/issues/?cursor=",
    ] {
        let (status, body) = get_json(&app, uri, &token).await;
        assert_eq!(status, 200, "{uri}");
        assert_eq!(body.as_array().unwrap().len(), 1, "{uri}");
    }
}

/// Without `PUBLIC_URL` the server cannot know its public address, so the
/// `Link` names a path relative to the one requested.
#[actix_web::test]
async fn link_header_is_relative_without_a_public_url() {
    let db = TestDb::new().await;
    let token = admin_token(&db.pool).await;
    project(&db.pool, "Demo App").await;
    let app = crate::common::app_with_config(&db.pool, config_at(None)).await;

    let req = test::TestRequest::get()
        .uri("/api/0/projects/")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let link = resp.headers().get("Link").unwrap().to_str().unwrap();

    assert!(link.starts_with("</api/0/projects/?&cursor="), "{link}");
}

/// Rustrak's Sentry issue ids are digits; the spec says so, so the
/// generated client refuses anything else before sending it.
#[cfg(feature = "openapi")]
#[actix_web::test]
async fn the_spec_constrains_sentry_issue_ids_to_digits() {
    use utoipa::OpenApi;
    let spec = serde_json::to_value(rustrak::openapi::ApiDoc::openapi()).unwrap();
    let paths = spec["paths"].as_object().unwrap();
    let mut checked = 0;
    for (path, item) in paths
        .iter()
        .filter(|(p, _)| p.contains("{issue_id}") && !p.starts_with("/api/projects"))
    {
        for op in item.as_object().unwrap().values() {
            for param in op["parameters"].as_array().unwrap() {
                if param["name"] == "issue_id" {
                    assert_eq!(param["schema"]["pattern"], "^[0-9]+$", "{path}");
                    checked += 1;
                }
            }
        }
    }
    assert_eq!(checked, 4, "get, put and latest event, plus the issue link");
}

/// `ProjectGroupIndexEndpoint` takes one `?environment=`, or `environment:x`
/// in the query: the issues seen there, counted there (`group.py:906-918`,
/// `group_stream.py:206-229`). One nothing was seen in is an empty list.
#[actix_web::test]
async fn issues_filter_by_environment() {
    let db = TestDb::new().await;
    let token = admin_token(&db.pool).await;
    let p = project(&db.pool, "Demo App").await;
    let now = Utc::now();
    let env = |name: &str, user: &str| json!({"environment": name, "user": {"id": user}});
    let shared = ingest_with(&db.pool, p.id, "Shared", "boom", now, env("acme", "a")).await;
    ingest_with(&db.pool, p.id, "Shared", "boom", now, env("lanza", "b")).await;
    ingest_with(&db.pool, p.id, "Shared", "boom", now, env("lanza", "c")).await;
    ingest_with(&db.pool, p.id, "AcmeOnly", "boom", now, env("acme", "d")).await;
    let app = crate::common::app_with_config(&db.pool, config()).await;
    let base = "/api/0/projects/rustrak/demo-app/issues/";

    for uri in [
        format!("{base}?environment=lanza&statsPeriod=24h"),
        format!("{base}?query=environment:lanza&statsPeriod=24h"),
        format!("{base}?query=is:unresolved%20environment:lanza&statsPeriod=24h"),
    ] {
        let (status, body) = get_json(&app, &uri, &token).await;
        assert_eq!(status, 200, "{uri}");
        let issues = body.as_array().unwrap();
        assert_eq!(issues.len(), 1, "{uri}");
        assert_eq!(issues[0]["id"], shared.numeric_id.to_string());
        assert_eq!(issues[0]["count"], "2", "{uri}: counted in lanza");
        assert_eq!(issues[0]["userCount"], 2, "{uri}");
        let points = issues[0]["stats"]["24h"].as_array().unwrap();
        let total: i64 = points.iter().map(|p| p[1].as_i64().unwrap()).sum();
        assert_eq!(total, 2, "{uri}");
    }

    // The parameter and the query both hold for the issue's events.
    let (_, body) = get_json(
        &app,
        &format!("{base}?environment=acme&query=environment:lanza"),
        &token,
    )
    .await;
    assert_eq!(body, json!([]));

    let (status, body) = get_json(&app, &format!("{base}?environment=nowhere"), &token).await;
    assert_eq!(status, 200);
    assert_eq!(body, json!([]));

    // A quoted name is one name, spaces and all.
    ingest_with(&db.pool, p.id, "Spaced", "boom", now, env("Acme Corp", "e")).await;
    for query in ["environment:%22lanza%22", "environment:%22Acme%20Corp%22"] {
        let (status, body) = get_json(&app, &format!("{base}?query={query}"), &token).await;
        assert_eq!(status, 200, "{query}");
        assert_eq!(body.as_array().unwrap().len(), 1, "{query}");
    }

    // The rest of Sentry's grammar for it is not supported.
    for query in ["!environment:acme", "environment:[acme,lanza]"] {
        let (status, _) = get_json(&app, &format!("{base}?query={query}"), &token).await;
        assert_eq!(status, 400, "{query}");
    }
}
