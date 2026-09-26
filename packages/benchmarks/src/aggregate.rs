//! Aggregation and comparison of matrix benchmark runs.
//!
//! A single benchmark run is noisy — especially on a laptop, where thermal
//! state and background work move throughput by more than the effect being
//! measured. The matrix therefore runs each combination several times, and this
//! module reduces those repeats to one figure per (variant, scenario, metric).
//!
//! ## Why the median, and why the spread is always shown
//!
//! The median is used rather than the mean because benchmark repeats are
//! skewed: a run occasionally gets interrupted and lands far slower, and one
//! such outlier drags a 3-sample mean noticeably while leaving the median
//! alone.
//!
//! The spread is reported alongside every comparison because a difference
//! smaller than the run-to-run variation is not a result. A 5% median gap
//! across repeats that themselves vary by 15% says nothing, and presenting it
//! as "PG18 is 5% faster" would be an invented finding.

use crate::report::BenchmarkResults;
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// A metric extracted from a run, with the direction that counts as better.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    HigherIsBetter,
    LowerIsBetter,
}

/// One metric's values across the repeats of a single (variant, scenario).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSummary {
    pub metric: String,
    pub median: f64,
    pub min: f64,
    pub max: f64,
    pub samples: usize,
    /// (max - min) / median, as a percentage: how much the repeats disagreed.
    pub spread_percent: f64,
}

/// Summary of one variant running one scenario.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CellSummary {
    pub label: String,
    pub scenario: String,
    pub runs: usize,
    pub metrics: BTreeMap<String, MetricSummary>,
}

/// Comparison of one metric between two variants.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricComparison {
    pub metric: String,
    pub baseline: f64,
    pub candidate: f64,
    pub change_percent: f64,
    /// Whether the change exceeds the noise observed within either variant.
    pub significant: bool,
    /// Largest spread seen in either variant, for context.
    pub noise_percent: f64,
    pub improvement: bool,
}

/// Extract the comparable metrics from one run.
///
/// Returns (name, value, direction). Metrics absent from a scenario simply do
/// not appear — a baseline run has no digest figures, a drain run has no
/// endpoint latencies.
fn extract_metrics(result: &BenchmarkResults) -> Vec<(String, f64, Direction)> {
    let mut metrics = Vec::new();
    let r = &result.results;

    metrics.push((
        "throughput_eps".to_string(),
        r.throughput.events_per_second,
        Direction::HigherIsBetter,
    ));
    metrics.push((
        "latency_p50_ms".to_string(),
        r.latency_ms.p50,
        Direction::LowerIsBetter,
    ));
    metrics.push((
        "latency_p95_ms".to_string(),
        r.latency_ms.p95,
        Direction::LowerIsBetter,
    ));
    metrics.push((
        "latency_p99_ms".to_string(),
        r.latency_ms.p99,
        Direction::LowerIsBetter,
    ));

    if let Some(ref memory) = r.memory_mb {
        metrics.push((
            "server_peak_mb".to_string(),
            memory.peak_mb,
            Direction::LowerIsBetter,
        ));
    }
    if let Some(ref cpu) = r.cpu_percent {
        metrics.push((
            "server_avg_cpu".to_string(),
            cpu.average_percent,
            Direction::LowerIsBetter,
        ));
    }
    if let Some(ref memory) = r.postgres_memory_mb {
        metrics.push((
            "pg_peak_mb".to_string(),
            memory.peak_mb,
            Direction::LowerIsBetter,
        ));
    }
    if let Some(ref cpu) = r.postgres_cpu_percent {
        metrics.push((
            "pg_avg_cpu".to_string(),
            cpu.average_percent,
            Direction::LowerIsBetter,
        ));
    }

    if let Some(ref digest) = r.digest {
        metrics.push((
            "digest_eps".to_string(),
            digest.digest_events_per_second,
            Direction::HigherIsBetter,
        ));
        metrics.push((
            "drain_secs".to_string(),
            digest.drain_secs,
            Direction::LowerIsBetter,
        ));
        metrics.push((
            "ingest_secs".to_string(),
            digest.ingest_secs,
            Direction::LowerIsBetter,
        ));
        metrics.push((
            "peak_backlog".to_string(),
            digest.peak_backlog as f64,
            Direction::LowerIsBetter,
        ));
    }

    if let Some(ref endpoints) = r.endpoints {
        for endpoint in endpoints {
            metrics.push((
                format!("read[{}]_p95_ms", endpoint.endpoint),
                endpoint.latency_ms.p95,
                Direction::LowerIsBetter,
            ));
        }
    }

    if let Some(ref pg) = r.postgres {
        for (key, direction) in [
            ("wal_mb", Direction::LowerIsBetter),
            ("cache_hit_ratio", Direction::HigherIsBetter),
            ("blk_io_time_ms", Direction::LowerIsBetter),
            ("transactions", Direction::HigherIsBetter),
            ("temp_mb", Direction::LowerIsBetter),
        ] {
            if let Some(value) = pg.delta.derived.get(key) {
                metrics.push((format!("pg_{}", key), *value, direction));
            }
        }
        metrics.push((
            "pg_db_mb".to_string(),
            pg.database_bytes as f64 / (1024.0 * 1024.0),
            Direction::LowerIsBetter,
        ));
    }

    metrics
}

