# Rustrak Server

The Rust API server. Actix-web 4, SQLx against SQLite (default) or PostgreSQL
(feature `postgres`), Tokio for background work. Root context: `/CLAUDE.md`.

## Layout

```
src/
├── main.rs, lib.rs, bootstrap.rs   entry and startup (superuser, migrations)
├── app.rs                          the one assembly: state, app, workers
├── config.rs, logging.rs           every env read, and env_logger setup
├── error.rs                        AppError, built on thiserror
├── openapi.rs                      utoipa spec; bin/gen_openapi.rs writes it out
├── routes/                         HTTP handlers, one module per resource
├── services/                       business logic the handlers call
├── models/                         domain types and their SQLx mappings
├── db/                             pool, queries and migrations
├── ingest/                         envelope parsing, temp storage, the pipeline
├── digest/                         grouping, issue creation and session counts, off the hot path
├── workers/                        Tokio background tasks
├── mcp/                            the MCP tools served at /mcp
├── auth/                           the three extractors
├── middleware/                     CORS and telemetry (extractors authenticate)
└── pagination/                     the page contract and the event cursor
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
| Session | humans on the dashboard | email + password, Argon2id; the httpOnly cookie holds an opaque id, the row (its SHA-256) is in `sessions` |
| Bearer token | API and scripts | 40-char hex, stored as its SHA-256 in `auth_tokens` |
| Sentry auth | SDKs on ingest routes | the key embedded in the DSN |
| OAuth access token | MCP clients, `/mcp` only | `rto_` + hex, hashed in `oauth_grants` |

They are separate on purpose, the same way GitHub separates sessions from PATs.
Session auth is better UX for people; token auth is the standard for machines.

DSN format: `http://<sentry_key>@<host>/<project_id>`.

## Permissions (`src/models/rbac.rs`)

A person's access is set on one page: a role and where it applies, or
permissions of their own. Never put projects inside a role: that needs one role
per combination of permissions and projects.

- **The catalog is code**: `catalog!` lists every `Resource` with its module,
  scope (`Instance` or `Project`) and the `Action`s (read, create, update,
  delete) it accepts. Settings are one resource per area (`storage`,
  `integrations`, `mcp`, …). A new resource is one line there. Seeing a
  project is not in it: any permission on the project implies it. List only
  what an endpoint enforces: a permission nothing checks is a checkbox that
  does nothing.
- **Writing implies reading** the same resource (`Grants::allows`): a form
  cannot edit what it cannot load. Not `api-tokens`, where reading is everyone's
  tokens and creating is your own.
- **A role can mix workspace and project permissions.** A new install has no
  roles and code never names one; memberships from before roles existed migrate
  to ordinary, editable ones. Creating a project extends the creator's own roles
  to it, so `projects:create` alone is refused: they could not see the project.
- **Assignments say where**: `role_assignments(user, role, project_id)`. NULL
  reaches every project. A role's project permissions apply there; its workspace
  permissions apply wherever it is held. Grants only add up; nothing is granted
  by default.
- **Personal permissions are a role**: `roles.user_id` marks one person's own
  role, which `RoleService::list` leaves out, nobody else can be given, and goes
  with them. `PUT /api/users/{id}/assignments` writes it with their assignments.
- **Owners are `users.role = 'admin'`**, not a role: they hold everything,
  and only an owner makes, unmakes or deletes one.
- **One check**: `actor.require(resource, action, Some(project_id))` in the
  handler. Cannot see the project → 404; sees it but lacks the permission →
  403. Listings use `actor.visible_projects()`.
- **No escalation**: writing a role, assigning one, inviting with one or
  adding a project member all need `Access::covers` (you hold it, where it
  applies). Nobody edits their own assignments or project roles.
- **Tokens are created from a session only**, with `api-tokens:create`, never
  with another token and never with a permission the creator lacks, so a
  limited token cannot mint its way out.
- **Tokens** act as their owner narrowed by their own scope
  (`Access::capped`: permissions, projects, expiry). A token with no owner
  (legacy) holds everything.

## The MCP server at `/mcp`

`routes/mcp.rs` is the endpoint and its switch; `src/mcp/` holds the tools,
one module per area, mirroring `routes/`.

- **Off until an admin turns it on** (`installation.mcp_enabled`, `PUT
  /api/settings/mcp`). Off, `/mcp` is a 404.
- **Bearer token only**: an API token or an OAuth access token. The session
  cookie is refused, so a browser tab cannot call tools for its user. The
  tools act as the token's owner.
