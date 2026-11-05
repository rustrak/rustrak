//! The roles, assignments and project-members API.

use crate::common::TestDb;
use crate::integration::team_rbac_test::create_test_config;
use actix_web::{test, web, App};
use rustrak::auth::rbac::{Action, Resource};
use rustrak::db::DbPool;
use rustrak::models::{CreateAuthToken, CreateProject, CreateUserRequest, Permission, UserRole};
use rustrak::routes;
use rustrak::services::{AuthTokenService, ProjectService, RoleService, UsersService};
use serde_json::{json, Value};

macro_rules! app {
    ($pool:expr) => {
        test::init_service(
            App::new()
                .app_data(web::Data::new($pool))
                .app_data(web::Data::new(create_test_config()))
                .configure(routes::roles::configure)
                .configure(routes::members::configure)
                .configure(routes::invitations::configure)
                .configure(routes::team::configure)
                .configure(routes::issues::configure)
                .configure(routes::projects::configure),
        )
        .await
    };
}

macro_rules! call {
    ($app:expr, $method:ident, $uri:expr, $token:expr) => {
        call!($app, $method, $uri, $token, Value::Null)
    };
    ($app:expr, $method:ident, $uri:expr, $token:expr, $body:expr) => {{
        let mut req = test::TestRequest::$method()
            .uri(AsRef::<str>::as_ref(&$uri))
            .insert_header(("Authorization", format!("Bearer {}", $token)));
        if $body != Value::Null {
            req = req.set_json($body);
        }
        let resp = test::call_service(&$app, req.to_request()).await;
        let status = resp.status().as_u16();
        let body: Value =
            serde_json::from_slice(&test::read_body(resp).await).unwrap_or(Value::Null);
        (status, body)
    }};
}

struct Who {
    id: i32,
    token: String,
}

async fn who(pool: &DbPool, email: &str, owner: bool) -> Who {
    let user = UsersService::create_user(
        pool,
        &CreateUserRequest {
            email: email.into(),
            password: "password123".into(),
        },
        if owner {
            UserRole::Admin
        } else {
            UserRole::Member
        },
    )
    .await
    .unwrap();
    let token = AuthTokenService::create_for_user(pool, CreateAuthToken::default(), Some(user.id))
        .await
        .unwrap()
        .token;
    Who { id: user.id, token }
}

async fn role(pool: &DbPool, name: &str, perms: &[(Resource, Action)]) -> i32 {
    RoleService::create(
        pool,
        name,
        "",
        perms
            .iter()
            .map(|&(resource, action)| Permission { resource, action })
            .collect(),
    )
    .await
    .unwrap()
    .id
}

async fn seeded(pool: &DbPool, name: &str) -> i32 {
    crate::common::ensure_role(pool, name).await
}

