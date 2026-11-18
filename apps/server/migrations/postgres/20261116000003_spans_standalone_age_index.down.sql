DROP INDEX IF EXISTS idx_spans_standalone_age;
CREATE INDEX IF NOT EXISTS idx_spans_standalone_project
    ON spans (project_id)
    WHERE transaction_id IS NULL;
