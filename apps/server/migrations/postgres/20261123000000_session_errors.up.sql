-- Errored sessions by id, as Relay counts them (`SessionUpdate::all_errors`):
-- a session is errored once, on any update with errors or an error status,
-- fatal ones included. A row is kept while its session can still be updated
-- (Relay's 5 day window); the aggregator deletes older ones.
CREATE TABLE session_errors (
    project_id INTEGER     NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    sid        TEXT        NOT NULL,
    started    TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (project_id, sid)
);

CREATE INDEX idx_session_errors_started ON session_errors(started);

-- `errored` now counts every errored session, crashed and abnormal included
-- (Relay's set); it counted only the non-fatal ones.
UPDATE session_counts SET errored = errored + crashed + abnormal;
