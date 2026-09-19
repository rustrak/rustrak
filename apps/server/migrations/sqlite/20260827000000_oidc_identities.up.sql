-- External identities are kept separate from users so the immutable OIDC
-- (issuer, subject) pair is authoritative even when an email address changes.
CREATE TABLE oidc_identities (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    issuer TEXT NOT NULL,
    subject TEXT NOT NULL,
    email_at_link TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_login TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (issuer, subject)
);

CREATE INDEX idx_oidc_identities_user ON oidc_identities(user_id);
