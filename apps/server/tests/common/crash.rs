//! Crash tests against the real binary: start it with a failpoint, drive
//! envelopes until it aborts, restart it without one, and check that every
//! acknowledged event was stored exactly once.
//!
//! `failpoint!` is compiled into the binary these tests spawn by the
//! `test-seams` feature every test build has.

use std::collections::HashSet;
use std::net::TcpListener;
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use rustrak::config::DatabaseConfig;
use tempfile::TempDir;

/// Envelopes sent per test, `AT_ONCE` at a time with a pause between, so
/// the digest forms batches of more than one event. The failpoint fires on
/// its `hit`th pass, so some events are digested before the crash and some
/// are only acknowledged.
const ENVELOPES: usize = 32;
const AT_ONCE: usize = 8;

/// The pass a failpoint fires on: an ingest one passes once per event, a
/// digest one once per batch.
fn hit(failpoint: &str) -> u32 {
    if failpoint.starts_with("digest.") {
        2
    } else {
        10
    }
}

/// One server process on a temp database and spool.
struct Server {
    child: Child,
    port: u16,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .expect("free port")
        .port()
}

fn database_url(dir: &Path) -> String {
    format!("sqlite://{}?mode=rwc", dir.join("rustrak.db").display())
}

fn spawn(dir: &Path, failpoint: Option<&str>) -> Server {
    let port = free_port();
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("server.log"))
        .expect("server log");
    let mut command = Command::new(env!("CARGO_BIN_EXE_rustrak"));
    command
        // No `.env` from the source tree: dotenvy reads the working directory.
        .current_dir(dir)
        .env("DATABASE_URL", database_url(dir))
        .env("INGEST_DIR", dir.join("ingest"))
        .env("HOST", "127.0.0.1")
        .env("PORT", port.to_string())
        .env("RUSTRAK_TELEMETRY", "false")
        .env("RUSTRAK_DASHBOARD", "off")
        .env("RUST_LOG", "warn")
        // The alert case's webhook listens on loopback.
        .env("RUSTRAK_ALERTS_ALLOW_PRIVATE_NETWORKS", "true")
        .env_remove("RUSTRAK_FAILPOINT")
        .stdout(Stdio::null())
        .stderr(log);
    if let Some(name) = failpoint {
        command
            .env("RUSTRAK_FAILPOINT", name)
            .env("RUSTRAK_FAILPOINT_HIT", hit(name).to_string());
    }
    Server {
        child: command.spawn().expect("spawn the server binary"),
        port,
    }
}

async fn wait_healthy(server: &mut Server, client: &reqwest::Client, dir: &Path) {
    let deadline = Instant::now() + Duration::from_secs(30);
    let url = format!("http://127.0.0.1:{}/health", server.port);
    loop {
        if let Ok(Some(status)) = server.child.try_wait() {
            panic!(
                "server exited before it was ready ({status}):\n{}",
                log_tail(dir)
            );
        }
        if client
            .get(&url)
            .send()
            .await
            .is_ok_and(|r| r.status().is_success())
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "server not ready:\n{}",
            log_tail(dir)
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn log_tail(dir: &Path) -> String {
    let log = std::fs::read_to_string(dir.join("server.log")).unwrap_or_default();
    let lines: Vec<_> = log.lines().collect();
    lines[lines.len().saturating_sub(30)..].join("\n")
}

async fn open_pool(dir: &Path) -> rustrak::db::DbPool {
    rustrak::db::create_pool(&DatabaseConfig {
        url: database_url(dir),
        ..DatabaseConfig::default()
    })
    .await
    .expect("pool")
}

/// The project the envelopes go to, created before the server first starts.
async fn create_project(dir: &Path) -> (i32, String) {
    let pool = open_pool(dir).await;
    rustrak::db::run_migrations(&pool)
        .await
        .expect("migrations");
    let project = super::create_ingest_project(&pool, "crash").await;
    pool.close().await;
    project
}

async fn stored_event_ids(dir: &Path) -> Vec<String> {
    let pool = open_pool(dir).await;
    let ids: Vec<uuid::Uuid> = sqlx::query_scalar("SELECT event_id FROM events")
        .fetch_all(&pool)
        .await
        .expect("events");
    pool.close().await;
    ids.into_iter().map(|id| id.simple().to_string()).collect()
}

fn pending_files(dir: &Path) -> usize {
    std::fs::read_dir(dir.join("ingest")).map_or(0, |entries| {
        entries
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".pending.json"))
            .count()
    })
}

fn dead_letters(dir: &Path) -> usize {
    std::fs::read_dir(dir.join("ingest").join("dead")).map_or(0, |entries| entries.count())
}

/// A webhook receiver on loopback: answers 200 to every POST and keeps
/// each body it got.
fn webhook_listener() -> (String, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
    use std::io::{BufRead, BufReader, Read, Write};
    let listener = TcpListener::bind("127.0.0.1:0").expect("webhook listener");
    let url = format!("http://{}/hook", listener.local_addr().unwrap());
    let bodies = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let kept = std::sync::Arc::clone(&bodies);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream);
            let mut length = 0;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some((name, value)) = line.split_once(':') {
                    if name.eq_ignore_ascii_case("content-length") {
                        length = value.trim().parse().unwrap_or(0);
                    }
                }
            }
            let mut body = vec![0; length];
            if reader.read_exact(&mut body).is_ok() {
                kept.lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&body).into_owned());
            }
            let _ = reader
                .into_inner()
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        }
    });
    (url, bodies)
}

