//! The admin commands: `createuser`, `healthcheck`, and the binary's own
//! dispatch.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;

use crate::common::{app, login, seed_user, TestDb};
use rustrak::cli::{self, CreateUser};
use rustrak::models::UserRole;
use rustrak::services::UsersService;

fn spec(email: &str, superuser: bool, force_update: bool) -> CreateUser {
    CreateUser {
        email: email.into(),
        password: None,
        superuser,
        force_update,
    }
}

#[tokio::test]
async fn createuser_creates_a_member_or_an_owner() {
    let db = TestDb::new().await;

    cli::create_user(&db.pool, &spec("Member@Example.com", false, false), "pw1")
        .await
        .unwrap();
    cli::create_user(&db.pool, &spec("owner@example.com", true, false), "pw2")
        .await
        .unwrap();

    let member = UsersService::get_by_email(&db.pool, "member@example.com")
        .await
        .unwrap()
        .expect("created, with the email normalized");
    assert!(!member.is_admin());
    assert!(member.verify_password("pw1").unwrap());
    let owner = UsersService::get_by_email(&db.pool, "owner@example.com")
        .await
        .unwrap()
        .unwrap();
    assert!(owner.is_admin());
}

#[tokio::test]
async fn createuser_refuses_an_existing_user_without_force_update() {
    let db = TestDb::new().await;
    seed_user(&db.pool, "a@example.com", UserRole::Member).await;

    let error = cli::create_user(&db.pool, &spec("a@example.com", true, false), "new")
        .await
        .expect_err("an existing user must not be changed silently");

    assert!(error.to_string().contains("--force-update"), "{error}");
    let user = UsersService::get_by_email(&db.pool, "a@example.com")
        .await
        .unwrap()
        .unwrap();
    assert!(user.verify_password("password123").unwrap());
    assert!(!user.is_admin());
}

/// Resetting a password and promoting to owner, with no session needed.
#[tokio::test]
async fn force_update_resets_the_password_promotes_and_signs_out() {
    let db = TestDb::new().await;
    seed_user(&db.pool, "a@example.com", UserRole::Member).await;
    let app = app(&db.pool).await;
    let cookie = login(&app, "a@example.com").await;

    cli::create_user(&db.pool, &spec("a@example.com", true, true), "new")
        .await
        .unwrap();

    let user = UsersService::get_by_email(&db.pool, "a@example.com")
        .await
        .unwrap()
        .unwrap();
    assert!(user.verify_password("new").unwrap());
    assert!(user.is_admin());
    let req = actix_web::test::TestRequest::get()
        .uri("/auth/me")
        .cookie(cookie)
        .to_request();
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401, "the old session must be ended");
}

/// Without `--superuser` an owner stays one: a password reset never demotes.
#[tokio::test]
async fn force_update_without_superuser_keeps_the_role() {
    let db = TestDb::new().await;
    seed_user(&db.pool, "a@example.com", UserRole::Admin).await;

    cli::create_user(&db.pool, &spec("a@example.com", false, true), "new")
        .await
        .unwrap();

    let user = UsersService::get_by_email(&db.pool, "a@example.com")
        .await
        .unwrap()
        .unwrap();
    assert!(user.is_admin());
}

#[tokio::test]
async fn createuser_refuses_an_empty_password() {
    let db = TestDb::new().await;

    assert!(
        cli::create_user(&db.pool, &spec("a@example.com", false, false), "")
            .await
            .is_err()
    );
    assert!(UsersService::get_by_email(&db.pool, "a@example.com")
        .await
        .unwrap()
        .is_none());
}

/// A one-shot HTTP server answering every request with `status_line`.
fn answering(status_line: &'static str) -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0u8; 1024];
        let n = stream.read(&mut request).unwrap();
        assert!(String::from_utf8_lossy(&request[..n]).starts_with("GET /health/ready "));
        let _ = write!(stream, "{status_line}\r\nContent-Length: 0\r\n\r\n");
    });
    addr
}

#[test]
fn healthcheck_passes_only_on_200() {
    assert_eq!(cli::healthcheck(answering("HTTP/1.1 200 OK")), Ok(()));
    let error = cli::healthcheck(answering("HTTP/1.1 503 Service Unavailable")).unwrap_err();
    assert!(error.contains("503"), "{error}");
}

#[test]
fn healthcheck_fails_when_nothing_listens() {
    let addr = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    assert!(cli::healthcheck(addr).is_err());
}

fn rustrak() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rustrak"));
    // Nothing from a developer's .env or shell leaks in.
    command.env_clear().current_dir(std::env::temp_dir());
    command
}

#[test]
fn the_binary_prints_its_version() {
    let out = rustrak().arg("--version").output().unwrap();
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        format!("rustrak {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn the_binary_refuses_an_unknown_command() {
    let out = rustrak().arg("serv").output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("serv"));
}

/// The issue's case: a `/` in the password reads as a bad port. The error
/// names the variable and never prints the password.
#[cfg(feature = "postgres")]
#[test]
fn check_config_names_a_bad_database_url() {
    let out = rustrak()
        .arg("check-config")
        .env("DATABASE_URL", "postgres://user:se/cret@db:5432/rustrak")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("DATABASE_URL"), "{stderr}");
    assert!(!stderr.contains("se/cret"), "{stderr}");
}

#[cfg(feature = "sqlite")]
#[test]
fn check_config_accepts_a_valid_configuration() {
    let out = rustrak()
        .arg("check-config")
        .env("DATABASE_URL", "sqlite://rustrak-check.db")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[cfg(feature = "sqlite")]
#[test]
fn check_config_names_a_bad_database_url() {
    let out = rustrak()
        .arg("check-config")
        .env("DATABASE_URL", "sqlite://rustrak.db?mode=sideways")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("DATABASE_URL"), "{stderr}");
}
