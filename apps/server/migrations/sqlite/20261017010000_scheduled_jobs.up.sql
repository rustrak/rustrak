-- Internal recurring jobs (the monitor sweep, automatic storage cleanup...).
-- One row per job: its user settings and when it runs next, persisted so a
-- restart neither skips nor repeats a run, plus how the last run went.
CREATE TABLE scheduled_jobs (
    key              VARCHAR(64) PRIMARY KEY,
    settings         TEXT NOT NULL DEFAULT '{}',
    next_run_at      TEXT,
    last_started_at  TEXT,
    last_finished_at TEXT,
    -- ok | error
    last_status      VARCHAR(16),
    last_result      TEXT,
    last_error       TEXT
);
