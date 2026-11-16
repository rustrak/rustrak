#!/usr/bin/env bash
# The server the e2e suite runs against: the real Rust binary, a fresh SQLite
# file, and the dashboard's `dist/` served from the same origin, which is how
# Rustrak ships. Nothing is mocked.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
data="$here/.data"
rm -rf "$data"
mkdir -p "$data"

if [ ! -f "$here/../dist/index.html" ]; then
  echo "e2e: no dashboard build; run 'pnpm --filter @rustrak/dashboard build' first" >&2
  exit 1
fi

export HOST=127.0.0.1
export PORT="${E2E_PORT:-8790}"
export DATABASE_URL="sqlite://$data/rustrak.db"
export SOURCEMAP_STORAGE_PATH="$data/sourcemaps"
export RUSTRAK_DASHBOARD_DIR="$here/../dist"
export SESSION_SECRET_KEY="e2e0000000000000000000000000000000000000000000000000000000000000"
export CREATE_SUPERUSER="admin@e2e.test:e2e-password"
# Alerts in the suite go to a webhook the test itself listens on, at 127.0.0.1.
export RUSTRAK_ALERTS_ALLOW_PRIVATE_NETWORKS=true
export RUST_LOG="${RUST_LOG:-warn}"

cd "$here/../../server"
exec cargo run --quiet --locked --bin rustrak
