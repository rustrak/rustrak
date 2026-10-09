---
"@rustrak/server": "patch"
---

Sessions with the protocol 1.6.0 `unhandled` status, sent by the JavaScript SDKs since 11.x, are now accepted and counted as errored instead of crashed (@edsonmartins). Unknown session statuses count as errored instead of dropping the session, matching Relay. Session counts are no longer flushed on the request path, which deadlocked on PostgreSQL and answered the SDK with a 500; buffered counts are still persisted on graceful shutdown. Healthy session counts are clamped at zero like Sentry.
