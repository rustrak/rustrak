//! The load generator: realistic Sentry error envelopes built once as a byte
//! template and patched per request (event id, group, release, environment),
//! so generating a request costs one memcpy and the server is the bottleneck.

use hdrhistogram::Histogram;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const ID_MARK: &str = "ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ"; // 32, an event id
const GROUP_MARK: &str = "~GGGGGG~"; // 8, "g0001234"
const RELEASE_MARK: &str = "~R~"; // 3, "007"
const ENV_MARK: &str = "~ENV~~~~~~"; // 10
const ENVIRONMENTS: [&str; 2] = ["production", "staging-eu"];
const RELEASES: u64 = 10;
/// 30% of the events go to the first 10 groups.
const HOT_GROUPS: u64 = 10;
const HOT_SHARE: f64 = 0.3;

/// An envelope with every per-event field at a known offset.
pub struct Template {
    bytes: Vec<u8>,
    ids: Vec<usize>,
    group: Vec<usize>,
    release: Vec<usize>,
    env: Vec<usize>,
    groups: u64,
}

fn offsets(haystack: &[u8], needle: &str) -> Vec<usize> {
    let needle = needle.as_bytes();
    haystack
        .windows(needle.len())
        .enumerate()
        .filter(|(_, w)| *w == needle)
        .map(|(i, _)| i)
        .collect()
}

/// A Python-SDK-shaped error event: 26 frames with context, 30 breadcrumbs,
/// tags, user, contexts and request, padded with `extra` up to `size` bytes.
fn event_json(size: usize) -> String {
    let frames: Vec<_> = (0..26)
        .map(|i| {
            serde_json::json!({
                "filename": format!("app/services/module_{i}.py"),
                "abs_path": format!("/srv/app/app/services/module_{i}.py"),
                "module": format!("app.services.module_{i}"),
                "function": format!("handle_step_{i}"),
                "lineno": 40 + i,
                "in_app": i % 3 != 0,
                "pre_context": ["    try:", "        value = compute(item)", "        result.append(value)"],
                "context_line": "        raise ValueError(f\"bad item {item!r}\")",
                "post_context": ["    except KeyError:", "        return None", ""],
                "vars": {"item": "{'id': 12345, 'kind': 'order'}", "result": "[]", "self": "<Handler object at 0x7f>"}
            })
        })
        .collect();
    let breadcrumbs: Vec<_> = (0..30)
        .map(|i| {
            serde_json::json!({
                "timestamp": 1_700_000_000.0 + i as f64,
                "type": if i % 2 == 0 { "http" } else { "default" },
                "category": if i % 2 == 0 { "httplib" } else { "query" },
                "message": format!("SELECT * FROM orders WHERE id = {i} AND status = 'pending'"),
                "data": {"url": format!("https://api.internal/v1/items/{i}"), "method": "GET", "status_code": 200},
                "level": "info"
            })
        })
        .collect();
    let mut event = serde_json::json!({
        "event_id": ID_MARK,
        "timestamp": 1_700_000_000.0,
        "platform": "python",
        "level": "error",
        "logger": "app.services",
        "server_name": "web-1",
        "release": format!("backend@1.4.{RELEASE_MARK}"),
        "environment": ENV_MARK,
        "transaction": "/api/orders/{order_id}",
        "exception": {"values": [{
            "type": "ValueError",
            "value": format!("bad item in {GROUP_MARK}"),
            "module": "builtins",
            "mechanism": {"type": "generic", "handled": false},
            "stacktrace": {"frames": frames}
        }]},
        "breadcrumbs": {"values": breadcrumbs},
        "tags": {"region": "eu-west-1", "customer_tier": "pro", "feature_flag": "new-checkout", "browser": "Chrome 120"},
        "user": {"id": "42", "email": "user42@example.com", "username": "user42", "ip_address": "10.0.0.42"},
        "contexts": {
            "os": {"name": "Linux", "version": "6.8"},
            "runtime": {"name": "CPython", "version": "3.12.1"},
            "trace": {"trace_id": "4c79f60c11214eb38604f4ae0781bfb2", "span_id": "fa90fdead5f74052"}
        },
        "request": {"url": "https://shop.example.com/api/orders/1", "method": "POST",
                    "headers": {"User-Agent": "Mozilla/5.0", "Accept": "application/json"}},
        "sdk": {"name": "sentry.python", "version": "2.19.0"},
        "extra": {}
    });
    let len = serde_json::to_string(&event).map(|s| s.len()).unwrap_or(0);
    if len + 20 < size {
        event["extra"]["padding"] = "x".repeat(size - len - 20).into();
    }
    serde_json::to_string(&event).expect("event serializes")
}

