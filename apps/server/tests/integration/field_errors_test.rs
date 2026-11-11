//! The wire contract for field-level validation errors.
//!
//! One test per annotated site, each asserting the **emitted JSON body**
//! rather than the `AppError` variant: status, `error.type`, and the exact
//! `error.fields` array. The annotations are a public API a form binds to, so
//! a refactor that quietly drops one has to fail here.
//!
//! Every test drives the real service (or the real endpoint) rather than
//! hand-building the error, which is the only way this catches a dropped
//! `.with_field(...)`.

use crate::common::seed_user;
use crate::common::TestDb;
use actix_web::body::to_bytes;
use actix_web::{test, ResponseError};
use rustrak::db::DbPool;
use rustrak::error::{AppError, FieldErrorCode};
use rustrak::models::{
    CreateAlertIntegration, CreateAuthToken, CreateInvitation, CreateProject, ProviderType,
    UpdateAlertIntegration, UpdateProject, UserRole,
};
use rustrak::services::{AlertService, AuthTokenService, InvitationService, ProjectService};
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Renders an error exactly as actix would put it on the wire.
async fn emitted(error: AppError) -> (u16, Value) {
    let response = error.error_response();
    let status = response.status().as_u16();
    let bytes = to_bytes(response.into_body())
        .await
        .expect("error body must be readable");
    let body = serde_json::from_slice(&bytes).expect("error body must be JSON");
    (status, body)
}

/// Asserts the emitted body carries exactly one field annotation.
///
/// The whole `fields` array is compared against a literal, so a stray extra
/// entry, or a `message` on a code that should not carry one, fails too.
async fn assert_one_field<T: std::fmt::Debug>(
    result: Result<T, AppError>,
    status: u16,
    error_type: &str,
    field: &str,
    code: &str,
) {
    let error = result.expect_err("expected an error, got a success");
    let (got_status, body) = emitted(error).await;

    assert_eq!(got_status, status, "status; body was {body}");
    assert_eq!(body["error"]["type"], error_type, "error.type");
    assert_eq!(
        body["error"]["fields"],
        json!([{ "field": field, "code": code }]),
        "error.fields"
    );
}

async fn seed_project(pool: &DbPool, name: &str, slug: Option<&str>) -> rustrak::models::Project {
    ProjectService::create(
        pool,
        CreateProject {
            name: name.to_string(),
            slug: slug.map(str::to_string),
            platform: None,
        },
    )
    .await
    .expect("seed project")
}

async fn seed_channel(pool: &DbPool, name: &str) -> rustrak::models::AlertIntegration {
    AlertService::create_channel(
        pool,
        &Default::default(),
        CreateAlertIntegration {
            name: name.to_string(),
            provider_type: ProviderType::Webhook,
            credentials: json!({ "url": "https://example.com/webhook" }),
            is_enabled: true,
        },
    )
    .await
    .expect("seed channel")
}

/// Every consumer written before `fields` existed reads a body with exactly
/// two keys. An unannotated error must keep emitting exactly that.
#[actix_web::test]
async fn plain_errors_emit_no_fields_key_at_all() {
    for error in [
        AppError::Validation("Name cannot be empty".to_string()),
        AppError::Conflict("Cannot delete the last admin".to_string()),
        AppError::NotFound("Project not found".to_string()),
    ] {
        let (_, body) = emitted(error).await;
        assert!(
            body["error"].get("fields").is_none(),
            "an unannotated error must not serialise a `fields` key, got {body}"
        );
        assert_eq!(
            body["error"].as_object().map(serde_json::Map::len),
            Some(2),
            "an unannotated error body must stay {{type, message}}, got {body}"
        );
    }
}

/// `Validation` carries fields as readily as `Conflict` does, and annotating
/// changes neither the status, the wire `type`, nor the message.
#[actix_web::test]
async fn annotating_preserves_status_type_and_message() {
    let plain = AppError::Validation("Invalid email format".to_string());
    let plain_message = plain.to_string();
    let (plain_status, plain_body) = emitted(plain).await;

    let annotated = AppError::Validation("Invalid email format".to_string())
        .with_field("email", FieldErrorCode::Invalid);
    let (status, body) = emitted(annotated).await;

    assert_eq!(status, 400);
    assert_eq!(status, plain_status);
    assert_eq!(body["error"]["type"], "ValidationError");
    assert_eq!(body["error"]["type"], plain_body["error"]["type"]);
    assert_eq!(body["error"]["message"], plain_message);
    assert_eq!(
        body["error"]["fields"],
        json!([{ "field": "email", "code": "invalid" }])
    );
}

