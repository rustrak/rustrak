//! Rustrak Benchmark Tool
//!
//! A comprehensive benchmarking suite for the Rustrak error tracking server.

mod aggregate;
mod config;
mod envelope;
mod metrics;
mod pgstats;
mod report;
mod runner;

use clap::{Parser, Subcommand};
use colored::Colorize;
use config::ScenarioConfig;
use runner::BenchmarkRunner;
use std::path::{Path, PathBuf};

/// Rustrak Server Benchmark Tool
#[derive(Parser)]
#[command(name = "rustrak-bench")]
#[command(author = "Rustrak Team")]
#[command(version = "0.1.0")]
#[command(about = "Benchmark suite for Rustrak server performance testing")]
#[command(long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Scenario to run (baseline, burst, sustained, stress)
    #[arg(short, long, default_value = "sustained")]
    scenario: String,

    /// Path to custom scenario configuration file (TOML)
    #[arg(short = 'f', long)]
    config_file: Option<PathBuf>,

    /// Server URL
    #[arg(long, default_value = "http://localhost:8080")]
    server: String,

    /// Project ID
    #[arg(long, default_value = "1")]
    project_id: u32,

    /// Sentry key for authentication
    #[arg(long, env = "SENTRY_KEY")]
    sentry_key: Option<String>,

    /// Docker container name for server metrics collection
    #[arg(long)]
    container: Option<String>,

    /// Docker container name for PostgreSQL metrics collection
    #[arg(long)]
    postgres_container: Option<String>,

    /// PostgreSQL connection string for engine statistics.
    /// Required by the drain and read scenarios.
    #[arg(long, env = "BENCH_POSTGRES_URL")]
    postgres_url: Option<String>,

    /// API token for read-path requests
    #[arg(long, env = "BENCH_API_TOKEN")]
    api_token: Option<String>,

    /// Label identifying this variant in comparison reports, e.g. "pg18"
    #[arg(long)]
    label: Option<String>,

    /// Repeat index, recorded so repeated runs can be aggregated
    #[arg(long, default_value = "0")]
    repeat: u32,

    /// Output directory for results
    #[arg(short, long, default_value = "results")]
    output: PathBuf,

    /// Skip warmup phase
    #[arg(long)]
    no_warmup: bool,

    /// Wait for server to be ready (timeout in seconds)
    #[arg(long, default_value = "30")]
    wait_timeout: u64,

    /// Skip waiting for server
    #[arg(long)]
    no_wait: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a benchmark scenario
    Run {
        /// Scenario name or path to config file
        scenario: Option<String>,
    },

    /// List available scenarios
    List,

    /// Compare two benchmark results
    Compare {
        /// Path to first results file
        old: PathBuf,
        /// Path to second results file
        new: PathBuf,
    },

    /// Show results from a previous run
    Show {
        /// Path to results file (defaults to latest.json)
        path: Option<PathBuf>,
    },

    /// Aggregate a matrix of runs and compare two variants
    Matrix {
        /// Directory holding the matrix result files
        #[arg(default_value = "results/matrix")]
        dir: PathBuf,
        /// Baseline variant label
        #[arg(long, default_value = "pg16")]
        baseline: String,
        /// Candidate variant label
        #[arg(long, default_value = "pg18")]
        candidate: String,
        /// Also write a Markdown report to this path
        #[arg(long)]
        markdown: Option<PathBuf>,
    },
}

fn print_banner() {
    println!(
        "{}",
        r#"
╔══════════════════════════════════════════════════════════════╗
║           🔬 Rustrak Server Benchmark Tool                    ║
╚══════════════════════════════════════════════════════════════╝
"#
        .cyan()
    );
}

fn list_scenarios() {
    println!("{}", "Available Scenarios".yellow().bold());
    println!();

    let scenarios = [
        (
            "baseline",
            "Measure baseline latency with minimal load (1 req/s)",
        ),
        (
            "burst",
            "Test handling of traffic spikes (10k events, pause, repeat)",
        ),
        (
            "sustained",
            "Sustained load for memory stability testing (1k req/s)",
        ),
        (
            "stress",
            "Find server limits by ramping up load until errors",
        ),
        (
            "drain",
            "Measure asynchronous digest throughput (needs --postgres-url)",
        ),
        (
            "read",
            "Measure dashboard query latency on a populated database (needs --postgres-url)",
        ),
    ];

    for (name, description) in scenarios {
        println!("  {} - {}", name.cyan().bold(), description);
    }

    println!();
    println!(
        "{}",
        "Use --config-file to load a custom scenario from TOML".dimmed()
    );
}

