//! Roles and assignments stored in the database, and the access they resolve to.

use crate::common::TestDb;
use rustrak::auth::rbac::{Action, ProjectScope, Resource};
use rustrak::db::DbPool;
use rustrak::models::{CreateProject, CreateUserRequest, User, UserRole};
use rustrak::services::{ProjectService, RoleService, UsersService};

async fn user(pool: &DbPool, email: &str) -> User {
    let req = CreateUserRequest {
        email: email.into(),
        password: "password123".into(),
    };
    UsersService::create_user(pool, &req, UserRole::Member)
        .await
        .unwrap()
}

async fn project(pool: &DbPool, name: &str) -> i32 {
    ProjectService::create(
        pool,
        CreateProject {
            name: name.into(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap()
    .id
}

async fn role_id(pool: &DbPool, name: &str) -> i32 {
    crate::common::ensure_role(pool, name).await
}

#[actix_web::test]
async fn a_new_install_has_no_roles() {
    let db = TestDb::new().await;
    assert!(RoleService::list(&db.pool).await.unwrap().is_empty());
}

#[actix_web::test]
async fn a_new_user_can_do_nothing() {
    let db = TestDb::new().await;
    let u = user(&db.pool, "new@example.com").await;
    let p = project(&db.pool, "p").await;
    let access = RoleService::access_for(&db.pool, u.id).await.unwrap();
    assert!(!access.can(Resource::Issues, Action::Read, Some(p)));
    assert_eq!(access.visible_projects(), ProjectScope::Only(vec![]));
}

#[actix_web::test]
async fn a_role_on_one_project_grants_exactly_its_permissions_there() {
    let db = TestDb::new().await;
    let u = user(&db.pool, "v@example.com").await;
    let p1 = project(&db.pool, "p1").await;
    let p2 = project(&db.pool, "p2").await;
    let viewer = role_id(&db.pool, "Viewer").await;
    RoleService::assign(&db.pool, u.id, viewer, Some(p1))
        .await
        .unwrap();

    let access = RoleService::access_for(&db.pool, u.id).await.unwrap();
    assert!(access.can(Resource::Issues, Action::Read, Some(p1)));
    assert!(!access.can(Resource::Issues, Action::Update, Some(p1)));
    assert!(!access.can(Resource::Issues, Action::Read, Some(p2)));
    assert_eq!(access.visible_projects(), ProjectScope::Only(vec![p1]));
}

#[actix_web::test]
async fn assigning_the_same_role_twice_is_idempotent_and_unassign_removes_it() {
    let db = TestDb::new().await;
    let u = user(&db.pool, "x@example.com").await;
    let editor = role_id(&db.pool, "Editor").await;
    RoleService::assign(&db.pool, u.id, editor, None)
        .await
        .unwrap();
    RoleService::assign(&db.pool, u.id, editor, None)
        .await
        .unwrap();
    assert_eq!(
        RoleService::assignments_for(&db.pool, u.id)
            .await
            .unwrap()
            .len(),
        1
    );

    RoleService::set_assignments(&db.pool, u.id, &[], None)
        .await
        .unwrap();
    let access = RoleService::access_for(&db.pool, u.id).await.unwrap();
    assert!(!access.can(Resource::Issues, Action::Read, Some(1)));
}

#[actix_web::test]
async fn an_instance_owner_can_do_everything() {
    let db = TestDb::new().await;
    let req = CreateUserRequest {
        email: "o@example.com".into(),
        password: "password123".into(),
    };
    let u = UsersService::create_user(&db.pool, &req, UserRole::Admin)
        .await
        .unwrap();
    let access = RoleService::access_for(&db.pool, u.id).await.unwrap();
    assert!(access.is_owner());
    assert!(access.can(Resource::Storage, Action::Delete, None));
}

/// Existing installs keep exactly the access they had: global admins stay
/// owners and project memberships become a same-named role, only for the
/// levels in use.
#[cfg(feature = "sqlite")]
#[actix_web::test]
async fn the_migration_carries_over_existing_admins_and_memberships() {
    use sqlx::migrate::Migrator;
    use std::borrow::Cow;

    const RBAC_VERSION: i64 = 20261105000000;
    let dir = tempfile::tempdir().unwrap();
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(dir.path().join("m.sqlite"))
                .create_if_missing(true),
        )
        .await
        .unwrap();
    let all = sqlx::migrate!("./migrations/sqlite");
    let mut before = Migrator::new(std::path::Path::new("./migrations/sqlite"))
        .await
        .unwrap();
    before.migrations = Cow::Owned(
        all.migrations
            .iter()
            .filter(|m| m.version < RBAC_VERSION)
            .cloned()
            .collect(),
    );
    before.run(&pool).await.unwrap();

    for (email, role) in [("a@x.com", "admin"), ("m@x.com", "member")] {
        sqlx::query(
            "INSERT INTO users (email, password_hash, is_active, role) VALUES ($1, 'h', 1, $2)",
        )
        .bind(email)
        .bind(role)
        .execute(&pool)
        .await
        .unwrap();
    }
    let p = project(&pool, "legacy").await;
    sqlx::query("INSERT INTO project_members (project_id, user_id, role) VALUES ($1, 2, 'editor')")
        .bind(p)
        .execute(&pool)
        .await
        .unwrap();

    all.run(&pool).await.unwrap();

    let admin = RoleService::access_for(&pool, 1).await.unwrap();
    assert!(admin.is_owner());
    let member = RoleService::access_for(&pool, 2).await.unwrap();
    assert!(member.can(Resource::Issues, Action::Update, Some(p)));
    assert!(!member.can(Resource::Project, Action::Delete, Some(p)));
    assert!(!member.can(Resource::Users, Action::Read, None));
    assert!(
        member.can(Resource::ApiTokens, Action::Create, None),
        "anyone could make a token before roles: an upgrade keeps that"
    );
    assert!(
        !member.can(Resource::ApiTokens, Action::Read, None),
        "their own tokens only"
    );
    let names: Vec<_> = RoleService::list(&pool)
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.name)
        .collect();
    assert_eq!(names, ["Editor"]);
}

/// Rolling back never gives more than a role held: a level takes every
/// permission it stands for, across the user's roles on the project, and a role
/// on every project counts on each one. Anything less is no membership.
#[cfg(feature = "sqlite")]
#[actix_web::test]
async fn rolling_the_migration_back_restores_only_complete_levels() {
    const BEFORE_RBAC: i64 = 20261105000000 - 1;
    const VIEWER: &[(&str, &str)] = &[
        ("issues", "read"),
        ("performance", "read"),
        ("logs", "read"),
        ("sessions", "read"),
        ("releases", "read"),
        ("source-maps", "read"),
        ("alert-rules", "read"),
        ("monitors", "read"),
        ("client-keys", "read"),
        ("project-members", "read"),
        ("project-github", "read"),
    ];
    const EDITOR_ADDS: &[(&str, &str)] = &[
        ("issues", "update"),
        ("issues", "delete"),
        ("releases", "create"),
        ("releases", "update"),
        ("source-maps", "create"),
        ("alert-rules", "create"),
        ("alert-rules", "update"),
        ("alert-rules", "delete"),
        ("monitors", "create"),
        ("monitors", "update"),
        ("monitors", "delete"),
        ("project", "update"),
        ("project-github", "update"),
    ];
    const ADMIN_ADDS: &[(&str, &str)] = &[("project", "delete"), ("project-members", "update")];

    let dir = tempfile::tempdir().unwrap();
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(dir.path().join("m.sqlite"))
                .create_if_missing(true),
        )
        .await
        .unwrap();
    let migrator = sqlx::migrate!("./migrations/sqlite");
    migrator.run(&pool).await.unwrap();

    let (p1, p2) = (project(&pool, "one").await, project(&pool, "two").await);
    let mut roles = 0;
    let mut give = async |email: &str, sets: &[&[&[(&str, &str)]]], place: Option<i32>| {
        let who = user(&pool, email).await;
        for set in sets {
            roles += 1;
            let permissions = set
                .iter()
                .flat_map(|pairs| pairs.iter())
                .map(|(r, a)| rustrak::models::Permission {
                    resource: Resource::parse(r).unwrap(),
                    action: Action::parse(a).unwrap(),
                })
                .collect();
            let role = RoleService::create(&pool, &format!("r{roles}"), "", permissions)
                .await
                .unwrap();
            RoleService::assign(&pool, who.id, role.id, place)
                .await
                .unwrap();
        }
        who.id
    };

    let admin = give("a@x.com", &[&[VIEWER, EDITOR_ADDS, ADMIN_ADDS]], Some(p1)).await;
    let everywhere = give("e@x.com", &[&[VIEWER, EDITOR_ADDS]], None).await;
    let split = give("s@x.com", &[&[VIEWER], &[EDITOR_ADDS]], Some(p1)).await;
    let viewer = give("v@x.com", &[&[VIEWER]], Some(p2)).await;
    // Partial grants: none of these stands for a whole level.
    give("m@x.com", &[&[&[("project-members", "update")]]], Some(p1)).await;
    give("l@x.com", &[&[&[("logs", "read")]]], Some(p1)).await;
    give("w@x.com", &[&[&[("storage", "read")]]], None).await;

    migrator.undo(&pool, BEFORE_RBAC).await.unwrap();

    let mut rows: Vec<(i32, i32, String)> =
        sqlx::query_as("SELECT project_id, user_id, role FROM project_members")
            .fetch_all(&pool)
            .await
            .unwrap();
    rows.sort();
    let mut expected = vec![
        (p1, admin, "admin".to_string()),
        (p1, everywhere, "editor".to_string()),
        (p2, everywhere, "editor".to_string()),
        (p1, split, "editor".to_string()),
        (p2, viewer, "viewer".to_string()),
    ];
    expected.sort();
    assert_eq!(rows, expected);
}
