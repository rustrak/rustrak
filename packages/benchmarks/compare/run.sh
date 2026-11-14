#!/usr/bin/env bash
# Rustrak against other Sentry-SDK-compatible servers, each run the way its own
# documentation installs it, all on the same Docker host under the same load.
#
#   compare/run.sh rustrak            # one product
#   compare/run.sh rustrak bugsink glitchtip rustrak-pg sentry
#   EVENTS=20000 IDLE=60 FLOOR=60 compare/run.sh bugsink
#
# Products: rustrak (SQLite), rustrak-pg, bugsink (PostgreSQL), bugsink-sqlite,
# glitchtip, sentry,
# sentry-errors (Sentry's official `errors-only` profile).
#
# Per product: fresh volumes, stack up, one project created through the
# product's own CLI or API, every container pinned to SERVER_CPUS, the load
# generator pinned to LOAD_CPUS. Memory is cgroup `anon` (no page cache) summed
# over every container of the stack, sampled once a second. Results land in
# results/compare/<run>/<product>.json.
set -euo pipefail

HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(git -C "$HERE" rev-parse --show-toplevel)
RUST=$(sed -n 's/^channel *= *"\(.*\)"/\1/p' "$REPO/rust-toolchain.toml")
TOOLCHAIN="rustrak-bench-toolchain:$RUST"
RUN=${RUN:-$(date +%Y%m%d-%H%M%S)}
OUT="$REPO/packages/benchmarks/results/compare/$RUN"
EVENTS=${EVENTS:-20000}
CONCURRENCY=${CONCURRENCY:-16}
IDLE=${IDLE:-60}
FLOOR=${FLOOR:-60}
DRAIN_TIMEOUT=${DRAIN_TIMEOUT:-1800}
SERVER_CPUS=${SERVER_CPUS:-0-3}
LOAD_CPUS=${LOAD_CPUS:-4-$(($(docker info --format '{{.NCPU}}') - 1))}
SENTRY_TAG=${SENTRY_TAG:-26.10.1}
SENTRY_DIR=${SENTRY_DIR:-$HOME/.cache/rustrak-compare/sentry-self-hosted-$SENTRY_TAG}
SAMPLER=cmp-sampler
mkdir -p "$OUT"

log() { printf '\033[36m[%s]\033[0m %s\n' "$(date +%H:%M:%S)" "$*" >&2; }

