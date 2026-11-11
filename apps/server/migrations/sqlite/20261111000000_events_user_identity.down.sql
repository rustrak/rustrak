DROP INDEX IF EXISTS idx_events_issue_ingested;
CREATE INDEX idx_events_issue_ingested ON events(issue_id, ingested_at);
ALTER TABLE events DROP COLUMN user_identity;
