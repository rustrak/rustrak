---
"@rustrak/server": minor
---

Add optional email delivery for team invitations

Admins can flip a "send by email" switch when inviting members. The switch is only available when a mail transport is configured: an enabled email alert integration (its SMTP credentials win) or server-level `SMTP_HOST`/`SMTP_PORT`/`SMTP_USERNAME`/`SMTP_PASSWORD`/`SMTP_FROM` environment variables. Delivered invitations record `email_sent_at`; undelivered ones can be re-sent via the new `POST /api/invitations/{token}/resend` endpoint (with `force: true` to re-deliver). A new `GET /api/invitations/email-status` endpoint reports whether delivery is available and from which source. Invitations without the switch keep the existing copy-the-link flow, and the alerts API now redacts `smtp_password` instead of returning it in plaintext.