# ── Load generator: rustrak-bench, built once into a volume ────────────────
build_bench() {
    docker image inspect "$TOOLCHAIN" >/dev/null 2>&1 || docker build -q -t "$TOOLCHAIN" - <<EOF
FROM rust:${RUST}-slim-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends build-essential strace procps \
    && rm -rf /var/lib/apt/lists/*
EOF
    (cd "$REPO" && COPYFILE_DISABLE=1 tar --no-xattrs -cf - --exclude=packages/benchmarks/target \
        --exclude=packages/benchmarks/results packages/benchmarks) |
        docker run --rm -i -v rustrak-bench-cargo:/usr/local/cargo/registry \
            -v rustrak-bench-cmp-target:/target "$TOOLCHAIN" bash -euo pipefail -c \
            "rm -rf /src && mkdir /src && tar -x -C /src && cd /src/packages/benchmarks && \
             CARGO_TARGET_DIR=/target cargo build --locked -q --release"
}

# ── Products: compose files, readiness, project setup, stored-event count ──
compose() { # <product> <docker compose args...>
    local p=$1; shift
    case "$p" in
        rustrak) docker compose -p cmp-rustrak -f "$HERE/rustrak/compose.yml" "$@" ;;
        rustrak-pg) docker compose -p cmp-rustrak-pg -f "$HERE/rustrak/compose.postgres.yml" "$@" ;;
        bugsink|glitchtip) docker compose -p "cmp-$p" -f "$HERE/$p/compose.yml" "$@" ;;
        bugsink-sqlite) docker compose -p cmp-bugsink-sqlite -f "$HERE/bugsink/compose.sqlite.yml" "$@" ;;
        sentry|sentry-errors)
            (cd "$SENTRY_DIR" && COMPOSE_PROFILES=$([ "$p" = sentry-errors ] && echo errors-only || echo feature-complete) \
                docker compose "$@") ;;
    esac
}
project_of() { case "$1" in sentry*) echo sentry-self-hosted ;; *) echo "cmp-$1" ;; esac; }
network_of() { echo "$(project_of "$1")_default"; }
base_url() {
    case "$1" in
        rustrak|rustrak-pg) echo http://server:8080 ;;
        bugsink|bugsink-sqlite|glitchtip) echo http://web:8000 ;;
        sentry*) echo http://nginx:80 ;;
    esac
}
health_path() {
    case "$1" in
        rustrak|rustrak-pg) echo /health ;;
        bugsink|bugsink-sqlite) echo /health/ready ;;
        glitchtip) echo /_health/ ;;
        sentry*) echo /_health/ ;;
    esac
}
on_net() { docker run --rm --network "$(network_of "$1")" curlimages/curl "${@:2}"; }

install_sentry() {
    [ -d "$SENTRY_DIR" ] || git clone -q --depth 1 --branch "$SENTRY_TAG" \
        https://github.com/getsentry/self-hosted.git "$SENTRY_DIR"
    local profile=feature-complete
    [ "$1" = sentry-errors ] && profile=errors-only
    # The installer is idempotent; it pulls, migrates and writes its .env. It
    # needs bash >= 4.4, which macOS lacks, so it runs in a Linux container
    # against the host's Docker, with the checkout at the same path so the
    # compose bind mounts resolve on the host.
    docker run --rm -v /var/run/docker.sock:/var/run/docker.sock -v "$SENTRY_DIR:$SENTRY_DIR" \
        -w "$SENTRY_DIR" -e COMPOSE_PROFILES=$profile docker:29-cli sh -c \
        'apk add -q bash git coreutils curl >/dev/null && ./install.sh --skip-user-creation \
            --no-report-self-hosted-issues --skip-commit-check' >"$OUT/$1-install.log" 2>&1 ||
        { log "$1: install failed, see $OUT/$1-install.log"; return 1; }
}

setup_project() { # prints "<project id> <key>"
    case "$1" in
        rustrak|rustrak-pg)
            local tok
            tok=$(docker logs "$(project_of "$1")-server-1" 2>&1 | sed -n 's/^Token: //p' | head -1)
            on_net "$1" -s -X POST http://server:8080/api/projects -H "Authorization: Bearer $tok" \
                -H 'Content-Type: application/json' -d '{"name":"bench"}' |
                python3 -I -c 'import json,sys; d=json.load(sys.stdin); print(d["id"], d["sentry_key"].replace("-",""))' ;;
        bugsink|bugsink-sqlite)
            docker exec "cmp-$1-web-1" bugsink-manage shell -c \
                'from projects.models import Project; p=Project.objects.create(name="bench", retention_max_event_count=100_000_000); print("DSN", p.id, p.sentry_key.hex)' \
                2>/dev/null | sed -n 's/^DSN //p' ;;
        glitchtip)
            docker exec cmp-glitchtip-web-1 ./manage.py shell -c '
from apps.organizations_ext.models import Organization
from apps.projects.models import Project, ProjectKey
org = Organization.objects.create(name="bench", slug="bench")
p = Project.objects.create(name="bench", organization=org)
k = ProjectKey.objects.filter(project=p).first() or ProjectKey.objects.create(project=p)
print("DSN", p.id, k.public_key.hex)' 2>/dev/null | sed -n 's/^DSN //p' ;;
        sentry*)
            # The installer creates organization "sentry" and project 1 ("internal").
            compose "$1" exec -T web sentry exec -c '
from sentry.models.projectkey import ProjectKey
print("DSN", 1, ProjectKey.objects.filter(project_id=1).first().public_key)' 2>/dev/null | sed -n 's/^DSN //p' ;;
    esac
}

stored_events() {
    case "$1" in
        rustrak) docker run --rm -v cmp-rustrak_data:/d:ro keinos/sqlite3 \
            sqlite3 -readonly /d/rustrak.db 'select count(*) from events' 2>/dev/null || echo 0 ;;
        rustrak-pg) docker exec cmp-rustrak-pg-postgres-1 psql -U rustrak -tAc 'select count(*) from events' ;;
        # WAL, and the files belong to Bugsink's user: a reader needs root and a
        # writable mount for the -shm file.
        bugsink-sqlite) docker run --rm --user 0 -v cmp-bugsink-sqlite_data:/d keinos/sqlite3 \
            sqlite3 /d/db.sqlite3 'select count(*) from events_event' 2>/dev/null || echo 0 ;;
        bugsink) docker exec cmp-bugsink-db-1 psql -U bugsinkuser -d bugsink -tAc 'select count(*) from events_event' ;;
        glitchtip) docker exec cmp-glitchtip-postgres-1 psql -U postgres -tAc 'select count(*) from issue_events_issueevent' ;;
        sentry*) compose "$1" exec -T clickhouse clickhouse-client -q 'select count() from errors_local' ;;
    esac
}

containers_of() { docker ps -q --no-trunc --filter "label=com.docker.compose.project=$(project_of "$1")"; }

# ── Memory and CPU sampler: one helper reading every container's cgroup ────
start_sampler() { # <product>
    docker rm -f "$SAMPLER" >/dev/null 2>&1 || true
    docker run -d --name "$SAMPLER" --cpuset-cpus "$LOAD_CPUS" -v /sys/fs/cgroup:/h:ro \
        -v "$OUT:/out" -e IDS="$(containers_of "$1" | tr '\n' ' ')" -e P="$1" alpine sh -c '
        while :; do
            t=$(date +%s); anon=0; cpu=0; n=0
            for id in $IDS; do
                [ -r /h/docker/$id/memory.stat ] || continue
                a=$(awk "/^anon /{print \$2}" /h/docker/$id/memory.stat)
                c=$(awk "/^usage_usec /{print \$2}" /h/docker/$id/cpu.stat)
                anon=$((anon + a)); cpu=$((cpu + c)); n=$((n + 1))
            done
            echo "$t,$anon,$cpu,$n" >> /out/$P-mem.csv
            sleep 1
        done' >/dev/null
}
mark() { echo "$(date +%s),$2" >>"$OUT/$1-phases.csv"; }

measure() { # <product>
    local p=$1 id key accepted stored last same t0
    log "$p: fresh stack"
    # A rerun into the same RUN starts clean: the CSVs below are appended to.
    rm -rf "$OUT/$p"-* "$OUT/$p.json"
    compose "$p" down -v --remove-orphans >/dev/null 2>&1 || true
    case "$p" in sentry*)
        # Sentry keeps its data in external volumes that `down -v` leaves;
        # the installer recreates them empty.
        docker volume ls -q | grep '^sentry-' | xargs -r docker volume rm >/dev/null
        install_sentry "$p" ;;
    esac
    t0=$(date +%s)
    compose "$p" up -d --quiet-pull >"$OUT/$p-up.log" 2>&1
    local ready=0
    for _ in $(seq 300); do
        if [ "$(on_net "$p" -s -o /dev/null -w '%{http_code}' "$(base_url "$p")$(health_path "$p")" 2>/dev/null)" = 200 ]; then
            ready=1; break
        fi
        sleep 2
    done
    [ "$ready" = 1 ] || { log "$p: not healthy after 10 min"; compose "$p" logs --tail 30 >&2; return 1; }
    echo "$(($(date +%s) - t0))" >"$OUT/$p-ready-secs"
    read -r id key < <(setup_project "$p")
    [ -n "${key:-}" ] || { log "$p: no project key"; return 1; }
    log "$p: ready in $(cat "$OUT/$p-ready-secs") s, project $id"
    # shellcheck disable=SC2046
    docker update --cpuset-cpus "$SERVER_CPUS" $(containers_of "$p") >/dev/null

    start_sampler "$p"
    mark "$p" idle
    sleep "$IDLE"

    cat >"$OUT/$p-scenario.toml" <<EOF
name = "compare"
description = "One burst of $EVENTS events"
scenario_type = "burst"
duration_secs = 0
target_rps = 0
concurrency = $CONCURRENCY
warmup_secs = 0
[burst]
burst_size = $EVENTS
pause_secs = 0
cycles = 1
[event]
breadcrumb_count = 30
stack_depth = 26
include_user = true
include_tags = true
include_extra = true
distinct_groups = 2000
EOF
    # Count only what this run stores (a product's own bootstrap may add events).
    local base
    base=$({ stored_events "$p" || true; } | tr -dc 0-9)
    mark "$p" load
    docker run --rm --network "$(network_of "$p")" --cpuset-cpus "$LOAD_CPUS" \
        -v rustrak-bench-cmp-target:/target:ro -v "$OUT:/out" "$TOOLCHAIN" \
        /target/release/rustrak-bench --server "$(base_url "$p")" --project-id "$id" --sentry-key "$key" \
        --scenario "/out/$p-scenario.toml" --no-wait --no-warmup -o "/out/$p-load" \
        >"$OUT/$p-load.log" 2>&1 || true
    mark "$p" drain
    log "$p: load sent, waiting for the stored count to settle"
    last=-1 same=0
    for _ in $(seq $((DRAIN_TIMEOUT / 2))); do
        stored=$({ stored_events "$p" || true; } | tr -dc 0-9)
        stored=$((${stored:-0} - ${base:-0}))
        echo "$(date +%s),${stored:-0}" >>"$OUT/$p-stored.csv"
        if [ "${stored:-0}" = "$last" ]; then same=$((same + 1)); else same=0; fi
        [ "$same" -ge 15 ] && break # 30 s without a new stored event
        last=${stored:-0}
        sleep 2
    done
    mark "$p" floor
    sleep "$FLOOR"
    mark "$p" end
    docker rm -f "$SAMPLER" >/dev/null

    docker system df -v --format '{{json .}}' >"$OUT/$p-df.json"
    # shellcheck disable=SC2046
    docker inspect --format '{{.Config.Image}}' $(containers_of "$p") | sort -u >"$OUT/$p-images.txt"
    containers_of "$p" | wc -l | tr -d ' ' >"$OUT/$p-containers"
    python3 -I "$HERE/summarize.py" "$OUT" "$p"
    compose "$p" down -v --remove-orphans >/dev/null 2>&1 || true
}

build_bench
for product in "$@"; do measure "$product"; done
log "results: $OUT"
