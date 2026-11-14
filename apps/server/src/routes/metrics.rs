//! Opt-in, unauthenticated Prometheus scrape. Keep it behind a private network
//! or a proxy allowlist: when enabled, anyone who can reach this port can read it.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use actix_web::{http::header, web, HttpResponse};

use crate::db::DbPool;
use crate::digest::processors::Processors;
use crate::telemetry::metrics::Spool;
use crate::telemetry::Counters;

/// How long a database size reading is reused.
const DB_SIZE_TTL: Duration = Duration::from_secs(15);

/// How often [`MetricsEndpoint::sample_spool`] walks the ingest directory.
/// The walk stats every file, so under a backlog of a million files it is
/// seconds of I/O: it runs on a timer, never on a scrape.
pub const SPOOL_SAMPLE_INTERVAL: Duration = Duration::from_secs(60);

/// The spool's bytes and the age of its oldest file, at the last walk.
#[derive(Clone, Copy)]
struct SpoolSample {
    bytes: u64,
    oldest_seconds: u64,
}

pub struct MetricsEndpoint {
    enabled: bool,
    ingest_dir: PathBuf,
    counters: &'static Counters,
    spool: Mutex<Option<SpoolSample>>,
    db_bytes: tokio::sync::Mutex<Option<(Instant, Option<u64>)>>,
}

impl MetricsEndpoint {
    pub fn new(enabled: bool, ingest_dir: PathBuf, counters: &'static Counters) -> Self {
        Self {
            enabled,
            ingest_dir,
            counters,
            spool: Mutex::new(None),
            db_bytes: tokio::sync::Mutex::new(None),
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Walks the ingest directory once for the size gauges. An unreadable
    /// directory drops them rather than reporting a stale or zero reading.
    pub async fn sample_spool(&self) {
        let dir = self.ingest_dir.clone();
        let sample = match tokio::task::spawn_blocking(move || pending_spool(&dir)).await {
            Ok(Ok(sample)) => Some(sample),
            Ok(Err(e)) => {
                log::warn!("metrics: could not read the ingest directory: {e}");
                None
            }
            Err(_) => None,
        };
        *self.spool.lock().unwrap_or_else(|e| e.into_inner()) = sample;
    }
}

pub async fn scrape(
    state: web::Data<MetricsEndpoint>,
    pool: web::Data<DbPool>,
    processors: web::Data<Processors>,
) -> HttpResponse {
    if !state.enabled {
        return HttpResponse::NotFound().finish();
    }

    let sample = *state.spool.lock().unwrap_or_else(|e| e.into_inner());
    let spool = Spool {
        // The pending set is the backlog, kept in memory: O(1) however many
        // files the spool holds.
        pending: processors.errors.pending().counts(0).installation as u64,
        digest_halted: processors.errors.is_halted(),
        bytes: sample.map(|s| s.bytes),
        oldest_seconds: sample.map(|s| s.oldest_seconds),
    };

    // Held while querying, so overlapping scrapes wait for one reading.
    let mut db = state.db_bytes.lock().await;
    let db_bytes = match *db {
        Some((read_at, db_bytes)) if read_at.elapsed() < DB_SIZE_TTL => db_bytes,
        _ => {
            // A failed size query must not be reported as a zero-byte database.
            let db_bytes = crate::db::maintenance::size_bytes(pool.get_ref())
                .await
                .inspect_err(|e| log::warn!("metrics: could not read the database size: {e}"))
                .ok()
                .and_then(|n| u64::try_from(n).ok());
            *db = Some((Instant::now(), db_bytes));
            db_bytes
        }
    };
    drop(db);

    HttpResponse::Ok()
        .insert_header((header::CACHE_CONTROL, "no-store"))
        .content_type("text/plain; version=0.0.4; charset=utf-8")
        .body(state.counters.render(spool, db_bytes))
}

/// Only pending records belong to the digest backlog, not other files in the
/// ingest directory nor `dead/`. A vanished entry during a walk is a normal race.
fn pending_spool(dir: &Path) -> std::io::Result<SpoolSample> {
    let mut spool = SpoolSample {
        bytes: 0,
        oldest_seconds: 0,
    };
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(spool),
        Err(e) => return Err(e),
    };
    let now = SystemTime::now();
    for entry in entries {
        let entry = entry?;
        if !entry
            .file_name()
            .to_string_lossy()
            .ends_with(".pending.json")
        {
            continue;
        }
        let metadata = match entry.metadata() {
            Ok(metadata) if metadata.is_file() => metadata,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Ok(_) => continue,
            Err(e) => return Err(e),
        };
        spool.bytes += metadata.len();
        if let Ok(age) = metadata
            .modified()
            .and_then(|mtime| now.duration_since(mtime).map_err(std::io::Error::other))
        {
            spool.oldest_seconds = spool.oldest_seconds.max(age.as_secs());
        }
    }
    Ok(spool)
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/metrics", web::get().to(scrape));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_are_opt_in() {
        assert!(!crate::config::Config::default().metrics_enabled);
    }

    #[test]
    fn only_pending_records_count_toward_the_spool() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("project-1-a.pending.json"), "event").unwrap();
        std::fs::write(dir.path().join("project-1-b.json"), "already digested").unwrap();
        std::fs::create_dir(dir.path().join("dead")).unwrap();
        std::fs::write(dir.path().join("dead/project-1-c.pending.json"), "dead").unwrap();
        let spool = pending_spool(dir.path()).unwrap();
        assert_eq!(spool.bytes, 5);
    }

    #[test]
    fn a_missing_directory_is_an_empty_spool() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("ingest");
        std::fs::create_dir(&dir).unwrap();
        std::fs::remove_dir(&dir).unwrap();

        let spool = pending_spool(&dir).unwrap();
        assert_eq!(spool.bytes, 0);
        assert_eq!(spool.oldest_seconds, 0);
        assert!(!dir.exists());
    }

    #[test]
    fn other_directory_errors_are_not_an_empty_spool() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("ingest");
        std::fs::write(&file, "not a directory").unwrap();

        assert_eq!(
            pending_spool(&file).err().unwrap().kind(),
            std::io::ErrorKind::NotADirectory,
        );
    }
}
