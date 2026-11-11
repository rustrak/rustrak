//! The tokens API: listing, minting from a session, deleting.

use crate::common::{create_test_token, login, seed_user, TestDb};
use actix_web::test;
use rustrak::models::{CreateAuthToken, UserRole};
use rustrak::services::AuthTokenService;
use serde_json::{json, Value};

#[actix_web::test]
async fn test_list_tokens_masks_token_values() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let app = crate::common::app(&db.pool).await;

    let req = test::TestRequest::get()
        .uri("/api/tokens")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();
    let body: Vec<Value> = test::call_and_read_body_json(&app, req).await;

    assert!(!body.is_empty());
    for token_obj in &body {
        let token_prefix = token_obj["token_prefix"].as_str().unwrap();
        assert!(token_prefix.ends_with("..."));
        assert_eq!(token_prefix.len(), 11);
        assert!(token_obj.get("token").is_none());
    }
}

#[actix_web::test]
async fn test_bearer_that_is_not_a_token_is_refused() {
    let db = TestDb::new().await;
    let app = crate::common::app(&db.pool).await;

    for header in ["Bearer invalid_token_here", "abc123", "Bearer "] {
        let req = test::TestRequest::get()
            .uri("/api/tokens")
            .insert_header(("Authorization", header))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401, "{header:?}");
    }
}

#[actix_web::test]
async fn test_create_token_from_a_session() {
    let db = TestDb::new().await;
    seed_user(&db.pool, "owner@example.com", UserRole::Admin).await;
    let app = crate::common::app(&db.pool).await;
    let cookie = login(&app, "owner@example.com").await;

    let mut tokens = std::collections::HashSet::new();
    for body in [json!({ "description": "New API token" }), json!({})] {
        let req = test::TestRequest::post()
            .uri("/api/tokens")
            .cookie(cookie.clone())
            .set_json(&body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
        let created: Value = test::read_body_json(resp).await;

        let token = created["token"].as_str().unwrap().to_string();
        assert_eq!(token.len(), 40);
        assert!(token
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)));
        assert!(created["id"].is_number());
        assert_eq!(created["description"], body["description"]);
        tokens.insert(token);
    }
    assert_eq!(tokens.len(), 2);
}

#[actix_web::test]
async fn test_a_token_cannot_expire_in_the_past() {
    let db = TestDb::new().await;
    seed_user(&db.pool, "owner@example.com", UserRole::Admin).await;
    let app = crate::common::app(&db.pool).await;
    let cookie = login(&app, "owner@example.com").await;

    let past = chrono::Utc::now() - chrono::Duration::minutes(1);
    let req = test::TestRequest::post()
        .uri("/api/tokens")
        .cookie(cookie)
        .set_json(json!({ "expires_at": past }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["error"]["fields"],
        json!([{ "field": "expires_at", "code": "invalid" }])
    );
}

#[actix_web::test]
async fn test_delete_token_success() {
    let db = TestDb::new().await;
    let auth_token = create_test_token(&db.pool).await;
    let token_to_delete = AuthTokenService::create(&db.pool, CreateAuthToken::default())
        .await
        .unwrap();
    let app = crate::common::app(&db.pool).await;

    let req = test::TestRequest::delete()
        .uri(&format!("/api/tokens/{}", token_to_delete.id))
        .insert_header(("Authorization", format!("Bearer {}", auth_token)))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 204);

    let req = test::TestRequest::get()
        .uri(&format!("/api/tokens/{}", token_to_delete.id))
        .insert_header(("Authorization", format!("Bearer {}", auth_token)))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 404);
}

#[actix_web::test]
async fn test_token_not_found() {
    let db = TestDb::new().await;
    let auth_token = create_test_token(&db.pool).await;
    let app = crate::common::app(&db.pool).await;

    for req in [
        test::TestRequest::get().uri("/api/tokens/99999"),
        test::TestRequest::delete().uri("/api/tokens/99999"),
        test::TestRequest::delete().uri("/api/tokens/not-a-number"),
    ] {
        let req = req
            .insert_header(("Authorization", format!("Bearer {}", auth_token)))
            .to_request();
        assert_eq!(test::call_service(&app, req).await.status(), 404);
    }
}

/// A token may delete itself, and is refused from then on.
#[actix_web::test]
async fn test_deleted_token_stops_working() {
    let db = TestDb::new().await;
    let token = AuthTokenService::create(&db.pool, CreateAuthToken::default())
        .await
        .unwrap();
    let app = crate::common::app(&db.pool).await;

    let req = test::TestRequest::delete()
        .uri(&format!("/api/tokens/{}", token.id))
        .insert_header(("Authorization", format!("Bearer {}", token.token)))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 204);

    let req = test::TestRequest::get()
        .uri("/api/tokens")
        .insert_header(("Authorization", format!("Bearer {}", token.token)))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 401);
}

#[actix_web::test]
async fn test_token_last_used_at_updated() {
    let db = TestDb::new().await;
    let token = AuthTokenService::create(&db.pool, CreateAuthToken::default())
        .await
        .unwrap();
    let app = crate::common::app(&db.pool).await;

    let last_used = || async {
        AuthTokenService::get_by_token(&db.pool, &token.token)
            .await
            .unwrap()
            .unwrap()
            .last_used_at
    };
    assert!(last_used().await.is_none());

    let req = test::TestRequest::get()
        .uri("/api/tokens")
        .insert_header(("Authorization", format!("Bearer {}", token.token)))
        .to_request();
    assert!(test::call_service(&app, req).await.status().is_success());

    crate::common::eventually(last_used).await;
}
