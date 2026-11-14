---
"@rustrak/server": minor
---

The digest commits events in batches through one writer: memory at rest drops from about 74 MB to 24 MB, and a backlog drains in less than half the time. The dashboard's event queries get the indexes they need and redundant ones are dropped. The first start after upgrading builds those indexes before the server opens its port, so it takes longer on a large database. Ingest answers 503 with `Retry-After` while the database is unavailable, the backlog is full (`INGEST_MAX_PENDING_EVENTS`) or the disk is nearly full (`INGEST_MIN_FREE_DISK_MB`). Shutdown drains the digest, and events that cannot be stored move to `dead/`. A numeric environment variable that does not parse now stops startup. Also: saved views on every list page, one Issues page for every project's errors and user feedback (also served as Sentry's `GET /api/0/organizations/{org}/issues/`), and `OIDC_ADDITIONAL_TRUSTED_AUDIENCES` for Zitadel and Keycloak (@lutek).
