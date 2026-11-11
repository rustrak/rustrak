//! Process-wide health counters. Every hook on the request path is one
//! relaxed atomic increment. The counters only grow: Prometheus reads them as
//! they are (`metrics.rs`), and the anonymous telemetry reads the difference
//! since its last report.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde::Serialize;

/// Why an ingest request was turned away. Reasons only, never the detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejection {
    RateLimit,
    Auth,
    TooLarge,
    Malformed,
    Other,
}

impl Rejection {
    pub(crate) const ALL: [(Rejection, &'static str); 5] = [
        (Rejection::RateLimit, "rate_limit"),
        (Rejection::Auth, "auth"),
        (Rejection::TooLarge, "too_large"),
        (Rejection::Malformed, "malformed"),
        (Rejection::Other, "other"),
    ];
}

/// Upper bounds, in milliseconds, of the latency buckets. One more bucket
/// past the last holds everything slower.
pub(crate) const LATENCY_BOUNDS_MS: [u64; 13] =
    [1, 2, 5, 10, 20, 50, 100, 200, 500, 1000, 2000, 5000, 10000];
pub(crate) const LATENCY_BUCKETS: usize = LATENCY_BOUNDS_MS.len() + 1;

/// Every counter's value at one moment.
#[derive(Clone)]
pub(crate) struct Totals {
    pub accepted: u64,
    pub rejected: [u64; 5],
    pub latency: [u64; LATENCY_BUCKETS],
    pub latency_us: u64,
    pub digest_ok: u64,
    pub digest_failed: u64,
    pub digest_rate_limited: u64,
    pub http_5xx_by_route: BTreeMap<String, u64>,
    pub alerts_failed_by_provider: BTreeMap<String, u64>,
    pub panics: BTreeMap<String, u64>,
    pub db_errors_by_code: BTreeMap<String, u64>,
}

impl Totals {
    const ZERO: Totals = Totals {
        accepted: 0,
        rejected: [0; 5],
        latency: [0; LATENCY_BUCKETS],
        latency_us: 0,
        digest_ok: 0,
        digest_failed: 0,
        digest_rate_limited: 0,
        http_5xx_by_route: BTreeMap::new(),
        alerts_failed_by_provider: BTreeMap::new(),
        panics: BTreeMap::new(),
        db_errors_by_code: BTreeMap::new(),
    };
}

pub struct Counters {
    accepted: AtomicU64,
    rejected: [AtomicU64; 5],
    latency: [AtomicU64; LATENCY_BUCKETS],
    latency_us: AtomicU64,
    digest_ok: AtomicU64,
    digest_failed: AtomicU64,
    digest_rate_limited: AtomicU64,
    http_5xx_by_route: Mutex<BTreeMap<String, u64>>,
    alerts_failed_by_provider: Mutex<BTreeMap<String, u64>>,
    panics: Mutex<BTreeMap<String, u64>>,
    db_errors_by_code: Mutex<BTreeMap<String, u64>>,
    /// What the last telemetry window ended at; the next one starts there.
    reported: Mutex<Totals>,
}

impl Default for Counters {
    fn default() -> Self {
        Self::new()
    }
}

impl Counters {
    pub const fn new() -> Self {
        Self {
            accepted: AtomicU64::new(0),
            rejected: [const { AtomicU64::new(0) }; 5],
            latency: [const { AtomicU64::new(0) }; LATENCY_BUCKETS],
            latency_us: AtomicU64::new(0),
            digest_ok: AtomicU64::new(0),
            digest_failed: AtomicU64::new(0),
            digest_rate_limited: AtomicU64::new(0),
            http_5xx_by_route: Mutex::new(BTreeMap::new()),
            alerts_failed_by_provider: Mutex::new(BTreeMap::new()),
            panics: Mutex::new(BTreeMap::new()),
            db_errors_by_code: Mutex::new(BTreeMap::new()),
            reported: Mutex::new(Totals::ZERO),
        }
    }

    /// The one set the whole process bumps.
    pub fn global() -> &'static Counters {
        static GLOBAL: OnceLock<Counters> = OnceLock::new();
        GLOBAL.get_or_init(Counters::new)
    }

    pub fn ingest_accepted(&self, latency: Duration) {
        self.accepted.fetch_add(1, Relaxed);
        let bucket = LATENCY_BOUNDS_MS
            .iter()
            .position(|bound| latency <= Duration::from_millis(*bound))
            .unwrap_or(LATENCY_BOUNDS_MS.len());
        self.latency[bucket].fetch_add(1, Relaxed);
        self.latency_us.fetch_add(
            u64::try_from(latency.as_micros()).unwrap_or(u64::MAX),
            Relaxed,
        );
    }

    pub fn ingest_rejected(&self, reason: Rejection) {
        self.rejected[reason as usize].fetch_add(1, Relaxed);
    }

    pub fn digest_ok(&self) {
        self.digest_ok.fetch_add(1, Relaxed);
    }

    pub fn digest_failed(&self) {
        self.digest_failed.fetch_add(1, Relaxed);
    }

    /// An accepted event the quota dropped at digest.
    pub fn digest_rate_limited(&self) {
        self.digest_rate_limited.fetch_add(1, Relaxed);
    }

    /// `route` is the matched pattern (`/api/issues/{id}`), never the path.
    pub fn http_5xx(&self, route: &str) {
        bump(&self.http_5xx_by_route, route);
    }

