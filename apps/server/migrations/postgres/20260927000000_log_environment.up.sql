ALTER TABLE logs ADD COLUMN environment TEXT;
UPDATE logs SET environment = attributes -> 'sentry.environment' ->> 'value'
WHERE attributes -> 'sentry.environment' ->> 'type' = 'string'
  AND jsonb_typeof(attributes -> 'sentry.environment' -> 'value') = 'string'
  AND attributes -> 'sentry.environment' ->> 'value' <> '';
CREATE INDEX idx_logs_project_environment ON logs(project_id, environment);
