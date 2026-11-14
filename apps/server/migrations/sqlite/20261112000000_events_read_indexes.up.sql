-- Indexes for the read paths over `events` that scanned a project's every
-- event. Each build reads the whole table once. Migrations run before the
-- server binds its port, so while they build nothing is listening: SDKs get
-- connection refused, not a slow answer, and retry or drop per their own
-- transport (about 2-3 s for a 6 GB database in the page cache, a minute or
-- two cold on a slow disk). Events already in the spool are digested once
-- the server is up.

-- Deleting a grouping or an issue cascades to its events by `grouping_id`,
-- a foreign key nothing indexed: every delete scanned the table.
CREATE INDEX IF NOT EXISTS idx_events_grouping ON events(grouping_id);

-- A release's issues, the releases page and its release x environment pairs.
-- Covering: `data` sits before these columns in the row, so a plan that has
-- to visit the row walks the payload's overflow pages.
CREATE INDEX IF NOT EXISTS idx_events_project_release
    ON events(project_id, release, environment, issue_id, ingested_at);

-- Project stats (`events/stats`, the headline counts and the projects list),
-- answered from the index alone. Replaces (project_id, event_type,
-- ingested_at), a prefix of it.
CREATE INDEX IF NOT EXISTS idx_events_project_type_ingested
    ON events(project_id, event_type, ingested_at, environment, release, level, issue_id);
DROP INDEX IF EXISTS idx_events_event_type;

-- No query reads events by project in digest order.
DROP INDEX IF EXISTS idx_events_project_digested;