impl Template {
    /// An envelope holding one error event of about `size` bytes and, for
    /// the large scenario, `attachments` items of `attachment_size` bytes.
    pub fn new(size: usize, attachments: usize, attachment_size: usize, groups: u32) -> Self {
        let event = event_json(size);
        let mut bytes = format!(
            "{{\"event_id\":\"{ID_MARK}\",\"sent_at\":\"2024-01-01T00:00:00Z\"}}\n{{\"type\":\"event\",\"length\":{}}}\n{event}\n",
            event.len()
        )
        .into_bytes();
        for i in 0..attachments {
            bytes.extend_from_slice(
                format!("{{\"type\":\"attachment\",\"length\":{attachment_size},\"filename\":\"blob-{i}.bin\",\"content_type\":\"application/octet-stream\"}}\n").as_bytes(),
            );
            bytes.extend((0..attachment_size).map(|j| (j % 251) as u8));
            bytes.push(b'\n');
        }
        let head = bytes.len().min(event.len() + 4096);
        Self {
            ids: offsets(&bytes[..head], ID_MARK),
            group: offsets(&bytes[..head], GROUP_MARK),
            release: offsets(&bytes[..head], RELEASE_MARK),
            env: offsets(&bytes[..head], ENV_MARK),
            bytes,
            groups: groups.into(),
        }
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// The `n`th request body: a fresh event id, a group (hot or uniform),
    /// a release and an environment, all from `rng`.
    pub fn render(&self, rng: &mut u64) -> Vec<u8> {
        let mut body = self.bytes.clone();
        let id = format!("{:016x}{:016x}", next(rng), next(rng));
        for &at in &self.ids {
            body[at..at + 32].copy_from_slice(id.as_bytes());
        }
        let group = if (next(rng) % 10_000) as f64 / 10_000.0 < HOT_SHARE {
            next(rng) % HOT_GROUPS
        } else {
            HOT_GROUPS + next(rng) % self.groups.saturating_sub(HOT_GROUPS).max(1)
        };
        let group = format!("g{group:07}");
        for &at in &self.group {
            body[at..at + 8].copy_from_slice(group.as_bytes());
        }
        let release = format!("{:03}", next(rng) % RELEASES);
        for &at in &self.release {
            body[at..at + 3].copy_from_slice(release.as_bytes());
        }
        let env = ENVIRONMENTS[(next(rng) % 2) as usize];
        for &at in &self.env {
            body[at..at + 10].copy_from_slice(env.as_bytes());
        }
        body
    }
}

/// xorshift64*: per-task, no locking, no dependency on `rand`'s API.
fn next(state: &mut u64) -> u64 {
    *state ^= *state >> 12;
    *state ^= *state << 25;
    *state ^= *state >> 27;
    state.wrapping_mul(0x2545_F491_4F6C_DD1D)
}

/// What one load phase saw from the client side.
pub struct LoadStats {
    pub elapsed: Duration,
    pub latency_us: Histogram<u64>,
    pub statuses: BTreeMap<u16, u64>,
    /// Connection errors and timeouts.
    pub errors: u64,
    /// Requests over 200 ms.
    pub slow: u64,
}

impl LoadStats {
    fn new() -> Self {
        Self {
            elapsed: Duration::ZERO,
            latency_us: Histogram::new_with_bounds(1, 120_000_000, 3).expect("histogram"),
            statuses: BTreeMap::new(),
            errors: 0,
            slow: 0,
        }
    }

    fn record(&mut self, latency: Duration, status: Option<u16>) {
        let us = (latency.as_micros() as u64).clamp(1, 120_000_000);
        self.latency_us.saturating_record(us);
        if latency > Duration::from_millis(200) {
            self.slow += 1;
        }
        match status {
            Some(code) => *self.statuses.entry(code).or_default() += 1,
            None => self.errors += 1,
        }
    }

    pub fn merge(&mut self, other: LoadStats) {
        self.latency_us.add(&other.latency_us).expect("same bounds");
        for (code, n) in other.statuses {
            *self.statuses.entry(code).or_default() += n;
        }
        self.errors += other.errors;
        self.slow += other.slow;
    }

    pub fn ok(&self) -> u64 {
        self.statuses.get(&200).copied().unwrap_or(0)
    }
}

/// Where and as whom the envelopes go.
#[derive(Clone)]
pub struct Target {
    pub client: reqwest::Client,
    pub url: String,
    pub auth: String,
}

impl Target {
    pub fn new(base: &str, project_id: i64, key: &str, connections: usize) -> Self {
        Self {
            client: reqwest::Client::builder()
                .pool_max_idle_per_host(connections)
                .http1_only()
                .timeout(Duration::from_secs(120))
                .build()
                .expect("client"),
            url: format!("{base}/api/{project_id}/envelope/"),
            auth: format!("Sentry sentry_key={key}, sentry_version=7, sentry_client=rustrak-lab/1"),
        }
    }

