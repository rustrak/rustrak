ALTER TABLE logs ADD COLUMN environment TEXT;
UPDATE logs SET environment = json_extract(attributes, '$."sentry.environment".value')
WHERE json_valid(attributes)
  AND json_extract(attributes, '$."sentry.environment".type') = 'string'
  AND json_type(attributes, '$."sentry.environment".value') = 'text'
  AND json_extract(attributes, '$."sentry.environment".value') <> '';
CREATE INDEX idx_logs_project_environment ON logs(project_id, environment);
