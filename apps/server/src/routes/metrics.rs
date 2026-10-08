//! Opt-in, unauthenticated Prometheus scrape. Keep it behind a private network
//! or a proxy allowlist: when enabled, anyone who can reach this port can read it.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use actix_web::{http::header, web, HttpResponse};

use crate::db::DbPool;
use crate::services::StorageService;
use crate::telemetry::metrics::Spool;
use crate::telemetry::Counters;

/// How long a spool and database reading is reused. Collecting them scans the
/// ingest directory, which is largest exactly when digest is behind.
const GAUGE_TTL: Duration = Duration::from_secs(15);

/// When the gauges were read, the spool and the database size.
type Reading = (Instant, Option<Spool>, Option<u64>);

pub struct MetricsEndpoint {
    enabled: bool,
    ingest_dir: PathBuf,
    counters: &'static Counters,
    gauges: tokio::sync::Mutex<Option<Reading>>,
}

impl MetricsEndpoint {
    pub fn new(enabled: bool, ingest_dir: PathBuf, counters: &'static Counters) -> Self {
        Self {
            enabled,
            ingest_dir,
            counters,
            gauges: tokio::sync::Mutex::new(None),
        }
    }
}

/// Off unless explicitly enabled; a typo must not silently change exposure.
pub fn enabled(value: Option<&str>) -> Result<bool, &'static str> {
    match value {
        None | Some("off") => Ok(false),
        Some("on") => Ok(true),
        _ => Err("RUSTRAK_METRICS must be 'on' or 'off'"),
    }
}

pub async fn scrape(state: web::Data<MetricsEndpoint>, pool: web::Data<DbPool>) -> HttpResponse {
    if !state.enabled {
        return HttpResponse::NotFound().finish();
    }

    // Held while collecting, so overlapping scrapes wait for one reading
    // instead of each scanning the spool.
    let mut gauges = state.gauges.lock().await;
    let (spool, db_bytes) = match *gauges {
        Some((read_at, spool, db_bytes)) if read_at.elapsed() < GAUGE_TTL => (spool, db_bytes),
        _ => {
            let dir = state.ingest_dir.clone();
            // An unreadable spool drops its gauges, not the whole scrape.
            let spool = match tokio::task::spawn_blocking(move || pending_spool(&dir)).await {
                Ok(Ok(spool)) => Some(spool),
                Ok(Err(e)) => {
                    log::warn!("metrics: could not read the ingest directory: {e}");
                    None
                }
                Err(_) => None,
            };
            // A failed size query must not be reported as a zero-byte database.
            let db_bytes = StorageService::db_size_bytes(pool.get_ref())
                .await
                .ok()
                .and_then(|n| u64::try_from(n).ok());
            *gauges = Some((Instant::now(), spool, db_bytes));
            (spool, db_bytes)
        }
    };
    drop(gauges);

    HttpResponse::Ok()
        .insert_header((header::CACHE_CONTROL, "no-store"))
        .content_type("text/plain; version=0.0.4; charset=utf-8")
        .body(state.counters.metrics().render(spool, db_bytes))
}

/// Only pending records belong to the digest backlog, not other files in the
/// ingest directory. A vanished entry during a scrape is a normal race.
fn pending_spool(dir: &Path) -> std::io::Result<Spool> {
    let mut spool = Spool::default();
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
        spool.pending += 1;
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
        assert_eq!(enabled(None), Ok(false));
        assert_eq!(enabled(Some("off")), Ok(false));
        assert_eq!(enabled(Some("on")), Ok(true));
        assert!(enabled(Some("true")).is_err());
    }

    #[test]
    fn only_pending_records_count_toward_the_spool() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("project-1-a.pending.json"), "event").unwrap();
        std::fs::write(dir.path().join("project-1-b.json"), "already digested").unwrap();
        let spool = pending_spool(dir.path()).unwrap();
        assert_eq!(spool.pending, 1);
        assert_eq!(spool.bytes, 5);
    }

    #[test]
    fn a_missing_directory_is_an_empty_spool() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("ingest");
        std::fs::create_dir(&dir).unwrap();
        std::fs::remove_dir(&dir).unwrap();

        let spool = pending_spool(&dir).unwrap();
        assert_eq!(spool.pending, 0);
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
