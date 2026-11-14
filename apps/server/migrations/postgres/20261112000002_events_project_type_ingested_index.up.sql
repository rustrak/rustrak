-- Project stats (`events/stats`, the headline counts and the projects list),
-- answered by an index-only scan. Replaces idx_events_event_type, a prefix of
-- it. One transaction: the old index goes only if its replacement was built.

CREATE INDEX IF NOT EXISTS idx_events_project_type_ingested
    ON events (project_id, event_type, ingested_at)
    INCLUDE (environment, release, level, issue_id);

DROP INDEX IF EXISTS idx_events_event_type;
