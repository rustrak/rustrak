//! Integration tests for the Authentication API
//!
//! Tests the complete authentication flow with a real PostgreSQL database.
//! Covers: login, logout, get current user, and session management.

use crate::common::TestDb;
use actix_web::test;
use rustrak::models::user::OidcIdentity;
use rustrak::models::User;
use rustrak::services::{OidcLinkPolicy, OidcOutcome, UsersService};
use serde_json::{json, Value};

/// Helper to create test user directly in DB
async fn create_test_user(
    pool: &rustrak::db::DbPool,
    email: &str,
    password: &str,
    is_admin: bool,
) -> User {
    let req = rustrak::models::CreateUserRequest {
        email: email.to_string(),
        password: password.to_string(),
    };
    let role = if is_admin {
        rustrak::models::UserRole::Admin
    } else {
        rustrak::models::UserRole::Member
    };
    UsersService::create_user(pool, &req, role)
        .await
        .expect("Failed to create test user")
}

#[actix_web::test]
async fn test_login_success() {
    let db = TestDb::new().await;
    let config = crate::common::config();

    create_test_user(&db.pool, "logintest@example.com", "password123", false).await;

    let app = crate::common::app_with_config(&db.pool, config).await;

    let req = test::TestRequest::post()
        .uri("/auth/login")
        .insert_header(("Content-Type", "application/json"))
        .set_json(json!({
            "email": "logintest@example.com",
            "password": "password123"
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["user"]["email"], "logintest@example.com");
    assert!(body["user"]["id"].is_number());
}

#[actix_web::test]
async fn test_login_wrong_password() {
    let db = TestDb::new().await;
    let config = crate::common::config();

    create_test_user(&db.pool, "wrongpass@example.com", "correctpassword", false).await;

    let app = crate::common::app_with_config(&db.pool, config).await;

    let req = test::TestRequest::post()
        .uri("/auth/login")
        .insert_header(("Content-Type", "application/json"))
        .set_json(json!({
            "email": "wrongpass@example.com",
            "password": "wrongpassword"
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn test_login_nonexistent_user() {
    let db = TestDb::new().await;
    let config = crate::common::config();

    let app = crate::common::app_with_config(&db.pool, config).await;

    let req = test::TestRequest::post()
        .uri("/auth/login")
        .insert_header(("Content-Type", "application/json"))
        .set_json(json!({
            "email": "nonexistent@example.com",
            "password": "password123"
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn test_login_inactive_user() {
    let db = TestDb::new().await;
    let config = crate::common::config();

    let user = create_test_user(&db.pool, "inactive@example.com", "password123", false).await;
    sqlx::query("UPDATE users SET is_active = false WHERE id = $1")
        .bind(user.id)
        .execute(&db.pool)
        .await
        .unwrap();

    let app = crate::common::app_with_config(&db.pool, config).await;

    let req = test::TestRequest::post()
        .uri("/auth/login")
        .insert_header(("Content-Type", "application/json"))
        .set_json(json!({
            "email": "inactive@example.com",
            "password": "password123"
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn test_login_updates_last_login() {
    let db = TestDb::new().await;
    let config = crate::common::config();

    let user = create_test_user(&db.pool, "lastlogin@example.com", "password123", false).await;

    // Initially, last_login should be None
    assert!(user.last_login.is_none());

    let app = crate::common::app_with_config(&db.pool, config).await;

    let req = test::TestRequest::post()
        .uri("/auth/login")
        .insert_header(("Content-Type", "application/json"))
        .set_json(json!({
            "email": "lastlogin@example.com",
            "password": "password123"
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let updated_user = UsersService::get_by_email(&db.pool, "lastlogin@example.com")
        .await
        .unwrap()
        .unwrap();
    assert!(updated_user.last_login.is_some());
}

#[actix_web::test]
async fn test_logout_success() {
    let db = TestDb::new().await;
    let config = crate::common::config();

    let app = crate::common::app_with_config(&db.pool, config).await;

    let req = test::TestRequest::post().uri("/auth/logout").to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 204);
}

#[actix_web::test]
async fn test_get_current_user_authenticated() {
    let db = TestDb::new().await;
    let config = crate::common::config();

    create_test_user(&db.pool, "currentuser@example.com", "password123", false).await;

    let app = crate::common::app_with_config(&db.pool, config).await;

    let login_req = test::TestRequest::post()
        .uri("/auth/login")
        .insert_header(("Content-Type", "application/json"))
        .set_json(json!({
            "email": "currentuser@example.com",
            "password": "password123"
        }))
        .to_request();

    let login_resp = test::call_service(&app, login_req).await;
    assert_eq!(login_resp.status(), 200);

    // Extract session cookie
    let cookies: Vec<_> = login_resp.headers().get_all("set-cookie").collect();
    assert!(!cookies.is_empty());

    let cookie_value = cookies[0].to_str().unwrap();

    let me_req = test::TestRequest::get()
        .uri("/auth/me")
        .insert_header(("Cookie", cookie_value))
        .to_request();

    let me_resp = test::call_service(&app, me_req).await;
    assert_eq!(me_resp.status(), 200);

    let body: Value = test::read_body_json(me_resp).await;
    assert_eq!(body["email"], "currentuser@example.com");
}

#[actix_web::test]
async fn test_password_is_hashed_in_database() {
    let db = TestDb::new().await;

    // Registration is invite-only, so create the user directly via the service
    // (the service is what performs the Argon2 hashing).
    create_test_user(&db.pool, "hashtest@example.com", "mysecretpassword", false).await;

    let user = UsersService::get_by_email(&db.pool, "hashtest@example.com")
        .await
        .unwrap()
        .unwrap();

    // Password hash should NOT be the plain password
    assert_ne!(user.password_hash, "mysecretpassword");

    // Password hash should be Argon2 format (starts with $argon2)
    assert!(user.password_hash.starts_with("$argon2"));
}

#[actix_web::test]
async fn test_different_passwords_produce_different_hashes() {
    let db = TestDb::new().await;

    let user1 = create_test_user(&db.pool, "user1@example.com", "password123", false).await;
    let user2 = create_test_user(&db.pool, "user2@example.com", "password123", false).await;

    // Even with same password, hashes should be different (due to random salt)
    assert_ne!(user1.password_hash, user2.password_hash);
}

#[actix_web::test]
async fn test_middleware_allows_authenticated_access() {
    let db = TestDb::new().await;
    let config = crate::common::config();

    create_test_user(&db.pool, "authuser@example.com", "password123", false).await;

    let app = crate::common::app_with_config(&db.pool, config).await;

    let login_req = test::TestRequest::post()
        .uri("/auth/login")
        .insert_header(("Content-Type", "application/json"))
        .set_json(json!({
            "email": "authuser@example.com",
            "password": "password123"
        }))
        .to_request();

    let login_resp = test::call_service(&app, login_req).await;
    let cookies: Vec<_> = login_resp.headers().get_all("set-cookie").collect();
    let cookie_value = cookies[0].to_str().unwrap();

    let req = test::TestRequest::get()
        .uri("/api/projects")
        .insert_header(("Cookie", cookie_value))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn test_middleware_exempts_auth_routes() {
    let db = TestDb::new().await;
    let config = crate::common::config();

    let app = crate::common::app_with_config(&db.pool, config).await;

    // Auth routes are reachable without a session: SSO config answers 200.
    let req = test::TestRequest::get()
        .uri("/auth/sso/config")
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn test_middleware_exempts_health_routes() {
    let db = TestDb::new().await;
    let config = crate::common::config();

    let app = crate::common::app_with_config(&db.pool, config).await;

    // Health routes should be accessible without authentication
    let req = test::TestRequest::get().uri("/health").to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn test_login_case_insensitive_email() {
    let db = TestDb::new().await;
    let config = crate::common::config();

    create_test_user(&db.pool, "CaseSensitive@example.com", "password123", false).await;

    // Stored addresses are normalized to lowercase.
    let stored = UsersService::get_by_email(&db.pool, "casesensitive@example.com")
        .await
        .unwrap()
        .expect("user exists");
    assert_eq!(stored.email, "casesensitive@example.com");

    let app = crate::common::app_with_config(&db.pool, config).await;

    // Login works with any casing of the same address
    for email in [
        "casesensitive@example.com",
        "CaseSensitive@example.com",
        "CASESENSITIVE@EXAMPLE.COM",
        "  CaseSensitive@Example.com  ",
    ] {
        let req = test::TestRequest::post()
            .uri("/auth/login")
            .insert_header(("Content-Type", "application/json"))
            .set_json(json!({
                "email": email,
                "password": "password123"
            }))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200, "login with {email:?} should succeed");
    }
}

/// Inserts a user as rows were written before email normalization existed.
async fn insert_legacy_user(pool: &rustrak::db::DbPool, email: &str, password: &str) {
    sqlx::query(
        "INSERT INTO users (email, password_hash, is_active, role) VALUES ($1, $2, true, 'member')",
    )
    .bind(email)
    .bind(User::hash_password(password).unwrap())
    .execute(pool)
    .await
    .expect("Failed to insert legacy user");
}

/// Logs in and returns the status and the email of the account signed into.
async fn login_as(pool: &rustrak::db::DbPool, email: &str, password: &str) -> (u16, String) {
    let app = crate::common::app_with_config(pool, crate::common::config()).await;

    let req = test::TestRequest::post()
        .uri("/auth/login")
        .set_json(json!({ "email": email, "password": password }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status().as_u16();
    let body: Value = test::read_body_json(resp).await;
    let signed_in = body["user"]["email"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    (status, signed_in)
}

#[actix_web::test]
async fn test_login_legacy_case_variant_accounts_stay_reachable() {
    let db = TestDb::new().await;
    insert_legacy_user(&db.pool, "user@example.com", "lower-pass").await;
    insert_legacy_user(&db.pool, "User@Example.com", "upper-pass").await;

    // Each historical account is reached by its exact stored casing.
    assert_eq!(
        login_as(&db.pool, "user@example.com", "lower-pass").await,
        (200, "user@example.com".to_string())
    );
    assert_eq!(
        login_as(&db.pool, "User@Example.com", "upper-pass").await,
        (200, "User@Example.com".to_string())
    );

    // Any other casing resolves deterministically to the oldest account.
    assert_eq!(
        login_as(&db.pool, "USER@EXAMPLE.COM", "lower-pass").await,
        (200, "user@example.com".to_string())
    );
}

/// An invitation created before emails were normalized can name a case
/// variant of an account that already exists. Accepting it must not create a
/// second account that `get_by_email` then treats as the same person.
#[actix_web::test]
async fn test_accepting_an_invitation_for_an_existing_case_variant_is_refused() {
    let db = TestDb::new().await;
    insert_legacy_user(&db.pool, "Bob@Example.com", "password123").await;
    sqlx::query(
        "INSERT INTO invitations (token, email, role, status, expires_at) VALUES ($1, $2, 'member', 'pending', $3)",
    )
    .bind("legacy-invite")
    .bind("bob@example.com")
    .bind(chrono::Utc::now() + chrono::Duration::days(1))
    .execute(&db.pool)
    .await
    .expect("Failed to insert legacy invitation");

    let result =
        rustrak::services::InvitationService::accept(&db.pool, "legacy-invite", "new-pass").await;

    assert!(
        matches!(result, Err(rustrak::error::AppError::Conflict(_))),
        "expected a conflict, got {:?}",
        result.map(|user| user.email)
    );
    let accounts: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE LOWER(email) = 'bob@example.com'")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(accounts, 1);
}

// Postgres' email_format constraint rejects non-ASCII addresses.
#[cfg(feature = "sqlite")]
#[actix_web::test]
async fn test_login_legacy_non_ascii_email_with_exact_casing() {
    let db = TestDb::new().await;
    insert_legacy_user(&db.pool, "Üser@Example.com", "password123").await;

    let (status, _) = login_as(&db.pool, "Üser@Example.com", "password123").await;
    assert_eq!(status, 200);
}

/// A user who has never chosen reports no preference, rather than a default.
///
/// The distinction is load-bearing: `null` means "has not chosen", which lets
/// the dashboard fall back to `Accept-Language`. A column defaulting to `'en'`
/// would force English on a reader who never touched the setting and make that
/// fallback unreachable.
#[actix_web::test]
async fn test_new_user_has_no_language_or_timezone_preference() {
    let db = TestDb::new().await;
    let config = crate::common::config();

    // Seeded: sign-up is by invitation only.
    create_test_user(&db.pool, "prefs@example.com", "password123", false).await;

    let app = crate::common::app_with_config(&db.pool, config).await;

    let login = test::TestRequest::post()
        .uri("/auth/login")
        .set_json(json!({ "email": "prefs@example.com", "password": "password123" }))
        .to_request();
    let resp = test::call_service(&app, login).await;
    assert_eq!(resp.status(), 200);
    let cookie = resp.response().cookies().next().unwrap().into_owned();

    let me = test::TestRequest::get()
        .uri("/auth/me")
        .cookie(cookie)
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, me).await;

    // Presence first, then the value. `body["missing"]` is `Null` in
    // serde_json, so asserting only the value would pass on a response that
    // carries no such field at all -- which is exactly the state this test was
    // written to reject.
    assert!(
        body.get("language").is_some(),
        "response carries no `language` field: {body}"
    );
    assert!(
        body.get("timezone").is_some(),
        "response carries no `timezone` field: {body}"
    );
    assert_eq!(body["language"], Value::Null, "language should start unset");
    assert_eq!(body["timezone"], Value::Null, "timezone should start unset");
}

/// A chosen preference survives the request that set it.
///
/// Asserted by reading it back through `/auth/me` rather than by querying the
/// table: the endpoint is the contract, and a test that checks the column
/// directly would keep passing if the read stopped exposing it.
#[actix_web::test]
async fn test_patch_me_persists_language_and_timezone() {
    let db = TestDb::new().await;
    let config = crate::common::config();

    create_test_user(&db.pool, "prefs2@example.com", "password123", false).await;

    let app = crate::common::app_with_config(&db.pool, config).await;

    let login = test::TestRequest::post()
        .uri("/auth/login")
        .set_json(json!({ "email": "prefs2@example.com", "password": "password123" }))
        .to_request();
    let resp = test::call_service(&app, login).await;
    let cookie = resp.response().cookies().next().unwrap().into_owned();

    let patch = test::TestRequest::patch()
        .uri("/auth/me")
        .cookie(cookie.clone())
        .set_json(json!({ "language": "zh", "timezone": "Asia/Tokyo" }))
        .to_request();
    let resp = test::call_service(&app, patch).await;
    assert_eq!(resp.status(), 200, "PATCH /auth/me should succeed");

    let me = test::TestRequest::get()
        .uri("/auth/me")
        .cookie(cookie)
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, me).await;

    assert_eq!(body["language"], "zh");
    assert_eq!(body["timezone"], "Asia/Tokyo");
}

/// The clock preference starts unset, is stored when chosen and is cleared by
/// an explicit `null`, without touching the other preferences.
///
/// Unset is its own state rather than `false`: it lets the dashboard keep the
/// language's own clock (24-hour in French, 12-hour in English) for a reader
/// who never opened the setting.
#[actix_web::test]
async fn test_patch_me_sets_and_clears_the_24_hour_clock() {
    let db = TestDb::new().await;
    let config = crate::common::config();

    create_test_user(&db.pool, "clock@example.com", "password123", false).await;

    let app = crate::common::app_with_config(&db.pool, config).await;

    let login = test::TestRequest::post()
        .uri("/auth/login")
        .set_json(json!({ "email": "clock@example.com", "password": "password123" }))
        .to_request();
    let resp = test::call_service(&app, login).await;
    let cookie = resp.response().cookies().next().unwrap().into_owned();

    let me = test::TestRequest::get()
        .uri("/auth/me")
        .cookie(cookie.clone())
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, me).await;
    assert!(
        body.get("clock_24h").is_some(),
        "response carries no `clock_24h` field: {body}"
    );
    assert_eq!(
        body["clock_24h"],
        Value::Null,
        "the clock should start unset"
    );

    let patch = test::TestRequest::patch()
        .uri("/auth/me")
        .cookie(cookie.clone())
        .set_json(json!({ "timezone": "Europe/Prague", "clock_24h": true }))
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, patch).await;
    assert_eq!(body["clock_24h"], true);

    // Naming only the clock leaves the zone alone, and `null` clears it.
    let patch = test::TestRequest::patch()
        .uri("/auth/me")
        .cookie(cookie.clone())
        .set_json(json!({ "clock_24h": null }))
        .to_request();
    let resp = test::call_service(&app, patch).await;
    assert_eq!(resp.status(), 200);

    let me = test::TestRequest::get()
        .uri("/auth/me")
        .cookie(cookie.clone())
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, me).await;
    assert_eq!(body["clock_24h"], Value::Null);
    assert_eq!(body["timezone"], "Europe/Prague");

    // A clock is a boolean, not a string that looks like one.
    let patch = test::TestRequest::patch()
        .uri("/auth/me")
        .cookie(cookie)
        .set_json(json!({ "clock_24h": "yes" }))
        .to_request();
    let resp = test::call_service(&app, patch).await;
    assert_eq!(resp.status(), 400);
}

/// The server validates the shape of a preference, not its membership.
///
/// It deliberately does not know which languages the dashboard ships: the
/// dashboard is optional in this architecture, and a hard-coded list here
/// would mean redeploying the API to add a locale to a frontend. What it does
/// refuse is anything that is not shaped like a language tag or a zone, so the
/// column cannot fill with junk.
#[actix_web::test]
async fn test_patch_me_rejects_malformed_preferences() {
    let db = TestDb::new().await;
    let config = crate::common::config();

    create_test_user(&db.pool, "prefs3@example.com", "password123", false).await;

    let app = crate::common::app_with_config(&db.pool, config).await;

    let login = test::TestRequest::post()
        .uri("/auth/login")
        .set_json(json!({ "email": "prefs3@example.com", "password": "password123" }))
        .to_request();
    let resp = test::call_service(&app, login).await;
    let cookie = resp.response().cookies().next().unwrap().into_owned();

    // Long enough to be a payload rather than a tag, and with characters no
    // language tag carries.
    let patch = test::TestRequest::patch()
        .uri("/auth/me")
        .cookie(cookie.clone())
        .set_json(json!({ "language": "<script>alert(1)</script>" }))
        .to_request();
    let resp = test::call_service(&app, patch).await;
    assert_eq!(
        resp.status(),
        400,
        "a malformed language should be rejected"
    );

    // And an unknown-but-well-formed tag is accepted, because membership is
    // the consumer's business.
    let patch = test::TestRequest::patch()
        .uri("/auth/me")
        .cookie(cookie)
        .set_json(json!({ "language": "pt-BR" }))
        .to_request();
    let resp = test::call_service(&app, patch).await;
    assert_eq!(
        resp.status(),
        200,
        "a well-formed unknown tag should be stored"
    );
}

/// Signs in as `email` and posts `body` to `/auth/me/password`, returning the
/// status and the body (`Null` when there is none).
async fn change_password_as(
    pool: &rustrak::db::DbPool,
    email: &str,
    password: &str,
    body: Value,
) -> (u16, Value) {
    let app = crate::common::app_with_config(pool, crate::common::config()).await;

    let login = test::TestRequest::post()
        .uri("/auth/login")
        .set_json(json!({ "email": email, "password": password }))
        .to_request();
    let resp = test::call_service(&app, login).await;
    let cookie = resp.response().cookies().next().unwrap().into_owned();

    let req = test::TestRequest::post()
        .uri("/auth/me/password")
        .cookie(cookie)
        .set_json(body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status().as_u16();
    let bytes = test::read_body(resp).await;
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[actix_web::test]
async fn test_change_password_replaces_the_login_password() {
    let db = TestDb::new().await;
    create_test_user(&db.pool, "pw1@example.com", "old-password", false).await;

    let (status, _) = change_password_as(
        &db.pool,
        "pw1@example.com",
        "old-password",
        json!({ "current_password": "old-password", "new_password": "new-password" }),
    )
    .await;
    assert_eq!(status, 204);

    assert_eq!(
        login_as(&db.pool, "pw1@example.com", "old-password")
            .await
            .0,
        401
    );
    assert_eq!(
        login_as(&db.pool, "pw1@example.com", "new-password")
            .await
            .0,
        200
    );
}

#[actix_web::test]
async fn test_change_password_requires_the_current_password() {
    let db = TestDb::new().await;
    create_test_user(&db.pool, "pw2@example.com", "old-password", false).await;

    let (status, body) = change_password_as(
        &db.pool,
        "pw2@example.com",
        "old-password",
        json!({ "current_password": "not-it", "new_password": "new-password" }),
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(
        body["error"]["fields"],
        json!([{ "field": "current_password", "code": "incorrect" }])
    );

    assert_eq!(
        login_as(&db.pool, "pw2@example.com", "old-password")
            .await
            .0,
        200
    );
}

#[actix_web::test]
async fn test_change_password_rejects_an_empty_new_password() {
    let db = TestDb::new().await;
    create_test_user(&db.pool, "pw3@example.com", "old-password", false).await;

    let (status, body) = change_password_as(
        &db.pool,
        "pw3@example.com",
        "old-password",
        json!({ "current_password": "old-password", "new_password": "" }),
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(
        body["error"]["fields"],
        json!([{ "field": "new_password", "code": "required" }])
    );

    assert_eq!(
        login_as(&db.pool, "pw3@example.com", "old-password")
            .await
            .0,
        200
    );
}

/// Two overlapping requests both load the user before either writes. The one
/// that lands second verified a hash that is no longer stored, so it must not
/// overwrite the first change.
#[actix_web::test]
async fn test_change_password_refuses_a_stale_verification() {
    let db = TestDb::new().await;
    create_test_user(&db.pool, "pw4@example.com", "old-password", false).await;
    let first = UsersService::get_by_email(&db.pool, "pw4@example.com")
        .await
        .unwrap()
        .unwrap();
    let second = first.clone();

    UsersService::change_password(&db.pool, first, "old-password", "owner-password")
        .await
        .unwrap();
    let stale =
        UsersService::change_password(&db.pool, second, "old-password", "attacker-password").await;
    assert!(stale.is_err());

    assert_eq!(
        login_as(&db.pool, "pw4@example.com", "owner-password")
            .await
            .0,
        200
    );
}

async fn oidc_outcome(
    pool: &rustrak::db::DbPool,
    subject: &str,
    email: &str,
    email_verified: bool,
    policy: OidcLinkPolicy,
) -> rustrak::error::AppResult<OidcOutcome> {
    let identity = OidcIdentity {
        issuer: "https://id.example.com".to_string(),
        subject: subject.to_string(),
        email: email.to_string(),
        email_verified,
        groups: Vec::new(),
    };
    UsersService::find_or_provision_oidc(pool, &identity, policy).await
}

/// A login expected to finish without asking for a password.
async fn oidc_login(
    pool: &rustrak::db::DbPool,
    subject: &str,
    email: &str,
    email_verified: bool,
    policy: OidcLinkPolicy,
) -> rustrak::error::AppResult<User> {
    oidc_outcome(pool, subject, email, email_verified, policy)
        .await
        .map(|outcome| match outcome {
            OidcOutcome::SignedIn(user) => user,
            OidcOutcome::ConfirmWithPassword(user) => {
                panic!("expected a sign-in, was asked to confirm {}", user.email)
            }
        })
}

fn linking(link_existing_accounts: bool, auto_provision: bool) -> OidcLinkPolicy {
    OidcLinkPolicy {
        auto_provision,
        link_existing_accounts,
        admin_groups: Vec::new(),
    }
}

fn linking_with_admin_groups(
    link_existing_accounts: bool,
    auto_provision: bool,
    admin_groups: &[&str],
) -> OidcLinkPolicy {
    OidcLinkPolicy {
        auto_provision,
        link_existing_accounts,
        admin_groups: admin_groups.iter().map(|group| group.to_string()).collect(),
    }
}

#[actix_web::test]
async fn test_oidc_first_login_provisions_admin_and_reuses_subject() {
    let db = TestDb::new().await;

    let first = oidc_login(
        &db.pool,
        "pocket-id-subject",
        "Owner@Example.com",
        true,
        linking(true, true),
    )
    .await
    .expect("first OIDC login should provision a user");

    assert_eq!(first.email, "owner@example.com");
    assert!(
        first.is_admin(),
        "the first account must bootstrap as admin"
    );
    let probe = uuid::Uuid::new_v4().to_string();
    assert!(
        !first
            .verify_password(&probe)
            .expect("the account must carry a well-formed hash for the NOT NULL column"),
        "an OIDC-provisioned account must not accept a password"
    );

    let returning = oidc_login(
        &db.pool,
        "pocket-id-subject",
        "new-address@example.com",
        true,
        linking(true, true),
    )
    .await
    .expect("returning OIDC login should resolve its linked account");

    assert_eq!(returning.id, first.id);
    assert_eq!(UsersService::user_count(&db.pool).await.unwrap(), 1);
}

#[actix_web::test]
async fn test_oidc_links_existing_verified_email_account() {
    let db = TestDb::new().await;
    let existing_password = format!("pass-{}", uuid::Uuid::new_v4());
    let existing =
        create_test_user(&db.pool, "existing@example.com", &existing_password, true).await;

    let linked = oidc_login(
        &db.pool,
        "existing-subject",
        "EXISTING@example.com",
        true,
        linking(true, true),
    )
    .await
    .expect("verified email should link to the existing account");

    assert_eq!(linked.id, existing.id);
    assert!(linked.verify_password(&existing_password).unwrap());
    assert_eq!(UsersService::user_count(&db.pool).await.unwrap(), 1);
}

#[actix_web::test]
async fn test_oidc_asks_for_the_password_of_an_existing_account_when_linking_is_disabled() {
    let db = TestDb::new().await;
    let password = format!("pass-{}", uuid::Uuid::new_v4());
    let existing = create_test_user(&db.pool, "owner@example.com", &password, true).await;

    // The provider vouches for the email, but that claim alone must not hand
    // over an account that already exists: its owner has to prove it.
    let outcome = oidc_outcome(
        &db.pool,
        "someone-else",
        "owner@example.com",
        true,
        linking(false, true),
    )
    .await
    .unwrap();

    match outcome {
        OidcOutcome::ConfirmWithPassword(user) => assert_eq!(user.id, existing.id),
        OidcOutcome::SignedIn(_) => panic!("an existing account was linked without its password"),
    }
    assert_eq!(UsersService::user_count(&db.pool).await.unwrap(), 1);

    // Nothing was linked: a retry with linking on is still an unknown identity.
    let linked = oidc_login(
        &db.pool,
        "someone-else",
        "owner@example.com",
        true,
        linking(true, true),
    )
    .await
    .expect("with linking enabled the same identity links");
    assert_eq!(linked.id, existing.id);
}

#[actix_web::test]
async fn test_oidc_links_an_existing_account_without_auto_provisioning() {
    let db = TestDb::new().await;
    let password = format!("pass-{}", uuid::Uuid::new_v4());
    let existing = create_test_user(&db.pool, "owner@example.com", &password, true).await;

    // Provisioning only governs creating accounts; linking one that exists is
    // a separate switch.
    let linked = oidc_login(
        &db.pool,
        "owner-subject",
        "owner@example.com",
        true,
        linking(true, false),
    )
    .await
    .expect("linking must not depend on auto-provisioning");

    assert_eq!(linked.id, existing.id);
}

#[actix_web::test]
async fn test_oidc_link_is_confirmed_only_with_the_account_password() {
    let db = TestDb::new().await;
    let password = format!("pass-{}", uuid::Uuid::new_v4());
    let existing = create_test_user(&db.pool, "owner@example.com", &password, true).await;
    let identity = OidcIdentity {
        issuer: "https://id.example.com".to_string(),
        subject: "owner-subject".to_string(),
        email: "owner@example.com".to_string(),
        email_verified: true,
        groups: Vec::new(),
    };

    let wrong =
        UsersService::confirm_oidc_link(&db.pool, &identity, existing.id, "not-it", None).await;
    assert!(matches!(
        wrong,
        Err(rustrak::error::AppError::Unauthorized(_))
    ));
    let still_unlinked = oidc_outcome(
        &db.pool,
        "owner-subject",
        "owner@example.com",
        true,
        linking(false, false),
    )
    .await
    .unwrap();
    assert!(matches!(
        still_unlinked,
        OidcOutcome::ConfirmWithPassword(_)
    ));

    let linked = UsersService::confirm_oidc_link(&db.pool, &identity, existing.id, &password, None)
        .await
        .expect("the right password links the identity");
    assert_eq!(linked.id, existing.id);

    // From now on the identity signs in directly, whatever the switches say.
    let returning = oidc_login(
        &db.pool,
        "owner-subject",
        "owner@example.com",
        true,
        linking(false, false),
    )
    .await
    .unwrap();
    assert_eq!(returning.id, existing.id);
}

#[actix_web::test]
async fn test_oidc_link_cannot_move_an_identity_to_a_second_account() {
    let db = TestDb::new().await;
    let first = create_test_user(&db.pool, "first@example.com", "password-one", true).await;
    let second = create_test_user(&db.pool, "second@example.com", "password-two", false).await;
    let identity = OidcIdentity {
        issuer: "https://id.example.com".to_string(),
        subject: "shared-subject".to_string(),
        email: "first@example.com".to_string(),
        email_verified: true,
        groups: Vec::new(),
    };

    UsersService::confirm_oidc_link(&db.pool, &identity, first.id, "password-one", None)
        .await
        .unwrap();
    let moved =
        UsersService::confirm_oidc_link(&db.pool, &identity, second.id, "password-two", None).await;

    assert!(matches!(moved, Err(rustrak::error::AppError::Conflict(_))));
    let owner = oidc_login(
        &db.pool,
        "shared-subject",
        "first@example.com",
        true,
        linking(false, false),
    )
    .await
    .unwrap();
    assert_eq!(owner.id, first.id);
}

#[actix_web::test]
async fn test_oidc_asks_for_the_password_when_the_email_is_unverified() {
    let db = TestDb::new().await;
    let existing_password = format!("pass-{}", uuid::Uuid::new_v4());
    let existing = create_test_user(&db.pool, "victim@example.com", &existing_password, true).await;

    // Automatic linking trusts the provider's verification; without it, only
    // the account's password can attach the identity.
    let outcome = oidc_outcome(
        &db.pool,
        "attacker-subject",
        "victim@example.com",
        false,
        linking(true, true),
    )
    .await
    .unwrap();

    assert!(matches!(
        outcome,
        OidcOutcome::ConfirmWithPassword(user) if user.id == existing.id
    ));
}

#[actix_web::test]
async fn test_oidc_unverified_email_provisions_new_account_when_no_existing_account() {
    let db = TestDb::new().await;
    let user = oidc_login(
        &db.pool,
        "new-subject",
        "brandnew@example.com",
        false,
        linking(true, true),
    )
    .await
    .expect("new account can be provisioned even if email is unverified");

    assert_eq!(user.email, "brandnew@example.com");
}

#[actix_web::test]
async fn test_oidc_links_exact_case_variant_when_multiple_legacy_accounts_exist() {
    let db = TestDb::new().await;
    let lower_password = format!("pass-{}", uuid::Uuid::new_v4());
    let upper_password = format!("pass-{}", uuid::Uuid::new_v4());
    insert_legacy_user(&db.pool, "user@example.com", &lower_password).await;
    insert_legacy_user(&db.pool, "User@Example.com", &upper_password).await;

    // SSO login with exact casing links to the upper-case account, not the lower-case one.
    let linked_upper = oidc_login(
        &db.pool,
        "upper-subject",
        "User@Example.com",
        true,
        linking(true, true),
    )
    .await
    .expect("exact casing should link to the matching legacy account");

    assert_eq!(linked_upper.email, "User@Example.com");
    assert!(linked_upper.verify_password(&upper_password).unwrap());

    // SSO login with lower casing links to the lower-case account.
    let linked_lower = oidc_login(
        &db.pool,
        "lower-subject",
        "user@example.com",
        true,
        linking(true, true),
    )
    .await
    .expect("exact casing should link to the matching legacy account");

    assert_eq!(linked_lower.email, "user@example.com");
    assert!(linked_lower.verify_password(&lower_password).unwrap());
    assert_ne!(linked_upper.id, linked_lower.id);
}

#[actix_web::test]
async fn test_oidc_refuses_ambiguous_linking_when_multiple_legacy_accounts_exist() {
    let db = TestDb::new().await;
    let lower_password = format!("pass-{}", uuid::Uuid::new_v4());
    let upper_password = format!("pass-{}", uuid::Uuid::new_v4());
    insert_legacy_user(&db.pool, "user@example.com", &lower_password).await;
    insert_legacy_user(&db.pool, "User@Example.com", &upper_password).await;

    // A casing that matches neither exact account cannot link to either.
    let result = oidc_login(
        &db.pool,
        "ambiguous-subject",
        "USER@EXAMPLE.COM",
        true,
        linking(true, true),
    )
    .await;

    assert!(matches!(
        result,
        Err(rustrak::error::AppError::Forbidden(_))
    ));
}

#[actix_web::test]
async fn test_oidc_auto_provision_can_be_disabled() {
    let db = TestDb::new().await;
    let result = oidc_login(
        &db.pool,
        "unknown-subject",
        "unknown@example.com",
        true,
        linking(true, false),
    )
    .await;

    assert!(matches!(
        result,
        Err(rustrak::error::AppError::Forbidden(_))
    ));
    assert_eq!(UsersService::user_count(&db.pool).await.unwrap(), 0);
}

/// A login carrying group claims, for the role-mapping tests.
async fn oidc_login_in_groups(
    pool: &rustrak::db::DbPool,
    subject: &str,
    email: &str,
    groups: &[&str],
    policy: OidcLinkPolicy,
) -> rustrak::error::AppResult<User> {
    let identity = OidcIdentity {
        issuer: "https://id.example.com".to_string(),
        subject: subject.to_string(),
        email: email.to_string(),
        email_verified: true,
        groups: groups.iter().map(|group| group.to_string()).collect(),
    };
    match UsersService::find_or_provision_oidc(pool, &identity, policy).await? {
        OidcOutcome::SignedIn(user) => Ok(user),
        OidcOutcome::ConfirmWithPassword(user) => {
            panic!("expected a sign-in, was asked to confirm {}", user.email)
        }
    }
}

#[actix_web::test]
async fn test_oidc_admin_groups_drive_the_role_both_ways() {
    let db = TestDb::new().await;
    // A local administrator owns the install, so the SSO account below is not
    // the protected primary one.
    create_test_user(&db.pool, "owner@example.com", "owner-password", true).await;
    let policy = linking_with_admin_groups(true, true, &["rustrak-admins"]);

    let member = oidc_login_in_groups(
        &db.pool,
        "sub-member",
        "member@example.com",
        &["users"],
        policy.clone(),
    )
    .await
    .expect("a provisioned account");
    assert!(
        !member.is_admin(),
        "a group outside the mapping is a member"
    );

    let promoted = oidc_login_in_groups(
        &db.pool,
        "sub-member",
        "member@example.com",
        &["users", "rustrak-admins"],
        policy.clone(),
    )
    .await
    .expect("a returning account");
    assert!(promoted.is_admin(), "joining the admin group promotes");
    let stored = UsersService::get_by_id(&db.pool, member.id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        stored.is_admin(),
        "the promotion is persisted, not per-request"
    );

    let demoted = oidc_login_in_groups(
        &db.pool,
        "sub-member",
        "member@example.com",
        &["users"],
        policy.clone(),
    )
    .await
    .expect("a returning account");
    assert!(!demoted.is_admin(), "leaving the admin group demotes");
    let stored = UsersService::get_by_id(&db.pool, member.id)
        .await
        .unwrap()
        .unwrap();
    assert!(!stored.is_admin(), "the demotion is persisted too");
}

#[actix_web::test]
async fn test_oidc_admin_groups_cannot_demote_the_primary_account() {
    let db = TestDb::new().await;
    let policy = linking_with_admin_groups(true, true, &["rustrak-admins"]);

    // The first account is the primary one. Its role is protected everywhere
    // else, and a claim must not be the way around that.
    let first = oidc_login_in_groups(
        &db.pool,
        "sub-primary",
        "owner@example.com",
        &[],
        policy.clone(),
    )
    .await
    .expect("the first account");
    assert!(first.is_admin());

    let again = oidc_login_in_groups(&db.pool, "sub-primary", "owner@example.com", &[], policy)
        .await
        .expect("the same account logging in again");
    assert!(
        again.is_admin(),
        "an empty group list must not demote the bootstrap superuser"
    );
}

#[actix_web::test]
async fn test_oidc_without_admin_groups_leaves_roles_alone() {
    let db = TestDb::new().await;
    let member = create_test_user(&db.pool, "member@example.com", "member-password", false).await;

    let signed_in = oidc_login_in_groups(
        &db.pool,
        "sub-member",
        "member@example.com",
        &["rustrak-admins"],
        linking(true, true),
    )
    .await
    .expect("a verified email links the existing account");

    assert_eq!(signed_in.id, member.id);
    assert!(
        !signed_in.is_admin(),
        "no mapping configured means the provider does not decide roles"
    );
}

#[actix_web::test]
async fn test_oidc_group_role_applies_only_after_the_link_is_confirmed() {
    let db = TestDb::new().await;
    let password = format!("pass-{}", uuid::Uuid::new_v4());
    let existing = create_test_user(&db.pool, "owner@example.com", &password, false).await;
    let policy = linking_with_admin_groups(false, false, &["rustrak-admins"]);
    let identity = OidcIdentity {
        issuer: "https://id.example.com".to_string(),
        subject: "owner-subject".to_string(),
        email: "owner@example.com".to_string(),
        email_verified: true,
        groups: vec!["rustrak-admins".to_string()],
    };

    // Matching the email alone must not touch the role: proving ownership is
    // what unlocks the claim.
    let outcome = UsersService::find_or_provision_oidc(&db.pool, &identity, policy.clone())
        .await
        .unwrap();
    assert!(matches!(outcome, OidcOutcome::ConfirmWithPassword(_)));
    let before = UsersService::get_by_id(&db.pool, existing.id)
        .await
        .unwrap()
        .unwrap();
    assert!(!before.is_admin(), "an unconfirmed link changes nothing");

    // The matched groups were read from the token at callback time; only the
    // password was missing, and the role is decided against the mapping as it
    // is now.
    let matched = policy.matched_admin_groups(&identity.groups);
    let linked = UsersService::confirm_oidc_link(
        &db.pool,
        &identity,
        existing.id,
        &password,
        policy.role_from_matched(&matched),
    )
    .await
    .expect("the right password links the identity and applies its groups");
    assert!(linked.is_admin());
}

#[actix_web::test]
async fn test_oidc_pending_link_does_not_grant_a_retired_admin_group() {
    // The link was started while `rustrak-admins` meant admin. The operator
    // then changed the mapping (or the provider's groups did) before the
    // password arrived.
    let started_with = linking_with_admin_groups(false, false, &["rustrak-admins"]);
    let matched = started_with.matched_admin_groups(&["rustrak-admins".to_string()]);
    assert_eq!(matched, ["rustrak-admins"]);

    let since_retired = linking_with_admin_groups(false, false, &["other-group"]);
    assert_eq!(
        since_retired.role_from_matched(&matched),
        Some(rustrak::models::UserRole::Member),
        "a group the operator has since removed must not still grant admin"
    );

    // With mapping turned off entirely, the role is left alone.
    assert_eq!(linking(false, false).role_from_matched(&matched), None);
}

#[actix_web::test]
async fn test_oidc_group_names_match_whole_names_only() {
    let policy = linking_with_admin_groups(true, true, &["admins"]);

    assert_eq!(
        policy.role_for(&["staff,admins".to_string()]),
        Some(rustrak::models::UserRole::Member),
        "a comma inside a group name must not act as a separator"
    );
    assert_eq!(
        policy.role_for(&["admins".to_string()]),
        Some(rustrak::models::UserRole::Admin)
    );
    assert_eq!(
        policy.role_for(&[]),
        Some(rustrak::models::UserRole::Member),
        "mapping configured: no admin group means member"
    );
    assert_eq!(
        linking(true, true).role_for(&["admins".to_string()]),
        None,
        "mapping not configured: roles are left alone"
    );
}

// ---------------------------------------------------------------------------
// Server-side sessions: the cookie carries an opaque id, the row is the session
// ---------------------------------------------------------------------------

async fn me_status<S>(app: &S, cookie: &actix_web::cookie::Cookie<'static>) -> u16
where
    S: actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
{
    let req = test::TestRequest::get()
        .uri("/auth/me")
        .cookie(cookie.clone())
        .to_request();
    test::call_service(app, req).await.status().as_u16()
}

async fn session_rows(pool: &rustrak::db::DbPool, user_id: i32) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[actix_web::test]
async fn test_logout_invalidates_session() {
    let db = TestDb::new().await;
    let user = create_test_user(&db.pool, "logouttest@example.com", "password123", false).await;
    let app = crate::common::app(&db.pool).await;

    let cookie = crate::common::login(&app, "logouttest@example.com").await;
    assert_eq!(me_status(&app, &cookie).await, 200);
    assert_eq!(session_rows(&db.pool, user.id).await, 1);

    let logout = test::TestRequest::post()
        .uri("/auth/logout")
        .cookie(cookie.clone())
        .to_request();
    assert_eq!(test::call_service(&app, logout).await.status(), 204);

    // The same cookie, replayed after logout, is refused: its row is gone.
    assert_eq!(me_status(&app, &cookie).await, 401);
    assert_eq!(session_rows(&db.pool, user.id).await, 0);
}

#[actix_web::test]
async fn test_session_row_stores_only_a_hash_of_the_id() {
    use actix_session::storage::SessionStore;
    use sha2::Digest;

    let db = TestDb::new().await;
    let store = rustrak::services::DbSessionStore::new(db.pool.clone());
    let key = store
        .save(
            std::collections::HashMap::new(),
            &actix_web::cookie::time::Duration::days(1),
        )
        .await
        .unwrap();

    let id_hash: String = sqlx::query_scalar("SELECT id_hash FROM sessions")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_ne!(id_hash, key.as_ref(), "the raw id is never stored");
    assert_eq!(id_hash, hex::encode(sha2::Sha256::digest(key.as_ref())));
}

#[actix_web::test]
async fn test_login_rotates_the_session_id() {
    let db = TestDb::new().await;
    create_test_user(&db.pool, "rotate@example.com", "password123", false).await;
    let app = crate::common::app(&db.pool).await;

    let first = crate::common::login(&app, "rotate@example.com").await;
    let login = test::TestRequest::post()
        .uri("/auth/login")
        .cookie(first.clone())
        .set_json(json!({ "email": "rotate@example.com", "password": "password123" }))
        .to_request();
    let resp = test::call_service(&app, login).await;
    assert_eq!(resp.status(), 200);
    let second = resp.response().cookies().next().unwrap().into_owned();

    assert_ne!(first.value(), second.value());
    assert_eq!(
        me_status(&app, &first).await,
        401,
        "the pre-login id is dead"
    );
    assert_eq!(me_status(&app, &second).await, 200);
}

#[actix_web::test]
async fn test_deactivated_user_session_is_refused() {
    let db = TestDb::new().await;
    let user = create_test_user(&db.pool, "deact@example.com", "password123", false).await;
    let app = crate::common::app(&db.pool).await;
    let cookie = crate::common::login(&app, "deact@example.com").await;

    sqlx::query("UPDATE users SET is_active = false WHERE id = $1")
        .bind(user.id)
        .execute(&db.pool)
        .await
        .unwrap();
    assert_eq!(me_status(&app, &cookie).await, 401);
}

#[actix_web::test]
async fn test_revoking_a_user_kills_every_session() {
    let db = TestDb::new().await;
    let user = create_test_user(&db.pool, "revoke@example.com", "password123", false).await;
    let app = crate::common::app(&db.pool).await;
    let a = crate::common::login(&app, "revoke@example.com").await;
    let b = crate::common::login(&app, "revoke@example.com").await;

    rustrak::services::AuthSessionService::revoke_user_sessions(&db.pool, user.id)
        .await
        .unwrap();
    assert_eq!(me_status(&app, &a).await, 401);
    assert_eq!(me_status(&app, &b).await, 401);
}

#[actix_web::test]
async fn test_deleting_a_user_deletes_their_sessions() {
    let db = TestDb::new().await;
    let user = create_test_user(&db.pool, "gone@example.com", "password123", false).await;
    let app = crate::common::app(&db.pool).await;
    crate::common::login(&app, "gone@example.com").await;

    UsersService::delete(&db.pool, user.id).await.unwrap();
    assert_eq!(session_rows(&db.pool, user.id).await, 0);
}

#[actix_web::test]
async fn test_password_change_signs_out_other_sessions() {
    let db = TestDb::new().await;
    create_test_user(&db.pool, "pwsess@example.com", "password123", false).await;
    let app = crate::common::app(&db.pool).await;
    let mine = crate::common::login(&app, "pwsess@example.com").await;
    let other = crate::common::login(&app, "pwsess@example.com").await;

    let req = test::TestRequest::post()
        .uri("/auth/me/password")
        .cookie(mine.clone())
        .set_json(json!({ "current_password": "password123", "new_password": "next-password" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 204);
    let renewed = resp.response().cookies().next().unwrap().into_owned();

    assert_eq!(me_status(&app, &other).await, 401, "other sessions end");
    assert_eq!(me_status(&app, &mine).await, 401, "the old id is rotated");
    assert_eq!(
        me_status(&app, &renewed).await,
        200,
        "this one stays signed in"
    );
}

#[actix_web::test]
async fn test_expired_session_is_refused() {
    let db = TestDb::new().await;
    create_test_user(&db.pool, "expired@example.com", "password123", false).await;
    let app = crate::common::app(&db.pool).await;
    let cookie = crate::common::login(&app, "expired@example.com").await;

    sqlx::query("UPDATE sessions SET expires_at = $1")
        .bind(chrono::Utc::now() - chrono::Duration::minutes(1))
        .execute(&db.pool)
        .await
        .unwrap();
    assert_eq!(me_status(&app, &cookie).await, 401);
}

#[actix_web::test]
async fn test_idle_session_is_refused() {
    let db = TestDb::new().await;
    create_test_user(&db.pool, "idle@example.com", "password123", false).await;
    let app = crate::common::app(&db.pool).await;
    let cookie = crate::common::login(&app, "idle@example.com").await;

    sqlx::query("UPDATE sessions SET last_seen_at = $1")
        .bind(chrono::Utc::now() - chrono::Duration::days(2))
        .execute(&db.pool)
        .await
        .unwrap();
    assert_eq!(me_status(&app, &cookie).await, 401);
}

#[actix_web::test]
async fn test_purge_removes_only_dead_sessions() {
    let db = TestDb::new().await;
    let user = create_test_user(&db.pool, "purge@example.com", "password123", false).await;
    let app = crate::common::app(&db.pool).await;
    crate::common::login(&app, "purge@example.com").await;
    crate::common::login(&app, "purge@example.com").await;

    sqlx::query(
        "UPDATE sessions SET expires_at = $1 WHERE id_hash = (SELECT MIN(id_hash) FROM sessions)",
    )
    .bind(chrono::Utc::now() - chrono::Duration::minutes(1))
    .execute(&db.pool)
    .await
    .unwrap();

    let purged = rustrak::services::AuthSessionService::purge_expired(&db.pool)
        .await
        .unwrap();
    assert_eq!(purged, 1);
    assert_eq!(session_rows(&db.pool, user.id).await, 1);
}