/// A new-issue rule on the project that posts to `url`.
async fn add_new_issue_rule(dir: &Path, project_id: i32, url: &str) {
    use rustrak::models::{
        AlertRuleChannelInput, AlertType, CreateAlertIntegration, CreateAlertRule, ProviderType,
    };
    use rustrak::services::AlertService;
    let pool = open_pool(dir).await;
    let smtp = rustrak::config::SmtpSettings::default();
    let channel = AlertService::create_channel(
        &pool,
        &smtp,
        CreateAlertIntegration {
            name: "crash hook".to_string(),
            provider_type: ProviderType::Webhook,
            credentials: serde_json::json!({ "url": url }),
            is_enabled: true,
        },
    )
    .await
    .expect("integration");
    AlertService::create_rule(
        &pool,
        project_id,
        CreateAlertRule {
            name: "crash rule".to_string(),
            alert_type: AlertType::NewIssue,
            channels: vec![AlertRuleChannelInput {
                integration_id: channel.id,
                routing_override: serde_json::json!({}),
            }],
            conditions: serde_json::json!({}),
            cooldown_minutes: 0,
        },
    )
    .await
    .expect("rule");
    pool.close().await;
}

async fn count(dir: &Path, sql: &'static str) -> i64 {
    let pool = open_pool(dir).await;
    let n = sqlx::query_scalar(sql)
        .fetch_one(&pool)
        .await
        .expect("count");
    pool.close().await;
    n
}

/// Runs the whole crash cycle at `failpoint` and asserts the guarantee: every
/// event that got a 200 is in `events` exactly once after the restart, the
/// spool drained, and nothing was dead-lettered.
pub async fn assert_acked_events_survive(failpoint: &str) {
    crash_cycle(failpoint, false).await;
}

/// [`assert_acked_events_survive`] with a new-issue alert rule posting to a
/// webhook: after the restart each issue has its one alert row, and no
/// alert was delivered twice.
pub async fn assert_alerts_survive(failpoint: &str) {
    let (temp, bodies) = crash_cycle(failpoint, true).await;
    let issues = count(temp.path(), "SELECT COUNT(*) FROM issues").await;
    let rows = count(temp.path(), "SELECT COUNT(*) FROM alert_history").await;
    assert!(issues > 0);
    assert_eq!(rows, issues, "one new-issue alert per issue");
    let bodies = bodies.lock().unwrap().clone();
    let unique: HashSet<_> = bodies.iter().collect();
    assert_eq!(unique.len(), bodies.len(), "an alert was delivered twice");
    assert!(bodies.len() as i64 <= rows);
}

async fn crash_cycle(
    failpoint: &str,
    alerts: bool,
) -> (TempDir, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
    let temp = TempDir::new().expect("temp dir");
    let dir = temp.path();
    let (project_id, key) = create_project(dir).await;
    let (hook, bodies) = webhook_listener();
    if alerts {
        add_new_issue_rule(dir, project_id, &hook).await;
    }
    let client = reqwest::Client::new();

    let mut server = spawn(dir, Some(failpoint));
    wait_healthy(&mut server, &client, dir).await;
    let url = format!(
        "http://127.0.0.1:{}/api/{project_id}/envelope/",
        server.port
    );

    let mut acked = HashSet::new();
    for group in 0..ENVELOPES / AT_ONCE {
        if server.child.try_wait().ok().flatten().is_some() {
            break;
        }
        let sends = (0..AT_ONCE).map(|i| {
            let (client, url, key) = (&client, &url, &key);
            async move {
                let id = uuid::Uuid::new_v4().simple().to_string();
                let event = super::EventBuilder::new()
                    .with_event_id(&id)
                    .with_exception("CrashTest", &format!("crash test {}", (group + i) % 3))
                    .build();
                let body = super::create_envelope(project_id as u32, key, &event);
                let sent = client
                    .post(url)
                    .header("X-Sentry-Auth", format!("Sentry sentry_key={key}"))
                    .body(body)
                    .send()
                    .await;
                sent.is_ok_and(|r| r.status().is_success()).then_some(id)
            }
        });
        acked.extend(
            futures_util::future::join_all(sends)
                .await
                .into_iter()
                .flatten(),
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // The failpoint aborts: SIGABRT, not a clean exit.
    let deadline = Instant::now() + Duration::from_secs(15);
    let status = loop {
        if let Some(status) = server.child.try_wait().expect("wait") {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "server never hit failpoint {failpoint}:\n{}",
            log_tail(dir)
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert_eq!(
        status.signal(),
        Some(libc::SIGABRT),
        "expected an abort at {failpoint}, got {status}:\n{}",
        log_tail(dir)
    );
    assert!(
        !acked.is_empty() && acked.len() < ENVELOPES,
        "the crash should land mid-run: {} of {ENVELOPES} acknowledged",
        acked.len()
    );
    drop(server);

    // Restart clean and let recovery drain the spool.
    let mut server = spawn(dir, None);
    wait_healthy(&mut server, &client, dir).await;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let stored: HashSet<_> = stored_event_ids(dir).await.into_iter().collect();
        if acked.is_subset(&stored) && pending_files(dir) == 0 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "after restart: {} acknowledged events missing, {} spool files left:\n{}",
            acked.difference(&stored).count(),
            pending_files(dir),
            log_tail(dir)
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    drop(server);

    let stored = stored_event_ids(dir).await;
    let unique: HashSet<_> = stored.iter().cloned().collect();
    assert_eq!(stored.len(), unique.len(), "an event was stored twice");
    let missing: Vec<_> = acked.difference(&unique).collect();
    assert!(missing.is_empty(), "acknowledged but lost: {missing:?}");
    assert_eq!(dead_letters(dir), 0, "events were dead-lettered");
    (temp, bodies)
}
