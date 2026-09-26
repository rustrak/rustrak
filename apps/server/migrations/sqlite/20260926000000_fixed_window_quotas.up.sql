-- Fixed-window quota counters, checked and incremented in the digest
-- transaction the way Relay's consistent rate limiter does. A window is
-- numbered as seconds since the epoch divided by its length.
ALTER TABLE installation ADD COLUMN quota_minute_window BIGINT NOT NULL DEFAULT 0;
ALTER TABLE installation ADD COLUMN quota_minute_count BIGINT NOT NULL DEFAULT 0;
ALTER TABLE installation ADD COLUMN quota_hour_window BIGINT NOT NULL DEFAULT 0;
ALTER TABLE installation ADD COLUMN quota_hour_count BIGINT NOT NULL DEFAULT 0;
ALTER TABLE projects ADD COLUMN quota_minute_window BIGINT NOT NULL DEFAULT 0;
ALTER TABLE projects ADD COLUMN quota_minute_count BIGINT NOT NULL DEFAULT 0;
ALTER TABLE projects ADD COLUMN quota_hour_window BIGINT NOT NULL DEFAULT 0;
ALTER TABLE projects ADD COLUMN quota_hour_count BIGINT NOT NULL DEFAULT 0;
-- Events the quota turned away after ingest had accepted them: Rustrak's
-- equivalent of a Relay `RateLimited` outcome.
ALTER TABLE projects ADD COLUMN rate_limited_event_count BIGINT NOT NULL DEFAULT 0;
-- A project's own limits, which can only tighten the operator's
-- MAX_EVENTS_PER_PROJECT_*: the analogue of Sentry's per-DSN rate limit.
ALTER TABLE projects ADD COLUMN rate_limit_per_minute BIGINT;
ALTER TABLE projects ADD COLUMN rate_limit_per_hour BIGINT;
