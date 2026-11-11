---
"@rustrak/server": minor
---

Server-side sessions (everyone signs in again once after upgrading), 401 before input validation, `per_page` above 100 answers 400, removed `/auth/register` and the deprecated `is_resolved`/`is_muted` issue fields, legacy `/store/` accepted, unsupported `Content-Encoding` answers 415, event attachments are kept, and much faster issue lists and ingest.
