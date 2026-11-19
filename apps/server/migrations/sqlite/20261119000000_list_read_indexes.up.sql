-- The reads that slowed down on large tables (issue #557). See the Postgres
-- migration of the same version for the measurements.

-- The span list is newest first within a project, and nothing was ordered
-- that way: each page sorted every span of the project.
CREATE INDEX IF NOT EXISTS idx_spans_project_start
    ON spans (project_id, start_timestamp);

-- Everything the performance pages aggregate, in the order they group by,
-- then by time, so each read is answered from the index alone. SQLite has no
-- INCLUDE: the columns Postgres only carries are keys here.
CREATE INDEX IF NOT EXISTS idx_transactions_group_window
    ON transactions (project_id, transaction_name, op, timestamp,
                     duration_ms, status, environment, id);
-- Both are prefixes of it.
DROP INDEX IF EXISTS idx_transactions_group_duration;
DROP INDEX IF EXISTS idx_transactions_project_name;
