//! Integration tests for team management & project-level RBAC.
//!
//! These tests authenticate via user-scoped Bearer tokens (which the actix test
//! framework preserves across requests, unlike session cookies). Each bearer
//! token is attributed to a user, so the API inherits that user's scope.

use crate::common::TestDb;
use actix_session::{storage::CookieSessionStore, SessionMiddleware};
use actix_web::{cookie::Key, test, web, App};
use chrono::Utc;
use rustrak::config::{Config, DashboardConfig, DatabaseConfig, RateLimitConfig, SecurityConfig};
use rustrak::db::DbPool;
use rustrak::models::{
    CreateAuthToken, CreateProject, CreateUserRequest, Issue, ProjectRole, User, UserRole,
};
use rustrak::routes;
use rustrak::services::grouping::DenormalizedFields;
use rustrak::services::{
    AuthTokenService, IssueService, ProjectMemberService, ProjectService, UsersService,
};
use serde_json::{json, Value};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn create_test_config() -> Config {
    Config {
        host: "127.0.0.1".to_string(),
        port: 0,
        database: DatabaseConfig {
            url: "postgres://test:test@localhost/test".to_string(),
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
        security: SecurityConfig {
            ssl_proxy: false,
            session_secret_key: None,
        },
        ingest_dir: None,
        public_url: None,
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

fn test_session_key() -> Key {
    Key::from(&[0u8; 64])
}

/// Builds the full app under test with all RBAC-relevant routes wired up.
macro_rules! build_app {
    ($pool:expr, $config:expr) => {
        test::init_service(
            App::new()
                .app_data(web::Data::new($pool))
                .app_data(web::Data::new($config))
                .wrap(
                    SessionMiddleware::builder(CookieSessionStore::default(), test_session_key())
                        .cookie_secure(false)
                        .build(),
                )
                .configure(routes::auth::configure)
                .configure(routes::events::configure)
                .configure(routes::issues::configure)
                .configure(routes::members::configure)
                .configure(routes::projects::configure)
                .configure(routes::tokens::configure)
                .configure(routes::team::configure)
                .configure(routes::invitations::configure),
        )
        .await
    };
}

/// Seeds a user with the given global role and returns it.
async fn seed_user(pool: &DbPool, email: &str, role: UserRole) -> User {
    let req = CreateUserRequest {
        email: email.to_string(),
        password: "password123".to_string(),
    };
    UsersService::create_user(pool, &req, role)
        .await
        .expect("seed user")
}

/// Creates a user-scoped bearer token and returns its raw value.
async fn seed_token_for(pool: &DbPool, user_id: i32) -> String {
    AuthTokenService::create_for_user(pool, CreateAuthToken { description: None }, Some(user_id))
        .await
        .expect("seed token")
        .token
}

/// Creates a legacy (user-less) admin-equivalent token.
async fn seed_legacy_token(pool: &DbPool) -> String {
    AuthTokenService::create(pool, CreateAuthToken { description: None })
        .await
        .expect("seed legacy token")
        .token
}

async fn seed_project(pool: &DbPool, name: &str) -> rustrak::models::Project {
    ProjectService::create(
        pool,
        CreateProject {
            name: name.to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .expect("seed project")
}

async fn add_member(pool: &DbPool, project_id: i32, user_id: i32, role: ProjectRole) {
    ProjectMemberService::upsert(pool, project_id, user_id, role)
        .await
        .expect("add member");
}

async fn seed_issue(pool: &DbPool, project_id: i32) -> Issue {
    let denormalized = DenormalizedFields {
        calculated_type: "TypeError".to_string(),
        calculated_value: "boom".to_string(),
        transaction: "/api/test".to_string(),
        last_frame_filename: "test.rs".to_string(),
        last_frame_module: "test_module".to_string(),
        last_frame_function: "test_function".to_string(),
        culprit: "test_function".to_string(),
        logger: String::new(),
        release: String::new(),
    };
    IssueService::create(
        pool,
        project_id,
        Utc::now(),
        &denormalized,
        Some("error"),
        Some("rust"),
    )
    .await
    .expect("seed issue")
}

fn bearer(token: &str) -> (&'static str, String) {
    ("Authorization", format!("Bearer {}", token))
}

// ---------------------------------------------------------------------------
// (a) Admin invites member → accept → user can log in and sees zero projects
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn admin_invites_member_accepts_and_sees_no_projects() {
    let db = TestDb::new().await;
    let config = create_test_config();

    let admin = seed_user(&db.pool, "admin@x.com", UserRole::Admin).await;
    let admin_token = seed_token_for(&db.pool, admin.id).await;

    let app = build_app!(db.pool.clone(), config);

    // Admin creates an invitation.
    let req = test::TestRequest::post()
        .uri("/api/invitations")
        .insert_header(bearer(&admin_token))
        .set_json(json!({ "email": "b@x.com", "role": "member" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201, "invitation should be created");
    let body: Value = test::read_body_json(resp).await;
    let token = body["token"].as_str().expect("invite token present");
    assert!(!token.is_empty());

    // Accept the invitation → creates user + logs in.
    let req = test::TestRequest::post()
        .uri("/auth/accept-invitation")
        .set_json(json!({ "token": token, "password": "newpass123" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201, "accept should create the user");
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["user"]["email"], "b@x.com");
    assert_eq!(body["user"]["role"], "member");

    // The new member can log in.
    let req = test::TestRequest::post()
        .uri("/auth/login")
        .set_json(json!({ "email": "b@x.com", "password": "newpass123" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200, "member can log in");

    // The member sees zero projects.
    let b = UsersService::get_by_email(&db.pool, "b@x.com")
        .await
        .unwrap()
        .unwrap();
    let b_token = seed_token_for(&db.pool, b.id).await;

    // Seed a project they are NOT a member of.
    seed_project(&db.pool, "Untouchable").await;

    let req = test::TestRequest::get()
        .uri("/api/projects")
        .insert_header(bearer(&b_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["items"].as_array().unwrap().len(),
        0,
        "member sees no projects until added to one"
    );
}

// ---------------------------------------------------------------------------
// (b) Viewer can read but not mutate; non-member project is 404
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn viewer_reads_but_cannot_mutate_and_nonmember_is_404() {
    let db = TestDb::new().await;
    let config = create_test_config();

    let member = seed_user(&db.pool, "v@x.com", UserRole::Member).await;
    let token = seed_token_for(&db.pool, member.id).await;

    let p1 = seed_project(&db.pool, "P1").await;
    let p2 = seed_project(&db.pool, "P2").await;
    add_member(&db.pool, p1.id, member.id, ProjectRole::Viewer).await;
    let issue = seed_issue(&db.pool, p1.id).await;

    let app = build_app!(db.pool.clone(), config);

    // GET P1 issues → 200 (viewer can read).
    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/issues", p1.id))
        .insert_header(bearer(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200, "viewer can list issues");

    // PATCH issue → 403 (viewer cannot mutate).
    let req = test::TestRequest::patch()
        .uri(&format!("/api/projects/{}/issues/{}", p1.id, issue.id))
        .insert_header(bearer(&token))
        .set_json(json!({ "is_resolved": true }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403, "viewer cannot resolve issues");

    // DELETE issue → 403.
    let req = test::TestRequest::delete()
        .uri(&format!("/api/projects/{}/issues/{}", p1.id, issue.id))
        .insert_header(bearer(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403, "viewer cannot delete issues");

    // GET P2 (non-member) → 404 (don't leak existence).
    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}", p2.id))
        .insert_header(bearer(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404, "non-member project is hidden as 404");
}

// ---------------------------------------------------------------------------
// (c) Project admin can add an editor, who can then mutate issues
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn project_admin_adds_editor_who_can_mutate() {
    let db = TestDb::new().await;
    let config = create_test_config();

    let padmin = seed_user(&db.pool, "padmin@x.com", UserRole::Member).await;
    let editor = seed_user(&db.pool, "editor@x.com", UserRole::Member).await;
    let padmin_token = seed_token_for(&db.pool, padmin.id).await;
    let editor_token = seed_token_for(&db.pool, editor.id).await;

    let p1 = seed_project(&db.pool, "P1").await;
    add_member(&db.pool, p1.id, padmin.id, ProjectRole::Admin).await;
    let issue = seed_issue(&db.pool, p1.id).await;

    let app = build_app!(db.pool.clone(), config);

    // Project admin adds the editor.
    let req = test::TestRequest::put()
        .uri(&format!("/api/projects/{}/members", p1.id))
        .insert_header(bearer(&padmin_token))
        .set_json(json!({ "user_id": editor.id, "role": "editor" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200, "project admin can add a member");

    // The editor can now resolve an issue.
    let req = test::TestRequest::patch()
        .uri(&format!("/api/projects/{}/issues/{}", p1.id, issue.id))
        .insert_header(bearer(&editor_token))
        .set_json(json!({ "is_resolved": true }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200, "editor can mutate issues");
}

// ---------------------------------------------------------------------------
// (d) Global admin sees all projects and all members
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn global_admin_sees_all_projects_and_members() {
    let db = TestDb::new().await;
    let config = create_test_config();

    let admin = seed_user(&db.pool, "admin@x.com", UserRole::Admin).await;
    let admin_token = seed_token_for(&db.pool, admin.id).await;
    let member = seed_user(&db.pool, "m@x.com", UserRole::Member).await;

    let p1 = seed_project(&db.pool, "P1").await;
    let p2 = seed_project(&db.pool, "P2").await;
    add_member(&db.pool, p1.id, member.id, ProjectRole::Viewer).await;

    let app = build_app!(db.pool.clone(), config);

    // Admin sees both projects, despite not being a project member.
    let req = test::TestRequest::get()
        .uri("/api/projects")
        .insert_header(bearer(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["items"].as_array().unwrap().len(), 2);

    // Admin can list members of P1 (ManageMembers) even though not a member.
    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}/members", p1.id))
        .insert_header(bearer(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body.as_array().unwrap().len(), 1);

    // Admin can view P2 too.
    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}", p2.id))
        .insert_header(bearer(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // Admin can list the team.
    let req = test::TestRequest::get()
        .uri("/api/team")
        .insert_header(bearer(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body.as_array().unwrap().len(), 2, "two users on team");
}

// ---------------------------------------------------------------------------
// (e) Bearer token created by a member only reaches that member's projects
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn member_token_only_reaches_member_projects() {
    let db = TestDb::new().await;
    let config = create_test_config();

    let member = seed_user(&db.pool, "m@x.com", UserRole::Member).await;
    let token = seed_token_for(&db.pool, member.id).await;

    let p1 = seed_project(&db.pool, "P1").await;
    let p2 = seed_project(&db.pool, "P2").await;
    add_member(&db.pool, p1.id, member.id, ProjectRole::Viewer).await;

    let app = build_app!(db.pool.clone(), config);

    // List → only P1.
    let req = test::TestRequest::get()
        .uri("/api/projects")
        .insert_header(bearer(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let ids: Vec<i64> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_i64().unwrap())
        .collect();
    assert_eq!(ids, vec![p1.id as i64]);

    // Direct GET of P2 → 404.
    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{}", p2.id))
        .insert_header(bearer(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);

    // A legacy (user-less) token is admin-equivalent → sees both.
    let legacy = seed_legacy_token(&db.pool).await;
    let req = test::TestRequest::get()
        .uri("/api/projects")
        .insert_header(bearer(&legacy))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["items"].as_array().unwrap().len(), 2);
}

// ---------------------------------------------------------------------------
// (f) POST /auth/register is invite-only (403)
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn register_is_invite_only() {
    let db = TestDb::new().await;
    let config = create_test_config();

    let app = build_app!(db.pool.clone(), config);

    let req = test::TestRequest::post()
        .uri("/auth/register")
        .set_json(json!({ "email": "new@x.com", "password": "password123" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403, "registration is invite-only");

    // No user should have been created.
    assert!(UsersService::get_by_email(&db.pool, "new@x.com")
        .await
        .unwrap()
        .is_none());
}

// ---------------------------------------------------------------------------
// (g) Removing the last project admin returns 409
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn removing_last_project_admin_returns_409() {
    let db = TestDb::new().await;
    let config = create_test_config();

    let admin = seed_user(&db.pool, "admin@x.com", UserRole::Admin).await;
    let admin_token = seed_token_for(&db.pool, admin.id).await;
    let padmin = seed_user(&db.pool, "padmin@x.com", UserRole::Member).await;

    let p1 = seed_project(&db.pool, "P1").await;
    add_member(&db.pool, p1.id, padmin.id, ProjectRole::Admin).await;

    let app = build_app!(db.pool.clone(), config);

    // Global admin tries to remove the only project admin → 409.
    let req = test::TestRequest::delete()
        .uri(&format!("/api/projects/{}/members/{}", p1.id, padmin.id))
        .insert_header(bearer(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 409, "cannot remove the last project admin");
}

// ---------------------------------------------------------------------------
// Bonus: invite with bad role → 400
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn invite_with_bad_role_is_400() {
    let db = TestDb::new().await;
    let config = create_test_config();

    let admin = seed_user(&db.pool, "admin@x.com", UserRole::Admin).await;
    let admin_token = seed_token_for(&db.pool, admin.id).await;

    let app = build_app!(db.pool.clone(), config);

    let req = test::TestRequest::post()
        .uri("/api/invitations")
        .insert_header(bearer(&admin_token))
        .set_json(json!({ "email": "x@x.com", "role": "superuser" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

// ---------------------------------------------------------------------------
// Bonus: non-admin cannot create invitations (403)
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn non_admin_cannot_invite() {
    let db = TestDb::new().await;
    let config = create_test_config();

    let member = seed_user(&db.pool, "m@x.com", UserRole::Member).await;
    let token = seed_token_for(&db.pool, member.id).await;

    let app = build_app!(db.pool.clone(), config);

    let req = test::TestRequest::post()
        .uri("/api/invitations")
        .insert_header(bearer(&token))
        .set_json(json!({ "email": "y@x.com", "role": "member" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}

// ---------------------------------------------------------------------------
// Review patches: hardening edge cases
// ---------------------------------------------------------------------------

/// P5: invitations with a malformed email are rejected (server-side validation).
#[actix_web::test]
async fn invitation_with_invalid_email_is_rejected() {
    let db = TestDb::new().await;
    let config = create_test_config();

    let admin = seed_user(&db.pool, "admin@x.com", UserRole::Admin).await;
    let admin_token = seed_token_for(&db.pool, admin.id).await;
    let app = build_app!(db.pool.clone(), config);

    let req = test::TestRequest::post()
        .uri("/api/invitations")
        .insert_header(bearer(&admin_token))
        .set_json(json!({ "email": "not-an-email", "role": "member" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400, "malformed email should be rejected");
}

/// Accept enforces no length policy (same as login): a short but non-empty
/// password is accepted; only an empty password is rejected.
#[actix_web::test]
async fn accept_invitation_allows_short_password_but_requires_nonempty() {
    let db = TestDb::new().await;
    let config = create_test_config();

    let admin = seed_user(&db.pool, "admin@x.com", UserRole::Admin).await;
    let admin_token = seed_token_for(&db.pool, admin.id).await;
    let app = build_app!(db.pool.clone(), config);

    // Create two invitations to test both branches.
    let mut tokens = Vec::new();
    for email in ["short@x.com", "empty@x.com"] {
        let req = test::TestRequest::post()
            .uri("/api/invitations")
            .insert_header(bearer(&admin_token))
            .set_json(json!({ "email": email, "role": "member" }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        let body: Value = test::read_body_json(resp).await;
        tokens.push(body["token"].as_str().unwrap().to_string());
    }

    // Short (non-empty) password is accepted.
    let req = test::TestRequest::post()
        .uri("/auth/accept-invitation")
        .set_json(json!({ "token": tokens[0], "password": "x" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201, "short password should be accepted");

    // Empty password is rejected.
    let req = test::TestRequest::post()
        .uri("/auth/accept-invitation")
        .set_json(json!({ "token": tokens[1], "password": "" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400, "empty password should be rejected");
}

/// Admins can delete other team members; deletion cascades memberships.
#[actix_web::test]
async fn admin_can_delete_a_member() {
    let db = TestDb::new().await;
    let config = create_test_config();

    let admin = seed_user(&db.pool, "admin@x.com", UserRole::Admin).await;
    let admin_token = seed_token_for(&db.pool, admin.id).await;
    let victim = seed_user(&db.pool, "victim@x.com", UserRole::Member).await;
    let app = build_app!(db.pool.clone(), config);

    let req = test::TestRequest::delete()
        .uri(&format!("/api/team/{}", victim.id))
        .insert_header(bearer(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 204, "admin should delete a member");
    assert!(UsersService::get_by_id(&db.pool, victim.id)
        .await
        .unwrap()
        .is_none());
}

/// The primary (first-registered) user cannot be deleted or demoted.
#[actix_web::test]
async fn primary_user_is_protected() {
    let db = TestDb::new().await;
    let config = create_test_config();

    // First seeded user is the primary (lowest id).
    let primary = seed_user(&db.pool, "primary@x.com", UserRole::Admin).await;
    let other_admin = seed_user(&db.pool, "admin2@x.com", UserRole::Admin).await;
    let other_token = seed_token_for(&db.pool, other_admin.id).await;
    let app = build_app!(db.pool.clone(), config);

    // Another admin cannot demote the primary.
    let req = test::TestRequest::patch()
        .uri(&format!("/api/team/{}/role", primary.id))
        .insert_header(bearer(&other_token))
        .set_json(json!({ "role": "member" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403, "primary cannot be demoted");

    // Another admin cannot delete the primary.
    let req = test::TestRequest::delete()
        .uri(&format!("/api/team/{}", primary.id))
        .insert_header(bearer(&other_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403, "primary cannot be deleted");
}

/// An admin cannot delete their own account.
#[actix_web::test]
async fn admin_cannot_delete_self() {
    let db = TestDb::new().await;
    let config = create_test_config();

    let primary = seed_user(&db.pool, "primary@x.com", UserRole::Admin).await;
    let admin = seed_user(&db.pool, "admin2@x.com", UserRole::Admin).await;
    let token = seed_token_for(&db.pool, admin.id).await;
    let _ = primary;
    let app = build_app!(db.pool.clone(), config);

    let req = test::TestRequest::delete()
        .uri(&format!("/api/team/{}", admin.id))
        .insert_header(bearer(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403, "cannot delete own account");
}

/// P1: a disabled user's bearer token no longer authenticates (mirrors login).
#[actix_web::test]
async fn disabled_user_token_is_rejected() {
    let db = TestDb::new().await;
    let config = create_test_config();

    let member = seed_user(&db.pool, "m@x.com", UserRole::Member).await;
    let token = seed_token_for(&db.pool, member.id).await;

    // Disable the account directly (no toggle endpoint exists yet).
    sqlx::query("UPDATE users SET is_active = $1 WHERE id = $2")
        .bind(false)
        .bind(member.id)
        .execute(&db.pool)
        .await
        .unwrap();

    let app = build_app!(db.pool.clone(), config);

    let req = test::TestRequest::get()
        .uri("/api/projects")
        .insert_header(bearer(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401, "disabled user's token must be rejected");
}

// ---------------------------------------------------------------------------
// (o) Viewer can read but not mutate the GH #165 issue-social/bulk endpoints;
//     editor can. These endpoints had zero dedicated RBAC coverage before —
//     `access::require` was verified correct by code review only.
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn viewer_cannot_mutate_new_issue_endpoints_but_editor_can() {
    let db = TestDb::new().await;
    let config = create_test_config();

    let viewer = seed_user(&db.pool, "viewer@x.com", UserRole::Member).await;
    let viewer_token = seed_token_for(&db.pool, viewer.id).await;
    let editor = seed_user(&db.pool, "editor@x.com", UserRole::Member).await;
    let editor_token = seed_token_for(&db.pool, editor.id).await;

    let project = seed_project(&db.pool, "Social Project").await;
    add_member(&db.pool, project.id, viewer.id, ProjectRole::Viewer).await;
    add_member(&db.pool, project.id, editor.id, ProjectRole::Editor).await;
    let issue = seed_issue(&db.pool, project.id).await;

    let app = build_app!(db.pool.clone(), config);

    // --- MutateIssue-gated endpoints: viewer 403, editor succeeds ---

    // Bulk update (PUT /issues).
    let req = test::TestRequest::put()
        .uri(&format!("/api/projects/{}/issues", project.id))
        .insert_header(bearer(&viewer_token))
        .set_json(json!({ "ids": [issue.id], "status": "resolved" }))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        403,
        "viewer cannot bulk-update issues"
    );
    let req = test::TestRequest::put()
        .uri(&format!("/api/projects/{}/issues", project.id))
        .insert_header(bearer(&editor_token))
        .set_json(json!({ "ids": [issue.id], "status": "resolved" }))
        .to_request();
    assert!(
        test::call_service(&app, req).await.status().is_success(),
        "editor can bulk-update issues"
    );

    // Bulk delete (DELETE /issues) — reseed a fresh issue since the one
    // above just got resolved, not deleted.
    let issue2 = seed_issue(&db.pool, project.id).await;
    let req = test::TestRequest::delete()
        .uri(&format!("/api/projects/{}/issues", project.id))
        .insert_header(bearer(&viewer_token))
        .set_json(json!({ "ids": [issue2.id] }))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        403,
        "viewer cannot bulk-delete issues"
    );

    // Comment (POST .../comments).
    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/projects/{}/issues/{}/comments",
            project.id, issue.id
        ))
        .insert_header(bearer(&viewer_token))
        .set_json(json!({ "text": "nope" }))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        403,
        "viewer cannot comment on issues"
    );
    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/projects/{}/issues/{}/comments",
            project.id, issue.id
        ))
        .insert_header(bearer(&editor_token))
        .set_json(json!({ "text": "editor comment" }))
        .to_request();
    assert!(
        test::call_service(&app, req).await.status().is_success(),
        "editor can comment on issues"
    );

    // User report (POST .../user-reports).
    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/projects/{}/issues/{}/user-reports",
            project.id, issue.id
        ))
        .insert_header(bearer(&viewer_token))
        .set_json(json!({ "comments": "nope" }))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        403,
        "viewer cannot submit a user report"
    );

    // --- ViewProject-gated endpoints: viewer succeeds (read + per-user toggles) ---

    for (method, path) in [
        (
            "get",
            format!("/api/projects/{}/issues/{}/hashes", project.id, issue.id),
        ),
        (
            "get",
            format!(
                "/api/projects/{}/issues/{}/aggregates",
                project.id, issue.id
            ),
        ),
        (
            "get",
            format!("/api/projects/{}/issues/{}/stats", project.id, issue.id),
        ),
        (
            "get",
            format!("/api/projects/{}/issues/{}/activity", project.id, issue.id),
        ),
        (
            "get",
            format!(
                "/api/projects/{}/issues/{}/user-reports",
                project.id, issue.id
            ),
        ),
    ] {
        let req = match method {
            "get" => test::TestRequest::get(),
            _ => unreachable!(),
        }
        .uri(&path)
        .insert_header(bearer(&viewer_token))
        .to_request();
        let status = test::call_service(&app, req).await.status();
        assert!(
            status.is_success(),
            "viewer should be able to GET {} (got {})",
            path,
            status
        );
    }

    // Bookmark/subscription/seen are per-user preferences, not project
    // mutations — a viewer (ViewProject) can toggle their own.
    let req = test::TestRequest::put()
        .uri(&format!(
            "/api/projects/{}/issues/{}/bookmark",
            project.id, issue.id
        ))
        .insert_header(bearer(&viewer_token))
        .set_json(json!({ "enabled": true }))
        .to_request();
    assert!(
        test::call_service(&app, req).await.status().is_success(),
        "viewer can bookmark an issue"
    );
    let req = test::TestRequest::put()
        .uri(&format!(
            "/api/projects/{}/issues/{}/subscription",
            project.id, issue.id
        ))
        .insert_header(bearer(&viewer_token))
        .set_json(json!({ "enabled": true }))
        .to_request();
    assert!(
        test::call_service(&app, req).await.status().is_success(),
        "viewer can subscribe to an issue"
    );
    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/projects/{}/issues/{}/seen",
            project.id, issue.id
        ))
        .insert_header(bearer(&viewer_token))
        .to_request();
    assert!(
        test::call_service(&app, req).await.status().is_success(),
        "viewer can mark an issue seen"
    );
}
