---
'@rustrak/server': patch
---

Start SSO discovery in the background so an unreachable provider cannot stop
the server. Discovery retries with backoff (30 seconds, doubling to 15 minutes)
and `/auth/sso/config` reports SSO as disabled until it succeeds, so a provider
that was down at boot needs no restart. A malformed issuer URL or a cleartext
endpoint is still refused at startup, since no retry can fix it.
