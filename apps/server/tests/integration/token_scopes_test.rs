//! API tokens: hashed at rest, limited to chosen permissions and projects,
//! never beyond their owner, and optionally expiring.

use crate::common::TestDb;
use crate::integration::team_rbac_test::create_test_config;
use actix_session::{storage::CookieSessionStore, SessionMiddleware};
use actix_web::{
    cookie::{Cookie, Key},
    test, web, App,
};
use rustrak::auth::rbac::{Action, Resource};
use rustrak::db::DbPool;
use rustrak::models::{CreateProject, CreateUserRequest, UserRole};
use rustrak::routes;
use rustrak::services::{AuthTokenService, ProjectService, UsersService};
use serde_json::{json, Value};

macro_rules! app {
    ($pool:expr) => {
        test::init_service(
            App::new()
                .app_data(web::Data::new($pool))
                .app_data(web::Data::new(create_test_config()))
                .app_data(web::Data::new(None::<rustrak::auth::OidcService>))
                .wrap(
                    SessionMiddleware::builder(
                        CookieSessionStore::default(),
                        Key::from(&[0u8; 64]),
                    )
                    .cookie_secure(false)
                    .build(),
                )
                .configure(routes::auth::configure)
                .configure(routes::tokens::configure)
                .configure(routes::issues::configure)
                .configure(routes::logs::configure)
                .configure(routes::alerts::configure)
                .configure(routes::projects::configure),
        )
        .await
    };
}

async fn status<S>(app: &S, uri: &str, token: &str) -> u16
where
    S: actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
{
    let req = test::TestRequest::get()
        .uri(uri)
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    test::call_service(app, req).await.status().as_u16()
}

