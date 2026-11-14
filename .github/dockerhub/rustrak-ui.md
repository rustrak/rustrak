# Rustrak UI

The [Rustrak](https://rustrak.dev) dashboard on its own host: nginx serving the dashboard and proxying the API to a Rustrak server.

**Most installations do not need this image.** `rustrak/rustrak-server` already serves the dashboard from `/`. Use this one only to keep the dashboard on a different host from the server.

Rustrak is an open-source, self-hosted error tracker written in Rust and compatible with the official Sentry SDKs. Source (AGPL-3.0): https://github.com/rustrak/rustrak

## Usage

Run the server with `RUSTRAK_DASHBOARD=off`, then:

```bash
docker run -d \
  --name rustrak-ui \
  -p 3000:3000 \
  -e RUSTRAK_API_URL=http://your-rustrak-server:8080 \
  rustrak/rustrak-ui:latest
```

| Variable | Default | Description |
|---|---|---|
| `RUSTRAK_API_URL` | `http://server:8080` | Rustrak server the API is proxied to. No path, no trailing slash. |
| `PORT` | `3000` | Port nginx listens on |

See [Dashboard on its own host](https://rustrak.dev/configuration/production#dashboard-on-its-own-host).
