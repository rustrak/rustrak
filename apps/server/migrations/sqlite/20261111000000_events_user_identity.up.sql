-- The identity an event's user is counted by: the first of `user.id`,
-- `email`, `username` and `ip_address` that is a non-empty string, prefixed
-- with its name (`user_identity_from_user_field`). Kept at digest time, so
-- the issue list counts users without reading every event's payload.
ALTER TABLE events ADD COLUMN user_identity TEXT;

-- The issue list counts the last 24 hours, and nothing else reads this, so
-- only recent events are filled in: rewriting every row of a large table to
-- fill a value nothing would read is not worth a slow upgrade. A date-only
-- bound compares below every stored format of the same day.
UPDATE events SET user_identity = CASE
    WHEN json_type(data, '$.user.id') = 'text' AND json_extract(data, '$.user.id') <> ''
        THEN 'id:' || json_extract(data, '$.user.id')
    WHEN json_type(data, '$.user.email') = 'text' AND json_extract(data, '$.user.email') <> ''
        THEN 'email:' || json_extract(data, '$.user.email')
    WHEN json_type(data, '$.user.username') = 'text' AND json_extract(data, '$.user.username') <> ''
        THEN 'username:' || json_extract(data, '$.user.username')
    WHEN json_type(data, '$.user.ip_address') = 'text' AND json_extract(data, '$.user.ip_address') <> ''
        THEN 'ip_address:' || json_extract(data, '$.user.ip_address')
END
WHERE ingested_at >= date('now', '-2 days');

-- `data` sits before every column after it in the row, so reading even
-- `environment` walks the payload's overflow pages. The index carries all
-- the issue list reads, and still serves (issue_id, ingested_at) lookups.
DROP INDEX IF EXISTS idx_events_issue_ingested;
CREATE INDEX idx_events_issue_ingested
    ON events(issue_id, ingested_at, environment, release, user_identity);
