-- The environment a log was sent from: Sentry's `sentry.environment`
-- attribute, kept in a column so the list can filter on it. NULL when the SDK
-- sent none; Relay does not fill one in either.
ALTER TABLE logs ADD COLUMN environment VARCHAR(64);

UPDATE logs
SET environment = NULLIF(left(attributes -> 'sentry.environment' ->> 'value', 64), '')
WHERE attributes IS NOT NULL;

CREATE INDEX idx_logs_project_environment ON logs(project_id, environment, timestamp DESC);
