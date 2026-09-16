---
"@rustrak/server": patch
---

Release images are now assembled from binaries compiled in CI instead of being
compiled inside `docker build`. The published `rustrak/rustrak-server` and
`rustrak/rustrak-ui` images have the same layout and contents as before; the
server binary is linked against Debian 12's glibc, the same one the distroless
runtime image ships. `docker compose build` and a local `docker build` still
compile from source.
