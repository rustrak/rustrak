# Rustrak Benchmarks

A comprehensive benchmarking suite for the Rustrak error tracking server. Measures performance metrics including throughput, latency, memory usage, and CPU consumption in a reproducible, isolated environment.

## Features

- **Custom Sentry Envelope Generator**: Generates valid Sentry envelope payloads with configurable complexity
- **Multiple Scenarios**: Baseline, burst, sustained, and stress test patterns
- **Docker Metrics**: Collects memory and CPU usage from Docker containers
- **JSON Output**: Results saved with timestamps for version comparison
- **Latency Histograms**: HDR histograms for accurate P50/P95/P99 measurements

## Quick Start

### Prerequisites

- Rust 1.75+
- Docker and Docker Compose
- A running Rustrak server (or use the included Docker setup)

### Using Docker Compose (Recommended)

```bash
# Start environment and auto-setup (creates project, gets credentials)
pnpm docker:up

# Run benchmark (uses saved credentials automatically)
pnpm bench

# View results
cat results/latest.json | jq '.results'

# Clean up
pnpm docker:down
```

The `docker:up` command automatically:
1. Starts PostgreSQL and Rustrak server containers
2. Creates a benchmark project
3. Saves credentials to `.bench-credentials`

### Against an Existing Server

```bash
# Build the benchmark tool
cargo build --release

# Run against your server
cargo run --release -- \
  --server http://localhost:8080 \
  --project-id 1 \
  --sentry-key YOUR_PROJECT_SENTRY_KEY \
  --scenario sustained
```

## Scenarios

| Scenario | Purpose | Configuration |
|----------|---------|---------------|
| `baseline` | Measure baseline latency | 1 req/s for 60s |
| `burst` | Test traffic spike handling | 10k events, 10s pause, 5 cycles |
| `sustained` | Test sustained load & memory | 1k req/s for 5 minutes |
| `stress` | Find server limits | Ramp up until 5% error rate |
| `drain` | Measure the digest pipeline | 20k events, then wait for the backlog |
| `read` | Measure dashboard query latency | Seed 30k events, then query for 60s |

### Why `drain` and `read` exist

Ingestion is two-phase. The HTTP endpoint parses the envelope, writes it to the
filesystem and returns `200` — then a spawned task does the database work
(grouping, issue upsert, event insert). The four original scenarios all measure
phase one, so they mostly report how fast Rustrak can acknowledge an event, not
how fast it can store one. Under load the acknowledgement stays quick while the
digest backlog grows behind it.

- **`drain`** sends a fixed batch and then waits for the backlog to reach zero,
  timing the digest itself. This is the scenario that reflects the database.
- **`read`** seeds a populated database and measures the queries the dashboard
  actually issues.

Both need `--postgres-url`: they observe digest progress by counting rows.

## PostgreSQL metrics

Passing `--postgres-url` also captures engine statistics for the run — buffer
hit ratio, WAL volume, checkpoint activity, index-vs-sequential access mix, temp
spill, and per-context I/O from `pg_stat_io`. Views are snapshotted before and
after and reported as deltas.

The collector reads views via `row_to_json` rather than into fixed structs,
because their shape changes between major versions (PG17 moved checkpoint
counters to `pg_stat_checkpointer`; PG18 dropped the timing columns from
`pg_stat_wal`). Whatever a given server exposes gets captured.

```bash
cargo run --release -- \
  --scenario drain \
  --postgres-url postgres://bench:bench@localhost:55432/rustrak_bench \
  --container rustrak-server-bench \
  --postgres-container rustrak-postgres-bench
```

## Comparing PostgreSQL versions

The compose file is parameterized by `PG_VERSION`, and `scripts/run-matrix.sh`
runs the full matrix, rebuilding a **fresh** environment for every single run:

```bash
# Full matrix: PG16 and PG18, five scenarios, three repeats each
./scripts/run-matrix.sh

# Narrower
PG_VERSIONS="16 18" SCENARIOS="drain read" REPEATS=5 ./scripts/run-matrix.sh

# Aggregate and compare
cargo run --release -- matrix --baseline pg16 --candidate pg18 \
  --markdown results/pg16-vs-pg18.md
```

Each run starts from an empty volume. This is not incidental: `drain` and `read`
leave tens of thousands of rows behind and PostgreSQL's shared buffers stay
warm, so a second run against that state would measure the leftovers.

The aggregation takes the **median** across repeats and reports the spread
alongside every comparison. Changes smaller than the observed run-to-run spread
are labelled "within noise" and should be read as no measurable difference —
on a laptop that band is wide, and a 5% gap between medians whose repeats vary
by 15% is not a finding.

### Grouping cardinality

`distinct_groups` controls how many issues the generated events collapse into.
It matters more than it looks: the generator puts a counter in each exception
message, and the server groups on that message, so leaving it unset gives every
event its own issue. That makes the database workload INSERT-only on `issues`,
whereas real traffic is mostly UPDATEs to existing rows.

```toml
[event]
distinct_groups = 300   # 15k events over 300 issues
```

Omit it to keep the original one-issue-per-event behaviour (worst case for issue
table growth).

### Traffic mix

By default the generator sends only error events. `transaction_ratio` mixes in
transactions, which exercise a different pipeline entirely: no grouping, one
`transactions` row, and one `spans` row per child span.

```toml
[event]
transaction_ratio = 0.2      # 20% transactions
spans_per_transaction = 10   # each writing 10 span rows
```

