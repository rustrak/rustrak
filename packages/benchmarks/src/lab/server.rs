//! One server process for one run: started on a wiped data directory (or a
//! fresh PostgreSQL database), set up over its own API, sampled, stopped.

use super::load::{self, Target, Template};
use super::procfs::{self, Sample};
use super::{LabArgs, Variant};
use anyhow::{bail, Context};
use hdrhistogram::Histogram;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const PORT: u16 = 18080;
const EMAIL: &str = "bench@example.com";
const PASSWORD: &str = "bench-password-1234";
const LAB_DB: &str = "rustrak_lab";

/// A `/metrics` reading.
#[derive(Debug, Clone, Copy, Default)]
pub struct Poll {
    pub t: f64,
    pub digested: u64,
    /// Rate-limited or failed digests: done, not stored.
    pub dropped: u64,
    pub pending: u64,
}

pub struct Credentials {
    pub project_id: i64,
    pub key: String,
}

pub struct Server {
    /// The process we spawned: the server itself, or strace around it.
    child: Child,
    pub pid: u32,
    pub base: String,
    started: Instant,
    strace_out: Option<PathBuf>,
    http: reqwest::Client,
    cookie: Option<String>,
}

fn wipe(dir: &Path) -> anyhow::Result<()> {
    if dir.exists() {
        std::fs::remove_dir_all(dir).with_context(|| format!("wipe {}", dir.display()))?;
    }
    std::fs::create_dir_all(dir.join("ingest"))?;
    Ok(())
}

async fn reset_postgres(url: &str) -> anyhow::Result<()> {
    let (client, conn) = tokio_postgres::connect(&format!("{url}/postgres"), tokio_postgres::NoTls)
        .await
        .context("connect to postgres")?;
    tokio::spawn(conn);
    // Two calls: a multi-statement string runs as one implicit transaction,
    // which DROP DATABASE refuses.
    client
        .batch_execute(&format!("DROP DATABASE IF EXISTS {LAB_DB} WITH (FORCE)"))
        .await?;
    client
        .batch_execute(&format!("CREATE DATABASE {LAB_DB}"))
        .await?;
    Ok(())
}

impl Server {
    pub async fn start(args: &LabArgs, variant: &Variant) -> anyhow::Result<Self> {
        wipe(&args.data_dir)?;
        if let Some(url) = &args.postgres {
            reset_postgres(url).await?;
        }
        Self::spawn(args, variant).await
    }

