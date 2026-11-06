//! Endpoints enforce exactly the permission they need, on the scope it applies to.

use crate::common::TestDb;
use crate::integration::team_rbac_test::create_test_config;
use actix_web::{test, web, App};
use rustrak::auth::rbac::{Action, Resource};
use rustrak::db::DbPool;
use rustrak::models::{CreateAuthToken, CreateProject, CreateUserRequest, Permission, UserRole};
use rustrak::routes;
use rustrak::services::{AuthTokenService, ProjectService, RoleService, UsersService};
use serde_json::Value;

macro_rules! app {
    ($pool:expr) => {
        test::init_service(
            App::new()
                .app_data(web::Data::new($pool))
                .app_data(web::Data::new(create_test_config()))
                .configure(routes::issues::configure)
                .configure(routes::logs::configure)
                .configure(routes::alerts::configure)
                .configure(routes::projects::configure),
        )
        .await
    };
}

/// A user holding one custom role made of `perms`, on the instance or one
/// project, and a bearer token for them.
async fn user_with(pool: &DbPool, perms: &[(Resource, Action)], project: Option<i32>) -> String {
    let n = RoleService::list(pool).await.unwrap().len();
    let email = format!("u{n}@example.com");
    let user = UsersService::create_user(
        pool,
        &CreateUserRequest {
            email,
            password: "password123".into(),
        },
        UserRole::Member,
    )
    .await
    .unwrap();
    let role = RoleService::create(
        pool,
        &format!("custom {n}"),
        "",
        perms
            .iter()
            .map(|&(resource, action)| Permission { resource, action })
            .collect(),
    )
    .await
    .unwrap();
    RoleService::assign(pool, user.id, role.id, project)
        .await
        .unwrap();
    AuthTokenService::create_for_user(pool, CreateAuthToken::default(), Some(user.id))
        .await
        .unwrap()
        .token
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

async fn status<S>(app: &S, method: &str, uri: &str, token: &str) -> u16
where
    S: actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
{
    let req = match method {
        "GET" => test::TestRequest::get(),
        "DELETE" => test::TestRequest::delete(),
        _ => unreachable!(),
    }
    .uri(uri)
    .insert_header(("Authorization", format!("Bearer {token}")))
    .to_request();
    test::call_service(app, req).await.status().as_u16()
}

/// Reading an integration shows it is there; only who may change it sees what
/// it holds.
#[actix_web::test]
async fn integration_credentials_are_masked_without_integrations_update() {
    let db = TestDb::new().await;
    let writer = user_with(
        &db.pool,
        &[
            (Resource::Integrations, Action::Create),
            (Resource::Integrations, Action::Update),
        ],
        None,
    )
    .await;
    let reader = user_with(&db.pool, &[(Resource::Integrations, Action::Read)], None).await;
    let creator = user_with(&db.pool, &[(Resource::Integrations, Action::Create)], None).await;
    let app = app!(db.pool.clone());

    let req = test::TestRequest::post()
        .uri("/api/integrations")
        .insert_header(("Authorization", format!("Bearer {writer}")))
        .set_json(serde_json::json!({
            "name": "hook",
            "provider_type": "webhook",
            "credentials": {
                "url": "https://example.com/SECRETPATH",
                "secret": "SECRETVALUE",
                "headers": { "X-Key": "SECRETHEADER" },
            },
        }))
        .to_request();
    let created: Value = test::call_and_read_body_json(&app, req).await;
    let id = created["id"].as_i64().unwrap();

    for token in [&reader, &creator] {
        for uri in [
            "/api/integrations".to_string(),
            format!("/api/integrations/{id}"),
        ] {
            let req = test::TestRequest::get()
                .uri(&uri)
                .insert_header(("Authorization", format!("Bearer {token}")))
                .to_request();
            let body = test::call_and_read_body(&app, req).await;
            let text = String::from_utf8_lossy(&body);
            assert!(text.contains("hook"), "{uri}: {text}");
            assert!(!text.contains("SECRET"), "{uri}: {text}");
        }
    }

    let req = test::TestRequest::get()
        .uri(&format!("/api/integrations/{id}"))
        .insert_header(("Authorization", format!("Bearer {writer}")))
        .to_request();
    let full: Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(full["credentials"]["secret"], "SECRETVALUE");
}

#[actix_web::test]
async fn a_settings_permission_opens_only_that_settings_area() {
    let db = TestDb::new().await;
    let p = project(&db.pool, "p").await;
    let token = user_with(&db.pool, &[(Resource::Integrations, Action::Read)], None).await;
    let app = app!(db.pool.clone());

    assert_eq!(status(&app, "GET", "/api/integrations", &token).await, 200);
    assert_eq!(
        status(&app, "DELETE", "/api/integrations/999", &token).await,
        403
    );
    assert_eq!(
        status(&app, "GET", &format!("/api/projects/{p}/issues"), &token).await,
        404,
        "a settings role sees no project"
    );
}

#[actix_web::test]
async fn a_project_role_reaches_only_its_project_and_its_resources() {
    let db = TestDb::new().await;
    let p1 = project(&db.pool, "p1").await;
    let p2 = project(&db.pool, "p2").await;
    let token = user_with(&db.pool, &[(Resource::Logs, Action::Read)], Some(p1)).await;
    let app = app!(db.pool.clone());

    assert_eq!(
        status(&app, "GET", &format!("/api/projects/{p1}/logs"), &token).await,
        200
    );
    assert_eq!(
        status(&app, "GET", &format!("/api/projects/{p1}/issues"), &token).await,
        403
    );
    assert_eq!(
        status(&app, "GET", &format!("/api/projects/{p2}/logs"), &token).await,
        404
    );
    assert_eq!(status(&app, "GET", "/api/integrations", &token).await, 403);

    let req = test::TestRequest::get()
        .uri("/api/projects")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, req).await).await;
    let ids: Vec<i64> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_i64().unwrap())
        .collect();
    assert_eq!(ids, vec![p1 as i64]);
}