The mix is deterministic and evenly spread rather than random, so two runs of the
same config send exactly the same sequence — random variation is the last thing
a comparison needs.

Note that in `drain`, the reported digest figures count error events only, since
the drain wait observes the `events` table. Transactions are still ingested and
still load the database; their effect shows up in the PostgreSQL row and WAL
counters rather than in `digest_eps`.

### Custom Scenarios

Create a TOML file with your configuration:

```toml
name = "custom"
description = "My custom scenario"
scenario_type = "sustained"

duration_secs = 120
target_rps = 500
concurrency = 25
warmup_secs = 10

[event]
breadcrumb_count = 3
stack_depth = 5
include_user = true
include_tags = true
```

Save it as `scenarios/custom.toml` and run `--scenario custom`, or keep it
anywhere and pass its path: `--scenario my-scenario.toml`. Container limits are
set in `docker-compose.benchmark.yml` (`SERVER_CPUS`, `PG_CPUS`), not here.

## CLI Usage

```text
rustrak-bench [OPTIONS] [COMMAND]

Commands:
  run      Run a benchmark scenario
  list     List available scenarios
  compare  Compare two benchmark results
  show     Show results from a previous run
  help     Print this message or the help of the given subcommand(s)

Options:
  -s, --scenario <SCENARIO>      Name in scenarios/ or path to a TOML [default: sustained]
      --server <URL>             Server URL [default: http://localhost:8080]
      --project-id <ID>          Project ID, required [env: PROJECT_ID]
      --sentry-key <KEY>         The project's DSN key, required [env: SENTRY_KEY]
      --container <NAME>         Docker container name for metrics
  -o, --output <PATH>            Output directory [default: results]
      --no-warmup                Skip warmup phase
      --wait-timeout <SECS>      Wait for server timeout [default: 30]
      --no-wait                  Skip waiting for server
  -h, --help                     Print help
  -V, --version                  Print version
```

## Output Format

Results are saved as JSON with the following structure:

```json
{
  "run_id": "20260124-sustained-042",
  "timestamp": "2026-01-24T15:30:00Z",
  "scenario": "sustained",
  "config": {
    "duration_secs": 300,
    "target_rps": 1000,
    "concurrency": 50,
    "warmup_secs": 10
  },
  "results": {
    "throughput": {
      "total_requests": 298500,
      "successful": 298450,
      "failed": 50,
      "events_per_second": 995
    },
    "latency_ms": {
      "p50": 8.2,
      "p95": 24.1,
      "p99": 45.3,
      "max": 128.7,
      "min": 2.1,
      "mean": 12.4
    },
    "memory_mb": {
      "idle": 48,
      "peak": 182,
      "average": 124
    },
    "cpu_percent": {
      "peak": 85,
      "average": 62
    },
    "errors": {
      "rate_limited_429": 0,
      "server_error_5xx": 50,
      "connection_failed": 0
    }
  }
}
```

## Comparing Results

Compare two benchmark runs to track performance changes:

```bash
cargo run --release -- compare results/old-run.json results/new-run.json
```

Output:
```text
Comparison: 20260120-sustained-001 → 20260124-sustained-042

Throughput
  Events/sec:  850.00 → 995.00 (+17.1%)

Latency P99
  38.50ms → 45.30ms (+17.7%)

Peak Memory
  165.0MB → 182.0MB (+10.3%)
```

## Docker Metrics Collection

To collect memory and CPU metrics from the server container:

```bash
cargo run --release -- \
  --scenario sustained \
  --container rustrak-server-bench
```

The benchmark tool uses the Docker API (via `bollard`) to poll container stats every second.

## Metrics Collected

### Throughput
- Total requests sent
- Successful requests (2xx responses)
- Failed requests
- Events per second achieved

### Latency
- P50, P95, P99 percentiles
- Min, max, and mean latency
- Measured using HDR histograms

### Memory (with --container)
- Idle memory (before test starts)
- Peak memory usage
- Average memory usage
- Memory limit (if set)

### CPU (with --container)
- Peak CPU usage percentage
- Average CPU usage percentage

### Errors
- Rate limited (429) count
- Server errors (5xx) count
- Connection failures

## Scripts

```bash
# Build release binary
pnpm build

# Start benchmark environment (auto-creates project)
pnpm docker:up

# Re-run setup if needed (creates project, saves credentials)
pnpm prepare-env

# Run default benchmark (sustained)
pnpm bench

# Run specific scenarios
pnpm bench:baseline
pnpm bench:burst
pnpm bench:sustained
pnpm bench:stress

# Stop environment
pnpm docker:down

# View logs
pnpm docker:logs

# Clean results and credentials
pnpm clean
```

## Performance Tips

1. **Use Docker resource limits** for reproducible results
2. **Run multiple times** and compare results
3. **Disable logging** on the server (`RUST_LOG=warn`)
4. **Use release builds** for the benchmark tool
5. **Isolate the network** using the provided docker-compose

## Troubleshooting

### Server not ready

```bash
# Increase wait timeout
cargo run --release -- --wait-timeout 60 --scenario baseline
```

### No metrics collected

Make sure you specify the correct container name:
```bash
docker ps  # Find container name
cargo run --release -- --container rustrak-server-bench --scenario baseline
```

### Rate limiting

The default docker-compose sets very high rate limits. For testing rate limiting behavior, modify the environment variables in `docker-compose.benchmark.yml`.
