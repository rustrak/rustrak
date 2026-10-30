-- In-flight email-delivery claims, recorded apart from email_sent_at.
--
-- A claim marks the invitation as being emailed right now, so an overlapping
-- resend cannot also send. A claim older than the SMTP horizon is a crashed or
-- wedged attempt, not a delivery in progress, so it stops blocking. NULL means
-- not currently claimed.
ALTER TABLE invitations ADD COLUMN email_delivery_claimed_at TEXT;
