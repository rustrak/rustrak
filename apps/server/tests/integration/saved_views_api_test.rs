//! Saved views (#531): a page's address saved under a name, personal or
//! shared with the project, and the one each page opens on by default.

use crate::common::{app, create_test_project, grant_project_role, seed_user, token_for, TestDb};
use actix_web::test;
use rustrak::models::UserRole;
use serde_json::{json, Value};

struct Team {
    db: TestDb,
    project: i32,
    alice: String,
    bob: String,
}

/// Two members of one project, Alice an Editor and Bob a Viewer.
async fn team() -> Team {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "web").await.id;
    let alice = seed_user(&db.pool, "alice@example.com", UserRole::Member).await;
    let bob = seed_user(&db.pool, "bob@example.com", UserRole::Member).await;
    grant_project_role(&db.pool, project, alice.id, "Editor")
        .await
        .unwrap();
    grant_project_role(&db.pool, project, bob.id, "Viewer")
        .await
        .unwrap();
    Team {
        alice: token_for(&db.pool, alice.id).await,
        bob: token_for(&db.pool, bob.id).await,
        project,
        db,
    }
}

async fn call(t: &Team, req: test::TestRequest, token: &str) -> (u16, Value) {
    let app = app(&t.db.pool).await;
    let resp = test::call_service(
        &app,
        req.insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    let status = resp.status().as_u16();
    let bytes = test::read_body(resp).await;
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn create(t: &Team, token: &str, body: Value) -> (u16, Value) {
    let uri = format!("/api/projects/{}/views", t.project);
    call(t, test::TestRequest::post().uri(&uri).set_json(body), token).await
}

async fn list(t: &Team, token: &str, page: &str) -> Vec<Value> {
    let uri = format!("/api/projects/{}/views?page={page}", t.project);
    let (status, body) = call(t, test::TestRequest::get().uri(&uri), token).await;
    assert_eq!(status, 200, "{body}");
    body.as_array().unwrap().clone()
}

fn names(views: &[Value]) -> Vec<&str> {
    views.iter().map(|v| v["name"].as_str().unwrap()).collect()
}

#[actix_web::test]
async fn a_personal_view_is_seen_only_by_its_creator() {
    let t = team().await;
    let (status, view) = create(
        &t,
        &t.alice,
        json!({"page": "issues", "name": "Mine in prod", "query": "q=assignee:me&environment=prod"}),
    )
    .await;
    assert_eq!(status, 201, "{view}");
    assert_eq!(view["visibility"], "owner");
    assert_eq!(view["query"], "q=assignee:me&environment=prod");

    assert_eq!(names(&list(&t, &t.alice, "issues").await), ["Mine in prod"]);
    assert!(list(&t, &t.bob, "issues").await.is_empty());
}

#[actix_web::test]
async fn a_shared_view_is_seen_by_everyone_on_the_project_and_only_on_its_page() {
    let t = team().await;
    let (status, view) = create(
        &t,
        &t.alice,
        json!({"page": "issues", "name": "Fatal", "query": "q=level:fatal", "visibility": "project"}),
    )
    .await;
    assert_eq!(status, 201, "{view}");
    assert_eq!(view["visibility"], "project");

    assert_eq!(names(&list(&t, &t.bob, "issues").await), ["Fatal"]);
    assert!(list(&t, &t.bob, "logs").await.is_empty());
}

#[actix_web::test]
async fn a_view_needs_a_name_and_a_page() {
    let t = team().await;
    let (status, body) = create(
        &t,
        &t.alice,
        json!({"page": "issues", "name": "  ", "query": "q=level:fatal"}),
    )
    .await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(body["error"]["fields"][0]["field"], "name");

    let (status, body) = create(&t, &t.alice, json!({"page": "", "name": "x", "query": ""})).await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(body["error"]["fields"][0]["field"], "page");

    let long = "q".repeat(4097);
    let (status, body) = create(
        &t,
        &t.alice,
        json!({"page": "issues", "name": "x", "query": long}),
    )
    .await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(body["error"]["fields"][0]["field"], "query");
    assert_eq!(body["error"]["fields"][0]["code"], "too_long");
}

#[actix_web::test]
async fn someone_outside_the_project_cannot_tell_it_exists() {
    let t = team().await;
    let carol = seed_user(&t.db.pool, "carol@example.com", UserRole::Member).await;
    let carol = token_for(&t.db.pool, carol.id).await;
    let (status, _) = create(
        &t,
        &carol,
        json!({"page": "issues", "name": "x", "query": ""}),
    )
    .await;
    assert_eq!(status, 404);
}

async fn update(t: &Team, token: &str, id: &Value, body: Value) -> (u16, Value) {
    let uri = format!("/api/projects/{}/views/{id}", t.project);
    call(
        t,
        test::TestRequest::patch().uri(&uri).set_json(body),
        token,
    )
    .await
}

async fn delete(t: &Team, token: &str, id: &Value) -> u16 {
    let uri = format!("/api/projects/{}/views/{id}", t.project);
    call(t, test::TestRequest::delete().uri(&uri), token)
        .await
        .0
}

#[actix_web::test]
async fn a_shared_view_is_changed_by_its_creator_or_a_project_admin() {
    let t = team().await;
    let (_, view) = create(
        &t,
        &t.alice,
        json!({"page": "issues", "name": "Fatal", "query": "q=level:fatal", "visibility": "project"}),
    )
    .await;
    let id = &view["id"];

    assert_eq!(
        update(&t, &t.bob, id, json!({"name": "Mine now"})).await.0,
        403
    );
    assert_eq!(delete(&t, &t.bob, id).await, 403);

    let (status, changed) = update(&t, &t.alice, id, json!({"query": "q=level:error"})).await;
    assert_eq!(status, 200, "{changed}");
    assert_eq!(changed["query"], "q=level:error");
    assert_eq!(changed["name"], "Fatal");

    let dave = seed_user(&t.db.pool, "dave@example.com", UserRole::Member).await;
    grant_project_role(&t.db.pool, t.project, dave.id, "Project Admin")
        .await
        .unwrap();
    let dave = token_for(&t.db.pool, dave.id).await;
    assert_eq!(delete(&t, &dave, id).await, 204);
    assert!(list(&t, &t.alice, "issues").await.is_empty());
}

#[actix_web::test]
async fn someone_elses_personal_view_does_not_exist_for_you() {
    let t = team().await;
    let (_, view) = create(
        &t,
        &t.alice,
        json!({"page": "issues", "name": "Mine", "query": ""}),
    )
    .await;
    assert_eq!(
        update(&t, &t.bob, &view["id"], json!({"name": "x"}))
            .await
            .0,
        404
    );
    assert_eq!(delete(&t, &t.bob, &view["id"]).await, 404);
    assert_eq!(delete(&t, &t.alice, &view["id"]).await, 204);
}

async fn set_default(t: &Team, token: &str, body: Value) -> (u16, Value) {
    let uri = format!("/api/projects/{}/views/default", t.project);
    call(t, test::TestRequest::put().uri(&uri).set_json(body), token).await
}

/// `(name, is_my_default, is_project_default)` of each view `token` sees.
async fn defaults(t: &Team, token: &str) -> Vec<(String, bool, bool)> {
    list(t, token, "issues")
        .await
        .iter()
        .map(|v| {
            (
                v["name"].as_str().unwrap().to_string(),
                v["is_my_default"].as_bool().unwrap(),
                v["is_project_default"].as_bool().unwrap(),
            )
        })
        .collect()
}

fn row(name: &str, mine: bool, project: bool) -> (String, bool, bool) {
    (name.to_string(), mine, project)
}

#[actix_web::test]
async fn a_page_opens_on_my_default_or_else_the_projects() {
    let t = team().await;
    let (_, fatal) = create(
        &t,
        &t.alice,
        json!({"page": "issues", "name": "Fatal", "query": "q=level:fatal", "visibility": "project"}),
    )
    .await;
    let (_, mine) = create(
        &t,
        &t.alice,
        json!({"page": "issues", "name": "Mine", "query": ""}),
    )
    .await;

    // The project's default is an admin's call, and only a shared view.
    let project_default = json!({"page": "issues", "view_id": fatal["id"], "scope": "project"});
    assert_eq!(
        set_default(&t, &t.bob, project_default.clone()).await.0,
        403
    );
    let dave = seed_user(&t.db.pool, "dave@example.com", UserRole::Member).await;
    grant_project_role(&t.db.pool, t.project, dave.id, "Project Admin")
        .await
        .unwrap();
    let dave = token_for(&t.db.pool, dave.id).await;
    assert_eq!(set_default(&t, &dave, project_default).await.0, 204);
    let (status, body) = set_default(
        &t,
        &t.alice,
        json!({"page": "issues", "view_id": mine["id"], "scope": "project"}),
    )
    .await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(body["error"]["fields"][0]["field"], "view_id");

    // Anyone picks their own, which only they see.
    let (status, body) = set_default(
        &t,
        &t.alice,
        json!({"page": "issues", "view_id": mine["id"], "scope": "user"}),
    )
    .await;
    assert_eq!(status, 204, "{body}");
    assert_eq!(
        defaults(&t, &t.alice).await,
        [row("Fatal", false, true), row("Mine", true, false)]
    );
    assert_eq!(defaults(&t, &t.bob).await, [row("Fatal", false, true)]);

    // And clears it.
    let (status, _) = set_default(
        &t,
        &t.alice,
        json!({"page": "issues", "view_id": null, "scope": "user"}),
    )
    .await;
    assert_eq!(status, 204);
    assert_eq!(
        defaults(&t, &t.alice).await,
        [row("Fatal", false, true), row("Mine", false, false)]
    );
}

#[actix_web::test]
async fn a_view_made_personal_stops_being_the_projects_default() {
    let t = team().await;
    let (_, fatal) = create(
        &t,
        &t.alice,
        json!({"page": "issues", "name": "Fatal", "query": "", "visibility": "project"}),
    )
    .await;
    let body = json!({"page": "issues", "view_id": fatal["id"], "scope": "project"});
    assert_eq!(set_default(&t, &t.alice, body).await.0, 204);

    update(&t, &t.alice, &fatal["id"], json!({"visibility": "owner"})).await;
    assert_eq!(defaults(&t, &t.alice).await, [row("Fatal", false, false)]);
}

// Workspace views: the pages that span every project (the global issues
// list) keep theirs under /api/views, with no project.

async fn ws(t: &Team, token: &str, req: test::TestRequest) -> (u16, Value) {
    call(t, req, token).await
}

async fn ws_list(t: &Team, token: &str) -> Vec<String> {
    let (status, body) = ws(
        t,
        token,
        test::TestRequest::get().uri("/api/views?page=issues"),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    body.as_array()
        .unwrap()
        .iter()
        .map(|v| v["name"].as_str().unwrap().to_string())
        .collect()
}

#[actix_web::test]
async fn workspace_views_are_kept_apart_from_any_projects() {
    let t = team().await;
    create(
        &t,
        &t.alice,
        json!({"page": "issues", "name": "In web", "query": ""}),
    )
    .await;
    let (status, view) = ws(
        &t,
        &t.alice,
        test::TestRequest::post()
            .uri("/api/views")
            .set_json(json!({"page": "issues", "name": "Everywhere", "query": "project=1"})),
    )
    .await;
    assert_eq!(status, 201, "{view}");
    assert_eq!(view["project_id"], Value::Null);

    assert_eq!(ws_list(&t, &t.alice).await, ["Everywhere"]);
    assert_eq!(names(&list(&t, &t.alice, "issues").await), ["In web"]);
    assert!(ws_list(&t, &t.bob).await.is_empty());
}

#[actix_web::test]
async fn a_shared_workspace_view_is_changed_by_its_creator_or_an_owner() {
    let t = team().await;
    let (_, view) = ws(
        &t,
        &t.alice,
        test::TestRequest::post().uri("/api/views").set_json(
            json!({"page": "issues", "name": "Team", "query": "q=x", "visibility": "project"}),
        ),
    )
    .await;
    let uri = format!("/api/views/{}", view["id"]);
    assert_eq!(ws_list(&t, &t.bob).await, ["Team"]);

    let (status, _) = ws(&t, &t.bob, test::TestRequest::delete().uri(&uri)).await;
    assert_eq!(status, 403);
    // Alice may update her project, which says nothing about the workspace.
    let default = json!({"page": "issues", "view_id": view["id"], "scope": "project"});
    let put_default = || {
        test::TestRequest::put()
            .uri("/api/views/default")
            .set_json(default.clone())
    };
    assert_eq!(ws(&t, &t.alice, put_default()).await.0, 403);

    let owner = seed_user(&t.db.pool, "owner@example.com", UserRole::Admin).await;
    let owner = token_for(&t.db.pool, owner.id).await;
    assert_eq!(ws(&t, &owner, put_default()).await.0, 204);
    let (status, body) = ws(
        &t,
        &t.bob,
        test::TestRequest::get().uri("/api/views?page=issues"),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body[0]["is_project_default"], true);
    assert_eq!(
        ws(&t, &owner, test::TestRequest::delete().uri(&uri))
            .await
            .0,
        204
    );
}

#[actix_web::test]
async fn a_workspace_default_is_mine_alone() {
    let t = team().await;
    let (_, view) = ws(
        &t,
        &t.alice,
        test::TestRequest::post()
            .uri("/api/views")
            .set_json(json!({"page": "issues", "name": "Mine", "query": "q=x"})),
    )
    .await;
    let set = |id: &Value| {
        test::TestRequest::put()
            .uri("/api/views/default")
            .set_json(json!({"page": "issues", "view_id": id, "scope": "user"}))
    };
    assert_eq!(ws(&t, &t.alice, set(&view["id"])).await.0, 204);
    let (_, body) = ws(
        &t,
        &t.alice,
        test::TestRequest::get().uri("/api/views?page=issues"),
    )
    .await;
    assert_eq!(body[0]["is_my_default"], true);
    // Picking it again, or clearing it, replaces rather than piles up.
    assert_eq!(ws(&t, &t.alice, set(&view["id"])).await.0, 204);
    assert_eq!(ws(&t, &t.alice, set(&Value::Null)).await.0, 204);
    let (_, body) = ws(
        &t,
        &t.alice,
        test::TestRequest::get().uri("/api/views?page=issues"),
    )
    .await;
    assert_eq!(body[0]["is_my_default"], false);
}
