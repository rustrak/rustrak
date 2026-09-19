-- External identities are kept separate from users so the immutable OIDC
-- (issuer, subject) pair is authoritative even when an email address changes.
CREATE TABLE oidc_identities (
    id SERIAL PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    issuer VARCHAR(2048) NOT NULL,
    subject VARCHAR(255) NOT NULL,
    email_at_link VARCHAR(255) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_login TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (issuer, subject)
);

CREATE INDEX idx_oidc_identities_user ON oidc_identities(user_id);
