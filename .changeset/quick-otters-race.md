---
"@rustrak/server": minor
---

Faster and lighter: ingest accepts about 20 times more events per second with single-digit millisecond latency, the issue list loads about 118 times faster, and the server uses 10% less memory at rest and 43% less after dashboard reads. Sessions now live on the server (everyone signs in once more after upgrading), webhook and Slack alerts no longer reach private networks unless `RUSTRAK_ALERTS_ALLOW_PRIVATE_NETWORKS=on`, `per_page` above 100 answers 400, `/auth/register` and the issue fields `is_resolved`/`is_muted` are removed, and ingest matches Relay on compressed bodies, the legacy `/store/` endpoint and event attachments.