async fn run_benchmark(cli: &Cli) -> anyhow::Result<()> {
    // Load scenario configuration
    let mut config = if let Some(ref config_file) = cli.config_file {
        println!(
            "{} {}",
            "Loading config from:".dimmed(),
            config_file.display()
        );
        ScenarioConfig::from_file(config_file)?
    } else if let Some(preset) = ScenarioConfig::from_name(&cli.scenario) {
        preset
    } else {
        anyhow::bail!(
            "Unknown scenario '{}'. Use --list to see available scenarios.",
            cli.scenario
        );
    };

    // Apply CLI overrides
    if cli.no_warmup {
        config.warmup_secs = 0;
    }

    // Get sentry key
    let sentry_key = cli.sentry_key.clone().unwrap_or_else(|| {
        // Generate a placeholder - user should provide real key
        "00000000-0000-0000-0000-000000000000".to_string()
    });

    // Create runner
    let mut runner = BenchmarkRunner::new(config, &cli.server, cli.project_id, &sentry_key)?;

    if let Some(ref container) = cli.container {
        runner = runner.with_container(container);
    }

    if let Some(ref container) = cli.postgres_container {
        runner = runner.with_postgres_container(container);
    }

    if let Some(ref url) = cli.postgres_url {
        runner = runner.with_postgres_url(url);
    }

    if let Some(ref token) = cli.api_token {
        runner = runner.with_api_token(token);
    }

    if let Some(ref label) = cli.label {
        runner = runner.with_label(label, cli.repeat);
    }

    // Wait for server
    if !cli.no_wait {
        runner.wait_for_server(cli.wait_timeout).await?;
    }

    // Run benchmark
    let results = runner.run().await?;

    // Print summary
    results.print_summary();

    // Save results
    let filepath = results.save(&cli.output)?;
    println!("\n{} {}", "Results saved to:".green(), filepath.cyan());

    Ok(())
}

async fn compare_results(old_path: &PathBuf, new_path: &PathBuf) -> anyhow::Result<()> {
    let old_json = std::fs::read_to_string(old_path)?;
    let new_json = std::fs::read_to_string(new_path)?;

    let old: report::BenchmarkResults = serde_json::from_str(&old_json)?;
    let new: report::BenchmarkResults = serde_json::from_str(&new_json)?;

    report::compare(&old, &new);

    Ok(())
}

async fn show_results(path: Option<PathBuf>, output_dir: &Path) -> anyhow::Result<()> {
    let filepath = path.unwrap_or_else(|| output_dir.join("latest.json"));

    if !filepath.exists() {
        anyhow::bail!("Results file not found: {}", filepath.display());
    }

    let json = std::fs::read_to_string(&filepath)?;
    let results: report::BenchmarkResults = serde_json::from_str(&json)?;

    results.print_summary();

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut cli = Cli::parse();

    print_banner();

    match cli.command.take() {
        Some(Commands::List) => {
            list_scenarios();
        }
        Some(Commands::Compare { old, new }) => {
            compare_results(&old, &new).await?;
        }
        Some(Commands::Show { path }) => {
            show_results(path, &cli.output).await?;
        }
        Some(Commands::Matrix {
            dir,
            baseline,
            candidate,
            markdown,
        }) => {
            let results = aggregate::load_results(&dir)?;
            if results.is_empty() {
                anyhow::bail!("No result files found in {}", dir.display());
            }
            println!(
                "{}",
                format!("Loaded {} runs from {}", results.len(), dir.display()).dimmed()
            );

            aggregate::print_matrix(&results, &baseline, &candidate);

            if let Some(path) = markdown {
                let report = aggregate::markdown_matrix(&results, &baseline, &candidate);
                std::fs::write(&path, report)?;
                println!(
                    "\n{} {}",
                    "Markdown report written to:".green(),
                    path.display().to_string().cyan()
                );
            }
        }
        Some(Commands::Run { scenario }) => {
            // Override scenario from subcommand if provided
            if let Some(s) = scenario {
                cli.scenario = s;
            }
            run_benchmark(&cli).await?;
        }
        None => {
            // Default: run benchmark with CLI args
            run_benchmark(&cli).await?;
        }
    }

    Ok(())
}
