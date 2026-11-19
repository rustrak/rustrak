-- The reads that slowed down on large tables (issue #557), measured on
-- 14.2M spans and 1.78M transactions (PostgreSQL 18).
--
-- Plain, transactional statements, not CONCURRENTLY (see
-- 20261116000000_trim_span_indexes): migrations run before the server
-- listens. Building both took 4 s there; allow a few minutes on tens of
-- millions of rows and a slow disk.

-- The span list is newest first within a project, and nothing was ordered
-- that way: each page sorted every span of the project (604k pages read
-- for 20 rows, 35 with this). `id`, the tie-break, is left out: the planner
-- sorts each run of equal timestamps as it reads them, and with it the
-- index is 672 MB instead of 427 MB.
CREATE INDEX IF NOT EXISTS idx_spans_project_start
    ON spans (project_id, start_timestamp);

-- Everything the performance pages aggregate, in the order they group by,
-- then by time. The endpoint list read through idx_transactions_group_duration
-- for its order and then fetched every row from the table (1.48M pages
-- read, 17.6k with this); one endpoint's figures, trend and slowest samples
-- are a range of it. `id` makes the ranking of the slowest samples
-- answerable without the table.
CREATE INDEX IF NOT EXISTS idx_transactions_group_window
    ON transactions (project_id, transaction_name, op, timestamp)
    INCLUDE (duration_ms, status, environment, id);
-- Both are prefixes of it.
DROP INDEX IF EXISTS idx_transactions_group_duration;
DROP INDEX IF EXISTS idx_transactions_project_name;

-- An index-only scan still reads the table for every page not yet marked
-- all-visible, and only a vacuum marks them. These tables are appended to,
-- and up to PostgreSQL 17 a table that only grows is vacuumed after
-- autovacuum_vacuum_insert_threshold + autovacuum_vacuum_insert_scale_factor
-- * reltuples inserts, 1000 + 0.2 * rows by default (PostgreSQL docs,
-- "Routine Vacuuming", The Autovacuum Daemon): up to a fifth of the rows,
-- the newest, which are the ones a time window reads. Measured on 16.16:
-- 45 s after 5% more rows, a count over them fetched every one from the
-- table at the default, and none at 1%. PostgreSQL 18 counts the factor
-- against the unfrozen part of the table only, so it already vacuums
-- sooner. The vacuum itself is small: it skips the pages already marked.
ALTER TABLE events SET (autovacuum_vacuum_insert_scale_factor = 0.01);
ALTER TABLE transactions SET (autovacuum_vacuum_insert_scale_factor = 0.01);
ALTER TABLE spans SET (autovacuum_vacuum_insert_scale_factor = 0.01);
ALTER TABLE logs SET (autovacuum_vacuum_insert_scale_factor = 0.01);
