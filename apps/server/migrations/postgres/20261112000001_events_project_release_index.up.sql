-- A release's issues, the releases page and its release x environment pairs,
-- answered by an index-only scan instead of a pass over the project's events.
-- Transactional, for the same reason as the previous migration.

CREATE INDEX IF NOT EXISTS idx_events_project_release
    ON events (project_id, release, environment) INCLUDE (issue_id, ingested_at);
