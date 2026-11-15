# Rustrak Server

Rustrak is an open-source, self-hosted error tracker written in Rust and compatible with the official Sentry SDKs. It runs as one container on SQLite or PostgreSQL, peaks under 60 MB of memory and accepts about 5,000 events per second on 4 cores. Besides errors it takes transactions, logs, release health, cron check-ins, user feedback and AI agent traces.

Apps keep their Sentry SDK; only the DSN changes. The dashboard is compiled into this image and served from `/`, on the same origin as the API.

- Website and docs: https://rustrak.dev
- Source (AGPL-3.0): https://github.com/rustrak/rustrak

## Tags

| Tag | Backend |
|---|---|
| `latest`, `vX.Y.Z` | SQLite, nothing else to run |
| `postgres`, `vX.Y.Z-postgres` | PostgreSQL, for higher volume or concurrent writes |
| `next`, `next-postgres` | Release candidates |

Images are built for `linux/amd64` and `linux/arm64`.

## Quick start

```bash
docker run -d \
  --name rustrak \
  -p 8080:8080 \
  -v rustrak_data:/data \
  -e SESSION_SECRET_KEY=$(openssl rand -hex 32) \
  -e CREATE_SUPERUSER=admin@example.com:changeme123 \
  -e PUBLIC_URL=http://localhost:8080 \
  rustrak/rustrak-server:latest
```

Open `http://localhost:8080` and sign in with the email and password from `CREATE_SUPERUSER`.

- `SESSION_SECRET_KEY` signs the session cookie. Keep the same value across restarts.
- `CREATE_SUPERUSER` is `email:password` and only creates the first account.
- `PUBLIC_URL` is the address your SDKs and browser reach. It becomes the host in every DSN.

SQLite suits personal projects, small teams and up to about 1,000 events an hour. For more, use the `postgres` image and set `DATABASE_URL`. Compose files for both: https://rustrak.dev/getting-started/installation

## More

- [Installation](https://rustrak.dev/getting-started/installation)
- [Environment variables](https://rustrak.dev/configuration/environment)
- [Production checklist](https://rustrak.dev/configuration/production)
