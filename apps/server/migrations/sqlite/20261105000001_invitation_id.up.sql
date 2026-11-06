-- What manages an invitation (list, resend, revoke) without knowing its token,
-- which claims the account.
ALTER TABLE invitations ADD COLUMN id TEXT NOT NULL DEFAULT '';
UPDATE invitations SET id = lower(hex(randomblob(20)));
