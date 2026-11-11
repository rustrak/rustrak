-- The identity an event's user is counted by, as Sentry's
-- `EventUser.tag_value`: the first of `user.id`, `username`, `email` and
-- `ip_address` that is a non-empty string, prefixed `id:`, `username:`,
-- `email:` or `ip:` (`user_identity_from_user_field`). Kept at digest time, so
-- the issue list counts users without detoasting every event's payload.
ALTER TABLE events ADD COLUMN user_identity TEXT;

-- The issue list counts the last 24 hours, and nothing else reads this, so
-- only recent events are filled in rather than rewriting the whole table.
UPDATE events SET user_identity = CASE
    WHEN jsonb_typeof(data -> 'user' -> 'id') = 'string' AND data -> 'user' ->> 'id' <> ''
        THEN 'id:' || (data -> 'user' ->> 'id')
    WHEN jsonb_typeof(data -> 'user' -> 'username') = 'string' AND data -> 'user' ->> 'username' <> ''
        THEN 'username:' || (data -> 'user' ->> 'username')
    WHEN jsonb_typeof(data -> 'user' -> 'email') = 'string' AND data -> 'user' ->> 'email' <> ''
        THEN 'email:' || (data -> 'user' ->> 'email')
    WHEN jsonb_typeof(data -> 'user' -> 'ip_address') = 'string' AND data -> 'user' ->> 'ip_address' <> ''
        THEN 'ip:' || (data -> 'user' ->> 'ip_address')
END
WHERE ingested_at >= NOW() - INTERVAL '2 days';
