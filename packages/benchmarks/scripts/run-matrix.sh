#!/bin/bash
# Run the benchmark matrix across PostgreSQL versions.
#
# For every (pg_version x scenario x repeat) combination this brings up a FRESH
# environment, runs one scenario, and saves a labelled result file.
#
# The teardown between runs is not optional. The drain and read scenarios leave
# tens of thousands of rows behind, and PostgreSQL's shared buffers stay warm; a
# second run against that state measures the leftovers, not the engine. Each run
# therefore starts from an empty volume.
#
# Usage:
#   ./scripts/run-matrix.sh                      # full matrix, defaults
#   PG_VERSIONS="16 18" SCENARIOS="drain read" REPEATS=3 ./scripts/run-matrix.sh
#
# Results land in results/matrix/<label>-<scenario>-r<n>.json

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# BENCH_DIR can be overridden so a copy of this script can be run from outside
# the repository. That matters for long matrix runs: bash reads a script
# incrementally as it executes, so editing this file mid-run can corrupt the
# job. Running a frozen copy from elsewhere makes the run immune to later edits.
BENCH_DIR="${BENCH_DIR:-$(dirname "$SCRIPT_DIR")}"
COMPOSE_FILE="$BENCH_DIR/docker-compose.benchmark.yml"
RESULTS_DIR="$BENCH_DIR/results/matrix"
LOG_DIR="$BENCH_DIR/results/logs"

# VARIANTS is the list of things to compare. Each entry is
#   <label>:<pg_version>[:<extra postgres args>]
# so a single matrix can compare major versions, or configurations of the same
# version, or both. The extra args field is what makes the second case possible:
# PostgreSQL 18's headline change is asynchronous I/O, and its effect is only
# visible by holding the version fixed and moving io_method.
#
# Examples:
#   VARIANTS="pg16:16 pg18:18"
#   VARIANTS="pg18-sync:18:-c io_method=sync pg18-worker:18:-c io_method=worker"
VARIANTS="${VARIANTS:-}"
PG_VERSIONS="${PG_VERSIONS:-16 18}"
SCENARIOS="${SCENARIOS:-baseline sustained burst drain read}"
REPEATS="${REPEATS:-3}"
PG_PORT="${PG_PORT:-55432}"

# Default the variant list to a plain version comparison.
if [ -z "$VARIANTS" ]; then
    for v in $PG_VERSIONS; do
        VARIANTS="$VARIANTS pg${v}:${v}"
    done
fi

mkdir -p "$RESULTS_DIR" "$LOG_DIR"

