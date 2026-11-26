---
"@rustrak/server": patch
---

Plugins use less memory and fewer threads: installing one no longer leaves compiler memory resident, every call runs on one shared pool of four threads with a queue that answers 503 after five seconds, a plugin that sleeps past its time budget is stopped at once, and a `noexec` plugin directory no longer recompiles plugins at every boot. Settings › Plugins becomes a searchable catalog with a plugin's README before installing it and `rustrak-plugin new` is interactive.
