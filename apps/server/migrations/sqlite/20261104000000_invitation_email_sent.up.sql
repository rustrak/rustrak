-- Invitation email delivery.
--
-- NULL means "no email has been sent for this invitation" -- either the
-- inviter chose manual sharing (the pre-existing behaviour), or the send
-- failed and the link is still copyable from the pending list. A timestamp
-- means the invitation email was accepted by SMTP for that recipient.
ALTER TABLE invitations ADD COLUMN email_sent_at TEXT;
