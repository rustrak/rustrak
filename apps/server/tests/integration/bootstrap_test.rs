//! Integration tests for Bootstrap functionality
//!
//! Tests the CREATE_SUPERUSER bootstrap mechanism

use crate::common::TestDb;
use rustrak::bootstrap;
use rustrak::services::UsersService;

#[tokio::test]
async fn test_bootstrap_creates_superuser_when_empty() {
    let db = TestDb::new().await;
    let spec = Some("admin@example.com:password123");

    // Run bootstrap
    let result = bootstrap::create_superuser_if_needed(&db.pool, spec).await;
    assert!(result.is_ok());

    let user = UsersService::get_by_email(&db.pool, "admin@example.com")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(user.email, "admin@example.com");
    assert!(user.is_admin());
    assert!(user.is_active);
}

#[tokio::test]
async fn test_bootstrap_skips_when_users_exist() {
    let db = TestDb::new().await;

    let req = rustrak::models::CreateUserRequest {
        email: "existing@example.com".to_string(),
        password: "password123".to_string(),
    };
    UsersService::create_user(&db.pool, &req, rustrak::models::UserRole::Member)
        .await
        .unwrap();
    let spec = Some("admin@example.com:password123");

    // Run bootstrap - should skip
    let result = bootstrap::create_superuser_if_needed(&db.pool, spec).await;
    assert!(result.is_ok());

    // Verify admin was NOT created
    let admin = UsersService::get_by_email(&db.pool, "admin@example.com")
        .await
        .unwrap();
    assert!(admin.is_none());
}

#[tokio::test]
async fn test_bootstrap_skips_when_env_not_set() {
    let db = TestDb::new().await;
    let spec = None;
    // Run bootstrap
    let result = bootstrap::create_superuser_if_needed(&db.pool, spec).await;
    assert!(result.is_ok());

    // Verify no users were created
    let count = UsersService::user_count(&db.pool).await.unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_bootstrap_fails_with_invalid_format() {
    let db = TestDb::new().await;

    // Invalid format (missing colon)
    let spec = Some("admin@example.com");

    let result = bootstrap::create_superuser_if_needed(&db.pool, spec).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_bootstrap_fails_with_empty_password() {
    let db = TestDb::new().await;

    // Empty password should be rejected
    let spec = Some("admin@example.com:");

    let result = bootstrap::create_superuser_if_needed(&db.pool, spec).await;
    assert!(result.is_err());

    // Verify no user was created
    let count = UsersService::user_count(&db.pool).await.unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_bootstrap_with_empty_string() {
    let db = TestDb::new().await;

    // Empty string
    let spec = Some("");

    let result = bootstrap::create_superuser_if_needed(&db.pool, spec).await;
    assert!(result.is_ok());

    // Verify no users were created
    let count = UsersService::user_count(&db.pool).await.unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_bootstrap_creates_admin_user() {
    let db = TestDb::new().await;

    let spec = Some("superadmin@example.com:superpassword123");

    let result = bootstrap::create_superuser_if_needed(&db.pool, spec).await;
    assert!(result.is_ok());

    let user = UsersService::get_by_email(&db.pool, "superadmin@example.com")
        .await
        .unwrap()
        .unwrap();

    assert!(user.is_admin());
}

#[tokio::test]
async fn test_bootstrap_password_is_hashed() {
    let db = TestDb::new().await;

    let spec = Some("hashcheck@example.com:testpassword123");

    let result = bootstrap::create_superuser_if_needed(&db.pool, spec).await;
    assert!(result.is_ok());

    let user = UsersService::get_by_email(&db.pool, "hashcheck@example.com")
        .await
        .unwrap()
        .unwrap();

    // Password should be hashed, not plain text
    assert_ne!(user.password_hash, "testpassword123");
    assert!(user.password_hash.starts_with("$argon2"));

    assert!(user.verify_password("testpassword123").unwrap());
}

#[tokio::test]
async fn test_bootstrap_with_email_containing_colon() {
    let db = TestDb::new().await;

    // Email with colon in local part is invalid according to our email regex constraint
    // This tests splitn behavior - splits on first colon
    let spec = Some("test:email@example.com:password123");

    let result = bootstrap::create_superuser_if_needed(&db.pool, spec).await;

    // Should fail because email doesn't match regex constraint
    assert!(result.is_err());
}

#[tokio::test]
async fn test_bootstrap_idempotent_across_restarts() {
    let db = TestDb::new().await;

    let spec = Some("restart@example.com:password123");

    // First "startup"
    let result1 = bootstrap::create_superuser_if_needed(&db.pool, spec).await;
    assert!(result1.is_ok());

    let user1 = UsersService::get_by_email(&db.pool, "restart@example.com")
        .await
        .unwrap()
        .unwrap();

    // Second "startup" - should skip
    let result2 = bootstrap::create_superuser_if_needed(&db.pool, spec).await;
    assert!(result2.is_ok());

    // Verify no duplicate was created
    let count = UsersService::user_count(&db.pool).await.unwrap();
    assert_eq!(count, 1);

    let user2 = UsersService::get_by_email(&db.pool, "restart@example.com")
        .await
        .unwrap()
        .unwrap();

    // Same user ID confirms no duplicate
    assert_eq!(user1.id, user2.id);
}

#[tokio::test]
async fn test_bootstrap_with_whitespace_in_email() {
    let db = TestDb::new().await;

    // Email with surrounding whitespace
    let spec = Some("  whitespace@example.com  :password123");

    let result = bootstrap::create_superuser_if_needed(&db.pool, spec).await;
    assert!(result.is_ok());

    // Email should be trimmed
    let user = UsersService::get_by_email(&db.pool, "whitespace@example.com")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(user.email, "whitespace@example.com");
}
