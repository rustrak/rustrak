//! `rustrak-bench lab`: the A/B harness that runs *inside* a Linux container
//! (see `scripts/lab.sh`). It starts each server build as a child process, so
//! its memory comes from `/proc/<pid>` (anonymous RSS, not the cgroup counter
//! that includes page cache), drives it with the in-process load generator,
//! and alternates builds run by run so drift hits all of them alike.

mod load;
mod procfs;
mod server;

use anyhow::{bail, Context};
use clap::{Args, ValueEnum};
use load::{Target, Template};
use procfs::Sample;
use serde::Serialize;
use server::{Poll, Server};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Scenario {
    /// A burst of ingest, then the drain.
    Ingest,
    /// A backlog of N events: memory per pending event, digest rate under it.
    Backlog,
    /// Fixed-rate ingest plus dashboard readers on a seeded database.
    Mixed,
    /// A few 4 MiB events and one ~64 MiB envelope: peak memory.
    Large,
}

#[derive(Debug, Args)]
pub struct LabArgs {
    #[arg(long, value_enum, default_value = "ingest")]
    pub scenario: Scenario,
    /// A build to measure, `name=/path/to/rustrak`. Repeat; runs alternate.
    #[arg(long = "variant", required = true)]
    pub variants: Vec<String>,
    /// Extra server environment for one variant, `name:KEY=VALUE`. Repeat.
    #[arg(long = "env")]
    pub envs: Vec<String>,
    /// Seeded-database directory for one variant (mixed), `name=/dir`.
    #[arg(long = "seed")]
    pub seeds: Vec<String>,
    /// Rounds: every variant runs once per round, the order rotating.
    #[arg(long, default_value_t = 3)]
    pub runs: usize,
    /// Load phase length (ingest, mixed).
    #[arg(long, default_value_t = 20)]
    pub duration: u64,
    #[arg(long, default_value_t = 16)]
    pub connections: usize,
    /// Offered rate in req/s: open loop. Unset: closed loop.
    #[arg(long)]
    pub rate: Option<f64>,
    /// Size of one error event in bytes.
    #[arg(long, default_value_t = 20_000)]
    pub event_size: usize,
    #[arg(long, default_value_t = 2000)]
    pub groups: u32,
    /// Idle phase before the load; idle memory is the median of its last 3 s.
    #[arg(long, default_value_t = 5)]
    pub idle_secs: u64,
    /// Wait after the drain; floor memory is the median of its last 3 s.
    #[arg(long, default_value_t = 10)]
    pub floor_secs: u64,
    /// Give up waiting for the drain after this long (leftovers are reported).
    #[arg(long, default_value_t = 180)]
    pub drain_timeout: u64,
    /// Flag a run as stalled when its slowest request took longer (ms)...
    #[arg(long, default_value_t = 2000)]
    pub stall_max_ms: u64,
    /// ...or when more than this many requests took over 200 ms.
    #[arg(long, default_value_t = 50)]
    pub stall_slow: u64,
    /// Events in the backlog scenario.
    #[arg(long, default_value_t = 30_000)]
    pub backlog_events: u64,
    /// Events the mixed scenario's seeded database holds.
    #[arg(long, default_value_t = 50_000)]
    pub seed_events: u64,
    /// Concurrent dashboard readers (mixed).
    #[arg(long, default_value_t = 4)]
    pub readers: usize,
    /// Count fsync/fdatasync with `strace -f --seccomp-bpf -c` (slower).
    #[arg(long)]
    pub strace: bool,
    /// CPUs the server is pinned to (`taskset -c`); empty: no pinning.
    #[arg(long, default_value = "")]
    pub server_cpus: String,
    /// Where the database and spool live; wiped before every run.
    #[arg(long, default_value = "/data/run")]
    pub data_dir: PathBuf,
    /// PostgreSQL server URL without a database (`postgres://u:p@host:5432`).
    /// Unset: SQLite in the data dir.
    #[arg(long)]
    pub postgres: Option<String>,
    /// Directory for the JSON summary.
    #[arg(long, default_value = "results/lab")]
    pub results_dir: PathBuf,
    /// Free-form `key=value` recorded in the JSON (git SHAs, profile...).
    #[arg(long = "meta")]
    pub meta: Vec<String>,
}

