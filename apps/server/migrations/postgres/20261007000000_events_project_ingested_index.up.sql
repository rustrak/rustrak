-- no-transaction
--
-- The storage cleanup deletes one project at a time in batches of the oldest
-- rows, `WHERE project_id = $1 AND ingested_at < $2`. transactions and logs
-- already carry a (project_id, ingested_at) index; events only had
-- (project_id, event_type, ingested_at), which cannot serve that range, so
-- every batch and every preview scanned the table.
--
-- CONCURRENTLY (hence `-- no-transaction`, and one statement per file) lets
-- ingestion keep writing while the index builds.

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_events_project_ingested
    ON events (project_id, ingested_at);