#[actix_web::test]
async fn an_instance_project_permission_reaches_every_project() {
    let db = TestDb::new().await;
    let p1 = project(&db.pool, "p1").await;
    let p2 = project(&db.pool, "p2").await;
    let token = user_with(&db.pool, &[(Resource::Issues, Action::Read)], None).await;
    let app = app!(db.pool.clone());

    for p in [p1, p2] {
        assert_eq!(
            status(&app, "GET", &format!("/api/projects/{p}/issues"), &token).await,
            200
        );
    }
}

#[actix_web::test]
async fn creating_a_project_needs_the_permission_and_a_way_to_reach_it() {
    let db = TestDb::new().await;
    let without = user_with(&db.pool, &[(Resource::Issues, Action::Read)], None).await;
    let blind = user_with(&db.pool, &[(Resource::Projects, Action::Create)], None).await;
    let with = user_with(
        &db.pool,
        &[
            (Resource::Projects, Action::Create),
            (Resource::Issues, Action::Read),
        ],
        None,
    )
    .await;
    let app = app!(db.pool.clone());

    let create = |token: &str| {
        test::TestRequest::post()
            .uri("/api/projects")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(serde_json::json!({ "name": "new" }))
            .to_request()
    };
    assert_eq!(
        test::call_service(&app, create(&without)).await.status(),
        403
    );
    assert_eq!(
        test::call_service(&app, create(&blind)).await.status(),
        403,
        "they could never see what they created"
    );
    let resp = test::call_service(&app, create(&with)).await;
    assert_eq!(resp.status(), 201);
    let id = test::read_body_json::<Value, _>(resp).await["id"]
        .as_i64()
        .unwrap();
    assert_eq!(
        status(&app, "GET", &format!("/api/projects/{id}/issues"), &with).await,
        200
    );
}
