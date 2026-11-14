//! Rustrak Benchmark Tool
//!
//! A comprehensive benchmarking suite for the Rustrak error tracking server.

mod aggregate;
mod config;
mod envelope;
mod lab;
mod metrics;
mod pgstats;
mod report;
mod runner;

use clap::{Parser, Subcommand};
use colored::Colorize;
use config::ScenarioConfig;
use runner::BenchmarkRunner;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "rustrak-bench", version)]
#[command(about = "Benchmark suite for Rustrak server performance testing")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Scenario name (see `list`) or path to a scenario TOML
    #[arg(short, long, default_value = "sustained")]
    scenario: String,

    /// Server URL
    #[arg(long, default_value = "http://localhost:8080")]
    server: String,

    /// Required to run a scenario
    #[arg(long, env = "PROJECT_ID")]
    project_id: Option<u32>,

    /// The project's DSN key, required to run a scenario: ingest rejects any
    /// other
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
    /// A/B lab inside a Linux container (run through scripts/lab.sh)
    Lab(Box<lab::LabArgs>),

    /// Run a benchmark scenario
    Run {
        /// Scenario name or path to a scenario TOML
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

fn list_scenarios() -> anyhow::Result<()> {
    println!("{}", "Available Scenarios".yellow().bold());
    println!();

    let mut paths: Vec<_> = std::fs::read_dir(config::scenarios_dir())?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<Result<_, _>>()?;
    paths.retain(|path| path.extension().is_some_and(|e| e == "toml"));
    paths.sort();
    for path in paths {
        let config = ScenarioConfig::load(&path.to_string_lossy())?;
        println!("  {} - {}", config.name.cyan().bold(), config.description);
    }
    Ok(())
}

async fn run_benchmark(cli: &Cli) -> anyhow::Result<()> {
    let mut config = ScenarioConfig::load(&cli.scenario)?;

    if cli.no_warmup {
        config.warmup_secs = 0;
    }

    let (Some(project_id), Some(sentry_key)) = (cli.project_id, cli.sentry_key.as_deref()) else {
        anyhow::bail!("--project-id and --sentry-key are required (or run `pnpm bench`)");
    };
    let mut runner = BenchmarkRunner::new(config, &cli.server, project_id, sentry_key)?;

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

    if !cli.no_wait {
        runner.wait_for_server(cli.wait_timeout).await?;
    }

    let results = runner.run().await?;

    results.print_summary();

    let filepath = results.save(&cli.output)?;
    println!("\n{} {}", "Results saved to:".green(), filepath.cyan());

    Ok(())
}

fn compare_results(old_path: &PathBuf, new_path: &PathBuf) -> anyhow::Result<()> {
    let old_json = std::fs::read_to_string(old_path)?;
    let new_json = std::fs::read_to_string(new_path)?;

    let old: report::BenchmarkResults = serde_json::from_str(&old_json)?;
    let new: report::BenchmarkResults = serde_json::from_str(&new_json)?;

    report::compare(&old, &new);

    Ok(())
}

fn show_results(path: Option<PathBuf>, output_dir: &Path) -> anyhow::Result<()> {
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
        Some(Commands::Lab(args)) => {
            lab::run(*args).await?;
        }
        Some(Commands::List) => {
            list_scenarios()?;
        }
        Some(Commands::Compare { old, new }) => {
            compare_results(&old, &new)?;
        }
        Some(Commands::Show { path }) => {
            show_results(path, &cli.output)?;
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
            if let Some(s) = scenario {
                cli.scenario = s;
            }
            run_benchmark(&cli).await?;
        }
        None => {
            run_benchmark(&cli).await?;
        }
    }

    Ok(())
}
