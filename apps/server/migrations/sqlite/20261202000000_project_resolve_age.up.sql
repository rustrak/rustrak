-- Sentry's `sentry:resolve_age`: hours an unresolved issue may go without
-- events before the scheduler resolves it. NULL turns it off.
ALTER TABLE projects ADD COLUMN resolve_age BIGINT;
