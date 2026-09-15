---
'@rustrak/server': minor
---

The dashboard is now served by the server, from one image.

`apps/webview-ui` (Next.js) is replaced by `apps/dashboard`, a React SPA built
with Vite and TanStack Router. It compiles to static files that the Rust server
hands out at `/`, so the same Actix process answers the page and the API.

**One container instead of two.** `rustrak/rustrak-ui` is no longer published,
the `ui` service is gone from `docker-compose.yml`, and `RUSTRAK_API_URL` is no
longer needed: the browser calls the API on the origin it loaded the page from,
which also keeps the session cookie first-party and removes CORS from the
dashboard's path. Open the server's own address — `:8080` by default.

The dashboard stays optional. It is mounted only when `RUSTRAK_DASHBOARD_DIR`
(default `./static`) holds an `index.html`, so an image built without one
serves the API exactly as before, and `cargo build` still works for anyone who
never installs Node.

Nothing about the dashboard looks or behaves differently. Two things did have
to change:

- **The sign-in page is at `/login`,** where it was `/auth/login`. `/auth` is
  the API's namespace, and now that the page and the API share an origin it
  cannot also be a page. Every other address is unchanged.
- **The update check runs in the browser** rather than in a server process, so
  the release feed is fetched per reader instead of once per hour per instance.
  It still sends nothing about the instance, still fails silently, and is now
  disabled at build time with `VITE_RUSTRAK_VERSION_CHECK_ENABLED=false`.