# Start from an empty results directory.
#
# Aggregation reads every file in here and groups by the label baked into each
# result, so leftovers are not inert: an interrupted run leaves behind a fully
# labelled file that silently becomes an extra repeat of that cell, shifting the
# median and collapsing the apparent spread. Previous results are archived
# rather than deleted, since they may be the only copy of a long run.
if [ -n "$(ls -A "$RESULTS_DIR" 2>/dev/null)" ]; then
    ARCHIVE_DIR="$BENCH_DIR/results/archive/$(date +%Y%m%d-%H%M%S)"
    mkdir -p "$ARCHIVE_DIR"
    mv "$RESULTS_DIR"/* "$ARCHIVE_DIR"/ 2>/dev/null || true
    echo "Archived previous matrix results to results/archive/$(basename "$ARCHIVE_DIR")"
fi

# PostgreSQL 18 relocated the data directory in the official image; see the
# comment on the volume mount in docker-compose.benchmark.yml.
data_mount_for() {
    if [ "$1" -ge 18 ]; then
        echo "/var/lib/postgresql"
    else
        echo "/var/lib/postgresql/data"
    fi
}

compose() {
    PG_VERSION="$PG_VERSION" \
    PG_PORT="$PG_PORT" \
    PG_DATA_MOUNT="$(data_mount_for "$PG_VERSION")" \
    PG_EXTRA_ARGS="${PG_EXTRA_ARGS:-}" \
    docker compose -f "$COMPOSE_FILE" "$@"
}

teardown() {
    # -v removes the named volume, which is what guarantees a cold start.
    compose down -v --remove-orphans >/dev/null 2>&1 || true
}

# Always leave the machine clean, including on Ctrl-C.
trap 'echo ""; echo "Interrupted - tearing down..."; teardown; exit 130' INT TERM

echo "════════════════════════════════════════════════════════════"
echo " Rustrak benchmark matrix"
echo "   Variants            :$VARIANTS"
echo "   Scenarios           : $SCENARIOS"
echo "   Repeats             : $REPEATS"
echo "════════════════════════════════════════════════════════════"
echo ""

# Build the tool once up front so compile time never lands inside a measured run.
echo "Building benchmark tool..."
(cd "$BENCH_DIR" && cargo build --release) || exit 1
BENCH_BIN="$BENCH_DIR/target/release/rustrak-bench"

TOTAL=0
FAILED=0

for VARIANT in $VARIANTS; do
    # <label>:<version>[:<extra args>] — the extra-args field may itself contain
    # colons, so only the first two fields are split off.
    LABEL="${VARIANT%%:*}"
    REST="${VARIANT#*:}"
    PG_VERSION="${REST%%:*}"
    if [ "$REST" = "$PG_VERSION" ]; then
        PG_EXTRA_ARGS=""
    else
        PG_EXTRA_ARGS="${REST#*:}"
    fi
    export PG_VERSION PG_EXTRA_ARGS

    echo ""
    echo "############################################################"
    echo "# Variant: $LABEL (PostgreSQL $PG_VERSION) ${PG_EXTRA_ARGS:+[$PG_EXTRA_ARGS]}"
    echo "############################################################"

    for SCENARIO in $SCENARIOS; do
        for REPEAT in $(seq 1 "$REPEATS"); do
            TOTAL=$((TOTAL + 1))
            RUN_NAME="${LABEL}-${SCENARIO}-r${REPEAT}"

            echo ""
            echo "────────────────────────────────────────────────────────────"
            echo " [$RUN_NAME]"
            echo "────────────────────────────────────────────────────────────"

            teardown

            echo "Starting PostgreSQL $PG_VERSION + server..."
            if ! compose up -d --wait >"$LOG_DIR/${RUN_NAME}-up.log" 2>&1; then
                echo "  environment failed to start (see $LOG_DIR/${RUN_NAME}-up.log)"
                compose logs --tail 50 >>"$LOG_DIR/${RUN_NAME}-up.log" 2>&1
                FAILED=$((FAILED + 1))
                continue
            fi

            # Confirm the engine actually running is the one requested — a stale
            # volume or an image cache surprise would otherwise silently mislabel
            # every result in this row.
            ACTUAL_PG=$(docker exec rustrak-postgres-bench \
                psql -U bench -d rustrak_bench -tAc "SHOW server_version" 2>/dev/null | tr -d ' ')
            echo "  PostgreSQL reports: ${ACTUAL_PG:-unknown}"
            case "$ACTUAL_PG" in
                "$PG_VERSION"*) ;;
                *)
                    echo "  version mismatch: wanted $PG_VERSION, got ${ACTUAL_PG:-nothing}"
                    FAILED=$((FAILED + 1))
                    continue
                    ;;
            esac

            echo "Preparing project and credentials..."
            if ! PG_PORT="$PG_PORT" bash "$BENCH_DIR/scripts/setup-benchmark.sh" \
                >"$LOG_DIR/${RUN_NAME}-setup.log" 2>&1; then
                echo "  setup failed (see $LOG_DIR/${RUN_NAME}-setup.log)"
                FAILED=$((FAILED + 1))
                continue
            fi

            # shellcheck disable=SC1090
            source "$BENCH_DIR/.bench-credentials"

            # Prefer the matrix-tuned config; fall back to scenarios/<name>.toml.
            SCENARIO_ARG="$SCENARIO"
            if [ -f "$BENCH_DIR/scenarios/matrix/${SCENARIO}.toml" ]; then
                SCENARIO_ARG="$BENCH_DIR/scenarios/matrix/${SCENARIO}.toml"
            fi

            echo "Running $SCENARIO..."
            if "$BENCH_BIN" \
                --scenario "$SCENARIO_ARG" \
                --server "$SERVER_URL" \
                --project-id "$PROJECT_ID" \
                --sentry-key "$SENTRY_KEY" \
                --api-token "$BENCH_API_TOKEN" \
                --postgres-url "$BENCH_POSTGRES_URL" \
                --container rustrak-server-bench \
                --postgres-container rustrak-postgres-bench \
                --label "$LABEL" \
                --repeat "$REPEAT" \
                --output "$RESULTS_DIR" \
                2>&1 | tee "$LOG_DIR/${RUN_NAME}.log"; then

                # The tool writes both <run_id>.json and latest.json. Rename the
                # run_id file to the matrix convention and drop latest.json, so
                # exactly one file per run remains — leaving all three would make
                # the aggregation count this run three times and collapse the
                # apparent spread between repeats to nothing.
                RUN_ID=$(grep -o '"run_id"[[:space:]]*:[[:space:]]*"[^"]*"' \
                    "$RESULTS_DIR/latest.json" 2>/dev/null | cut -d'"' -f4)

                if [ -n "$RUN_ID" ] && [ -f "$RESULTS_DIR/${RUN_ID}.json" ]; then
                    mv "$RESULTS_DIR/${RUN_ID}.json" "$RESULTS_DIR/${RUN_NAME}.json"
                    rm -f "$RESULTS_DIR/latest.json"
                    echo "  saved: results/matrix/${RUN_NAME}.json"
                else
                    echo "  could not locate result file for $RUN_NAME"
                    FAILED=$((FAILED + 1))
                fi
            else
                echo "  run failed (see $LOG_DIR/${RUN_NAME}.log)"
                FAILED=$((FAILED + 1))
            fi
        done
    done
done

teardown

echo ""
echo "════════════════════════════════════════════════════════════"
echo " Matrix complete: $((TOTAL - FAILED))/$TOTAL runs succeeded"
echo " Results: $RESULTS_DIR"
echo "════════════════════════════════════════════════════════════"
