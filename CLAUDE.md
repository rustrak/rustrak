# Rustrak

Self-hosted error tracking, compatible with Sentry SDKs. A Rust API server with
a small memory footprint, which also hands out an optional dashboard compiled
to static files.

```
Sentry SDK  ──▶  Rustrak server  ──▶  PostgreSQL
(any app)        (Rust/Actix-web)
                  serves /api and, if
                  one was built, / too
```

**One process and one image.** The dashboard is a Vite SPA compiled into
`apps/server/static`; the same Actix instance that answers `/api/projects`
answers `/`. That puts the browser and the API on one origin, which is what
keeps the session cookie first-party and removes CORS from the dashboard's path.

It stays optional, and that is still the point: an image built with no
dashboard serves the API and nothing else, and `cargo build` works for anyone
who never installs Node.

## Layout

| Path | What |
|---|---|
| `apps/server` | Rust API server. The product. |
| `apps/dashboard` | `@rustrak/dashboard`, the SPA the server serves |
| `apps/docs` | Public documentation site (Nextra) |
| `packages/ui` | `@rustrak/ui`, the design system. Storybook only for now |
| `packages/client` | `@rustrak/client`, the TypeScript API client |
| `packages/mcp` | `@rustrak/mcp`, MCP server over the client |
| `packages/test-sentry` | CLI to send test events to a DSN |
| `packages/benchmarks` | Load and throughput benchmarks |

Each app and package has its own `CLAUDE.md` with its architecture and rules.
Read that one before working inside it.

## Commands

```bash
docker compose up -d postgres     # database
cd apps/server && cargo run       # server on :8000
pnpm dev                          # dashboard and docs

pnpm test                         # everything except the Rust side
(cd apps/server && cargo test)    # unit, integration and e2e
pnpm run ci                       # what CI runs: ci:web (turbo) then ci:rust (cargo)
```

## CI and releases

Nothing in `.github/workflows/` caches anything: every job compiles from
scratch, and the time of a cold run is the time. Speed comes from one runner
per concern instead. `ci.yml` runs `pnpm run ci:web` (turbo over the
JavaScript packages), `rust-lint` (rustfmt, clippy for both backends, a type
check of the benchmark crate), `rust-test` (the SQLite suites and the OpenAPI
drift check) and `postgres-e2e`, four runners in parallel. `docker-publish.yml`
builds each image once per architecture on a native runner and merges the
digests into one manifest list.

- **Nothing in CI builds a release binary.** `cargo build --release` with fat
  LTO and one codegen unit is ten minutes of single-threaded work that no PR
  check reads. It runs only inside `docker build`, once per image and
  architecture, at release time.
- **CI pins the Rust the images are built with.** `RUST_TOOLCHAIN` in
  `ci.yml` and `release.yml`, and the `cargo-chef` image tag in
  `apps/server/Dockerfile`; bump them together.
- **Turbo never caches a cargo task** (`cache: false` in `turbo.json`), and
  cargo tasks do not run in parallel under turbo: they queue on the target
  directory lock. CI calls cargo directly for that reason.
- **The two crates stay separate.** `apps/server` and `packages/benchmarks`
  cannot share a Cargo workspace: `testcontainers` pins `bollard 0.20` and the
  benchmarks need `bollard 0.21`, and their `bollard-stubs` `=` pins conflict
  in one lockfile.

First run needs a superuser and a session key:

```bash
openssl rand -hex 32              # put in .env as SESSION_SECRET_KEY
CREATE_SUPERUSER="admin@example.com:password" cargo run
```

## Conventions

- Rust goes through `rustfmt` and `clippy`. TypeScript through Biome.
- Commit messages are conventional and in English: `type: description`.
- Tests come with the change, not after it.
- `scripts/bundle-dashboard.sh` is the seam between the two build systems: the
  dashboard owns `dist/`, the server owns `static/`, and the script copies one
  to the other. A missing build is not an error anywhere in that chain.

## Versioning

`@rustrak/server`, `@rustrak/dashboard`, `@rustrak/client` and `@rustrak/mcp` are a
`fixed` group in `.changeset/config.json`. They always share one number, even
when a package has no changes, because that number identifies the **Rustrak
release** rather than the semver of any single artifact. It is what lets someone
answer "which version am I running?" and makes `@rustrak/client@X.Y.Z`
known-compatible with server `X.Y.Z` without a compatibility matrix.

- A changeset only needs to name `@rustrak/server`; the group propagates it.
- The group takes the highest bump present.
- **While on `0.x`, never write a `major` changeset.** Use `minor` for breaking
  changes. A `major` would push the product to 1.0.0 as a side effect.
- `apps/docs` sits outside the group and bumps only when a changeset names it.
- `scripts/sync-version.sh` copies the version into `Cargo.toml` afterwards.
