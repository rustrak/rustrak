//! `RUSTRAK_CONFIG` against the real binary: a good file is applied before
//! the server listens, a bad one stops it with a message and a non-zero exit.

use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use tempfile::TempDir;

use crate::common::crash::free_port;

const KEY: &str = "0123456789abcdef0123456789abcdef";
const TOKEN: &str = "0123456789abcdef0123456789abcdef01234567";

fn server(dir: &Path, config: &Path, port: u16) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rustrak"));
    command
        // No `.env` from the source tree: dotenvy reads the working directory.
        .current_dir(dir)
        .env(
            "DATABASE_URL",
            format!("sqlite://{}?mode=rwc", dir.join("rustrak.db").display()),
        )
        .env("INGEST_DIR", dir.join("ingest"))
        .env("HOST", "127.0.0.1")
        .env("PORT", port.to_string())
        .env("PUBLIC_URL", "https://errors.example.com")
        .env("RUSTRAK_TELEMETRY", "false")
        .env("RUSTRAK_DASHBOARD", "off")
        .env("RUST_LOG", "info")
        .env("RUSTRAK_CONFIG", config)
        .env_remove("CREATE_SUPERUSER")
        .env_remove("RUSTRAK_BOOTSTRAP_TOKEN")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

fn write_config(dir: &Path, toml: &str) -> std::path::PathBuf {
    let path = dir.join("rustrak.toml");
    std::fs::write(&path, toml).unwrap();
    path
}

/// Runs a server that must refuse to start, and returns how it ended. A
/// server still running after the deadline is killed and fails the test.
fn run_to_exit(mut command: Command) -> (Output, String) {
    let mut child = command.spawn().expect("spawn the server binary");
    let deadline = Instant::now() + Duration::from_secs(30);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("the server started when it should have refused to");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let output = child.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (output, stderr)
}

fn refused(toml: &str, extra_env: &[(&str, &str)]) -> (Output, String) {
    let dir = TempDir::new().unwrap();
    let config = write_config(dir.path(), toml);
    let mut command = server(dir.path(), &config, free_port());
    for (k, v) in extra_env {
        command.env(k, v);
    }
    run_to_exit(command)
}

#[tokio::test]
async fn serves_the_declared_project_and_its_dsn_to_the_declared_token() {
    let dir = TempDir::new().unwrap();
    let config = write_config(
        dir.path(),
        &format!(
            "[[bootstrap.projects]]\nid = 7\nname = \"Storefront\"\nsentry_key = \"{KEY}\"\n\
             [[bootstrap.tokens]]\nname = \"ci\"\ntoken = \"{TOKEN}\"\n"
        ),
    );
    let port = free_port();
    let mut child = server(dir.path(), &config, port)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn the server binary");

    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{port}/api/projects/7");
    let deadline = Instant::now() + Duration::from_secs(30);
    let body: serde_json::Value = loop {
        if let Ok(Some(status)) = child.try_wait() {
            panic!("the server exited ({status})");
        }
        if let Ok(r) = client.get(&url).bearer_auth(TOKEN).send().await {
            if r.status().is_success() {
                break r.json().await.unwrap();
            }
        }
        assert!(Instant::now() < deadline, "the server never answered");
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    let _ = child.kill();
    let _ = child.wait();

    assert_eq!(body["name"], "Storefront");
    assert_eq!(body["dsn"], format!("https://{KEY}@errors.example.com/7"));
}

#[test]
fn refuses_to_start_on_a_file_it_cannot_parse_without_echoing_a_secret() {
    let (output, stderr) = refused(
        &format!("[[bootstrap.tokens]]\nname = \"ci\"\ntokn = \"{TOKEN}\"\n"),
        &[],
    );
    assert!(!output.status.success());
    assert!(
        stderr.contains("rustrak.toml") && stderr.contains("tokn"),
        "{stderr}"
    );
    assert!(
        !stderr.contains(TOKEN),
        "the log leaks the secret:\n{stderr}"
    );
}

#[test]
fn refuses_to_start_when_the_file_does_not_exist() {
    let dir = TempDir::new().unwrap();
    let missing = dir.path().join("nowhere.toml");
    let (output, stderr) = run_to_exit(server(dir.path(), &missing, free_port()));
    assert!(!output.status.success());
    assert!(stderr.contains("nowhere.toml"), "{stderr}");
}

#[tokio::test]
async fn refuses_to_start_when_the_database_contradicts_the_file() {
    let dir = TempDir::new().unwrap();
    let config = write_config(
        dir.path(),
        &format!("[[bootstrap.projects]]\nid = 1\nname = \"App\"\nsentry_key = \"{KEY}\"\n"),
    );
    // Up once, so project 1 exists with KEY.
    let port = free_port();
    let mut child = server(dir.path(), &config, port)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let client = reqwest::Client::new();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        assert!(
            child.try_wait().unwrap().is_none(),
            "the first start failed"
        );
        let health = client
            .get(format!("http://127.0.0.1:{port}/health"))
            .send()
            .await;
        if health.is_ok_and(|r| r.status().is_success()) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the first start never became healthy"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let _ = child.kill();
    let _ = child.wait();

    write_config(
        dir.path(),
        "[[bootstrap.projects]]\nid = 1\nname = \"App\"\nsentry_key = \"fedcba9876543210fedcba9876543210\"\n",
    );
    let (output, stderr) = run_to_exit(server(dir.path(), &config, free_port()));
    assert!(!output.status.success());
    assert!(
        stderr.contains("project 1") && stderr.contains("sentry_key"),
        "{stderr}"
    );
}

#[test]
fn refuses_to_start_with_two_superusers_to_create() {
    let (output, stderr) = refused(
        "[bootstrap.superuser]\nemail = \"admin@example.com\"\npassword = \"from-the-file\"\n",
        &[("CREATE_SUPERUSER", "other@example.com:from-the-env")],
    );
    assert!(!output.status.success());
    assert!(
        stderr.contains("CREATE_SUPERUSER") && stderr.contains("bootstrap.superuser"),
        "{stderr}"
    );
}