async fn project(pool: &DbPool) -> i32 {
    ProjectService::create(
        pool,
        CreateProject {
            name: "p".into(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap()
    .id
}

#[actix_web::test]
async fn the_catalog_lists_every_resource_with_its_module_scope_and_actions() {
    let db = TestDb::new().await;
    let u = who(&db.pool, "u@x.com", false).await;
    let app = app!(db.pool.clone());
    let (status, body) = call!(app, get, "/api/access/catalog", u.token);
    assert_eq!(status, 200);
    let storage = body["resources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == "storage")
        .unwrap();
    assert_eq!(storage["module"], "settings");
    assert_eq!(storage["scope"], "instance");
    assert_eq!(storage["actions"], json!(["read", "update", "delete"]));
}

#[actix_web::test]
async fn my_access_reports_what_i_can_do_where() {
    let db = TestDb::new().await;
    let u = who(&db.pool, "u@x.com", false).await;
    let p = project(&db.pool).await;
    let r = role(&db.pool, "logs", &[(Resource::Logs, Action::Read)]).await;
    RoleService::assign(&db.pool, u.id, r, Some(p))
        .await
        .unwrap();
    let app = app!(db.pool.clone());

    let (status, body) = call!(app, get, "/api/access/me", u.token);
    assert_eq!(status, 200);
    assert_eq!(body["is_owner"], false);
    assert_eq!(body["instance"], json!([]));
    assert_eq!(
        body["projects"],
        json!([{ "project_id": p, "permissions": [{ "resource": "logs", "action": "read" }] }])
    );
}

#[actix_web::test]
async fn roles_are_managed_with_the_roles_permission_and_never_beyond_your_own() {
    let db = TestDb::new().await;
    let editor_of_roles = who(&db.pool, "r@x.com", false).await;
    let r = role(
        &db.pool,
        "role manager",
        &[
            (Resource::Roles, Action::Read),
            (Resource::Roles, Action::Create),
            (Resource::Roles, Action::Update),
            (Resource::Roles, Action::Delete),
        ],
    )
    .await;
    let reads = role(
        &db.pool,
        "issue reader",
        &[(Resource::Issues, Action::Read)],
    )
    .await;
    for held in [r, reads] {
        RoleService::assign(&db.pool, editor_of_roles.id, held, None)
            .await
            .unwrap();
    }
    let nobody = who(&db.pool, "n@x.com", false).await;
    let app = app!(db.pool.clone());

    let (status, _) = call!(app, get, "/api/roles", nobody.token);
    assert_eq!(status, 403);

    let new = json!({ "name": "Readers", "description": "", "permissions": [{ "resource": "issues", "action": "read" }] });
    let (status, body) = call!(app, post, "/api/roles", editor_of_roles.token, new);
    assert_eq!(status, 201, "{body}");
    let id = body["id"].as_i64().unwrap();

    let escalate = json!({ "name": "Readers", "description": "", "permissions": [{ "resource": "issues", "action": "delete" }] });
    let (status, _) = call!(
        app,
        put,
        format!("/api/roles/{id}"),
        editor_of_roles.token,
        escalate
    );
    assert_eq!(status, 403, "cannot write a permission you do not hold");

    let (status, body) = call!(app, get, "/api/roles", editor_of_roles.token);
    assert_eq!(status, 200);
    assert!(body
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["name"] == "Readers"));

    let (status, _) = call!(
        app,
        delete,
        format!("/api/roles/{id}"),
        editor_of_roles.token
    );
    assert_eq!(status, 204);
}

#[actix_web::test]
async fn a_role_on_all_projects_makes_you_a_member_of_each() {
    let db = TestDb::new().await;
    let owner = who(&db.pool, "o@x.com", true).await;
    let u = who(&db.pool, "u@x.com", false).await;
    let p = project(&db.pool).await;
    let viewer = seeded(&db.pool, "Viewer").await;
    RoleService::assign(&db.pool, u.id, viewer, None)
        .await
        .unwrap();
    let app = app!(db.pool.clone());

    let (status, body) = call!(app, get, format!("/api/projects/{p}/members"), owner.token);
    assert_eq!(status, 200);
    assert_eq!(body[0]["user_id"], u.id, "{body}");
    assert_eq!(
        body[0]["roles"],
        json!([{ "id": viewer, "name": "Viewer" }])
    );
}

#[actix_web::test]
async fn a_role_in_use_cannot_be_deleted() {
    let db = TestDb::new().await;
    let owner = who(&db.pool, "o@x.com", true).await;
    let u = who(&db.pool, "u@x.com", false).await;
    let r = role(&db.pool, "readers", &[(Resource::Issues, Action::Read)]).await;
    RoleService::assign(&db.pool, u.id, r, None).await.unwrap();
    let app = app!(db.pool.clone());
    let (status, _) = call!(app, delete, format!("/api/roles/{r}"), owner.token);
    assert_eq!(status, 409);
}

#[actix_web::test]
async fn a_mixed_role_on_a_project_holds_its_workspace_part_everywhere() {
    let db = TestDb::new().await;
    let owner = who(&db.pool, "o@x.com", true).await;
    let u = who(&db.pool, "u@x.com", false).await;
    let p = project(&db.pool).await;
    let app = app!(db.pool.clone());
    let new = json!({
        "name": "Support",

        "permissions": [
            { "resource": "issues", "action": "read" },
            { "resource": "storage", "action": "read" },
        ],
    });
    let (status, body) = call!(app, post, "/api/roles", owner.token, new);
    assert_eq!(status, 201, "{body}");
    let support = body["id"].as_i64().unwrap();

    let (status, body) = call!(
        app,
        put,
        format!("/api/users/{}/assignments", u.id),
        owner.token,
        json!({ "assignments": [{ "role_id": support, "project_id": p }] })
    );
    assert_eq!(status, 200, "{body}");
    let access = RoleService::access_for(&db.pool, u.id).await.unwrap();
    assert!(access.can(Resource::Storage, Action::Read, None));
    assert!(access.can(Resource::Issues, Action::Read, Some(p)));
    assert!(!access.can(Resource::Issues, Action::Read, Some(p + 1)));
}

#[actix_web::test]
async fn a_workspace_only_role_does_not_make_you_a_project_member() {
    let db = TestDb::new().await;
    let owner = who(&db.pool, "o@x.com", true).await;
    let u = who(&db.pool, "u@x.com", false).await;
    let p = project(&db.pool).await;
    let admin = seeded(&db.pool, "Workspace Admin").await;
    RoleService::assign(&db.pool, u.id, admin, None)
        .await
        .unwrap();
    let app = app!(db.pool.clone());
    let (status, body) = call!(app, get, format!("/api/projects/{p}/members"), owner.token);
    assert_eq!(status, 200);
    assert_eq!(body, json!([]));
}

#[actix_web::test]
async fn personal_permissions_belong_to_one_person_and_stay_out_of_the_roles_list() {
    let db = TestDb::new().await;
    let owner = who(&db.pool, "o@x.com", true).await;
    let u = who(&db.pool, "u@x.com", false).await;
    let other = who(&db.pool, "t@x.com", false).await;
    let p = project(&db.pool).await;
    let app = app!(db.pool.clone());
    let uri = format!("/api/users/{}/assignments", u.id);

    let (status, body) = call!(
        app,
        put,
        uri,
        owner.token,
        json!({ "assignments": [], "personal": {
            "permissions": [
                { "resource": "logs", "action": "read" },
                { "resource": "storage", "action": "read" },
            ],
            "project_ids": [p],
        }})
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["assignments"][0]["is_personal"], true);
    assert_eq!(body["assignments"][0]["project_id"], p);
    assert_eq!(body["personal_permissions"].as_array().unwrap().len(), 2);
    let personal = body["assignments"][0]["role_id"].as_i64().unwrap();

    let access = RoleService::access_for(&db.pool, u.id).await.unwrap();
    assert!(access.can(Resource::Logs, Action::Read, Some(p)));
    assert!(!access.can(Resource::Logs, Action::Read, Some(p + 1)));
    assert!(access.can(Resource::Storage, Action::Read, None));

    let (_, body) = call!(app, get, "/api/roles", owner.token);
    assert!(!body.as_array().unwrap().iter().any(|r| r["id"] == personal));
    let (status, _) = call!(app, get, uri, owner.token);
    assert_eq!(status, 200);

    let (status, _) = call!(
        app,
        put,
        format!("/api/users/{}/assignments", other.id),
        owner.token,
        json!({ "assignments": [{ "role_id": personal, "project_id": null }] })
    );
    assert_eq!(status, 400, "someone else's personal role cannot be given");
    let (status, _) = call!(
        app,
        put,
        format!("/api/roles/{personal}"),
        owner.token,
        json!({ "name": "Taken", "permissions": [] })
    );
    assert_eq!(status, 404, "it is not a role of the list");

    // Saving with no personal access removes the personal role.
    let (status, body) = call!(app, put, uri, owner.token, json!({ "assignments": [] }));
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["personal_permissions"], json!([]));
    let access = RoleService::access_for(&db.pool, u.id).await.unwrap();
    assert!(!access.can(Resource::Storage, Action::Read, None));
}

#[actix_web::test]
async fn personal_permissions_cannot_exceed_your_own() {
    let db = TestDb::new().await;
    let manager = who(&db.pool, "m@x.com", false).await;
    let u = who(&db.pool, "u@x.com", false).await;
    let users = role(
        &db.pool,
        "user manager",
        &[
            (Resource::Users, Action::Read),
            (Resource::Users, Action::Update),
        ],
    )
    .await;
    RoleService::assign(&db.pool, manager.id, users, None)
        .await
        .unwrap();
    let app = app!(db.pool.clone());
    let uri = format!("/api/users/{}/assignments", u.id);

    let (status, _) = call!(
        app,
        put,
        uri,
        manager.token,
        json!({ "assignments": [], "personal": {
            "permissions": [{ "resource": "storage", "action": "delete" }],
            "project_ids": null,
        }})
    );
    assert_eq!(status, 403);
    let (status, _) = call!(
        app,
        put,
        uri,
        manager.token,
        json!({ "assignments": [], "personal": {
            "permissions": [{ "resource": "users", "action": "read" }],
            "project_ids": null,
        }})
    );
    assert_eq!(status, 200);
}

#[actix_web::test]
async fn deleting_a_user_deletes_their_personal_role() {
    let db = TestDb::new().await;
    let owner = who(&db.pool, "o@x.com", true).await;
    let u = who(&db.pool, "u@x.com", false).await;
    let app = app!(db.pool.clone());
    let (status, body) = call!(
        app,
        put,
        format!("/api/users/{}/assignments", u.id),
        owner.token,
        json!({ "assignments": [], "personal": {
            "permissions": [{ "resource": "logs", "action": "read" }],
            "project_ids": null,
        }})
    );
    assert_eq!(status, 200, "{body}");
    let (status, body) = call!(app, delete, format!("/api/team/{}", u.id), owner.token);
    assert_eq!(status, 204, "{body}");
    let (left,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM roles WHERE user_id IS NOT NULL")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
}

#[actix_web::test]
async fn whoever_creates_a_project_holds_their_own_roles_on_it() {
    let db = TestDb::new().await;
    let u = who(&db.pool, "u@x.com", false).await;
    let p = project(&db.pool).await;
    let creator = role(
        &db.pool,
        "creator",
        &[
            (Resource::Projects, Action::Create),
            (Resource::Issues, Action::Read),
        ],
    )
    .await;
    RoleService::assign(&db.pool, u.id, creator, Some(p))
        .await
        .unwrap();
    let app = app!(db.pool.clone());

    let (status, body) = call!(
        app,
        post,
        "/api/projects",
        u.token,
        json!({ "name": "Mine" })
    );
    assert_eq!(status, 201, "{body}");
    let mine = body["id"].as_i64().unwrap() as i32;
    let access = RoleService::access_for(&db.pool, u.id).await.unwrap();
    assert!(access.can(Resource::Issues, Action::Read, Some(mine)));
    assert!(!access.can(Resource::Issues, Action::Delete, Some(mine)));
}

#[actix_web::test]
async fn a_user_can_hold_a_workspace_role_all_projects_and_exceptions() {
    let db = TestDb::new().await;
    let owner = who(&db.pool, "o@x.com", true).await;
    let u = who(&db.pool, "u@x.com", false).await;
    let p = project(&db.pool).await;
    let admin = seeded(&db.pool, "Workspace Admin").await;
    let viewer = seeded(&db.pool, "Viewer").await;
    let editor = seeded(&db.pool, "Editor").await;
    let app = app!(db.pool.clone());

    let (status, body) = call!(
        app,
        put,
        format!("/api/users/{}/assignments", u.id),
        owner.token,
        json!({ "assignments": [{ "role_id": admin, "project_id": null },
            { "role_id": viewer, "project_id": null },
            { "role_id": editor, "project_id": p },
        ] })
    );
    assert_eq!(status, 200, "{body}");
    let access = RoleService::access_for(&db.pool, u.id).await.unwrap();
    assert!(access.can(Resource::Users, Action::Update, None));
    assert!(access.can(Resource::Issues, Action::Read, Some(p + 1)));
    assert!(!access.can(Resource::Issues, Action::Update, Some(p + 1)));
    assert!(access.can(Resource::Issues, Action::Update, Some(p)));
}

#[actix_web::test]
async fn an_owner_sets_a_users_assignments_on_the_instance_and_on_projects() {
    let db = TestDb::new().await;
    let owner = who(&db.pool, "o@x.com", true).await;
    let u = who(&db.pool, "u@x.com", false).await;
    let p = project(&db.pool).await;
    let viewer = seeded(&db.pool, "Viewer").await;
    let editor = seeded(&db.pool, "Editor").await;
    let app = app!(db.pool.clone());

    let body = json!({ "assignments": [{ "role_id": viewer, "project_id": null }, { "role_id": editor, "project_id": p }] });
    let (status, _) = call!(
        app,
        put,
        format!("/api/users/{}/assignments", u.id),
        owner.token,
        body
    );
    assert_eq!(status, 200);

    let (status, body) = call!(
        app,
        get,
        format!("/api/users/{}/assignments", u.id),
        owner.token
    );
    assert_eq!(status, 200);
    assert_eq!(body["assignments"].as_array().unwrap().len(), 2);

    let access = RoleService::access_for(&db.pool, u.id).await.unwrap();
    assert!(access.can(Resource::Issues, Action::Update, Some(p)));
    assert!(!access.can(Resource::Issues, Action::Update, Some(p + 1)));
}

#[actix_web::test]
async fn nobody_edits_their_own_assignments_or_hands_out_more_than_they_hold() {
    let db = TestDb::new().await;
    let manager = who(&db.pool, "m@x.com", false).await;
    let target = who(&db.pool, "t@x.com", false).await;
    let users = role(
        &db.pool,
        "user manager",
        &[
            (Resource::Users, Action::Read),
            (Resource::Users, Action::Update),
        ],
    )
    .await;
    RoleService::assign(&db.pool, manager.id, users, None)
        .await
        .unwrap();
    let viewer = seeded(&db.pool, "Viewer").await;
    let app = app!(db.pool.clone());

    let (status, _) = call!(
        app,
        put,
        format!("/api/users/{}/assignments", manager.id),
        manager.token,
        json!({ "assignments": [{ "role_id": users, "project_id": null }] })
    );
    assert_eq!(status, 403, "own assignments are off limits");

    let (status, _) = call!(
        app,
        put,
        format!("/api/users/{}/assignments", target.id),
        manager.token,
        json!({ "assignments": [{ "role_id": viewer, "project_id": null }] })
    );
    assert_eq!(status, 403, "Viewer reads issues, which the manager cannot");

    let (status, _) = call!(
        app,
        put,
        format!("/api/users/{}/assignments", target.id),
        manager.token,
        json!({ "assignments": [{ "role_id": users, "project_id": null }] })
    );
    assert_eq!(status, 200, "a role within the manager's own permissions");
}

#[actix_web::test]
async fn a_project_admin_manages_that_projects_members() {
    let db = TestDb::new().await;
    let padmin = who(&db.pool, "pa@x.com", false).await;
    let member = who(&db.pool, "m@x.com", false).await;
    let p = project(&db.pool).await;
    let admin_role = seeded(&db.pool, "Project Admin").await;
    let editor = seeded(&db.pool, "Editor").await;
    RoleService::assign(&db.pool, padmin.id, admin_role, Some(p))
        .await
        .unwrap();
    let app = app!(db.pool.clone());

    let (status, body) = call!(
        app,
        put,
        format!("/api/projects/{p}/members"),
        padmin.token,
        json!({ "user_id": member.id, "role_ids": [editor] })
    );
    assert_eq!(status, 200, "{body}");

    let (status, body) = call!(app, get, format!("/api/projects/{p}/members"), member.token);
    assert_eq!(status, 200);
    let row = body
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["user_id"] == member.id)
        .unwrap()
        .clone();
    assert_eq!(row["email"], "m@x.com");
    assert_eq!(row["roles"], json!([{ "id": editor, "name": "Editor" }]));

    let (status, _) = call!(
        app,
        put,
        format!("/api/projects/{p}/members"),
        padmin.token,
        json!({ "user_id": padmin.id, "role_ids": [] })
    );
    assert_eq!(status, 403, "own membership is off limits");

    let (status, _) = call!(
        app,
        put,
        format!("/api/projects/{p}/members"),
        padmin.token,
        json!({ "user_id": member.id, "role_ids": [999999] })
    );
    assert_eq!(status, 400);

    let (status, _) = call!(
        app,
        delete,
        format!("/api/projects/{p}/members/{}", member.id),
        padmin.token
    );
    assert_eq!(status, 204);
    let (status, _) = call!(app, get, format!("/api/projects/{p}/issues"), member.token);
    assert_eq!(status, 404);
}

#[actix_web::test]
async fn an_invitation_can_carry_a_role_on_a_project() {
    let db = TestDb::new().await;
    let owner = who(&db.pool, "o@x.com", true).await;
    let p = project(&db.pool).await;
    let editor = seeded(&db.pool, "Editor").await;
    let app = app!(db.pool.clone());

    let (status, body) = call!(
        app,
        post,
        "/api/invitations",
        owner.token,
        json!({ "email": "new@x.com", "role": "member", "role_id": editor, "project_id": p })
    );
    assert_eq!(status, 201, "{body}");
    assert_eq!(body["role_id"], editor);
    assert_eq!(body["project_id"], p);

    let token = body["token"].as_str().unwrap();
    let user = rustrak::services::InvitationService::accept(&db.pool, token, "pw")
        .await
        .unwrap();
    let access = RoleService::access_for(&db.pool, user.id).await.unwrap();
    assert!(access.can(Resource::Issues, Action::Update, Some(p)));
    assert!(!access.can(Resource::Issues, Action::Update, Some(p + 1)));
}

#[actix_web::test]
async fn an_inviter_cannot_hand_out_more_than_they_hold() {
    let db = TestDb::new().await;
    let inviter = who(&db.pool, "i@x.com", false).await;
    let p = project(&db.pool).await;
    let r = role(
        &db.pool,
        "inviter",
        &[(Resource::Invitations, Action::Create)],
    )
    .await;
    RoleService::assign(&db.pool, inviter.id, r, None)
        .await
        .unwrap();
    let viewer = seeded(&db.pool, "Viewer").await;
    let app = app!(db.pool.clone());

    let (status, _) = call!(
        app,
        post,
        "/api/invitations",
        inviter.token,
        json!({ "email": "a@x.com", "role": "admin" })
    );
    assert_eq!(status, 403, "only an owner invites an owner");

    let (status, _) = call!(
        app,
        post,
        "/api/invitations",
        inviter.token,
        json!({ "email": "b@x.com", "role": "member", "role_id": viewer, "project_id": p })
    );
    assert_eq!(status, 403, "the inviter cannot read that project");

    let (status, _) = call!(
        app,
        post,
        "/api/invitations",
        inviter.token,
        json!({ "email": "c@x.com", "role": "member" })
    );
    assert_eq!(status, 201, "a plain invitation grants nothing");
}

#[actix_web::test]
async fn only_an_owner_makes_or_unmakes_an_owner() {
    let db = TestDb::new().await;
    let _primary = who(&db.pool, "p@x.com", true).await;
    let other_owner = who(&db.pool, "o@x.com", true).await;
    let manager = who(&db.pool, "m@x.com", false).await;
    let target = who(&db.pool, "t@x.com", false).await;
    let r = role(
        &db.pool,
        "user manager",
        &[
            (Resource::Users, Action::Update),
            (Resource::Users, Action::Delete),
        ],
    )
    .await;
    RoleService::assign(&db.pool, manager.id, r, None)
        .await
        .unwrap();
    let app = app!(db.pool.clone());

    let (status, _) = call!(
        app,
        patch,
        format!("/api/team/{}/role", target.id),
        manager.token,
        json!({ "role": "admin" })
    );
    assert_eq!(status, 403, "promoting to owner");
    let (status, _) = call!(
        app,
        patch,
        format!("/api/team/{}/role", other_owner.id),
        manager.token,
        json!({ "role": "member" })
    );
    assert_eq!(status, 403, "demoting an owner");
    let (status, _) = call!(
        app,
        delete,
        format!("/api/team/{}", other_owner.id),
        manager.token
    );
    assert_eq!(status, 403, "deleting an owner");
    let (status, _) = call!(
        app,
        delete,
        format!("/api/team/{}", target.id),
        manager.token
    );
    assert_eq!(status, 204, "deleting a member is what users:delete is for");
}
