---
"@rustrak/server": "patch"
---

A transaction built from a streamed segment root now gets a valid RFC 9562 (version 8) UUID, so the dashboard no longer rejects it, and a migration repairs the ids 0.30.0 stored (@janatjak)
