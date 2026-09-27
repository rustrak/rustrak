# Rustrak Server

The Rustrak server: a Rust API compatible with Sentry SDKs. It also serves the
dashboard at `/` on the same port, so one process answers both the page and the
API. `RUSTRAK_DASHBOARD=off` keeps it API-only.

## Features

- Sentry envelope protocol support
- Two-phase event ingestion (fast ingest + async digest)
- Issue grouping with custom fingerprints
- Rate limiting (per-project and global)
- Session-based authentication for the dashboard
- Token authentication for API access
- Serves the compiled dashboard from `./static`, when a build is present

## Requirements

- Rust 1.80+
- No database to run: SQLite is the default. PostgreSQL 16+ is optional, with
  `--features postgres` (or the `:postgres` image)

## Quick Start

```bash
export SESSION_SECRET_KEY="$(openssl rand -hex 32)"
export CREATE_SUPERUSER="admin@example.com:password123"
export DATABASE_URL="sqlite://./rustrak.db"
export SOURCEMAP_STORAGE_PATH="./data/sourcemaps"

cargo run
```

Or copy `.env.example` to `.env`, which sets the same paths. The server listens
on `http://localhost:8080`. It serves the dashboard only if `./static` holds a
build (`pnpm build --filter=@rustrak/server` from the repository root puts one
there).

With PostgreSQL:

```bash
docker compose -f ../../docker-compose.dev.yml up -d postgres
export DATABASE_URL="postgres://rustrak:rustrak@localhost:5432/rustrak"
cargo run --no-default-features --features postgres
```

## Docker

```bash
docker run -d \
  --name rustrak \
  -p 8080:8080 \
  -v rustrak_data:/data \
  -e SESSION_SECRET_KEY="$(openssl rand -hex 32)" \
  -e CREATE_SUPERUSER=admin@example.com:changeme123 \
  rustrak/rustrak-server:latest
```

Open `http://localhost:8080` for the dashboard; the API is on the same port.
The database and uploaded source maps live in the `/data` volume. Use the
`:postgres` image and set `DATABASE_URL` for PostgreSQL.

## Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `DATABASE_URL` | No | `sqlite:///data/rustrak.db` | Database connection string. Required with the `postgres` feature |
| `SESSION_SECRET_KEY` | Production | - | 64-char hex key for sessions |
| `HOST` | No | `0.0.0.0` | Server bind address |
| `PORT` | No | `8080` | Server port |
| `RUST_LOG` | No | `info` | Log level |
| `CREATE_SUPERUSER` | No | - | Create admin user `email:password` |
| `SSL_PROXY` | No | `false` | Enable secure cookies (behind HTTPS) |
| `PUBLIC_URL` | No | `http://{HOST}:{PORT}` | The address people and SDKs reach; used in DSNs and alert links |
| `RUSTRAK_DASHBOARD` | No | `on` | `off` keeps the server API-only |
| `RUSTRAK_DASHBOARD_DIR` | No | `./static` | Where the compiled dashboard is; skipped when absent |
| `DASHBOARD_URL` | No | `PUBLIC_URL` | Base of alert links, for a dashboard on another host |
| `SOURCEMAP_STORAGE_PATH` | No | `/data/sourcemaps` | Where uploaded source maps are stored |
| `RUSTRAK_TELEMETRY` | No | `on` | `off` stops the anonymous heartbeat |

The full list is in the [environment reference](https://rustrak.github.io/rustrak/configuration/environment).

## API Endpoints

| Endpoint | Method | Auth | Description |
|----------|--------|------|-------------|
| `/api/{project_id}/envelope/` | POST | Sentry | Event ingestion |
| `/api/projects` | GET/POST | Bearer | List/create projects |
| `/api/projects/{id}` | GET/PATCH/DELETE | Bearer | Project CRUD |
| `/api/projects/{id}/issues` | GET | Bearer | List issues |
| `/api/projects/{id}/issues/{issueId}` | GET/PATCH/DELETE | Bearer | Issue CRUD |
| `/auth/login` | POST | - | Session login |
| `/auth/logout` | POST | Session | Session logout |
| `/health` | GET | - | Health check |

## Development

```bash
# Run tests
cargo test

# Run with hot reload
cargo watch -x run

# Format code
cargo fmt

# Lint
cargo clippy
```

### OpenAPI spec

The server generates an OpenAPI 3.x spec via the `openapi` feature flag.

**Regenerate and commit the spec after any API change** (new route, changed body/response, added param):

```bash
cargo run --bin gen_openapi --features openapi
git add openapi.json
git commit -m "chore(openapi): update spec"
```

The docs site copies the spec at CI build time; `apps/docs/public/openapi.json` is not committed.

To run the server with the interactive explorer at `/docs`:

```bash
cargo run --features openapi
# → http://localhost:8080/docs
# → http://localhost:8080/api-docs/openapi.json
```

## License

GPL-3.0
