-- no-transaction
--
-- Answers the cleanup preview's "does this issue keep any event newer than
-- the cutoff" in one probe per issue instead of a pass over all of its events.
-- CONCURRENTLY for the same reason as the previous migration.

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_events_issue_ingested
    ON events (issue_id, ingested_at);