async fn owner_token(pool: &DbPool) -> (i32, String) {
    let u = UsersService::create_user(
        pool,
        &CreateUserRequest {
            email: "o@x.com".into(),
            password: "password123".into(),
        },
        UserRole::Admin,
    )
    .await
    .unwrap();
    let t = AuthTokenService::create_for_user(
        pool,
        rustrak::models::CreateAuthToken::default(),
        Some(u.id),
    )
    .await
    .unwrap();
    (u.id, t.token)
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

/// Signs in, for what only a session may do.
async fn login<S>(app: &S, email: &str) -> Cookie<'static>
where
    S: actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
{
    let req = test::TestRequest::post()
        .uri("/auth/login")
        .set_json(json!({ "email": email, "password": "password123" }))
        .to_request();
    let resp = test::call_service(app, req).await;
    assert_eq!(resp.status(), 200);
    let cookie = resp.response().cookies().next().unwrap().into_owned();
    cookie
}

async fn create<S>(app: &S, session: &Cookie<'static>, body: Value) -> (u16, Value)
where
    S: actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
{
    let req = test::TestRequest::post()
        .uri("/api/tokens")
        .cookie(session.clone())
        .set_json(body)
        .to_request();
    let resp = test::call_service(app, req).await;
    let status = resp.status().as_u16();
    (status, test::read_body_json(resp).await)
}

#[actix_web::test]
async fn a_token_is_limited_to_its_permissions_and_projects() {
    let db = TestDb::new().await;
    owner_token(&db.pool).await;
    let p1 = project(&db.pool, "p1").await;
    let p2 = project(&db.pool, "p2").await;
    let app = app!(db.pool.clone());
    let session = login(&app, "o@x.com").await;

    let (code, body) = create(
        &app,
        &session,
        json!({
            "description": "ci",
            "permissions": [{ "resource": "logs", "action": "read" }],
            "project_ids": [p1],
        }),
    )
    .await;
    assert_eq!(code, 201, "{body}");
    assert_eq!(
        body["permissions"],
        json!([{ "resource": "logs", "action": "read" }])
    );
    assert_eq!(body["project_ids"], json!([p1]));
    let scoped = body["token"].as_str().unwrap().to_string();

    assert_eq!(
        status(&app, &format!("/api/projects/{p1}/logs"), &scoped).await,
        200
    );
    assert_eq!(
        status(&app, &format!("/api/projects/{p1}/issues"), &scoped).await,
        403
    );
    assert_eq!(
        status(&app, &format!("/api/projects/{p2}/logs"), &scoped).await,
        404
    );
    assert_eq!(status(&app, "/api/integrations", &scoped).await, 403);
}

#[actix_web::test]
async fn a_token_never_exceeds_its_owner() {
    let db = TestDb::new().await;
    let p = project(&db.pool, "p").await;
    let member = UsersService::create_user(
        &db.pool,
        &CreateUserRequest {
            email: "m@x.com".into(),
            password: "password123".into(),
        },
        UserRole::Member,
    )
    .await
    .unwrap();
    crate::common::grant_project_role(&db.pool, p, member.id, "Viewer")
        .await
        .unwrap();
    let t = AuthTokenService::create_for_user(
        &db.pool,
        rustrak::models::CreateAuthToken::default(),
        Some(member.id),
    )
    .await
    .unwrap();
    let app = app!(db.pool.clone());

    assert_eq!(
        status(&app, &format!("/api/projects/{p}/issues"), &t.token).await,
        200
    );
    assert_eq!(status(&app, "/api/integrations", &t.token).await, 403);
}

#[actix_web::test]
async fn a_token_cannot_create_tokens() {
    let db = TestDb::new().await;
    let (owner_id, unrestricted) = owner_token(&db.pool).await;
    let limited = AuthTokenService::create_for_user(
        &db.pool,
        rustrak::models::CreateAuthToken {
            permissions: Some(vec![rustrak::models::Permission {
                resource: Resource::Logs,
                action: Action::Read,
            }]),
            ..Default::default()
        },
        Some(owner_id),
    )
    .await
    .unwrap()
    .token;
    let legacy = AuthTokenService::create(&db.pool, Default::default())
        .await
        .unwrap()
        .token;
    let app = app!(db.pool.clone());

    for token in [&unrestricted, &limited, &legacy] {
        let req = test::TestRequest::post()
            .uri("/api/tokens")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({ "description": "minted" }))
            .to_request();
        assert_eq!(test::call_service(&app, req).await.status(), 403);
    }
    let minted: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM auth_tokens WHERE description = 'minted'")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(minted, 0);
}

#[actix_web::test]
async fn a_token_cannot_be_given_what_its_owner_does_not_hold() {
    let db = TestDb::new().await;
    let p = project(&db.pool, "p").await;
    let member = UsersService::create_user(
        &db.pool,
        &CreateUserRequest {
            email: "m@x.com".into(),
            password: "password123".into(),
        },
        UserRole::Member,
    )
    .await
    .unwrap();
    crate::common::grant_project_role(&db.pool, p, member.id, "Viewer")
        .await
        .unwrap();
    let app = app!(db.pool.clone());
    let session = login(&app, "m@x.com").await;

    let (code, _) = create(
        &app,
        &session,
        json!({ "permissions": [{ "resource": "storage", "action": "delete" }] }),
    )
    .await;
    assert_eq!(code, 403);
    let (code, body) = create(
        &app,
        &session,
        json!({ "permissions": [{ "resource": "logs", "action": "read" }] }),
    )
    .await;
    assert_eq!(code, 201, "held on one project is enough: {body}");
}

#[actix_web::test]
async fn an_expired_token_is_refused() {
    let db = TestDb::new().await;
    owner_token(&db.pool).await;
    let app = app!(db.pool.clone());
    let session = login(&app, "o@x.com").await;
    let (code, body) = create(
        &app,
        &session,
        json!({ "description": "old", "expires_at": "2000-01-01T00:00:00Z" }),
    )
    .await;
    assert_eq!(code, 201, "{body}");
    let expired = body["token"].as_str().unwrap();
    assert_eq!(status(&app, "/api/tokens", expired).await, 401);
}

#[actix_web::test]
async fn tokens_are_stored_hashed_and_shown_only_once() {
    let db = TestDb::new().await;
    let (_, owner) = owner_token(&db.pool).await;
    let app = app!(db.pool.clone());
    let session = login(&app, "o@x.com").await;
    let (_, body) = create(&app, &session, json!({ "description": "x" })).await;
    let raw = body["token"].as_str().unwrap();
    let id = body["id"].as_i64().unwrap();

    let stored: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM auth_tokens WHERE token = $1")
        .bind(raw)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(stored, 0, "the raw token is never stored");

    let req = test::TestRequest::get()
        .uri(&format!("/api/tokens/{id}"))
        .insert_header(("Authorization", format!("Bearer {owner}")))
        .to_request();
    let shown: Value = test::read_body_json(test::call_service(&app, req).await).await;
    assert!(shown.get("token").is_none(), "{shown}");
    assert_eq!(shown["token_prefix"], format!("{}...", &raw[..8]));
}

#[actix_web::test]
async fn tokens_written_before_hashing_keep_working_once_hashed() {
    let db = TestDb::new().await;
    let raw = "0123456789abcdef0123456789abcdef01234567";
    sqlx::query("INSERT INTO auth_tokens (token, description) VALUES ($1, 'legacy')")
        .bind(raw)
        .execute(&db.pool)
        .await
        .unwrap();
    AuthTokenService::hash_plaintext_tokens(&db.pool)
        .await
        .unwrap();
    assert!(AuthTokenService::get_by_token(&db.pool, raw)
        .await
        .unwrap()
        .is_some());
    let app = app!(db.pool.clone());
    assert_eq!(status(&app, "/api/tokens", raw).await, 200);
}
