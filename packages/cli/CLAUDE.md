# `rustrak` CLI

The command-line client for a Rustrak instance. Triage issues, read logs and
agent traces, check release health, and manage projects from a terminal — all
over the same REST API the dashboard uses.

```
                        ┌──────────────────┐
  rustrak CLI  ────────▶│  Rustrak server  │────▶ SQLite / PostgreSQL
  (bearer token)        │  /api/*          │
                        └──────────────────┘
```

Root context: `/CLAUDE.md`. The API contract this crate reads is
`apps/server/openapi.json`; every response type in `src/api.rs` mirrors a schema
there and ignores fields it does not name, so a newer server does not break an
older CLI.

## Authenticate once

The server mints API tokens from a **session**, never from a token
(`POST /api/tokens` refuses a bearer caller). So `auth login` does the two-step
the dashboard does, without a browser:

```
POST /auth/login   (email + password) ──▶ session cookie
POST /api/tokens   (that cookie)      ──▶ 40-hex bearer token, shown once
```

Only the token is stored; the session is short-lived and thrown away. That is
why this crate depends on `reqwest` with the `cookies` feature: the login
client needs a cookie jar, and every other command does not.

```bash
rustrak auth login --url https://rustrak.example.com
# Email: you@example.com
# Password:            (not echoed)
```

For an instance with SSO-only sign-in, mint a token in **Settings → API
tokens** and hand it over instead:

```bash
rustrak auth login --url https://rustrak.example.com --token <TOKEN>
```

## Configuration

One file, `~/.config/rustrak/config.toml` (or the platform's config
directory), written `0600` on unix:

```toml
current = "default"

[profiles.default]
url = "https://rustrak.example.com"
token = "…"
```

Flags beat the environment, which beats the file:

| | Flag | Environment | File |
|---|---|---|---|
| Instance | `--url` | `RUSTRAK_URL` | `profiles.<p>.url` |
| Token | `--token` | `RUSTRAK_TOKEN` | `profiles.<p>.token` |
| Profile | `-p, --profile` | `RUSTRAK_PROFILE` | `current` |
| Project | `--project` | `RUSTRAK_PROJECT` | — |

The environment variables are the whole config for CI: set `RUSTRAK_URL` and
`RUSTRAK_TOKEN` and no file is read.

## Output

A terminal gets a table; a pipe gets JSON, because a table parsed by `jq` is
worthless and a user redirecting to a file usually wants the data. `--json`
forces JSON, `--table` forces a table, `NO_COLOR` turns colour off.

```bash
rustrak issue list                 # table, if stdout is a terminal
rustrak issue list --json | jq .   # JSON, always
```

Exit codes: `0` ok, `1` a general failure, `2` a usage error (clap), `3` not
authenticated, `4` forbidden, `5` not found.

## Commands

| Group | What it does |
|---|---|
| `auth` | `login`, `status`, `whoami`, `logout` |
| `config` | `list`, `show`, `use`, `set`, `remove`, `path` |
| `project` | `list`, `get`, `create`, `update`, `delete`, `stats`, `sessions`, `envs`, `dsn`, `use` |
| `issue` | `list`, `get`, `resolve`, `unresolve`, `ignore`, `priority`, `events`, `event`, `activity`, `comment`, `seen`, `tags`, `delete`, `stats` |
| `event` | `get` |
| `log` | `list`, `tail`, `get` |
| `trace` | `summary`, `list`, `models`, `tools` |
| `perf` | `list`, `stats` |
| `release` | `list`, `get`, `create`, `delete` |
| `monitor` | `list`, `get`, `checkins`, `mute` |
| `alert` | `list` |
| `team` | `list` |
| `token` | `list`, `create`, `revoke` |
| `send` | send a test event to a DSN, as an SDK would |
| `api` | call any endpoint the commands do not wrap yet |
| `version`, `health` | the instance's version, and whether it answers |
| `completion` | a shell completion script |

Issues are addressed by UUID or by the `PROJECT-12` short id the dashboard
shows; `issue` commands resolve the short id to a UUID first.

Two clap constraints are load-bearing and easy to reintroduce, so
`tests/cli_tree.rs` pins them: a positional named `project` collides with the
global `--project` flag (both key on the same id), and one named `version`
collides with the auto-generated `--version`. They are `project_ref` and
`release_version`, with an explicit `#[arg(id = ...)]`.

## Layout

```
src/
├── main.rs       entry: parse, dispatch, map an Error to an exit code
├── lib.rs        the module tree
├── cli.rs        the clap tree: global flags and every subcommand
├── client.rs     the one place requests are made, and the error document
├── config.rs     the config file, profiles, and flag/env/file precedence
├── error.rs      Error, with the process exit code
├── output.rs     table vs JSON, and the formatting helpers
├── api.rs        response types (mirroring openapi.json) and path builders
└── commands/
    ├── mod.rs    Context: the resolved client, project and output format
    ├── dispatch.rs  the command -> module match
    └── *.rs      one module per command group
```

`commands/mod.rs`'s `Context` is built once and handed to every handler, so no
handler reads the environment or resolves a project reference twice.

## Tests

```bash
cargo test                     # unit tests, in each module
cargo clippy --all-targets -- -D warnings
cargo fmt -- --check
```

Unit tests cover the pure parts — config precedence, DSN parsing, error-document
parsing, table formatting — because the rest is HTTP, and the server's own
suites already prove the endpoints.
