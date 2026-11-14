#!/usr/bin/env bash
# The Linux A/B lab: builds `main` (baseline) and the working tree (candidate)
# inside a Linux container, then runs `rustrak-bench lab` there, which starts
# each server as a child process and reads its memory from /proc.
#
#   scripts/lab.sh                          # ingest, SQLite, 3 rounds main vs wt
#   scripts/lab.sh --scenario backlog --runs 1
#   scripts/lab.sh --postgres               # ingest against a postgres:16 container
#   scripts/lab.sh --no-baseline --also arena:MALLOC_ARENA_MAX=2
#   scripts/lab.sh --release                # the repo's real release profile (LTO)
#   scripts/lab.sh -- --duration 30 --strace   # anything after -- goes to `lab`
#
# Everything persists in Docker volumes named rustrak-bench-* (cargo registry,
# one target dir per tree, the binaries, the run data, the mixed-scenario seed).
# `scripts/lab.sh --wipe` removes them and the toolchain image.
set -euo pipefail

REPO=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
RUST=$(sed -n 's/^channel *= *"\(.*\)"/\1/p' "$REPO/rust-toolchain.toml")
IMAGE="rustrak-bench-toolchain:$RUST"
NET=rustrak-bench-net
PG=rustrak-bench-pg
RESULTS="$REPO/packages/benchmarks/results/lab"

SCENARIO=ingest RUNS=3 BACKEND=sqlite PROFILE=lab BASELINE=1 TMPFS=0 BUILD_ONLY=0
ALSO=() LAB_ARGS=()
while [ $# -gt 0 ]; do
    case "$1" in
        --scenario) SCENARIO=$2; shift 2 ;;
        --runs) RUNS=$2; shift 2 ;;
        --postgres) BACKEND=postgres; shift ;;
        --release) PROFILE=release; shift ;;
        --no-baseline) BASELINE=0; shift ;;
        --also) ALSO+=("$2"); shift 2 ;;
        --tmpfs) TMPFS=1; shift ;;
        --build-only) BUILD_ONLY=1; shift ;;
        --wipe)
            docker rm -f "$PG" >/dev/null 2>&1 || true
            docker volume ls -q --filter name=rustrak-bench- | xargs -r docker volume rm
            docker network rm "$NET" >/dev/null 2>&1 || true
            docker image rm "$IMAGE" 2>/dev/null || true
            exit 0 ;;
        --) shift; LAB_ARGS=("$@"); break ;;
        *) echo "unknown option $1 (see the header of $0)" >&2; exit 2 ;;
    esac
done

NCPU=$(docker info --format '{{.NCPU}}')
SERVER_CPUS=${SERVER_CPUS:-0-3}
if [ -z "${LOAD_CPUS:-}" ] && [ "$NCPU" -lt 5 ]; then
    echo "Docker has $NCPU CPUs: the defaults need 5 or more; set SERVER_CPUS and LOAD_CPUS" >&2
    exit 1
fi
LOAD_CPUS=${LOAD_CPUS:-4-$((NCPU - 1))}
JOBS=${CARGO_BUILD_JOBS:-$NCPU}

# ── Toolchain image: the repo's Rust plus strace ────────────────────────────
if ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
    echo "building $IMAGE"
    docker build -q -t "$IMAGE" - <<EOF
FROM rust:${RUST}-slim-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends build-essential strace procps \
    && rm -rf /var/lib/apt/lists/*
EOF
fi

in_box() {
    docker run --rm -i \
        -v rustrak-bench-cargo:/usr/local/cargo/registry \
        -v rustrak-bench-src:/src \
        -v rustrak-bench-target-main:/target/main \
        -v rustrak-bench-target-wt:/target/wt \
        -v rustrak-bench-target-tools:/target/tools \
        -v rustrak-bench-bin:/bins \
        -e CARGO_BUILD_JOBS="$JOBS" \
        "$IMAGE" bash -euo pipefail -c "$1"
}

# ── Sources: main via git archive (only when main moved), the working tree
#    copied as it is now, so a run measures one fixed snapshot ──────────────
MAIN_SHA=$(git -C "$REPO" rev-parse main)
if [ "$BASELINE" = 1 ]; then
    if [ "$(in_box 'cat /src/main/.sha 2>/dev/null || true')" != "$MAIN_SHA" ]; then
        echo "baseline: main @ ${MAIN_SHA:0:9}"
        git -C "$REPO" archive main apps/server |
            in_box "rm -rf /src/main && mkdir -p /src/main && tar -x -C /src/main && echo $MAIN_SHA > /src/main/.sha"
    fi
fi
copy_tree() { # <repo-relative dir> <dest under /src>
    (cd "$REPO" && COPYFILE_DISABLE=1 tar --no-xattrs -cf - \
        --exclude="$1/target" --exclude="$1/static" --exclude="$1/results" --exclude="$1/.env" "$1") |
        in_box "rm -rf /src/$2.new && mkdir -p /src/$2.new && tar -x -C /src/$2.new 2>/dev/null \
                && rm -rf /src/$2 && mv /src/$2.new /src/$2"
}
copy_tree apps/server wt
copy_tree packages/benchmarks tools
WT_DIFF=$(git -C "$REPO" diff main --stat -- apps/server | tail -1 | sed 's/^ *//')

