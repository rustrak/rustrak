#!/usr/bin/env bash
set -euo pipefail

# Sync version from apps/server/package.json to apps/server/Cargo.toml
# Run after `changeset version` to keep Cargo.toml in sync.

SERVER_DIR="$(cd "$(dirname "$0")/../apps/server" && pwd)"
PACKAGE_JSON="$SERVER_DIR/package.json"
CARGO_TOML="$SERVER_DIR/Cargo.toml"

if [ ! -f "$PACKAGE_JSON" ]; then
  echo "ERROR: $PACKAGE_JSON not found"
  exit 1
fi

VERSION=$(node -p "require('$PACKAGE_JSON').version")

if [ -z "$VERSION" ]; then
  echo "ERROR: could not read version from $PACKAGE_JSON"
  exit 1
fi

perl -pi -e "s/^version = \".+\"/version = \"$VERSION\"/" "$CARGO_TOML"

if ! grep -Eq "^version = \"$VERSION\"$" "$CARGO_TOML"; then
  echo "ERROR: failed to update version in $CARGO_TOML"
  exit 1
fi

# Refresh the crate's own entry in Cargo.lock and nothing else. This used to be
# `cargo generate-lockfile`, which throws the lockfile away and resolves every
# dependency to its newest compatible version: each version PR silently
# upgraded the whole dependency tree, in the one PR CI does not run on.
cd "$SERVER_DIR" && cargo update --workspace
# The spec carries the version in `info.version`.
cd "$SERVER_DIR" && cargo run --bin gen_openapi --features openapi

# The plugin SDK crates are published with the release's number too, the
# macros crate's version pinned where rustrak-plugin depends on it.
SDK_DIR="$SERVER_DIR/../../packages/rustrak-plugin"
perl -pi -e "s/^version = \".+\"/version = \"$VERSION\"/" \
  "$SDK_DIR/Cargo.toml" "$SDK_DIR/macros/Cargo.toml"
perl -pi -e "s/(rustrak-plugin-macros = \{ path = \"macros\", version = )\".+\"/\$1\"$VERSION\"/" \
  "$SDK_DIR/Cargo.toml"
if ! grep -q "rustrak-plugin-macros = { path = \"macros\", version = \"$VERSION\" }" "$SDK_DIR/Cargo.toml"; then
  echo "ERROR: failed to update the SDK's version in $SDK_DIR/Cargo.toml"
  exit 1
fi
cd "$SDK_DIR" && cargo update --workspace
cd "$SERVER_DIR/tests/fixtures/hello-plugin" && cargo update -p rustrak-plugin -p rustrak-plugin-macros

echo "Synced version $VERSION to Cargo.toml"