    async fn send(&self, body: Vec<u8>) -> Option<u16> {
        let response = self
            .client
            .post(&self.url)
            .header("X-Sentry-Auth", &self.auth)
            .header("Content-Type", "application/x-sentry-envelope")
            .body(body)
            .send()
            .await
            .ok()?;
        let status = response.status().as_u16();
        // Drain the body so the connection goes back to the pool.
        let _ = response.bytes().await;
        Some(status)
    }
}

/// `connections` workers, each sending back to back until `duration` is up
/// or `limit` requests went out in total.
pub async fn closed_loop(
    target: &Target,
    template: Arc<Template>,
    connections: usize,
    duration: Duration,
    limit: Option<u64>,
) -> LoadStats {
    let started = Instant::now();
    let sent = Arc::new(AtomicU64::new(0));
    let workers: Vec<_> = (0..connections)
        .map(|worker| {
            let target = target.clone();
            let template = template.clone();
            let sent = sent.clone();
            tokio::spawn(async move {
                let mut stats = LoadStats::new();
                let mut rng = 0x9E37_79B9_7F4A_7C15
                    ^ ((worker as u64 + 1) << 32)
                    ^ started.elapsed().as_nanos() as u64;
                while started.elapsed() < duration
                    && limit.is_none_or(|l| sent.fetch_add(1, Ordering::Relaxed) < l)
                {
                    let body = template.render(&mut rng);
                    let t0 = Instant::now();
                    let status = target.send(body).await;
                    stats.record(t0.elapsed(), status);
                }
                stats
            })
        })
        .collect();
    let mut total = LoadStats::new();
    for worker in workers {
        total.merge(worker.await.expect("load worker"));
    }
    total.elapsed = started.elapsed();
    total
}

/// A fixed offered rate, whatever the server does: each request is sent at
/// its scheduled time and its latency counts from then, so a stall shows up
/// as latency instead of as fewer requests (no coordinated omission).
pub async fn open_loop(
    target: &Target,
    template: Arc<Template>,
    rate: f64,
    duration: Duration,
) -> LoadStats {
    let started = Instant::now();
    let interval = Duration::from_secs_f64(1.0 / rate);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut rng = 0xD1B5_4A32_D192_ED03 ^ started.elapsed().as_nanos() as u64;
    let mut n = 0u32;
    while started.elapsed() < duration {
        let scheduled = started + interval * n;
        tokio::time::sleep_until(scheduled.into()).await;
        let body = template.render(&mut rng);
        let target = target.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            let status = target.send(body).await;
            let _ = tx.send((scheduled.elapsed(), status));
        });
        n += 1;
    }
    drop(tx);
    let mut total = LoadStats::new();
    while let Some((latency, status)) = rx.recv().await {
        total.record(latency, status);
    }
    total.elapsed = started.elapsed();
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(body: &[u8]) -> (serde_json::Value, serde_json::Value) {
        let text = std::str::from_utf8(body).unwrap();
        let mut lines = text.lines();
        let header: serde_json::Value = serde_json::from_str(lines.next().unwrap()).unwrap();
        lines.next();
        let event: serde_json::Value = serde_json::from_str(lines.next().unwrap()).unwrap();
        (header, event)
    }

    #[test]
    fn a_rendered_envelope_is_valid_and_has_its_own_id() {
        let template = Template::new(20_000, 0, 0, 2000);
        assert!(
            (20_000..21_000).contains(&template.len()),
            "{}",
            template.len()
        );
        let mut rng = 7;
        let (header, event) = parse(&template.render(&mut rng));
        let id = header["event_id"].as_str().unwrap();
        assert_eq!(id.len(), 32);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(event["event_id"], header["event_id"]);
        assert!(event["exception"]["values"][0]["value"]
            .as_str()
            .unwrap()
            .contains("bad item in g"));
        assert_eq!(
            event["exception"]["values"][0]["stacktrace"]["frames"]
                .as_array()
                .unwrap()
                .len(),
            26
        );
        assert!(ENVIRONMENTS.contains(&event["environment"].as_str().unwrap()));
        let (other, _) = parse(&template.render(&mut rng));
        assert_ne!(other["event_id"], header["event_id"]);
    }

    #[test]
    fn hot_groups_get_their_share() {
        let template = Template::new(2_000, 0, 0, 2000);
        let mut rng = 11;
        let hot = (0..5_000)
            .filter(|_| {
                let (_, event) = parse(&template.render(&mut rng));
                let value = event["exception"]["values"][0]["value"]
                    .as_str()
                    .unwrap()
                    .to_string();
                let group: u32 = value.rsplit('g').next().unwrap().parse().unwrap();
                group < 10
            })
            .count();
        assert!((1_300..1_700).contains(&hot), "{hot} of 5000 on hot groups");
    }

    #[test]
    fn attachments_follow_the_event() {
        let template = Template::new(2_000, 2, 1_000, 2000);
        let mut rng = 3;
        let body = template.render(&mut rng);
        let text = String::from_utf8_lossy(&body);
        assert_eq!(text.matches("\"type\":\"attachment\"").count(), 2);
        assert!(body.len() > 4_000);
    }
}
