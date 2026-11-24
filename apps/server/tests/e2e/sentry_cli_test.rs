//! `sentry-cli releases` against the real binary (#494, #91): every command a
//! CI pipeline runs succeeds, and what it set reads back.
//!
//! Needs a sentry-cli binary: set `SENTRY_CLI` to its path, for example
//! `npm install @sentry/cli` and `SENTRY_CLI=node_modules/.bin/sentry-cli`.
//! Without it the test says so and checks nothing.

use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use tempfile::TempDir;

use crate::common::crash::free_port;

const TOKEN: &str = "0123456789abcdef0123456789abcdef01234567";

/// A running server, killed and reaped when dropped.
struct Running(Child);

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start_server(dir: &Path, port: u16) -> Running {
    let config = dir.join("rustrak.toml");
    std::fs::write(
        &config,
        format!(
            "[[bootstrap.projects]]\nid = 1\nname = \"Web\"\nslug = \"web\"\n\
             sentry_key = \"0123456789abcdef0123456789abcdef\"\n\
             [[bootstrap.tokens]]\nname = \"ci\"\ntoken = \"{TOKEN}\"\n"
        ),
    )
    .unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_rustrak"))
        .current_dir(dir)
        .env(
            "DATABASE_URL",
            format!("sqlite://{}?mode=rwc", dir.join("rustrak.db").display()),
        )
        .env("INGEST_DIR", dir.join("ingest"))
        .env("HOST", "127.0.0.1")
        .env("PORT", port.to_string())
        .env("RUSTRAK_TELEMETRY", "false")
        .env("RUSTRAK_DASHBOARD", "off")
        .env("RUSTRAK_CONFIG", &config)
        .env_remove("CREATE_SUPERUSER")
        .env_remove("RUSTRAK_BOOTSTRAP_TOKEN")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn the server binary");
    let running = Running(child);
    let deadline = Instant::now() + Duration::from_secs(30);
    while std::net::TcpStream::connect(("127.0.0.1", port)).is_err() {
        assert!(Instant::now() < deadline, "the server never listened");
        std::thread::sleep(Duration::from_millis(50));
    }
    running
}

/// A git checkout with two commits and a remote, for `set-commits --local`.
fn git_repo(dir: &Path) -> Vec<String> {
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    git(&["init", "-q"]);
    git(&["config", "user.email", "ada@example.com"]);
    git(&["config", "user.name", "Ada"]);
    git(&["remote", "add", "origin", "https://github.com/acme/web.git"]);
    for message in ["first", "second"] {
        std::fs::write(dir.join("f.txt"), message).unwrap();
        git(&["add", "f.txt"]);
        git(&["commit", "-qm", message]);
    }
    git(&["log", "--format=%H"])
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn sentry_cli_runs_every_releases_command() {
    let Some(cli) = std::env::var_os("SENTRY_CLI") else {
        eprintln!("skipped: set SENTRY_CLI to a sentry-cli binary to run this test");
        return;
    };
    // The CLI runs inside a temporary checkout, so a relative path such as
    // `node_modules/.bin/sentry-cli` is resolved here first. A bare name is
    // left to the PATH.
    let cli = std::path::PathBuf::from(cli);
    let cli = if cli.is_relative() && cli.components().count() > 1 {
        std::env::current_dir().unwrap().join(cli)
    } else {
        cli
    };
    let server_dir = TempDir::new().unwrap();
    let repo = TempDir::new().unwrap();
    let port = free_port();
    let _server = start_server(server_dir.path(), port);
    let commits = git_repo(repo.path());

    let run = |args: &[&str]| -> Output {
        Command::new(&cli)
            .args(args)
            .current_dir(repo.path())
            .env("SENTRY_URL", format!("http://127.0.0.1:{port}"))
            .env("SENTRY_AUTH_TOKEN", TOKEN)
            .env("SENTRY_ORG", "acme")
            .env("SENTRY_PROJECT", "web")
            .env("SENTRY_LOAD_DOTENV", "0")
            .output()
            .expect("run sentry-cli")
    };
    let ok = |args: &[&str]| -> String {
        let out = run(args);
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        assert!(
            out.status.success(),
            "sentry-cli {args:?} failed:\n{stdout}\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        stdout
    };

    ok(&["releases", "new", "1.0.0"]);
    ok(&["releases", "new", "1.0.0"]); // already exists: 208, still a success
    ok(&["releases", "finalize", "1.0.0"]);
    ok(&["releases", "set-commits", "1.0.0", "--local"]);
    ok(&["releases", "deploys", "1.0.0", "new", "-e", "production"]);
    assert!(ok(&["releases", "deploys", "1.0.0", "list"]).contains("production"));
    assert!(ok(&["releases", "list"]).contains("1.0.0"));
    let info = ok(&["releases", "info", "1.0.0", "--show-commits"]);
    for sha in &commits {
        assert!(
            info.contains(sha.as_str()),
            "commit {sha} missing from:\n{info}"
        );
    }
    ok(&["releases", "delete", "1.0.0"]);
    assert!(
        !run(&["releases", "info", "1.0.0"]).status.success(),
        "a deleted release is not found"
    );
}
