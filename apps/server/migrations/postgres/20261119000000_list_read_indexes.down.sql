ALTER TABLE events RESET (autovacuum_vacuum_insert_scale_factor);
ALTER TABLE transactions RESET (autovacuum_vacuum_insert_scale_factor);
ALTER TABLE spans RESET (autovacuum_vacuum_insert_scale_factor);
ALTER TABLE logs RESET (autovacuum_vacuum_insert_scale_factor);
CREATE INDEX IF NOT EXISTS idx_transactions_project_name
    ON transactions (project_id, transaction_name);
CREATE INDEX IF NOT EXISTS idx_transactions_group_duration
    ON transactions (project_id, transaction_name, op)
    WHERE duration_ms IS NOT NULL;
DROP INDEX IF EXISTS idx_transactions_group_window;
DROP INDEX IF EXISTS idx_spans_project_start;
