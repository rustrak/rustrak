---
"@rustrak/server": patch
---

A 24-hour clock setting in Preferences, stored per user and left to the reader's language until they choose (@janatjak). The dashboard loads much less on first visit: pages leave the entry chunk, assets ship precompressed as Brotli and gzip and are served as they are, and the API client validates with Zod Mini. Signing in now applies the reader's saved language, timezone and clock straight away (@janatjak), and a stale session answer no longer undoes a sign-in.
