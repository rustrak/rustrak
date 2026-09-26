//! Process-wide health counters. Every hook on the request path is one
//! relaxed atomic increment; the reporter drains them once per window.

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

/// Upper bounds, in milliseconds, of the latency buckets. Anything slower
/// than the last one lands in it.
const LATENCY_BOUNDS_MS: [u64; 13] = [1, 2, 5, 10, 20, 50, 100, 200, 500, 1000, 2000, 5000, 10000];

pub struct Counters {
    ingest_accepted: AtomicU64,
    rejected: [AtomicU64; 5],
    latency: [AtomicU64; LATENCY_BOUNDS_MS.len()],
    digest_ok: AtomicU64,
    digest_failed: AtomicU64,
    digest_rate_limited: AtomicU64,
    http_5xx_by_route: Mutex<BTreeMap<String, u64>>,
    alerts_failed_by_provider: Mutex<BTreeMap<String, u64>>,
    panics: Mutex<BTreeMap<String, u64>>,
}

impl Default for Counters {
    fn default() -> Self {
        Self::new()
    }
}

impl Counters {
    pub const fn new() -> Self {
        Self {
            ingest_accepted: AtomicU64::new(0),
            rejected: [const { AtomicU64::new(0) }; 5],
            latency: [const { AtomicU64::new(0) }; LATENCY_BOUNDS_MS.len()],
            digest_ok: AtomicU64::new(0),
            digest_failed: AtomicU64::new(0),
            digest_rate_limited: AtomicU64::new(0),
            http_5xx_by_route: Mutex::new(BTreeMap::new()),
            alerts_failed_by_provider: Mutex::new(BTreeMap::new()),
            panics: Mutex::new(BTreeMap::new()),
        }
    }

    /// The one set the whole process bumps.
    pub fn global() -> &'static Counters {
        static GLOBAL: OnceLock<Counters> = OnceLock::new();
        GLOBAL.get_or_init(Counters::new)
    }

    pub fn ingest_accepted(&self, latency: Duration) {
        self.ingest_accepted.fetch_add(1, Relaxed);
        let ms = latency.as_millis() as u64;
        let bucket = LATENCY_BOUNDS_MS
            .iter()
            .position(|bound| ms <= *bound)
            .unwrap_or(LATENCY_BOUNDS_MS.len() - 1);
        self.latency[bucket].fetch_add(1, Relaxed);
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

    /// Reads everything and starts the next window from zero.
    pub fn snapshot_and_reset(&self) -> Health {
        self.read(|c| c.swap(0, Relaxed), std::mem::take)
    }

    /// Reads everything and leaves the window as it is.
    pub fn snapshot(&self) -> Health {
        self.read(|c| c.load(Relaxed), |m| m.clone())
    }

    fn read(
        &self,
        counter: impl Fn(&AtomicU64) -> u64,
        map: impl Fn(&mut BTreeMap<String, u64>) -> BTreeMap<String, u64>,
    ) -> Health {
        let latency: Vec<u64> = self.latency.iter().map(&counter).collect();
        let rejected = |r: Rejection| counter(&self.rejected[r as usize]);
        let take = |m: &Mutex<BTreeMap<String, u64>>| {
            map(&mut m.lock().unwrap_or_else(|e| e.into_inner()))
        };
        Health {
            ingest: Ingest {
                accepted: counter(&self.ingest_accepted),
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
                ok: counter(&self.digest_ok),
                failed: counter(&self.digest_failed),
                rate_limited: counter(&self.digest_rate_limited),
            },
            http_5xx_by_route: take(&self.http_5xx_by_route),
            alerts_failed_by_provider: take(&self.alerts_failed_by_provider),
            panics: take(&self.panics),
        }
    }
}

fn bump(map: &Mutex<BTreeMap<String, u64>>, key: &str) {
    let mut map = map.lock().unwrap_or_else(|e| e.into_inner());
    *map.entry(key.to_string()).or_insert(0) += 1;
}

/// The upper bound of the bucket the `q`-th sample falls in.
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
            return Some(LATENCY_BOUNDS_MS[i]);
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