/// The direction registry, so comparison can classify a change as improvement
/// or regression without re-deriving it from a run.
fn metric_directions(results: &[BenchmarkResults]) -> BTreeMap<String, Direction> {
    let mut directions = BTreeMap::new();
    for result in results {
        for (name, _, direction) in extract_metrics(result) {
            directions.insert(name, direction);
        }
    }
    directions
}

fn median(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[mid - 1] + values[mid]) / 2.0
    } else {
        values[mid]
    }
}

/// Load every result file in a directory.
///
/// `latest.json` is skipped: it is a copy of whichever run finished last and
/// would double-count that run's numbers.
pub fn load_results(dir: &Path) -> std::io::Result<Vec<BenchmarkResults>> {
    let mut results = Vec::new();

    for entry in fs::read_dir(dir)? {
        let path = entry?.path();

        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if path.file_name().and_then(|n| n.to_str()) == Some("latest.json") {
            continue;
        }

        let json = fs::read_to_string(&path)?;
        match serde_json::from_str::<BenchmarkResults>(&json) {
            Ok(result) => results.push(result),
            Err(e) => eprintln!("{}", format!("Skipping {}: {}", path.display(), e).yellow()),
        }
    }

    Ok(results)
}

/// Group runs into (variant, scenario) cells and summarize each.
pub fn summarize(results: &[BenchmarkResults]) -> Vec<CellSummary> {
    let mut grouped: BTreeMap<(String, String), Vec<&BenchmarkResults>> = BTreeMap::new();

    for result in results {
        let label = result
            .environment
            .as_ref()
            .map(|e| e.label.clone())
            .unwrap_or_else(|| "unlabelled".to_string());
        grouped
            .entry((label, result.scenario.clone()))
            .or_default()
            .push(result);
    }

    grouped
        .into_iter()
        .map(|((label, scenario), runs)| {
            let mut by_metric: BTreeMap<String, Vec<f64>> = BTreeMap::new();
            for run in &runs {
                for (name, value, _) in extract_metrics(run) {
                    by_metric.entry(name).or_default().push(value);
                }
            }

            let metrics = by_metric
                .into_iter()
                .map(|(name, mut values)| {
                    let med = median(&mut values);
                    let min = values.first().copied().unwrap_or(0.0);
                    let max = values.last().copied().unwrap_or(0.0);
                    let spread = if med.abs() > f64::EPSILON {
                        (max - min) / med.abs() * 100.0
                    } else {
                        0.0
                    };

                    (
                        name.clone(),
                        MetricSummary {
                            metric: name,
                            median: med,
                            min,
                            max,
                            samples: values.len(),
                            spread_percent: spread,
                        },
                    )
                })
                .collect();

            CellSummary {
                label,
                scenario,
                runs: runs.len(),
                metrics,
            }
        })
        .collect()
}

/// Compare two variants for one scenario.
pub fn compare_cells(
    baseline: &CellSummary,
    candidate: &CellSummary,
    directions: &BTreeMap<String, Direction>,
) -> Vec<MetricComparison> {
    let mut comparisons = Vec::new();

    for (name, base) in &baseline.metrics {
        let Some(cand) = candidate.metrics.get(name) else {
            continue;
        };

        // A metric that is zero on both sides carries no information; a metric
        // that is zero only on the baseline has no meaningful percentage.
        if base.median.abs() < f64::EPSILON {
            continue;
        }

        let change = (cand.median - base.median) / base.median.abs() * 100.0;

        // Treat the larger of the two within-variant spreads as the noise
        // floor. Anything smaller than that is indistinguishable from run-to-run
        // variation and must not be reported as an effect.
        let noise = base.spread_percent.max(cand.spread_percent);

        let direction = directions
            .get(name)
            .copied()
            .unwrap_or(Direction::HigherIsBetter);
        let improvement = match direction {
            Direction::HigherIsBetter => change > 0.0,
            Direction::LowerIsBetter => change < 0.0,
        };

        comparisons.push(MetricComparison {
            metric: name.clone(),
            baseline: base.median,
            candidate: cand.median,
            change_percent: change,
            significant: change.abs() > noise && change.abs() >= 3.0,
            noise_percent: noise,
            improvement,
        });
    }

    comparisons
}

