-- Whether the account's owner proved they own its email. Only an account SSO
-- provisioned from an email the provider had not verified is FALSE; such an
-- account is never linked to another identity on the provider's word.
ALTER TABLE users ADD COLUMN email_verified BOOLEAN NOT NULL DEFAULT TRUE;

-- Before this column, an account SSO provisioned from an unverified email
-- looked like any other. No row records what the provider said, so every
-- account with a linked SSO identity starts unverified: its own logins keep
-- working, and only automatic linking of another identity into it stops.
UPDATE users SET email_verified = FALSE WHERE id IN (SELECT user_id FROM oidc_identities);
