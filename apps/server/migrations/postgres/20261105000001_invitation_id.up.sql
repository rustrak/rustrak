-- What manages an invitation (list, resend, revoke) without knowing its token,
-- which claims the account.
ALTER TABLE invitations ADD COLUMN id VARCHAR(40) NOT NULL DEFAULT '';
UPDATE invitations SET id = md5(random()::text || token);