/// Print the full matrix comparison to the console.
pub fn print_matrix(results: &[BenchmarkResults], baseline_label: &str, candidate_label: &str) {
    let summaries = summarize(results);
    let directions = metric_directions(results);

    println!("\n{}", "═".repeat(78).cyan());
    println!(
        "{} {} {} {}",
        "Matrix comparison:".bold(),
        baseline_label.dimmed(),
        "→".dimmed(),
        candidate_label.cyan().bold()
    );
    println!("{}", "═".repeat(78).cyan());

    let scenarios: Vec<String> = {
        let mut names: Vec<String> = summaries.iter().map(|s| s.scenario.clone()).collect();
        names.sort();
        names.dedup();
        names
    };

    for scenario in &scenarios {
        let baseline = summaries
            .iter()
            .find(|s| &s.scenario == scenario && s.label == baseline_label);
        let candidate = summaries
            .iter()
            .find(|s| &s.scenario == scenario && s.label == candidate_label);

        let (Some(baseline), Some(candidate)) = (baseline, candidate) else {
            continue;
        };

        println!(
            "\n{} {} {}",
            "Scenario:".yellow().bold(),
            scenario.cyan().bold(),
            format!("({} vs {} runs)", baseline.runs, candidate.runs).dimmed()
        );
        println!(
            "  {:<28} {:>12} {:>12} {:>10} {:>10}",
            "metric".dimmed(),
            baseline_label.dimmed(),
            candidate_label.dimmed(),
            "change".dimmed(),
            "noise".dimmed()
        );

        for comparison in compare_cells(baseline, candidate, &directions) {
            let change = format!("{:+.1}%", comparison.change_percent);
            let rendered = if !comparison.significant {
                change.dimmed()
            } else if comparison.improvement {
                change.green().bold()
            } else {
                change.red().bold()
            };

            println!(
                "  {:<28} {:>12.2} {:>12.2} {:>10} {:>9.1}%",
                comparison.metric,
                comparison.baseline,
                comparison.candidate,
                rendered,
                comparison.noise_percent
            );
        }
    }

    println!("\n{}", "═".repeat(78).cyan());
    println!(
        "{}",
        "Dimmed changes fall within run-to-run noise and should be read as 'no difference'."
            .dimmed()
    );
    println!("{}", "═".repeat(78).cyan());
}

