---
"@rustrak/server": minor
---

Database spans take about half the space: a span's description is stored once per project, parent and segment ids take 8 bytes, and a transaction's spans no longer repeat its platform, release and environment (`spans` table −42% on PostgreSQL, −50% on SQLite). Span and performance reads stay fast on tens of millions of rows (span list 97 s to 20 ms on 42M spans) with two new indexes, list totals capped at 1000 (`total_capped`) and a 90-day window on the transaction and span lists. Deleting a project answers at once and removes its rows in the background. Spans streamed by JavaScript SDK 11 show up in Performance as transactions. Spans without an op get `default` or the op Relay infers. Measurements and reports by (@janatjak).