    /// Starts on whatever is in the data dir (the seeded copy, for mixed).
    async fn spawn(args: &LabArgs, variant: &Variant) -> anyhow::Result<Self> {
        let dir = &args.data_dir;
        let database_url = match &args.postgres {
            Some(url) => format!("{url}/{LAB_DB}"),
            None => format!("sqlite://{}?mode=rwc", dir.join("rustrak.db").display()),
        };
        let mut argv: Vec<String> = Vec::new();
        if !args.server_cpus.is_empty() {
            argv.extend(["taskset".into(), "-c".into(), args.server_cpus.clone()]);
        }
        let strace_out = args.strace.then(|| dir.join("strace.txt"));
        if let Some(out) = &strace_out {
            argv.extend(
                [
                    "strace",
                    "-f",
                    "--seccomp-bpf",
                    "-c",
                    "-e",
                    "trace=fsync,fdatasync",
                    "-o",
                ]
                .map(String::from),
            );
            argv.push(out.display().to_string());
        }
        argv.push(variant.bin.display().to_string());
        let log = std::fs::File::create(dir.join("server.log"))?;
        let mut command = Command::new(&argv[0]);
        command
            .args(&argv[1..])
            .current_dir(dir)
            .env("DATABASE_URL", database_url)
            .env("INGEST_DIR", dir.join("ingest"))
            .env("SOURCEMAP_STORAGE_PATH", dir.join("sourcemaps"))
            .env("HOST", "127.0.0.1")
            .env("PORT", PORT.to_string())
            .env("CREATE_SUPERUSER", format!("{EMAIL}:{PASSWORD}"))
            .env("SESSION_SECRET_KEY", "ab".repeat(32))
            .env("RUSTRAK_METRICS", "true")
            .env("RUSTRAK_TELEMETRY", "false")
            .env("RUSTRAK_DASHBOARD", "off")
            .env("RUST_LOG", "warn")
            .env("INGEST_MAX_PENDING_EVENTS", "10000000")
            .env("MAX_EVENTS_PER_MINUTE", "1000000000")
            .env("MAX_EVENTS_PER_HOUR", "1000000000")
            .env("MAX_EVENTS_PER_PROJECT_PER_MINUTE", "1000000000")
            .env("MAX_EVENTS_PER_PROJECT_PER_HOUR", "1000000000")
            .envs(variant.env.iter().map(|(k, v)| (k, v)))
            .stdout(log.try_clone()?)
            .stderr(log);
        let child = command.spawn().with_context(|| format!("spawn {argv:?}"))?;
        let started = Instant::now();
        let pid = if strace_out.is_some() {
            // The server is strace's child.
            let path = format!("/proc/{0}/task/{0}/children", child.id());
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                let children = std::fs::read_to_string(&path).unwrap_or_default();
                if let Some(pid) = children.split_whitespace().next() {
                    break pid.parse()?;
                }
                if Instant::now() > deadline {
                    bail!("strace never started the server");
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        } else {
            child.id()
        };
        let mut server = Self {
            child,
            pid,
            base: format!("http://127.0.0.1:{PORT}"),
            started,
            strace_out,
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()?,
            cookie: None,
        };
        server.wait_ready(dir).await?;
        Ok(server)
    }

    async fn wait_ready(&mut self, dir: &Path) -> anyhow::Result<()> {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            if let Some(status) = self.child.try_wait()? {
                let log = std::fs::read_to_string(dir.join("server.log")).unwrap_or_default();
                bail!("server exited ({status}):\n{log}");
            }
            let health = self.http.get(format!("{}/health", self.base)).send().await;
            if health.is_ok_and(|r| r.status().is_success()) {
                return Ok(());
            }
            if Instant::now() > deadline {
                bail!("server not ready in 60 s");
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// Seconds since the process started.
    pub fn t(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    async fn login(&mut self) -> anyhow::Result<String> {
        if let Some(cookie) = &self.cookie {
            return Ok(cookie.clone());
        }
        let response = self
            .http
            .post(format!("{}/auth/login", self.base))
            .json(&serde_json::json!({"email": EMAIL, "password": PASSWORD}))
            .send()
            .await?
            .error_for_status()
            .context("login")?;
        let cookie = response
            .headers()
            .get_all(reqwest::header::SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok()?.split(';').next().map(str::to_string))
            .collect::<Vec<_>>()
            .join("; ");
        self.cookie = Some(cookie.clone());
        Ok(cookie)
    }

    /// The project the load goes to: the first one, created when missing.
    pub async fn setup(&mut self) -> anyhow::Result<Credentials> {
        let cookie = self.login().await?;
        let list: serde_json::Value = self
            .http
            .get(format!("{}/api/projects", self.base))
            .header("Cookie", &cookie)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let project = match list["items"].as_array().and_then(|a| a.first()) {
            Some(p) => p.clone(),
            None => {
                self.http
                    .post(format!("{}/api/projects", self.base))
                    .header("Cookie", &cookie)
                    .json(&serde_json::json!({"name": "lab", "slug": "lab"}))
                    .send()
                    .await?
                    .error_for_status()
                    .context("create project")?
                    .json()
                    .await?
            }
        };
        Ok(Credentials {
            project_id: project["id"].as_i64().context("project id")?,
            key: project["sentry_key"]
                .as_str()
                .context("sentry_key")?
                .to_string(),
        })
    }

    pub async fn token(&mut self) -> anyhow::Result<String> {
        let cookie = self.login().await?;
        let body: serde_json::Value = self
            .http
            .post(format!("{}/api/tokens", self.base))
            .header("Cookie", cookie)
            .json(&serde_json::json!({"description": "lab readers"}))
            .send()
            .await?
            .error_for_status()
            .context("create token")?
            .json()
            .await?;
        Ok(body["token"].as_str().context("token")?.to_string())
    }

    pub async fn poll(&self) -> Option<Poll> {
        poll(&self.http, &self.base, self.started).await
    }

    /// Memory at 4 Hz and `/metrics` at 1 Hz until the task is aborted.
    pub fn sample_in_background(
        &self,
        samples: Arc<Mutex<Vec<Sample>>>,
        polls: Arc<Mutex<Vec<Poll>>>,
    ) -> tokio::task::JoinHandle<()> {
        let (pid, started, http, base) =
            (self.pid, self.started, self.http.clone(), self.base.clone());
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_millis(250));
            let mut n = 0u64;
            loop {
                n += 1;
                tick.tick().await;
                if let Some(s) = procfs::sample(pid, started.elapsed().as_secs_f64()) {
                    samples.lock().unwrap().push(s);
                }
                if n.is_multiple_of(4) {
                    if let Some(p) = poll(&http, &base, started).await {
                        polls.lock().unwrap().push(p);
                    }
                }
            }
        })
    }

    /// SIGTERM, wait, SIGKILL if it hangs. Returns the fsync count when
    /// strace was counting.
    pub async fn stop(mut self) -> Option<u64> {
        let _ = Command::new("kill")
            .args(["-TERM", &self.pid.to_string()])
            .status();
        let deadline = Instant::now() + Duration::from_secs(30);
        while self.child.try_wait().ok().flatten().is_none() {
            if Instant::now() > deadline {
                let _ = Command::new("kill")
                    .args(["-KILL", &self.pid.to_string()])
                    .status();
                let _ = self.child.kill();
                let _ = self.child.wait();
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let out = std::fs::read_to_string(self.strace_out.as_ref()?).ok()?;
        Some(fsync_calls(&out))
    }
}

/// Sum of the `calls` column for fsync/fdatasync in `strace -c` output.
fn fsync_calls(summary: &str) -> u64 {
    summary
        .lines()
        .filter_map(|line| {
            let cols: Vec<&str> = line.split_whitespace().collect();
            let name = *cols.last()?;
            if name != "fsync" && name != "fdatasync" {
                return None;
            }
            // % time, seconds, usecs/call, calls, [errors], syscall
            cols.get(3)?.parse::<u64>().ok()
        })
        .sum()
}

fn metric(text: &str, name: &str) -> u64 {
    text.lines()
        .find_map(|l| l.strip_prefix(name)?.trim().parse::<f64>().ok())
        .unwrap_or(0.0) as u64
}

async fn poll(http: &reqwest::Client, base: &str, started: Instant) -> Option<Poll> {
    let text = http
        .get(format!("{base}/metrics"))
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;
    Some(Poll {
        t: started.elapsed().as_secs_f64(),
        digested: metric(&text, "rustrak_digest_total{result=\"ok\"}"),
        dropped: metric(&text, "rustrak_digest_total{result=\"rate_limited\"}")
            + metric(&text, "rustrak_digest_total{result=\"failed\"}"),
        pending: metric(&text, "rustrak_spool_pending "),
    })
}

/// For `mixed`: the variant's seeded database copied into the data dir,
/// built first when the seed directory has none (one per schema, so the
/// script keys the directory by the migration list).
pub async fn start_seeded(
    args: &LabArgs,
    variant: &Variant,
    template: Arc<Template>,
) -> anyhow::Result<Server> {
    let seed = variant
        .seed
        .clone()
        .with_context(|| format!("mixed needs --seed {}=/dir", variant.name))?;
    let seeded = seed.join("rustrak.db");
    if !seeded.exists() {
        println!(
            "  seeding {} with {} events",
            seed.display(),
            args.seed_events
        );
        let mut server = Server::start(args, variant).await?;
        let creds = server.setup().await?;
        let target = Target::new(&server.base, creds.project_id, &creds.key, args.connections);
        let stats = load::closed_loop(
            &target,
            template,
            args.connections,
            Duration::from_secs(3600),
            Some(args.seed_events),
        )
        .await;
        let polls = Mutex::new(Vec::new());
        super::wait_drained(&server, &polls, stats.ok(), Duration::from_secs(1800)).await;
        server.stop().await;
        std::fs::create_dir_all(&seed)?;
        for suffix in ["", "-wal", "-shm"] {
            let from = args.data_dir.join(format!("rustrak.db{suffix}"));
            if from.exists() {
                std::fs::copy(&from, seed.join(format!("rustrak.db{suffix}")))?;
            }
        }
    }
    wipe(&args.data_dir)?;
    for suffix in ["", "-wal", "-shm"] {
        let from = seed.join(format!("rustrak.db{suffix}"));
        if from.exists() {
            let to = args.data_dir.join(format!("rustrak.db{suffix}"));
            std::fs::copy(&from, &to)?;
            // On disk before the run, so its writeback is not counted in it.
            std::fs::File::open(&to)?.sync_all()?;
        }
    }
    Server::spawn(args, variant).await
}

pub struct ReadStats {
    pub rps: f64,
    pub p50_ms: f64,
    pub p99_ms: f64,
}

/// `readers` workers cycling over the dashboard's issue and event queries.
pub async fn readers(
    base: &str,
    project_id: i64,
    token: &str,
    readers: usize,
    duration: Duration,
) -> anyhow::Result<ReadStats> {
    let http = reqwest::Client::new();
    let api = format!("{base}/api/projects/{project_id}");
    let auth = format!("Bearer {token}");
    let list: serde_json::Value = http
        .get(format!("{api}/issues?limit=1&sort=event_count&order=desc"))
        .header("Authorization", &auth)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let issue = list["items"][0]["id"]
        .to_string()
        .trim_matches('"')
        .to_string();
    let urls = Arc::new(vec![
        format!("{api}/issues?limit=50&sort=last_seen&order=desc"),
        format!("{api}/issues?limit=50&sort=event_count&order=desc"),
        format!("{api}/issues/{issue}"),
        format!("{api}/issues/{issue}/events?limit=50"),
    ]);
    let started = Instant::now();
    let workers: Vec<_> = (0..readers)
        .map(|w| {
            let (http, urls, auth) = (http.clone(), urls.clone(), auth.clone());
            tokio::spawn(async move {
                let mut h = Histogram::<u64>::new_with_bounds(1, 60_000_000, 3).expect("histogram");
                let mut i = w;
                while started.elapsed() < duration {
                    let t0 = Instant::now();
                    let ok = http
                        .get(&urls[i % urls.len()])
                        .header("Authorization", &auth)
                        .send()
                        .await
                        .is_ok_and(|r| r.status().is_success());
                    if ok {
                        h.saturating_record(t0.elapsed().as_micros() as u64);
                    }
                    i += 1;
                }
                h
            })
        })
        .collect();
    let mut total = Histogram::<u64>::new_with_bounds(1, 60_000_000, 3).expect("histogram");
    for w in workers {
        total.add(&w.await?)?;
    }
    Ok(ReadStats {
        rps: total.len() as f64 / started.elapsed().as_secs_f64(),
        p50_ms: total.value_at_quantile(0.5) as f64 / 1000.0,
        p99_ms: total.value_at_quantile(0.99) as f64 / 1000.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_fsync_and_fdatasync_calls() {
        let out = "% time     seconds  usecs/call     calls    errors syscall\n\
                   ------ ----------- ----------- --------- --------- ----------------\n\
                    90.00    1.000000          50     20000           fsync\n\
                    10.00    0.100000          10       500         3 fdatasync\n\
                   ------ ----------- ----------- --------- --------- ----------------\n\
                   100.00    1.100000          53     20500         3 total\n";
        assert_eq!(fsync_calls(out), 20_500);
    }

    #[test]
    fn reads_counters_from_the_exposition() {
        let text = "rustrak_digest_total{result=\"ok\"} 1234\nrustrak_spool_pending 7\nrustrak_spool_pending_x 9\n";
        assert_eq!(metric(text, "rustrak_digest_total{result=\"ok\"}"), 1234);
        assert_eq!(metric(text, "rustrak_spool_pending "), 7);
        assert_eq!(metric(text, "rustrak_digest_total{result=\"failed\"}"), 0);
    }
}
