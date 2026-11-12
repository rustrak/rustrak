---
"@rustrak/server": patch
---

SSO works with providers that put more than the client ID into the ID token's `aud`: list the extra values in `OIDC_ADDITIONAL_TRUSTED_AUDIENCES`. A token with several audiences must carry `azp` equal to the client ID. Unset keeps the strict check, and the log line for a rejected audience now names the setting.