pub struct Variant {
    pub name: String,
    pub bin: PathBuf,
    pub env: Vec<(String, String)>,
    pub seed: Option<PathBuf>,
}

fn variants(args: &LabArgs) -> anyhow::Result<Vec<Variant>> {
    let mut out = Vec::new();
    for spec in &args.variants {
        let (name, bin) = spec.split_once('=').context("--variant name=/path")?;
        out.push(Variant {
            name: name.to_string(),
            bin: bin.into(),
            env: Vec::new(),
            seed: None,
        });
    }
    let find = |out: &mut Vec<Variant>, name: &str| -> anyhow::Result<usize> {
        out.iter()
            .position(|v| v.name == name)
            .with_context(|| format!("no variant named {name}"))
    };
    for spec in &args.envs {
        let (name, kv) = spec.split_once(':').context("--env name:KEY=VALUE")?;
        let (k, v) = kv.split_once('=').context("--env name:KEY=VALUE")?;
        let i = find(&mut out, name)?;
        out[i].env.push((k.to_string(), v.to_string()));
    }
    for spec in &args.seeds {
        let (name, dir) = spec.split_once('=').context("--seed name=/dir")?;
        let i = find(&mut out, name)?;
        out[i].seed = Some(dir.into());
    }
    Ok(out)
}

/// One run's numbers, in table order. `None` is "not measured here".
type Row = Vec<(&'static str, Option<f64>)>;

#[derive(Serialize)]
struct RunRecord {
    variant: String,
    round: usize,
    stalled: bool,
    statuses: BTreeMap<u16, u64>,
    metrics: BTreeMap<&'static str, f64>,
    #[serde(skip)]
    row: Row,
}

const MB: f64 = 1024.0 * 1024.0;

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(|a, b| a.total_cmp(b));
    let n = values.len();
    if n == 0 {
        f64::NAN
    } else if n % 2 == 1 {
        values[n / 2]
    } else {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    }
}

/// Median anon RSS over `[from, to]` seconds.
fn anon_median(samples: &[Sample], from: f64, to: f64) -> Option<f64> {
    let mut v: Vec<f64> = samples
        .iter()
        .filter(|s| s.t >= from && s.t <= to)
        .map(|s| s.anon as f64)
        .collect();
    (!v.is_empty()).then(|| median(&mut v))
}

fn max_in(samples: &[Sample], from: f64, to: f64, f: impl Fn(&Sample) -> u64) -> Option<f64> {
    samples
        .iter()
        .filter(|s| s.t >= from && s.t <= to)
        .map(|s| f(s) as f64)
        .reduce(f64::max)
}

/// The sample nearest to `t`.
fn at(samples: &[Sample], t: f64) -> Option<Sample> {
    samples
        .iter()
        .min_by(|a, b| (a.t - t).abs().total_cmp(&(b.t - t).abs()))
        .copied()
}

/// What the timeline of one run says, whatever the scenario.
struct Timeline {
    samples: Vec<Sample>,
    polls: Vec<Poll>,
    load_start: f64,
    drained: f64,
    end: f64,
}

