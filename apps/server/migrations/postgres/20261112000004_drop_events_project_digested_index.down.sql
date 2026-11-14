CREATE INDEX IF NOT EXISTS idx_events_project_digested
    ON events (project_id, digested_at DESC);
