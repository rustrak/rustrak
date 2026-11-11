-- Dashboard sign-in sessions, kept server-side: the cookie carries only an
-- opaque random id, and only its SHA-256 is stored, so logout and revocation
-- end a session for good. `user_id` is NULL while a session holds only
-- pre-login state (an SSO or GitHub flow in progress).
CREATE TABLE sessions (
    id_hash CHAR(64) PRIMARY KEY,
    user_id INTEGER REFERENCES users(id) ON DELETE CASCADE,
    data TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    last_seen_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_sessions_user_id ON sessions(user_id);
CREATE INDEX idx_sessions_expires_at ON sessions(expires_at);