- **OAuth** (`routes/oauth.rs`, `services/oauth.rs`): Rustrak is its own
  authorization server, **only with `PUBLIC_URL`**: the issuer and the
  resource are named after it, never after a request's headers or the bind
  address. Without it discovery is a 404 and `/mcp` challenges with plain
  `Bearer`.
  - Clients identify with a Client ID Metadata Document, fetched only from
    public addresses (`is_public`, which also unwraps NAT64/6to4; IP pinned,
    no redirects, one 5 s budget). Redirects must be https, http on
    loopback (any port), or an app's own scheme; never `javascript:` and
    the like (`redirect_allowed`).
  - PKCE S256 is required; codes and tokens are stored as SHA-256 hashes; an
    access token is bound to `<PUBLIC_URL>/mcp` and refused everywhere else.
  - Refresh tokens rotate inside one write transaction, and every spent one
    is kept in `oauth_spent_refresh_tokens`: one presented again revokes the
    grant (RFC 9700 4.14.2).
  - `/oauth/authorize` is a dashboard page at `dashboard_url()` (consent every
    time), and the shell is served with `frame-ancestors 'none'` so it
    cannot be clickjacked. A request whose client or redirect does not
    verify is never redirected.
- **Stateless** (protocol `2026-07-28`, `rmcp`): one `POST` per call, nothing
  kept between calls, no session store.
- **A tool calls the REST handler**, in-process, with extractors built from its
  arguments (`web::Path::from`, `web::Query(a.rest)`, `web::Json(...)`).
  Access checks, validation and the response are the handler's own, so a tool
  cannot drift from the endpoint. Do not reimplement a handler in a tool.
- **A tool's arguments are the handler's own query or body type**, wrapped
  with its path ids (`InProject<T>`, `OnIssue<T>`, `OnMonitor<T>`), so those
  types derive `schemars::JsonSchema`. A field with no doc comment gets
  `#[schemars(description = "...")]`, which leaves `openapi.json` alone.
- **A handler a tool calls must return a `Send` future.** Await a value before
  starting `HttpResponse::Ok()`; an `actix_web::Error` argument needs a
  `Send` inner function (see `issues::delete_issues`).
- `tests/fixtures/mcp_tools.json` pins every tool's description and required
  arguments (`mcp_test.rs`), so adding or removing a tool updates it.

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
`/health`, `/docs`, `/api-docs`, `/metrics`, `/mcp`, `/.well-known`) never
answer with the application shell. An
unmatched path under one of them is a JSON 404 like any other error, because a
mistyped endpoint answering `200 text/html` makes `@rustrak/client` report a
schema failure against itself. `apps/dashboard/vite.config.ts` proxies the same
prefixes in development; **the two lists must not drift.**

## `openapi.json` is a contract

`@rustrak/client` is generated from it and validates every response with it,
so the spec has to say exactly what the handler sends. Regenerate it with
`cargo run --bin gen_openapi --features openapi`; CI fails when it is stale,
and `tests/integration/openapi_contract_test.rs` calls every `GET` and fails
when a real response disagrees with it.

- Every handler is listed in `paths(...)` in `openapi.rs`, every response
  declares its `body =`, and every status it can return is documented.
- Return a `ToSchema` struct, never `json!`: an inline body has no schema.
- utoipa leaves every `Option` field out of `required`, since only serde knows
  about `skip_serializing_if`. A response field that is always sent, as `null`
  when empty, needs `#[cfg_attr(feature = "openapi", schema(required))]`. The
  contract test fails on a `null` field that is not required.
- A `String` that only holds a fixed set of values gets
  `schema(value_type = SomeEnum)`, so the client types it as that set.
- Sentry event ids are not RFC 4122 UUIDs: give them `value_type = String`.
- Every `utoipa::path` declares `operation_id = "resource.method"`: it is the
  client method's name (`issues.list` is `client.issues.list()`), so changing
  it is a breaking change. The handler's doc comment's first sentence is the
  client README's description of it.

## Tests

```bash
cargo test                       # all three suites
cargo test --test unit_tests
cargo test --test integration_tests
cargo test --test e2e_tests
```

`tests/unit` is pure logic, `tests/integration` hits the database, `tests/e2e`
drives real HTTP. Shared fixtures live in `tests/common`; a test server is
`common::app(&pool)` (or `common::state` + `common::app_with` to swap one
piece), the same `app::app` main serves. Server changes are written
test-first.

**Crash tests** (`tests/e2e/crash_test.rs`) run with the rest: they start the
real binary, abort it at a `failpoint!("name")` (`RUSTRAK_FAILPOINT=name`),
restart it and assert every 200'd event is stored exactly once. The macro only
exists in test builds (`test-seams`). A new crash-safety boundary gets a
`failpoint!` and one test there.

## Conventions

- `rustfmt` and `clippy` both clean before a commit.
- Errors are `AppError` via `thiserror`, never `unwrap` on a request path.
- Async everywhere; nothing blocking inside a handler.
- Logging through `log`, configured with `RUST_LOG`.

Targets: under 100MB idle, under 50ms P99 on ingestion, and a distroless image
under 20MB.
