-- The cleanup ages standalone spans out by COALESCE(timestamp,
-- start_timestamp). Indexing that expression after project_id makes each
-- batch a range scan instead of a pass over every span the project keeps,
-- held under the write lock. project_id still leads, so the per-project
-- counts idx_spans_standalone_project served read this one.
--
-- Plain, transactional statements, not CONCURRENTLY (see
-- 20261116000000_trim_span_indexes).
DROP INDEX IF EXISTS idx_spans_standalone_project;
CREATE INDEX IF NOT EXISTS idx_spans_standalone_age
    ON spans (project_id, COALESCE(timestamp, start_timestamp))
    WHERE transaction_id IS NULL;
