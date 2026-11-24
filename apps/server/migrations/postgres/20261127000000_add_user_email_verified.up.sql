-- Whether the account's owner proved they own its email. Only an account SSO
-- provisioned from an email the provider had not verified is FALSE; such an
-- account is never linked to another identity on the provider's word.
ALTER TABLE users ADD COLUMN email_verified BOOLEAN NOT NULL DEFAULT TRUE;