impl Timeline {
    fn memory(&self, row: &mut Row) {
        let s = &self.samples;
        let idle = anon_median(s, self.load_start - 3.0, self.load_start);
        let peak = max_in(s, self.load_start, self.drained, |x| x.anon);
        let floor = anon_median(s, self.end - 3.0, self.end);
        row.push(("anon_idle_mb", idle.map(|v| v / MB)));
        row.push(("anon_peak_mb", peak.map(|v| v / MB)));
        row.push(("anon_floor_mb", floor.map(|v| v / MB)));
        row.push((
            "file_peak_mb",
            max_in(s, self.load_start, self.end, |x| x.file).map(|v| v / MB),
        ));
        row.push(("hwm_mb", s.last().map(|x| x.hwm as f64 / MB)));
        row.push(("threads_max", max_in(s, 0.0, self.end, |x| x.threads)));
        row.push((
            "cgroup_incl_cache_peak_mb",
            max_in(s, 0.0, self.end, |x| x.cgroup_incl_cache).map(|v| v / MB),
        ));
    }

    /// Digest progress between two times, from the 1 Hz `/metrics` polls.
    fn digested(&self, from: f64, to: f64) -> Option<f64> {
        let near = |t: f64| {
            self.polls
                .iter()
                .min_by(|a, b| (a.t - t).abs().total_cmp(&(b.t - t).abs()))
        };
        Some(near(to)?.digested as f64 - near(from)?.digested as f64)
    }

    fn per_event(&self, row: &mut Row, events: f64) {
        let (Some(a), Some(b)) = (at(&self.samples, self.load_start), self.samples.last()) else {
            return;
        };
        let events = events.max(1.0);
        row.push((
            "written_kb_per_ev",
            Some((b.write_bytes - a.write_bytes) as f64 / 1024.0 / events),
        ));
        row.push((
            "disk_kb_per_ev",
            Some((b.disk_write_bytes - a.disk_write_bytes) as f64 / 1024.0 / events),
        ));
        row.push((
            "cpu_ms_per_ev",
            Some((b.cpu_ticks - a.cpu_ticks) as f64 / procfs::TICKS_PER_SEC * 1000.0 / events),
        ));
    }
}

