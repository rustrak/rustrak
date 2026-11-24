//! The reader's own sidebar: which projects it shows, and in what order.

use crate::common::{create_test_token, seed_user, token_for, TestDb};
use actix_web::test;
use rustrak::models::UserRole;
use serde_json::{json, Value};

async fn call(db: &TestDb, token: &str, body: Option<Value>) -> (u16, Value) {
    let app = crate::common::app(&db.pool).await;
    let req = match &body {
        Some(body) => test::TestRequest::put().set_json(body),
        None => test::TestRequest::get(),
    }
    .uri("/api/sidebar/projects")
    .insert_header(("Authorization", format!("Bearer {token}")))
    .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status().as_u16();
    let bytes = test::read_body(resp).await;
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// Nothing saved yet reads as the default: every project, in manual order.
#[actix_web::test]
async fn test_a_new_reader_gets_the_default() {
    let db = TestDb::new().await;
    let ana = token_for(
        &db.pool,
        seed_user(&db.pool, "ana@x.com", UserRole::Admin).await.id,
    )
    .await;

    let (status, body) = call(&db, &ana, None).await;

    assert_eq!(status, 200);
    assert_eq!(body, json!({ "sort": "manual", "order": [], "hidden": [] }));
}

/// What one reader saves comes back to them, and to nobody else.
#[actix_web::test]
async fn test_the_sidebar_is_saved_per_reader() {
    let db = TestDb::new().await;
    let ana = token_for(
        &db.pool,
        seed_user(&db.pool, "ana@x.com", UserRole::Admin).await.id,
    )
    .await;
    let bob = token_for(
        &db.pool,
        seed_user(&db.pool, "bob@x.com", UserRole::Admin).await.id,
    )
    .await;
    let saved = json!({ "sort": "name", "order": [3, 1, 2], "hidden": [2] });

    assert_eq!(
        call(&db, &ana, Some(saved.clone())).await,
        (200, saved.clone())
    );
    assert_eq!(call(&db, &ana, None).await, (200, saved));
    let changed = json!({ "sort": "manual", "order": [1], "hidden": [] });
    assert_eq!(call(&db, &ana, Some(changed.clone())).await.1, changed);
    assert_eq!(
        call(&db, &bob, None).await.1,
        json!({ "sort": "manual", "order": [], "hidden": [] })
    );
}

/// A token without a user has no sidebar to save, and an unknown sort is
/// refused rather than stored.
#[actix_web::test]
async fn test_saving_needs_a_reader_and_a_known_sort() {
    let db = TestDb::new().await;
    let ana = token_for(
        &db.pool,
        seed_user(&db.pool, "ana@x.com", UserRole::Admin).await.id,
    )
    .await;
    let userless = create_test_token(&db.pool).await;
    let body = json!({ "sort": "manual", "order": [], "hidden": [] });

    assert_eq!(call(&db, &userless, Some(body)).await.0, 403);
    // Refused as the documented ErrorResponse, not Actix's plain text.
    let bad = json!({ "sort": "random", "order": [], "hidden": [] });
    let (status, body) = call(&db, &ana, Some(bad)).await;
    assert_eq!(status, 400);
    assert!(body.is_object(), "a JSON error body, got {body}");
}