/// `custom` is the only code that may carry prose, and it round-trips.
#[actix_web::test]
async fn a_custom_code_carries_its_message() {
    let error = AppError::Validation("Rejected".to_string()).with_fields(vec![
        rustrak::error::FieldError::custom(
            "credentials.webhook_url",
            "Slack rejected this webhook.",
        ),
    ]);
    let (_, body) = emitted(error).await;

    assert_eq!(
        body["error"]["fields"],
        json!([{
            "field": "credentials.webhook_url",
            "code": "custom",
            "message": "Slack rejected this webhook.",
        }])
    );
}

// =============================================================================
// services/project.rs — CreateProject / UpdateProject bodies
//
// Field names are the request-body keys (`name`, `slug`), never the UI label.
//
// The `name` collisions below are reachable in both dialects: SQLite declares
// `name VARCHAR(255) NOT NULL UNIQUE` in the initial schema and Postgres adds
// `projects_name_key UNIQUE (name)` in
// `migrations/postgres/20260119000001_add_projects_name_unique.up.sql`. These
// tests therefore run un-gated under `--features postgres` too, which is the
// production dialect.
// =============================================================================

/// `create`, slug the user typed is already taken (the ordinary pre-check).
#[actix_web::test]
async fn create_with_taken_user_slug_blames_slug() {
    let db = TestDb::new().await;
    seed_project(&db.pool, "First Project", Some("shared-slug")).await;

    let result = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "Second Project".to_string(),
            slug: Some("shared-slug".to_string()),
            platform: None,
        },
    )
    .await;

    assert_one_field(result, 409, "Conflict", "slug", "already_exists").await;
}

/// `update`, slug already taken by another project (the ordinary pre-check).
#[actix_web::test]
async fn update_with_taken_slug_blames_slug() {
    let db = TestDb::new().await;
    seed_project(&db.pool, "Taken Slug Project", None).await;
    let target = seed_project(&db.pool, "Target Slug Project", None).await;

    let result = ProjectService::update(
        &db.pool,
        target.id,
        UpdateProject {
            name: None,
            platform: None,
            slug: Some("taken-slug-project".to_string()),
            ..Default::default()
        },
    )
    .await;

    assert_one_field(result, 409, "Conflict", "slug", "already_exists").await;
}

/// `update`, slug taken between the pre-check and the UPDATE. Production only
/// reaches this by losing a TOCTOU race, hence the seam.
#[actix_web::test]
async fn update_losing_the_slug_race_blames_slug() {
    let db = TestDb::new().await;
    seed_project(&db.pool, "Racer Project", Some("raced-slug")).await;
    let target = seed_project(&db.pool, "Target Project", None).await;

    let result = ProjectService::update_with_raced_slug(
        &db.pool,
        target.id,
        UpdateProject {
            name: None,
            platform: None,
            slug: Some("raced-slug".to_string()),
            ..Default::default()
        },
    )
    .await;

    assert_one_field(result, 409, "Conflict", "slug", "already_exists").await;
}

/// `update`, the unique violation is on `name`, not `slug`.
#[actix_web::test]
async fn update_with_taken_name_blames_name() {
    let db = TestDb::new().await;
    seed_project(&db.pool, "Taken Name", None).await;
    let target = seed_project(&db.pool, "Target Name", None).await;

    let result = ProjectService::update(
        &db.pool,
        target.id,
        UpdateProject {
            name: Some("Taken Name".to_string()),
            platform: None,
            slug: None,
            ..Default::default()
        },
    )
    .await;

    assert_one_field(result, 409, "Conflict", "name", "already_exists").await;
}

/// The slug arm of the one `return` whose two arms blame two different
/// inputs: the INSERT lost the race on the slug the user typed.
#[actix_web::test]
async fn create_losing_the_user_slug_race_blames_slug() {
    let db = TestDb::new().await;
    seed_project(&db.pool, "Holder Project", Some("raced-user-slug")).await;

    let result = ProjectService::create_with_raced_user_slug(
        &db.pool,
        "A Free Name",
        "raced-user-slug",
        None,
    )
    .await;

    assert_one_field(result, 409, "Conflict", "slug", "already_exists").await;
}

