-- Indexes the storage cleanup walks.
--
-- The cleanup deletes one project at a time in batches of the oldest rows,
-- `WHERE project_id = ? AND ingested_at < ?`. transactions and logs already
-- carry a (project_id, ingested_at) index; events only had
-- (project_id, event_type, ingested_at), which cannot serve that range, so
-- every batch and every preview scanned the table. On events that is worse
-- than it sounds: `data` sits before `ingested_at` in the row, so reading the
-- date walks the payload's overflow pages.
--
-- (issue_id, ingested_at) answers the preview's "does this issue keep any
-- event newer than the cutoff" in one probe per issue instead of a pass over
-- all of its events.
CREATE INDEX IF NOT EXISTS idx_events_project_ingested ON events(project_id, ingested_at);
CREATE INDEX IF NOT EXISTS idx_events_issue_ingested ON events(issue_id, ingested_at);
