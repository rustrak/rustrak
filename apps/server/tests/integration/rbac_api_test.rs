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
                .configure(routes::releases::configure)
                .configure(routes::projects::configure)
                .configure(routes::sourcemaps::configure)
                .configure(routes::github::configure)
                .configure(routes::tokens::configure)
                .configure(routes::alerts::configure),
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

/// Who manages a project's members needs people and roles to choose from,
/// without being able to read the team or the roles of the whole instance.
#[actix_web::test]
async fn a_member_manager_is_offered_who_and_what_they_can_give() {
    let db = TestDb::new().await;
    let manager = who(&db.pool, "manager@x.com", false).await;
    let member = who(&db.pool, "member@x.com", false).await;
    let boss = who(&db.pool, "boss@x.com", true).await;
    let p = project(&db.pool).await;
    let manages = role(
        &db.pool,
        "manages",
        &[
            (Resource::ProjectMembers, Action::Update),
            (Resource::Issues, Action::Read),
        ],
    )
    .await;
    let reads = role(&db.pool, "reads", &[(Resource::Issues, Action::Read)]).await;
    role(&db.pool, "deletes", &[(Resource::Issues, Action::Delete)]).await;
    RoleService::assign(&db.pool, manager.id, manages, Some(p))
        .await
        .unwrap();
    RoleService::assign(&db.pool, member.id, reads, Some(p))
        .await
        .unwrap();
    let app = app!(db.pool.clone());
    let uri = format!("/api/projects/{p}/members/candidates");

    let (status, body) = call!(app, get, uri, manager.token);
    assert_eq!(status, 200, "{body}");
    let emails: Vec<_> = body["users"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["email"].as_str().unwrap())
        .collect();
    assert_eq!(emails, ["member@x.com"], "not themselves, not an owner");
    let roles: Vec<_> = body["roles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    assert!(roles.contains(&"reads"), "{roles:?}");
    assert!(
        !roles.contains(&"deletes"),
        "beyond what they hold: {roles:?}"
    );

    let (status, _) = call!(app, get, uri, member.token);
    assert_eq!(status, 403, "sees the project, may not manage it");
    let (status, _) = call!(
        app,
        get,
        format!("/api/projects/{}/members/candidates", p + 1),
        manager.token
    );
    assert_eq!(status, 404);
    let (status, _) = call!(app, get, uri, boss.token);
    assert_eq!(status, 200);
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

/// A user holding `perms` where `project_id` says (`None`: everywhere).
async fn holder(
    pool: &DbPool,
    email: &str,
    perms: &[(Resource, Action)],
    project_id: Option<i32>,
) -> Who {
    let u = who(pool, email, false).await;
    let r = role(pool, email, perms).await;
    RoleService::assign(pool, u.id, r, project_id)
        .await
        .unwrap();
    u
}

/// An invitation's token claims its account, so reading the list must not hand
/// out one the reader could not have created. The `id` is what manages it.
#[actix_web::test]
async fn an_invitation_token_only_reaches_who_could_create_that_invitation() {
    let db = TestDb::new().await;
    let owner = who(&db.pool, "o@x.com", true).await;
    let reader = holder(
        &db.pool,
        "reader@x.com",
        &[(Resource::Invitations, Action::Read)],
        None,
    )
    .await;
    let inviter = holder(
        &db.pool,
        "inviter@x.com",
        &[
            (Resource::Invitations, Action::Read),
            (Resource::Invitations, Action::Create),
        ],
        None,
    )
    .await;
    let app = app!(db.pool.clone());

    for (email, role) in [("boss@x.com", "admin"), ("dev@x.com", "member")] {
        let (status, body) = call!(
            app,
            post,
            "/api/invitations",
            owner.token,
            json!({ "email": email, "role": role })
        );
        assert_eq!(status, 201, "{body}");
        assert!(body["token"].is_string(), "the creator gets the link");
    }

    let list = |body: &Value, email: &str| -> Value {
        body.as_array()
            .unwrap()
            .iter()
            .find(|i| i["email"] == email)
            .unwrap()
            .clone()
    };

    let (status, body) = call!(app, get, "/api/invitations", reader.token);
    assert_eq!(status, 200, "{body}");
    for email in ["boss@x.com", "dev@x.com"] {
        assert!(list(&body, email)["token"].is_null(), "{body}");
    }

    let (_, body) = call!(app, get, "/api/invitations", inviter.token);
    assert!(list(&body, "boss@x.com")["token"].is_null(), "{body}");
    assert!(list(&body, "dev@x.com")["token"].is_string(), "{body}");

    let (_, body) = call!(app, get, "/api/invitations", owner.token);
    let boss = list(&body, "boss@x.com");
    assert!(boss["token"].is_string(), "{body}");
    let id = boss["id"].as_str().unwrap();
    assert_ne!(boss["token"].as_str().unwrap(), id);

    let (status, _) = call!(
        app,
        delete,
        format!("/api/invitations/{}", boss["token"].as_str().unwrap()),
        owner.token
    );
    assert_eq!(status, 404, "a token does not address an invitation");
    let (status, _) = call!(app, delete, format!("/api/invitations/{id}"), owner.token);
    assert_eq!(status, 204);
}

/// Resending names no secret either: the email goes to the invitee, and the
/// response must not hand the link to whoever asked for it.
#[actix_web::test]
async fn resending_does_not_hand_out_an_owner_invitation_token() {
    let db = TestDb::new().await;
    let owner = who(&db.pool, "o@x.com", true).await;
    let inviter = holder(
        &db.pool,
        "inviter@x.com",
        &[(Resource::Invitations, Action::Create)],
        None,
    )
    .await;
    let app = app!(db.pool.clone());
    let (_, body) = call!(
        app,
        post,
        "/api/invitations",
        owner.token,
        json!({ "email": "boss@x.com", "role": "admin" })
    );
    let id = body["id"].as_str().unwrap();

    let (status, body) = call!(
        app,
        post,
        format!("/api/invitations/{id}/resend"),
        inviter.token
    );
    assert_eq!(status, 403, "{body}");
}

#[actix_web::test]
async fn a_deleted_role_cannot_leave_a_pending_invitation_without_it() {
    let db = TestDb::new().await;
    let owner = who(&db.pool, "o@x.com", true).await;
    let r = role(&db.pool, "r", &[(Resource::Issues, Action::Read)]).await;
    let app = app!(db.pool.clone());
    let (_, body) = call!(
        app,
        post,
        "/api/invitations",
        owner.token,
        json!({ "email": "dev@x.com", "role": "member", "role_id": r })
    );
    let id = body["id"].as_str().unwrap();

    let (status, _) = call!(app, delete, format!("/api/roles/{r}"), owner.token);
    assert_eq!(status, 409);

    let (status, _) = call!(app, delete, format!("/api/invitations/{id}"), owner.token);
    assert_eq!(status, 204);
    let (status, _) = call!(app, delete, format!("/api/roles/{r}"), owner.token);
    assert_eq!(status, 204, "a revoked invitation holds nothing back");
}

#[actix_web::test]
async fn the_ingest_key_needs_client_keys_read() {
    let db = TestDb::new().await;
    let p = project(&db.pool).await;
    let logs = holder(
        &db.pool,
        "logs@x.com",
        &[(Resource::Logs, Action::Read)],
        Some(p),
    )
    .await;
    let keys = holder(
        &db.pool,
        "keys@x.com",
        &[(Resource::ClientKeys, Action::Read)],
        Some(p),
    )
    .await;
    let app = app!(db.pool.clone());

    let (status, body) = call!(app, get, format!("/api/projects/{p}"), logs.token);
    assert_eq!(status, 200, "{body}");
    assert!(
        body["sentry_key"].is_null() && body["dsn"].is_null(),
        "{body}"
    );
    let (_, body) = call!(app, get, "/api/projects", logs.token);
    let listed = &body["items"][0];
    assert!(
        listed["sentry_key"].is_null() && listed["dsn"].is_null(),
        "{body}"
    );

    let (_, body) = call!(app, get, format!("/api/projects/{p}"), keys.token);
    assert!(
        body["sentry_key"].is_string() && body["dsn"].is_string(),
        "{body}"
    );
    let (_, body) = call!(app, get, "/api/projects", keys.token);
    assert!(body["items"][0]["dsn"].is_string(), "{body}");
}

#[actix_web::test]
async fn chunk_upload_needs_a_source_map_grant_somewhere() {
    let db = TestDb::new().await;
    let p = project(&db.pool).await;
    let owner = who(&db.pool, "o@x.com", true).await;
    let empty = AuthTokenService::create_for_user(
        &db.pool,
        CreateAuthToken {
            permissions: Some(vec![]),
            ..Default::default()
        },
        Some(owner.id),
    )
    .await
    .unwrap()
    .token;
    let reader = holder(
        &db.pool,
        "reader@x.com",
        &[(Resource::SourceMaps, Action::Read)],
        Some(p),
    )
    .await;
    let uploader = holder(
        &db.pool,
        "up@x.com",
        &[(Resource::SourceMaps, Action::Create)],
        Some(p),
    )
    .await;
    let app = app!(db.pool.clone());
    let upload = "/api/0/organizations/o/chunk-upload/";
    let multipart = ("content-type", "multipart/form-data; boundary=b");

    for token in [empty, reader.token] {
        let req = test::TestRequest::post()
            .uri(upload)
            .insert_header(("Authorization", format!("Bearer {token}")))
            .insert_header(multipart)
            .set_payload("--b--\r\n")
            .to_request();
        assert_eq!(test::call_service(&app, req).await.status(), 403);
    }

    let req = test::TestRequest::post()
        .uri(upload)
        .insert_header(("Authorization", format!("Bearer {}", uploader.token)))
        .insert_header(multipart)
        .set_payload("--b--\r\n")
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        400,
        "past the check: an upload with no parts is invalid"
    );
}

#[actix_web::test]
async fn recording_a_github_installation_needs_update() {
    let db = TestDb::new().await;
    let reader = holder(
        &db.pool,
        "reader@x.com",
        &[(Resource::GithubApp, Action::Read)],
        None,
    )
    .await;
    let app = app!(db.pool.clone());
    let (status, body) = call!(
        app,
        get,
        "/api/github/installed?installation_id=99",
        reader.token
    );
    assert_eq!(status, 403, "{body}");
}

#[actix_web::test]
async fn member_routes_answer_404_for_what_does_not_exist() {
    let db = TestDb::new().await;
    let p = project(&db.pool).await;
    let owner = who(&db.pool, "o@x.com", true).await;
    let app = app!(db.pool.clone());
    let (nowhere, nobody) = (9_999, 9_999);

    let (status, _) = call!(
        app,
        get,
        format!("/api/projects/{nowhere}/members"),
        owner.token
    );
    assert_eq!(status, 404);
    let (status, _) = call!(
        app,
        delete,
        format!("/api/projects/{nowhere}/members/{}", owner.id + 1),
        owner.token
    );
    assert_eq!(status, 404);
    let (status, _) = call!(
        app,
        put,
        format!("/api/projects/{nowhere}/members"),
        owner.token,
        json!({ "user_id": nobody, "role_ids": [] })
    );
    assert_eq!(status, 404);
    let (status, _) = call!(
        app,
        put,
        format!("/api/projects/{p}/members"),
        owner.token,
        json!({ "user_id": nobody, "role_ids": [] })
    );
    assert_eq!(status, 404);
    let (status, _) = call!(
        app,
        put,
        format!("/api/users/{nobody}/assignments"),
        owner.token,
        json!({ "assignments": [] })
    );
    assert_eq!(status, 404);
}

#[actix_web::test]
async fn project_stats_need_issues_read() {
    let db = TestDb::new().await;
    let p = project(&db.pool).await;
    let members = holder(
        &db.pool,
        "m@x.com",
        &[(Resource::ProjectMembers, Action::Read)],
        Some(p),
    )
    .await;
    let issues = holder(
        &db.pool,
        "i@x.com",
        &[(Resource::Issues, Action::Read)],
        Some(p),
    )
    .await;
    let app = app!(db.pool.clone());
    let uri = "/api/projects?stats_period=24h";

    let (_, body) = call!(app, get, uri, members.token);
    assert!(body["items"][0].get("stats").is_none(), "{body}");
    let (_, body) = call!(app, get, uri, issues.token);
    assert!(body["items"][0]["stats"].is_object(), "{body}");
}

#[actix_web::test]
async fn a_releases_new_issues_need_issues_read() {
    let db = TestDb::new().await;
    let p = project(&db.pool).await;
    let u = holder(
        &db.pool,
        "r@x.com",
        &[(Resource::Releases, Action::Read)],
        Some(p),
    )
    .await;
    let app = app!(db.pool.clone());
    let (status, body) = call!(
        app,
        get,
        format!("/api/projects/{p}/releases/1.0/new-issues"),
        u.token
    );
    assert_eq!(status, 403, "{body}");
}

#[actix_web::test]
async fn a_role_name_longer_than_its_column_is_a_validation_error() {
    let db = TestDb::new().await;
    let owner = who(&db.pool, "o@x.com", true).await;
    let app = app!(db.pool.clone());
    let (status, body) = call!(
        app,
        post,
        "/api/roles",
        owner.token,
        json!({ "name": "x".repeat(101), "permissions": [] })
    );
    assert_eq!(status, 400, "{body}");
    let (status, body) = call!(
        app,
        post,
        "/api/roles",
        owner.token,
        json!({ "name": "é".repeat(100), "permissions": [] })
    );
    assert_eq!(status, 201, "the limit counts characters: {body}");
}

/// The keys in a creation response follow what the caller holds on the new
/// project, which is the same answer `GET` gives them afterwards.
#[actix_web::test]
async fn creating_a_project_returns_its_keys_only_to_who_holds_them_there() {
    let db = TestDb::new().await;
    let p = project(&db.pool).await;
    let owner = who(&db.pool, "o@x.com", true).await;
    let new_project = |name: &str| json!({ "name": name });
    let create = (Resource::Projects, Action::Create);
    let keys = (Resource::ClientKeys, Action::Read);

    // Reaches every project through one grant, holds the keys on only one:
    // no role extends to the new project, so nothing gives them there.
    let wide = holder(
        &db.pool,
        "wide@x.com",
        &[create, (Resource::Issues, Action::Read)],
        None,
    )
    .await;
    let on_one = role(&db.pool, "keys on one", &[keys]).await;
    RoleService::assign(&db.pool, wide.id, on_one, Some(p))
        .await
        .unwrap();
    // Its roles extend to what it creates, keys included.
    let extended = holder(&db.pool, "ext@x.com", &[create, keys], Some(p)).await;
    // Its list does not grow to reach what it creates.
    let limited = AuthTokenService::create_for_user(
        &db.pool,
        CreateAuthToken {
            project_ids: Some(vec![p]),
            ..Default::default()
        },
        Some(owner.id),
    )
    .await
    .unwrap()
    .token;
    let app = app!(db.pool.clone());

    let (status, body) = call!(app, post, "/api/projects", wide.token, new_project("a"));
    assert_eq!(status, 201, "{body}");
    assert!(
        body["dsn"].is_null() && body["sentry_key"].is_null(),
        "{body}"
    );

    let (status, body) = call!(app, post, "/api/projects", limited, new_project("b"));
    assert_eq!(status, 201, "{body}");
    assert!(
        body["dsn"].is_null() && body["sentry_key"].is_null(),
        "{body}"
    );

    let (status, body) = call!(app, post, "/api/projects", extended.token, new_project("c"));
    assert_eq!(status, 201, "{body}");
    assert!(body["dsn"].is_string(), "{body}");
    let (_, again) = call!(
        app,
        get,
        format!("/api/projects/{}", body["id"]),
        extended.token
    );
    assert_eq!(again["dsn"], body["dsn"]);

    let (status, body) = call!(app, post, "/api/projects", owner.token, new_project("d"));
    assert_eq!(status, 201, "{body}");
    assert!(body["dsn"].is_string(), "{body}");
}

/// A leaked CI token must not reach the rest of its owner's credentials.
#[actix_web::test]
async fn a_scoped_token_cannot_manage_its_owners_other_tokens() {
    let db = TestDb::new().await;
    let p = project(&db.pool).await;
    let owner = holder(
        &db.pool,
        "o@x.com",
        &[(Resource::Logs, Action::Read)],
        Some(p),
    )
    .await;
    let scoped = AuthTokenService::create_for_user(
        &db.pool,
        CreateAuthToken {
            permissions: Some(vec![Permission {
                resource: Resource::Logs,
                action: Action::Read,
            }]),
            ..Default::default()
        },
        Some(owner.id),
    )
    .await
    .unwrap();
    let other =
        AuthTokenService::create_for_user(&db.pool, CreateAuthToken::default(), Some(owner.id))
            .await
            .unwrap();
    let app = app!(db.pool.clone());

    let (status, _) = call!(app, get, "/api/tokens", scoped.token);
    assert_eq!(status, 403);
    let (status, _) = call!(app, get, format!("/api/tokens/{}", other.id), scoped.token);
    assert_eq!(status, 403);
    let (status, _) = call!(
        app,
        delete,
        format!("/api/tokens/{}", other.id),
        scoped.token
    );
    assert_eq!(status, 403);

    // Unlimited, it still manages what is its owner's.
    let (status, _) = call!(app, get, "/api/tokens", other.token);
    assert_eq!(status, 200);
    let (status, _) = call!(
        app,
        delete,
        format!("/api/tokens/{}", scoped.id),
        other.token
    );
    assert_eq!(status, 204);
}

/// A token sees a project only through a permission it shares with its owner
/// there, not through one that only its owner holds.
#[actix_web::test]
async fn a_token_does_not_see_projects_its_permissions_do_not_reach() {
    let db = TestDb::new().await;
    let a = project(&db.pool).await;
    let b = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "other".into(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap()
    .id;
    let owner = holder(
        &db.pool,
        "o@x.com",
        &[(Resource::Logs, Action::Read)],
        Some(a),
    )
    .await;
    let on_b = role(&db.pool, "issues", &[(Resource::Issues, Action::Read)]).await;
    RoleService::assign(&db.pool, owner.id, on_b, Some(b))
        .await
        .unwrap();
    let limited = AuthTokenService::create_for_user(
        &db.pool,
        CreateAuthToken {
            permissions: Some(vec![Permission {
                resource: Resource::Logs,
                action: Action::Read,
            }]),
            ..Default::default()
        },
        Some(owner.id),
    )
    .await
    .unwrap()
    .token;
    let app = app!(db.pool.clone());

    let (status, body) = call!(app, get, "/api/projects", limited);
    assert_eq!(status, 200, "{body}");
    let ids: Vec<i64> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_i64().unwrap())
        .collect();
    assert_eq!(ids, [i64::from(a)]);
    let (status, _) = call!(app, get, format!("/api/projects/{a}"), limited);
    assert_eq!(status, 200);
    let (status, _) = call!(app, get, format!("/api/projects/{b}"), limited);
    assert_eq!(status, 404);

    // Its owner, with no token in the way, sees both.
    let (_, body) = call!(app, get, "/api/projects", owner.token);
    assert_eq!(body["total_count"], 2, "{body}");
}

/// Personal permissions belong to the member's own page: the project members
/// API neither lists them nor takes them away.
#[actix_web::test]
async fn the_members_api_leaves_personal_permissions_alone() {
    let db = TestDb::new().await;
    let p = project(&db.pool).await;
    let owner = who(&db.pool, "o@x.com", true).await;
    let member = who(&db.pool, "m@x.com", false).await;
    let shared = role(&db.pool, "viewer", &[(Resource::Issues, Action::Read)]).await;
    let app = app!(db.pool.clone());
    let (status, body) = call!(
        app,
        put,
        format!("/api/users/{}/assignments", member.id),
        owner.token,
        json!({
            "assignments": [{ "role_id": shared, "project_id": p }],
            "personal": {
                "permissions": [{ "resource": "logs", "action": "read" }],
                "project_ids": [p],
            },
        })
    );
    assert_eq!(status, 200, "{body}");

    let (_, body) = call!(app, get, format!("/api/projects/{p}/members"), owner.token);
    let roles = &body[0]["roles"];
    assert_eq!(roles.as_array().unwrap().len(), 1, "{body}");
    assert_eq!(roles[0]["id"], shared);

    // What the listing shows can be sent back as it is.
    let (status, body) = call!(
        app,
        put,
        format!("/api/projects/{p}/members"),
        owner.token,
        json!({ "user_id": member.id, "role_ids": [shared] })
    );
    assert_eq!(status, 200, "{body}");
    let (status, _) = call!(
        app,
        delete,
        format!("/api/projects/{p}/members/{}", member.id),
        owner.token
    );
    assert_eq!(status, 204);

    let (_, access) = call!(
        app,
        get,
        format!("/api/users/{}/assignments", member.id),
        owner.token
    );
    let held = access["assignments"].as_array().unwrap();
    assert_eq!(held.len(), 1, "{access}");
    assert_eq!(held[0]["is_personal"], true, "{access}");
}

/// Choosing where an alert goes takes the channels' names, not their secrets.
#[actix_web::test]
async fn alert_rule_holders_list_channels_without_their_credentials() {
    let db = TestDb::new().await;
    let p = project(&db.pool).await;
    let owner = who(&db.pool, "o@x.com", true).await;
    let rules = holder(
        &db.pool,
        "r@x.com",
        &[(Resource::AlertRules, Action::Read)],
        Some(p),
    )
    .await;
    let unrelated = holder(
        &db.pool,
        "l@x.com",
        &[(Resource::Logs, Action::Read)],
        Some(p),
    )
    .await;
    let app = app!(db.pool.clone());
    let (status, body) = call!(
        app,
        post,
        "/api/integrations",
        owner.token,
        json!({
            "name": "ops",
            "provider_type": "webhook",
            "credentials": { "url": "https://hooks.example.com/SECRET" },
        })
    );
    assert_eq!(status, 201, "{body}");

    let (status, body) = call!(app, get, "/api/integrations", rules.token);
    assert_eq!(status, 200, "{body}");
    assert_eq!(body[0]["name"], "ops");
    assert_eq!(body[0]["credentials"], json!({}), "{body}");
    assert!(!body.to_string().contains("SECRET"), "{body}");

    let (status, body) = call!(app, get, "/api/integrations", owner.token);
    assert_eq!(status, 200);
    assert_ne!(body[0]["credentials"], json!({}), "{body}");

    let (status, _) = call!(app, get, "/api/integrations", unrelated.token);
    assert_eq!(status, 403);
}