    pub fn alert_failed(&self, provider: &str) {
        bump(&self.alerts_failed_by_provider, provider);
    }

    /// `location` is `file:line` inside our own tree; the message never comes here.
    pub fn panic(&self, location: &str) {
        bump(&self.panics, location);
    }

    /// A database error that failed a request or a digest, by its code alone:
    /// a SQLSTATE on Postgres (`22001`), an extended result code on SQLite.
    pub fn db_error(&self, code: &str) {
        bump(&self.db_errors_by_code, code);
    }

    /// Every counter as it stands.
    pub(crate) fn totals(&self) -> Totals {
        let load = |c: &AtomicU64| c.load(Relaxed);
        let map =
            |m: &Mutex<BTreeMap<String, u64>>| m.lock().unwrap_or_else(|e| e.into_inner()).clone();
        Totals {
            accepted: load(&self.accepted),
            rejected: self.rejected.each_ref().map(load),
            latency: self.latency.each_ref().map(load),
            latency_us: load(&self.latency_us),
            digest_ok: load(&self.digest_ok),
            digest_failed: load(&self.digest_failed),
            digest_rate_limited: load(&self.digest_rate_limited),
            http_5xx_by_route: map(&self.http_5xx_by_route),
            alerts_failed_by_provider: map(&self.alerts_failed_by_provider),
            panics: map(&self.panics),
            db_errors_by_code: map(&self.db_errors_by_code),
        }
    }

    /// The window since the last report, which starts the next one.
    pub fn snapshot_and_reset(&self) -> Health {
        let mut reported = self.reported.lock().unwrap_or_else(|e| e.into_inner());
        let now = self.totals();
        let window = window(&now, &reported);
        *reported = now;
        window
    }

    /// The window since the last report, leaving it open.
    pub fn snapshot(&self) -> Health {
        let reported = self.reported.lock().unwrap_or_else(|e| e.into_inner());
        window(&self.totals(), &reported)
    }
}

/// What happened between `since` and `now`.
fn window(now: &Totals, since: &Totals) -> Health {
    let latency: Vec<u64> = (0..LATENCY_BUCKETS)
        .map(|i| now.latency[i] - since.latency[i])
        .collect();
    let rejected = |r: Rejection| now.rejected[r as usize] - since.rejected[r as usize];
    let map = |now: &BTreeMap<String, u64>, since: &BTreeMap<String, u64>| {
        now.iter()
            .map(|(k, v)| (k.clone(), v - since.get(k).copied().unwrap_or(0)))
            .filter(|(_, v)| *v > 0)
            .collect()
    };
    Health {
        ingest: Ingest {
            accepted: now.accepted - since.accepted,
            rejected: Rejected {
                rate_limit: rejected(Rejection::RateLimit),
                auth: rejected(Rejection::Auth),
                too_large: rejected(Rejection::TooLarge),
                malformed: rejected(Rejection::Malformed),
                other: rejected(Rejection::Other),
            },
            latency_ms: Latency {
                p50: percentile(&latency, 0.50),
                p99: percentile(&latency, 0.99),
            },
        },
        digest: Digest {
            ok: now.digest_ok - since.digest_ok,
            failed: now.digest_failed - since.digest_failed,
            rate_limited: now.digest_rate_limited - since.digest_rate_limited,
        },
        http_5xx_by_route: map(&now.http_5xx_by_route, &since.http_5xx_by_route),
        alerts_failed_by_provider: map(
            &now.alerts_failed_by_provider,
            &since.alerts_failed_by_provider,
        ),
        panics: map(&now.panics, &since.panics),
        db_errors_by_code: map(&now.db_errors_by_code, &since.db_errors_by_code),
    }
}

fn bump(map: &Mutex<BTreeMap<String, u64>>, key: &str) {
    let mut map = map.lock().unwrap_or_else(|e| e.into_inner());
    *map.entry(key.to_string()).or_insert(0) += 1;
}

/// The upper bound of the bucket the `q`-th sample falls in; the bucket past
/// the last bound reports that bound.
fn percentile(buckets: &[u64], q: f64) -> Option<u64> {
    let total: u64 = buckets.iter().sum();
    if total == 0 {
        return None;
    }
    let rank = ((total as f64) * q).ceil().max(1.0) as u64;
    let mut seen = 0;
    for (i, count) in buckets.iter().enumerate() {
        seen += count;
        if seen >= rank {
            return Some(LATENCY_BOUNDS_MS[i.min(LATENCY_BOUNDS_MS.len() - 1)]);
        }
    }
    Some(LATENCY_BOUNDS_MS[LATENCY_BOUNDS_MS.len() - 1])
}

/// One window of health counters, as it goes on the wire.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Health {
    pub ingest: Ingest,
    pub digest: Digest,
    pub http_5xx_by_route: BTreeMap<String, u64>,
    pub alerts_failed_by_provider: BTreeMap<String, u64>,
    pub panics: BTreeMap<String, u64>,
    pub db_errors_by_code: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Ingest {
    pub accepted: u64,
    pub rejected: Rejected,
    pub latency_ms: Latency,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Rejected {
    pub rate_limit: u64,
    pub auth: u64,
    pub too_large: u64,
    pub malformed: u64,
    pub other: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Latency {
    pub p50: Option<u64>,
    pub p99: Option<u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Digest {
    pub ok: u64,
    pub failed: u64,
    /// Accepted events the quota dropped instead of storing.
    pub rate_limited: u64,
}
