---
"@rustrak/server": patch
---

Session items with the `unhandled` status (sessions protocol 1.6.0, sent by the JavaScript SDKs since 11.x instead of `crashed` for unhandled errors) are accepted and counted as errored sessions; the `unhandled` counter of pre-aggregated `sessions` items is summed the same way. They used to be dropped, which reported a crash-free rate of 100% and zero errored sessions for browser apps that had thrown.
