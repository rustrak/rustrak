-- Deleting a grouping or an issue cascades to its events by `grouping_id`,
-- a foreign key nothing indexed: every delete scanned the table.
--
-- A plain, transactional CREATE INDEX, not CONCURRENTLY: migrations run
-- before the server listens and one process owns the database, so nothing
-- writes while it builds, and a build that fails rolls back instead of
-- leaving an INVALID index for the retry to skip.

CREATE INDEX IF NOT EXISTS idx_events_grouping
    ON events (grouping_id);
