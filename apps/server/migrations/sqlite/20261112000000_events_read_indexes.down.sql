CREATE INDEX IF NOT EXISTS idx_events_project_digested ON events(project_id, digested_at DESC);
CREATE INDEX IF NOT EXISTS idx_events_event_type ON events(project_id, event_type, ingested_at DESC);
DROP INDEX IF EXISTS idx_events_project_type_ingested;
DROP INDEX IF EXISTS idx_events_project_release;
DROP INDEX IF EXISTS idx_events_grouping;
