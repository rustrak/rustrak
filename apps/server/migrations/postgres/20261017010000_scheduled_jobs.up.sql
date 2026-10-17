-- Internal recurring jobs (the monitor sweep, automatic storage cleanup...).
-- One row per job: its user settings and when it runs next, persisted so a
-- restart neither skips nor repeats a run, plus how the last run went.
CREATE TABLE scheduled_jobs (
    key              VARCHAR(64) PRIMARY KEY,
    settings         JSONB NOT NULL DEFAULT '{}',
    next_run_at      TIMESTAMPTZ,
    last_started_at  TIMESTAMPTZ,
    last_finished_at TIMESTAMPTZ,
    -- ok | error
    last_status      VARCHAR(16),
    last_result      JSONB,
    last_error       TEXT
);