/// Render the comparison as Markdown.
pub fn markdown_matrix(
    results: &[BenchmarkResults],
    baseline_label: &str,
    candidate_label: &str,
) -> String {
    let summaries = summarize(results);
    let directions = metric_directions(results);
    let mut out = String::new();

    out.push_str(&format!(
        "# Benchmark matrix: {} vs {}\n\n",
        baseline_label, candidate_label
    ));

    // Record the engine versions actually measured, so the document does not
    // rely on the labels being honest.
    for label in [baseline_label, candidate_label] {
        if let Some(version) = results
            .iter()
            .find(|r| {
                r.environment
                    .as_ref()
                    .map(|e| e.label == label)
                    .unwrap_or(false)
            })
            .and_then(|r| r.environment.as_ref())
            .and_then(|e| e.postgres_version.as_ref())
        {
            out.push_str(&format!("- `{}`: {}\n", label, version));
        }
    }
    out.push('\n');

    let scenarios: Vec<String> = {
        let mut names: Vec<String> = summaries.iter().map(|s| s.scenario.clone()).collect();
        names.sort();
        names.dedup();
        names
    };

    for scenario in &scenarios {
        let baseline = summaries
            .iter()
            .find(|s| &s.scenario == scenario && s.label == baseline_label);
        let candidate = summaries
            .iter()
            .find(|s| &s.scenario == scenario && s.label == candidate_label);

        let (Some(baseline), Some(candidate)) = (baseline, candidate) else {
            continue;
        };

        out.push_str(&format!(
            "## {} ({} runs per variant)\n\n",
            scenario,
            baseline.runs.min(candidate.runs)
        ));
        out.push_str(&format!(
            "| metric | {} | {} | change | noise | verdict |\n",
            baseline_label, candidate_label
        ));
        out.push_str("|---|---:|---:|---:|---:|---|\n");

        for comparison in compare_cells(baseline, candidate, &directions) {
            let verdict = if !comparison.significant {
                "within noise"
            } else if comparison.improvement {
                "better"
            } else {
                "worse"
            };

            out.push_str(&format!(
                "| {} | {:.2} | {:.2} | {:+.1}% | ±{:.1}% | {} |\n",
                comparison.metric,
                comparison.baseline,
                comparison.candidate,
                comparison.change_percent,
                comparison.noise_percent,
                verdict
            ));
        }
        out.push('\n');
    }

    out.push_str(
        "\n> Changes marked \"within noise\" are smaller than the spread between \
         repeats of the same variant, and should be read as no measurable difference.\n",
    );

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_of_even_and_odd_counts() {
        assert_eq!(median(&mut [3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&mut [4.0, 1.0, 3.0, 2.0]), 2.5);
        assert_eq!(median(&mut []), 0.0);
    }

    #[test]
    fn median_ignores_a_single_wild_outlier() {
        // The reason the median is used at all: one interrupted repeat.
        let mut with_outlier = [100.0, 102.0, 900.0];
        assert_eq!(median(&mut with_outlier), 102.0);
    }

    fn cell(label: &str, metric: &str, median: f64, spread: f64) -> CellSummary {
        CellSummary {
            label: label.to_string(),
            scenario: "drain".to_string(),
            runs: 3,
            metrics: [(
                metric.to_string(),
                MetricSummary {
                    metric: metric.to_string(),
                    median,
                    min: median,
                    max: median,
                    samples: 3,
                    spread_percent: spread,
                },
            )]
            .into_iter()
            .collect(),
        }
    }

    #[test]
    fn change_smaller_than_noise_is_not_significant() {
        let directions = [("digest_eps".to_string(), Direction::HigherIsBetter)]
            .into_iter()
            .collect();

        // 5% faster, but repeats varied by 15%: not a finding.
        let baseline = cell("pg16", "digest_eps", 100.0, 15.0);
        let candidate = cell("pg18", "digest_eps", 105.0, 15.0);

        let comparisons = compare_cells(&baseline, &candidate, &directions);

        assert_eq!(comparisons.len(), 1);
        assert!(!comparisons[0].significant);
        assert!((comparisons[0].change_percent - 5.0).abs() < 0.01);
    }

    #[test]
    fn change_larger_than_noise_is_significant() {
        let directions = [("digest_eps".to_string(), Direction::HigherIsBetter)]
            .into_iter()
            .collect();

        let baseline = cell("pg16", "digest_eps", 100.0, 2.0);
        let candidate = cell("pg18", "digest_eps", 130.0, 3.0);

        let comparisons = compare_cells(&baseline, &candidate, &directions);

        assert!(comparisons[0].significant);
        assert!(comparisons[0].improvement);
    }

    #[test]
    fn lower_is_better_metrics_invert_the_verdict() {
        let directions = [("latency_p99_ms".to_string(), Direction::LowerIsBetter)]
            .into_iter()
            .collect();

        let baseline = cell("pg16", "latency_p99_ms", 100.0, 1.0);
        let candidate = cell("pg18", "latency_p99_ms", 70.0, 1.0);

        let comparisons = compare_cells(&baseline, &candidate, &directions);

        assert!(comparisons[0].significant);
        // Latency went down, which is an improvement despite the negative change.
        assert!(comparisons[0].improvement);
        assert!(comparisons[0].change_percent < 0.0);
    }

    #[test]
    fn tiny_changes_are_never_significant_even_with_zero_noise() {
        let directions = [("digest_eps".to_string(), Direction::HigherIsBetter)]
            .into_iter()
            .collect();

        // Perfectly repeatable runs still should not turn 1% into a headline.
        let baseline = cell("pg16", "digest_eps", 100.0, 0.0);
        let candidate = cell("pg18", "digest_eps", 101.0, 0.0);

        let comparisons = compare_cells(&baseline, &candidate, &directions);

        assert!(!comparisons[0].significant);
    }

    #[test]
    fn zero_baseline_metrics_are_skipped_rather_than_dividing_by_zero() {
        let directions = BTreeMap::new();
        let baseline = cell("pg16", "peak_backlog", 0.0, 0.0);
        let candidate = cell("pg18", "peak_backlog", 42.0, 0.0);

        let comparisons = compare_cells(&baseline, &candidate, &directions);

        assert!(comparisons.is_empty());
    }
}
