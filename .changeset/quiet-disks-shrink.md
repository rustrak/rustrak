---
"@rustrak/server": minor
---

Events, transactions and spans take far less disk: payloads are stored zstd-compressed with per-instance dictionaries, spans are no longer duplicated inside transactions, unused span indexes are dropped and retention now deletes standalone spans. On PostgreSQL, Compact tables on the Storage page rewrites the tables with `VACUUM FULL` to give the space back, and autovacuum runs at 1% dead rows. New `OIDC_GROUP_ROLES` maps provider groups to project roles on every SSO login (@rane-keba). Deleting an issue returns to the issue list, and lettre 0.11.24 fixes the RUSTSEC SMTP advisories. Upgrading is one-way: back up first, since v0.28.0 cannot start on a migrated database.
