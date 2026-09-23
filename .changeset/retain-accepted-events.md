---
"@rustrak/server": patch
---

Keep acknowledged error events queued when quota becomes unavailable before digest, so the recovery worker can process them later instead of silently deleting them.
