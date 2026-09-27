# Rustrak Server

The Rust API server. Actix-web 4, SQLx against SQLite (default) or PostgreSQL
(feature `postgres`), Tokio for background work. Root context: `/CLAUDE.md`.

## Layout

```
src/
├── main.rs, lib.rs, bootstrap.rs   entry, wiring, startup (superuser, migrations)
├── config.rs, logging.rs           env config and env_logger setup
├── error.rs                        AppError, built on thiserror
├── openapi.rs                      utoipa spec; bin/gen_openapi.rs writes it out
├── routes/                         HTTP handlers, one module per resource
├── services/                       business logic the handlers call
├── models/                         domain types and their SQLx mappings
├── db/                             pool, queries and migrations
├── ingest/                         Sentry envelope parsing and temp storage
├── digest/                         grouping and issue creation, off the hot path
├── workers/                        Tokio background tasks
├── auth/                           the three extractors
├── middleware/                     rate limiting, auth enforcement, tracing
└── pagination/                     cursor and offset helpers
```

## Ingestion is two-phase

1. **Ingest**, synchronous, target under 50ms: parse the envelope, validate,
   store raw, return 200. The SDK is never made to wait on grouping.
2. **Digest**, asynchronous: compute the fingerprint, create or update the
   Issue, store the event.

Events group by custom fingerprint if the SDK sent one, otherwise by exception
type plus the first line of the message plus transaction, with a message-based
and then a generic fallback. The algorithm is deterministic: same input, same
group, always.

## Three ways to authenticate

| Method | Who | How |
|---|---|---|
| Session | humans on the dashboard | email + password, Argon2id, httpOnly cookie |
| Bearer token | API and scripts | 40-char hex against `auth_tokens` |
| Sentry auth | SDKs on ingest routes | the key embedded in the DSN |

They are separate on purpose, the same way GitHub separates sessions from PATs.
Session auth is better UX for people; token auth is the standard for machines.

DSN format: `http://<sentry_key>@<host>/<project_id>`.

## It also serves the dashboard

`routes/dashboard.rs` mounts `apps/dashboard`'s compiled output at `/`, so one
process answers both the page and the API and the session cookie stays
first-party. `RUSTRAK_DASHBOARD_DIR` says where the build is (default
`./static`, which is where `scripts/bundle-dashboard.sh` puts it).

**It stays optional.** No `index.html` in that directory means nothing is
mounted and the server behaves exactly as it did before: the premise is that
the server alone is a complete product, and `cargo build` has to keep working
for anyone who never installs Node. `RUSTRAK_DASHBOARD=off` skips a build that
is present, which is how a published image runs API-only.

The one rule the fallback must not break: `API_PREFIXES` (`/api`, `/auth`,
`/health`, `/docs`, `/api-docs`) never answer with the application shell. An
unmatched path under one of them is a JSON 404 like any other error, because a
mistyped endpoint answering `200 text/html` makes `@rustrak/client` report a
schema failure against itself. `apps/dashboard/vite.config.ts` proxies the same
five prefixes in development; **the two lists must not drift.**

## Tests

```bash
cargo test                       # all three suites
cargo test --test unit_tests
cargo test --test integration_tests
cargo test --test e2e_tests
```

`tests/unit` is pure logic, `tests/integration` hits the database, `tests/e2e`
drives real HTTP. Shared fixtures live in `tests/common`. Server changes are
written test-first.

## Conventions

- `rustfmt` and `clippy` both clean before a commit.
- Errors are `AppError` via `thiserror`, never `unwrap` on a request path.
- Async everywhere; nothing blocking inside a handler.
- Logging through `log`, configured with `RUST_LOG`.

Targets: under 100MB idle, under 50ms P99 on ingestion, and a distroless image
under 20MB.