/// The name arm of that same `return`: the slug the user typed is free, so
/// the collision was on `name`. Annotating the statement instead of the branch
/// would have blamed `slug` here.
#[actix_web::test]
async fn create_losing_the_user_slug_race_on_name_blames_name() {
    let db = TestDb::new().await;
    seed_project(&db.pool, "Held Name", Some("held-slug")).await;

    let result =
        ProjectService::create_with_raced_user_slug(&db.pool, "Held Name", "a-free-slug", None)
            .await;

    assert_one_field(result, 409, "Conflict", "name", "already_exists").await;
}

/// Derived slug, retry path: the regenerated slug is unchanged, so the
/// collision was on `name`.
#[actix_web::test]
async fn create_with_derived_slug_still_free_blames_name() {
    let db = TestDb::new().await;
    seed_project(&db.pool, "Alpha", None).await;

    // `alpha-1` is exactly what `generate_unique_slug` would return for
    // "Alpha" now, so the retry finds nothing new to try.
    let result = ProjectService::create_with_stale_slug(&db.pool, "Alpha", "alpha-1", None).await;

    assert_one_field(result, 409, "Conflict", "name", "already_exists").await;
}

/// Derived slug, retry path: the regenerated slug differs, the retry runs,
/// and the retry's own violation is on `name`.
#[actix_web::test]
async fn create_retrying_a_derived_slug_blames_name() {
    let db = TestDb::new().await;
    seed_project(&db.pool, "Beta", None).await;

    // Stale `beta`: the retry regenerates `beta-1`, inserts again, and hits
    // the name constraint.
    let result = ProjectService::create_with_stale_slug(&db.pool, "Beta", "beta", None).await;

    assert_one_field(result, 409, "Conflict", "name", "already_exists").await;
}

/// The address already belongs to a user.
#[actix_web::test]
async fn inviting_an_existing_user_blames_email() {
    let db = TestDb::new().await;
    let admin = seed_user(&db.pool, "admin@example.com", UserRole::Admin).await;
    seed_user(&db.pool, "taken@example.com", UserRole::Member).await;

    let result = InvitationService::create(
        &db.pool,
        CreateInvitation {
            email: "taken@example.com".to_string(),
            role: "member".to_string(),
            send_email: false,
            ..Default::default()
        },
        admin.id,
    )
    .await;

    assert_one_field(result, 409, "Conflict", "email", "already_exists").await;
}

/// The address already has an invitation pending.
#[actix_web::test]
async fn inviting_a_pending_email_blames_email() {
    let db = TestDb::new().await;
    let admin = seed_user(&db.pool, "admin@example.com", UserRole::Admin).await;

    InvitationService::create(
        &db.pool,
        CreateInvitation {
            email: "pending@example.com".to_string(),
            role: "member".to_string(),
            send_email: false,
            ..Default::default()
        },
        admin.id,
    )
    .await
    .expect("first invitation must succeed");

    let result = InvitationService::create(
        &db.pool,
        CreateInvitation {
            email: "pending@example.com".to_string(),
            role: "member".to_string(),
            send_email: false,
            ..Default::default()
        },
        admin.id,
    )
    .await;

    assert_one_field(result, 409, "Conflict", "email", "already_exists").await;
}

/// Creating an integration under a name already in use.
#[actix_web::test]
async fn creating_a_duplicate_integration_blames_name() {
    let db = TestDb::new().await;
    seed_channel(&db.pool, "Ops Webhook").await;

    let result = AlertService::create_channel(
        &db.pool,
        &Default::default(),
        CreateAlertIntegration {
            name: "Ops Webhook".to_string(),
            provider_type: ProviderType::Webhook,
            credentials: json!({ "url": "https://example.com/other" }),
            is_enabled: true,
        },
    )
    .await;

    assert_one_field(result, 409, "Conflict", "name", "already_exists").await;
}