/// Polls `/metrics` until the backlog is gone or stops moving. Returns when
/// it drained (or gave up) and how many acknowledged events are left.
async fn wait_drained(server: &Server, polls: &Mutex<Vec<Poll>>, acked: u64, timeout: Duration) {
    let started = Instant::now();
    let mut last = (u64::MAX, Instant::now());
    loop {
        let poll = server.poll().await;
        if let Some(poll) = poll {
            let done = poll.digested + poll.dropped;
            polls.lock().unwrap().push(poll);
            if poll.pending == 0 && done >= acked {
                return;
            }
            if done != last.0 {
                last = (done, Instant::now());
            } else if last.1.elapsed() > Duration::from_secs(15) {
                eprintln!("  drain stopped moving at {done}/{acked}");
                return;
            }
        }
        if started.elapsed() > timeout {
            eprintln!("  drain timed out");
            return;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

fn load_row(row: &mut Row, stats: &load::LoadStats) {
    let secs = stats.elapsed.as_secs_f64().max(0.001);
    let h = &stats.latency_us;
    row.push(("req_per_s", Some(stats.ok() as f64 / secs)));
    row.push(("p50_ms", Some(h.value_at_quantile(0.5) as f64 / 1000.0)));
    row.push(("p99_ms", Some(h.value_at_quantile(0.99) as f64 / 1000.0)));
    row.push(("max_ms", Some(h.max() as f64 / 1000.0)));
    row.push(("slow_200ms", Some(stats.slow as f64)));
    row.push(("non_200", Some((h.len() - stats.ok()) as f64)));
}

async fn run_once(
    args: &LabArgs,
    variant: &Variant,
    template: Arc<Template>,
) -> anyhow::Result<(Row, BTreeMap<u16, u64>, bool)> {
    let mut server = match args.scenario {
        Scenario::Mixed => server::start_seeded(args, variant, template.clone()).await?,
        _ => Server::start(args, variant).await?,
    };
    let creds = server.setup().await?;
    let target = Target::new(&server.base, creds.project_id, &creds.key, args.connections);

    // Memory at 4 Hz, `/metrics` at 1 Hz, both for the whole run.
    let samples = Arc::new(Mutex::new(Vec::new()));
    let polls = Arc::new(Mutex::new(Vec::new()));
    let sampling = server.sample_in_background(samples.clone(), polls.clone());

    tokio::time::sleep(Duration::from_secs(args.idle_secs)).await;
    let load_start = server.t();
    let mut row = Row::new();
    let duration = Duration::from_secs(args.duration);

    let (stats, readers) = match args.scenario {
        Scenario::Ingest => (
            run_load(args, &target, template.clone(), duration, None).await,
            None,
        ),
        Scenario::Backlog => (
            load::closed_loop(
                &target,
                template.clone(),
                args.connections,
                Duration::from_secs(3600),
                Some(args.backlog_events),
            )
            .await,
            None,
        ),
        Scenario::Large => {
            let mut stats = load::closed_loop(
                &target,
                template.clone(),
                1,
                Duration::from_secs(600),
                Some(5),
            )
            .await;
            let big = Arc::new(Template::new(
                4 * 1024 * 1024 - 65_536,
                60,
                1024 * 1024 - 64,
                args.groups,
            ));
            println!("  large envelope: {:.1} MiB", big.len() as f64 / MB);
            let one = load::closed_loop(&target, big, 1, Duration::from_secs(600), Some(1)).await;
            stats.merge(one);
            (stats, None)
        }
        Scenario::Mixed => {
            let token = server.token().await?;
            let readers = server::readers(
                &server.base,
                creds.project_id,
                &token,
                args.readers,
                duration,
            );
            let ingest = run_load(
                args,
                &target,
                template.clone(),
                duration,
                Some(args.rate.unwrap_or(500.0)),
            );
            let (stats, readers) = tokio::join!(ingest, readers);
            (stats, Some(readers?))
        }
    };
    let load_end = server.t();
    let acked = stats.ok();
    let max_ms = stats.latency_us.max() as f64 / 1000.0;
    let stalled = max_ms > args.stall_max_ms as f64 || stats.slow > args.stall_slow;

    wait_drained(
        &server,
        &polls,
        acked,
        Duration::from_secs(args.drain_timeout),
    )
    .await;
    let drained = server.t();
    tokio::time::sleep(Duration::from_secs(args.floor_secs)).await;
    let end = server.t();
    sampling.abort();
    let fsyncs = server.stop().await;

    let timeline = Timeline {
        samples: samples.lock().unwrap().clone(),
        polls: polls.lock().unwrap().clone(),
        load_start,
        drained,
        end,
    };
    load_row(&mut row, &stats);
    let last = timeline.polls.last().copied().unwrap_or_default();
    row.push((
        "digest_eps_in",
        timeline
            .digested(load_start, load_end)
            .map(|d| d / (load_end - load_start).max(0.001)),
    ));
    row.push((
        "digest_eps_after",
        timeline
            .digested(load_end, drained)
            .map(|d| d / (drained - load_end).max(0.001)),
    ));
    row.push(("drain_s", Some(drained - load_end)));
    // Acknowledged and never stored: a dropped digest is a lost event too.
    row.push(("leftover", Some(acked.saturating_sub(last.digested) as f64)));
    row.push(("dropped", Some(last.dropped as f64)));
    timeline.memory(&mut row);
    timeline.per_event(&mut row, acked as f64);
    row.push((
        "fsync_per_ev",
        fsyncs.map(|n| n as f64 / acked.max(1) as f64),
    ));

    if args.scenario == Scenario::Backlog {
        // Memory held per pending event: the anon growth over idle at the
        // moment the backlog peaked, divided by its size.
        let idle = anon_median(&timeline.samples, load_start - 3.0, load_start).unwrap_or(0.0);
        if let Some(peak) = timeline.polls.iter().max_by_key(|p| p.pending) {
            let anon = at(&timeline.samples, peak.t)
                .map(|s| s.anon as f64)
                .unwrap_or(0.0);
            row.push(("backlog_max", Some(peak.pending as f64)));
            row.push((
                "bytes_per_pending",
                Some((anon - idle).max(0.0) / (peak.pending.max(1)) as f64),
            ));
        }
    }
    if let Some(r) = readers {
        row.push(("read_rps", Some(r.rps)));
        row.push(("read_p50_ms", Some(r.p50_ms)));
        row.push(("read_p99_ms", Some(r.p99_ms)));
        row.push(("read_errors", Some(r.errors as f64)));
    }
    let mut statuses = stats.statuses.clone();
    if stats.errors > 0 {
        statuses.insert(0, stats.errors);
    }
    Ok((row, statuses, stalled))
}

async fn run_load(
    args: &LabArgs,
    target: &Target,
    template: Arc<Template>,
    duration: Duration,
    rate: Option<f64>,
) -> load::LoadStats {
    match rate.or(args.rate) {
        Some(rate) => load::open_loop(target, template, rate, duration).await,
        None => load::closed_loop(target, template, args.connections, duration, None).await,
    }
}

fn fmt(v: Option<f64>) -> String {
    match v {
        None => "-".into(),
        Some(v) if v.is_nan() => "-".into(),
        Some(v) if v.abs() >= 100.0 => format!("{v:.0}"),
        Some(v) if v.abs() >= 10.0 => format!("{v:.1}"),
        Some(v) => format!("{v:.2}"),
    }
}

fn print_table(
    runs: &[RunRecord],
    variants: &[Variant],
) -> BTreeMap<String, BTreeMap<&'static str, (f64, f64)>> {
    let Some(first) = runs.first() else {
        return BTreeMap::new();
    };
    let keys: Vec<&str> = first.row.iter().map(|(k, _)| *k).collect();
    let widths: Vec<usize> = keys.iter().map(|k| k.len().max(6)).collect();
    let line = |cells: Vec<String>| {
        cells
            .iter()
            .zip(std::iter::once(&14).chain(widths.iter()))
            .map(|(c, w)| format!("{c:>w$}"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    println!();
    println!(
        "{}",
        line(
            std::iter::once("run".to_string())
                .chain(keys.iter().map(|k| k.to_string()))
                .collect()
        )
    );
    for run in runs {
        let name = format!(
            "{}#{}{}",
            run.variant,
            run.round + 1,
            if run.stalled { "!" } else { "" }
        );
        println!(
            "{}",
            line(
                std::iter::once(name)
                    .chain(run.row.iter().map(|(_, v)| fmt(*v)))
                    .collect()
            )
        );
    }
    let mut summary = BTreeMap::new();
    println!(
        "-- median (spread = (max-min)/median %), stalled runs (!) excluded when others exist"
    );
    for variant in variants {
        let mine: Vec<&RunRecord> = runs.iter().filter(|r| r.variant == variant.name).collect();
        let clean: Vec<&RunRecord> = mine.iter().copied().filter(|r| !r.stalled).collect();
        let pick = if clean.is_empty() { mine } else { clean };
        let mut medians = Vec::new();
        let mut spreads = Vec::new();
        let mut cells = BTreeMap::new();
        for key in &keys {
            let mut values: Vec<f64> = pick
                .iter()
                .filter_map(|r| r.row.iter().find(|(k, _)| k == key).and_then(|(_, v)| *v))
                .filter(|v| !v.is_nan())
                .collect();
            if values.is_empty() {
                medians.push("-".to_string());
                spreads.push("-".to_string());
                continue;
            }
            let lo = values.iter().copied().fold(f64::INFINITY, f64::min);
            let hi = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let m = median(&mut values);
            let spread = if m.abs() > f64::EPSILON {
                (hi - lo) / m.abs() * 100.0
            } else {
                0.0
            };
            medians.push(fmt(Some(m)));
            spreads.push(format!("{spread:.0}%"));
            cells.insert(*key, (m, spread));
        }
        println!(
            "{}",
            line(
                std::iter::once(format!("{} med", variant.name))
                    .chain(medians)
                    .collect()
            )
        );
        println!(
            "{}",
            line(
                std::iter::once(format!("{} spr", variant.name))
                    .chain(spreads)
                    .collect()
            )
        );
        summary.insert(variant.name.clone(), cells);
    }
    summary
}

pub async fn run(args: LabArgs) -> anyhow::Result<()> {
    let variants = variants(&args)?;
    if args.scenario == Scenario::Mixed && args.postgres.is_some() {
        bail!("the mixed scenario seeds SQLite only");
    }
    for v in &variants {
        if !v.bin.exists() {
            bail!("{}: no binary at {}", v.name, v.bin.display());
        }
    }
    let size = match args.scenario {
        Scenario::Large => 4 * 1024 * 1024 - 65_536,
        _ => args.event_size,
    };
    let template = Arc::new(Template::new(size, 0, 0, args.groups));
    println!(
        "lab: scenario {:?}, {} variant(s) x {} round(s), envelope {} bytes",
        args.scenario,
        variants.len(),
        args.runs,
        template.len()
    );
    let started = Instant::now();
    let meta: BTreeMap<_, _> = args.meta.iter().filter_map(|m| m.split_once('=')).collect();
    std::fs::create_dir_all(&args.results_dir)?;
    let path = args.results_dir.join(
        format!(
            "{}-{:?}.json",
            chrono::Utc::now().format("%Y%m%d-%H%M%S"),
            args.scenario
        )
        .to_lowercase(),
    );
    // Rewritten after every run, so an interrupted session keeps the finished ones.
    let save = |runs: &[RunRecord], summary: Option<&_>| -> anyhow::Result<()> {
        let json = serde_json::json!({
            "scenario": args.scenario,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "args": format!("{args:?}"),
            "meta": meta,
            "variants": variants.iter().map(|v| serde_json::json!({
                "name": v.name, "bin": v.bin, "env": v.env,
            })).collect::<Vec<_>>(),
            "runs": runs,
            "median_and_spread_pct": summary,
        });
        std::fs::write(&path, serde_json::to_vec_pretty(&json)?)?;
        Ok(())
    };
    let mut runs = Vec::new();
    for round in 0..args.runs {
        for i in 0..variants.len() {
            // Rotate the order each round: A B, B A, A B...
            let variant = &variants[(i + round) % variants.len()];
            println!("== round {} / {}: {}", round + 1, args.runs, variant.name);
            let (row, statuses, stalled) = run_once(&args, variant, template.clone()).await?;
            let metrics = row.iter().filter_map(|(k, v)| v.map(|v| (*k, v))).collect();
            runs.push(RunRecord {
                variant: variant.name.clone(),
                round,
                stalled,
                statuses,
                metrics,
                row,
            });
            save(&runs, None)?;
        }
    }
    let summary = print_table(&runs, &variants);
    println!("\ntotal {:.0} s", started.elapsed().as_secs_f64());
    save(&runs, Some(&summary))?;
    println!("summary: {}", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_of_odd_and_even() {
        assert_eq!(median(&mut [3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&mut [4.0, 1.0, 2.0, 3.0]), 2.5);
        assert!(median(&mut []).is_nan());
    }

    #[test]
    fn idle_and_floor_are_medians_of_their_window() {
        let s = |t: f64, anon: u64| Sample {
            t,
            anon,
            ..Sample::default()
        };
        let samples = [
            s(0.0, 50),
            s(1.0, 10),
            s(2.0, 12),
            s(3.0, 11),
            s(4.0, 900),
            s(9.0, 20),
        ];
        assert_eq!(anon_median(&samples, 1.0, 3.0), Some(11.0));
        assert_eq!(max_in(&samples, 0.0, 9.0, |x| x.anon), Some(900.0));
        assert_eq!(anon_median(&samples, 20.0, 30.0), None);
    }
}
