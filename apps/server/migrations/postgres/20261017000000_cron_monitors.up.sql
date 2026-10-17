-- Cron monitors (Sentry Crons).
-- A monitor expects a job to check in on a schedule; each environment it
-- checks in from is tracked separately; every run is one check-in.
CREATE TABLE monitors (
    id          UUID PRIMARY KEY,
    project_id  INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    slug        VARCHAR(50) NOT NULL,
    name        VARCHAR(128) NOT NULL,
    -- Validated MonitorConfig: schedule, margin, max runtime, timezone, thresholds.
    config      JSONB NOT NULL,
    is_muted    BOOLEAN NOT NULL DEFAULT FALSE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (project_id, slug)
);

CREATE TABLE monitor_environments (
    id                  UUID PRIMARY KEY,
    monitor_id          UUID NOT NULL REFERENCES monitors(id) ON DELETE CASCADE,
    environment         VARCHAR(64) NOT NULL,
    -- active (waiting for a first ok) | ok | error (incident open)
    status              VARCHAR(16) NOT NULL DEFAULT 'active',
    next_checkin        TIMESTAMPTZ,
    next_checkin_latest TIMESTAMPTZ,
    last_checkin        TIMESTAMPTZ,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (monitor_id, environment)
);

-- The sweep's scan for missed check-ins.
CREATE INDEX idx_monitor_environments_due ON monitor_environments(next_checkin_latest);

CREATE TABLE monitor_checkins (
    id                     UUID PRIMARY KEY,
    -- The SDK's check_in_id; ids are only unique within a monitor.
    guid                   UUID NOT NULL,
    project_id             INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    monitor_id             UUID NOT NULL REFERENCES monitors(id) ON DELETE CASCADE,
    monitor_environment_id UUID NOT NULL REFERENCES monitor_environments(id) ON DELETE CASCADE,
    -- ok | error | in_progress | missed | timeout | unknown
    status                 VARCHAR(16) NOT NULL,
    duration_ms            BIGINT,
    date_added             TIMESTAMPTZ NOT NULL,
    date_updated           TIMESTAMPTZ NOT NULL,
    expected_time          TIMESTAMPTZ,
    timeout_at             TIMESTAMPTZ,
    trace_id               VARCHAR(64)
);

CREATE INDEX idx_monitor_checkins_guid ON monitor_checkins(monitor_id, guid);
CREATE INDEX idx_monitor_checkins_env_date ON monitor_checkins(monitor_environment_id, date_added DESC);
CREATE INDEX idx_monitor_checkins_open ON monitor_checkins(timeout_at) WHERE status = 'in_progress';
-- Retention purge.
CREATE INDEX idx_monitor_checkins_date ON monitor_checkins(date_added);
