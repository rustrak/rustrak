---
"@rustrak/server": minor
---

Plugins (beta). Rustrak installs and runs plugins compiled to WebAssembly components, each sandboxed per request with its own SQLite file, settings, permissions and pages in the dashboard, installed from a signed registry or a `.rtp` package. The plugin SDK ships with each release: `@rustrak/plugin` (the `rustrak-plugin` CLI and the UI framework) on npm and the `rustrak-plugin` crate on crates.io. `@rustrak/client` gains the plugin and plugin storage endpoints.