# ── Builds (incremental: the target dirs live in volumes) ──────────────────
if [ "$PROFILE" = lab ]; then
    # A fast release: no LTO, 16 codegen units, incremental. For A/B work;
    # --release gives the shipped profile for final numbers.
    PROFILE_ARGS="--config 'profile.lab.inherits=\"release\"' --config profile.lab.lto=false \
        --config profile.lab.codegen-units=16 --config profile.lab.incremental=true --profile lab"
else
    PROFILE_ARGS="--release"
fi
FEATURES=""
[ "$BACKEND" = postgres ] && FEATURES="--no-default-features --features postgres"
TREES="wt"
[ "$BASELINE" = 1 ] && TREES="main wt"
build_cmd=""
for tree in $TREES; do
    # `;` not `&&` before cp: a failed build must end the list so `-e` stops
    # here instead of leaving last run's binary in /bins to be measured.
    build_cmd+="cd /src/$tree/apps/server && CARGO_TARGET_DIR=/target/$tree cargo build --locked -q $PROFILE_ARGS $FEATURES --bin rustrak; \
        cp /target/$tree/$PROFILE/rustrak /bins/$tree-$BACKEND-$PROFILE; "
done
build_cmd+="cd /src/tools/packages/benchmarks && CARGO_TARGET_DIR=/target/tools cargo build --locked -q \
    --config 'profile.lab.inherits=\"release\"' --config profile.lab.lto=false --config profile.lab.codegen-units=16 --profile lab"
echo "building ($PROFILE profile, $BACKEND): $TREES + load generator"
start=$(date +%s)
in_box "$build_cmd"
echo "built in $(($(date +%s) - start)) s"
[ "$BUILD_ONLY" = 1 ] && exit 0

# ── Run ─────────────────────────────────────────────────────────────────────
VARIANTS=()
[ "$BASELINE" = 1 ] && VARIANTS+=(--variant "main=/bins/main-$BACKEND-$PROFILE")
VARIANTS+=(--variant "wt=/bins/wt-$BACKEND-$PROFILE")
for spec in ${ALSO[@]+"${ALSO[@]}"}; do # name:K=V[,K=V]
    name=${spec%%:*}
    VARIANTS+=(--variant "$name=/bins/wt-$BACKEND-$PROFILE")
    IFS=, read -ra kvs <<<"${spec#*:}"
    for kv in "${kvs[@]}"; do VARIANTS+=(--env "$name:$kv"); done
done
if [ "$SCENARIO" = mixed ]; then
    for tree in $TREES; do
        key=$(in_box "ls /src/$tree/apps/server/migrations/sqlite | sha256sum | cut -c1-12")
        VARIANTS+=(--seed "$tree=/seed/$tree-$key")
    done
    # An --also variant runs the working tree's binary on its seed.
    for spec in ${ALSO[@]+"${ALSO[@]}"}; do
        VARIANTS+=(--seed "${spec%%:*}=/seed/wt-$key")
    done
fi

DOCKER_ARGS=(--rm --name rustrak-bench-lab --cap-add SYS_PTRACE
    -v rustrak-bench-bin:/bins:ro -v rustrak-bench-target-tools:/target/tools:ro
    -v rustrak-bench-seed:/seed -v "$RESULTS:/results")
if [ "$TMPFS" = 1 ]; then
    DOCKER_ARGS+=(--tmpfs /data:size=8g) # CPU-only control: no disk at all
else
    DOCKER_ARGS+=(-v rustrak-bench-data:/data)
fi
PG_ARGS=()
if [ "$BACKEND" = postgres ]; then
    docker network inspect "$NET" >/dev/null 2>&1 || docker network create "$NET" >/dev/null
    if ! docker inspect "$PG" >/dev/null 2>&1; then
        docker run -d --name "$PG" --network "$NET" -e POSTGRES_PASSWORD=bench \
            -v rustrak-bench-pgdata:/var/lib/postgresql/data postgres:16 >/dev/null
    fi
    docker start "$PG" >/dev/null # a no-op unless a Docker restart stopped it
    for _ in $(seq 60); do
        docker exec "$PG" pg_isready -U postgres >/dev/null 2>&1 && break
        sleep 1
    done
    docker exec "$PG" pg_isready -U postgres >/dev/null 2>&1 ||
        { docker logs --tail 20 "$PG" >&2; echo "$PG not ready after 60 s" >&2; exit 1; }
    DOCKER_ARGS+=(--network "$NET")
    PG_ARGS=(--postgres "postgres://postgres:bench@$PG:5432")
fi
mkdir -p "$RESULTS"
docker run "${DOCKER_ARGS[@]}" "$IMAGE" \
    taskset -c "$LOAD_CPUS" /target/tools/lab/rustrak-bench lab \
    --scenario "$SCENARIO" --runs "$RUNS" --server-cpus "$SERVER_CPUS" \
    --results-dir /results "${VARIANTS[@]}" ${PG_ARGS[@]+"${PG_ARGS[@]}"} \
    --meta "main=$MAIN_SHA" --meta "wt_diff=$WT_DIFF" --meta "profile=$PROFILE" \
    --meta "backend=$BACKEND" --meta "tmpfs=$TMPFS" --meta "cpus=server:$SERVER_CPUS,load:$LOAD_CPUS" \
    ${LAB_ARGS[@]+"${LAB_ARGS[@]}"}
[ "$BACKEND" = postgres ] && docker rm -f "$PG" >/dev/null
exit 0
