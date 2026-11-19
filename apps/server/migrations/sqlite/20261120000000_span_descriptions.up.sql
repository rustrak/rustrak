-- A span's description stored once per project (issue #558). See the
-- Postgres migration of the same version for the measurements.
--
-- AUTOINCREMENT, so the id of a removed row is never given to another
-- description.
CREATE TABLE span_descriptions (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id   INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    hash         BLOB NOT NULL,
    text         TEXT NOT NULL,
    last_used_at TEXT NOT NULL,
    UNIQUE (project_id, hash)
);

ALTER TABLE spans ADD COLUMN description_id INTEGER;