/// Renaming an integration onto a name already in use.
#[actix_web::test]
async fn renaming_an_integration_onto_a_taken_name_blames_name() {
    let db = TestDb::new().await;
    seed_channel(&db.pool, "Taken Integration").await;
    let target = seed_channel(&db.pool, "Target Integration").await;

    let result = AlertService::update_channel(
        &db.pool,
        &Default::default(),
        target.id,
        UpdateAlertIntegration {
            name: Some("Taken Integration".to_string()),
            credentials: None,
            is_enabled: None,
        },
    )
    .await;

    assert_one_field(result, 409, "Conflict", "name", "already_exists").await;
}

// =============================================================================
// routes/team.rs — UpdateUserRole body
//
// The only one of the fourteen that lives in a route rather than a service, so
// it is driven over HTTP and its real response body is read.
// =============================================================================

/// Demoting the last global admin. Asserted end to end: this is the one site
/// where the emitted body is the actual HTTP response, not a rendered
/// `AppError`.
#[actix_web::test]
async fn demoting_the_last_admin_blames_role() {
    let db = TestDb::new().await;

    // The primary account is `MIN(id)`, so seed a member first: that keeps the
    // "primary admin" guard from firing before the last-admin guard.
    seed_user(&db.pool, "primary@example.com", UserRole::Member).await;
    let last_admin = seed_user(&db.pool, "admin@example.com", UserRole::Admin).await;

    let token = AuthTokenService::create(&db.pool, CreateAuthToken::default())
        .await
        .expect("seed admin-equivalent token")
        .token;

    let app = crate::common::app_with_config(&db.pool, crate::common::config()).await;

    let request = test::TestRequest::patch()
        .uri(&format!("/api/team/{}/role", last_admin.id))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(json!({ "role": "member" }))
        .to_request();
    let response = test::call_service(&app, request).await;

    assert_eq!(response.status().as_u16(), 409);
    let body: Value = test::read_body_json(response).await;
    assert_eq!(body["error"]["type"], "Conflict");
    assert_eq!(
        body["error"]["fields"],
        json!([{ "field": "role", "code": "invalid" }])
    );
}

/// The sibling 400 on the same body key. A role the enum does not contain is
/// `(role, invalid)` too: same field, same code, different status. The pair
/// used to be split — an inline error for "cannot demote the last admin" and a
/// form-level banner for "not a valid role" — which is backwards.
#[actix_web::test]
async fn an_unparseable_global_role_blames_role() {
    let db = TestDb::new().await;
    let target = seed_user(&db.pool, "member@example.com", UserRole::Member).await;

    let token = AuthTokenService::create(&db.pool, CreateAuthToken::default())
        .await
        .expect("seed admin-equivalent token")
        .token;

    let app = crate::common::app_with_config(&db.pool, crate::common::config()).await;

    let request = test::TestRequest::patch()
        .uri(&format!("/api/team/{}/role", target.id))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(json!({ "role": "superuser" }))
        .to_request();
    let response = test::call_service(&app, request).await;

    assert_eq!(response.status().as_u16(), 400);
    let body: Value = test::read_body_json(response).await;
    assert_eq!(body["error"]["type"], "ValidationError");
    assert_eq!(
        body["error"]["fields"],
        json!([{ "field": "role", "code": "invalid" }])
    );
}

/// A role id that does not exist, blamed on `role_ids`.
#[actix_web::test]
async fn an_unknown_project_role_blames_role_ids() {
    let db = TestDb::new().await;
    let project = seed_project(&db.pool, "Role Parsing Project", None).await;
    let user = seed_user(&db.pool, "member@example.com", UserRole::Member).await;

    let token = AuthTokenService::create(&db.pool, CreateAuthToken::default())
        .await
        .expect("seed admin-equivalent token")
        .token;

    let app = crate::common::app_with_config(&db.pool, crate::common::config()).await;

    let request = test::TestRequest::put()
        .uri(&format!("/api/projects/{}/members", project.id))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(json!({ "user_id": user.id, "role_ids": [999_999] }))
        .to_request();
    let response = test::call_service(&app, request).await;

    assert_eq!(response.status().as_u16(), 400);
    let body: Value = test::read_body_json(response).await;
    assert_eq!(body["error"]["type"], "ValidationError");
    assert_eq!(
        body["error"]["fields"],
        json!([{ "field": "role_ids", "code": "invalid" }])
    );
}
