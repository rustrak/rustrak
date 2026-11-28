---
"@rustrak/cli": minor
---

Add `rustrak`, an official command-line client.

It signs in with an email and password, trades the session for a long-lived API
token (`POST /api/tokens` refuses a token caller, so the login flow is the same
two steps the dashboard takes), and stores only the token. Every later command
is a plain Bearer call: triage issues by short id or UUID, read structured logs
and follow them with `log tail`, inspect AI agent traces and model/tool
breakdowns, rank transactions by p95, check release health, and manage
projects, cron monitors, alert rules and tokens. `send` posts a test event to a
DSN the way an SDK does, and `api` reaches anything the commands do not wrap.

A terminal gets tables and a pipe gets JSON, `NO_COLOR` is honoured, and
`RUSTRAK_URL`/`RUSTRAK_TOKEN` are enough to run it in CI with no config file.
